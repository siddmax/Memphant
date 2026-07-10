//! MCP contract (Task 7): the committed artifact carries camelCase
//! `inputSchema` for all seven tools; a persistent in-process rmcp session
//! completes initialize → tools/list → tools/call retain → recall without
//! closing the transport first; startup refuses to bind without a tenant.

use std::path::Path;
use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{ApiKeyRow, InMemoryStore, NoopEmbedding, SystemClock};
use memphant_mcp::{BoundTenant, MemphantMcp, api_key_hash, resolve_tenant};
use memphant_runtime::AnyStore;
use memphant_types::{TenantId, TrustLevel};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};

const TOOL_NAMES: [&str; 7] = [
    "retain", "recall", "reflect", "correct", "forget", "trace", "mark",
];

fn dev_handler(store: InMemoryStore, tenant: TenantId) -> MemphantMcp {
    let service = MemoryService::new(
        Arc::new(AnyStore::Mem(store)),
        Arc::new(SystemClock),
        Arc::new(NoopEmbedding),
    );
    MemphantMcp::new(
        service,
        BoundTenant {
            tenant,
            max_trust: TrustLevel::TrustedSystem,
            dev_mode: true,
        },
    )
}

#[test]
fn artifact_has_camel_case_input_schema_for_all_seven_tools() {
    let generated = memphant_mcp::tools_artifact();
    let committed: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../mcp/memphant.tools.v1.json"),
        )
        .expect("committed artifact readable"),
    )
    .expect("committed artifact is JSON");

    for artifact in [&generated, &committed] {
        let tools = artifact.as_array().expect("artifact is a tool array");
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect();
        for name in TOOL_NAMES {
            assert!(names.contains(&name), "missing tool {name}");
        }
        for tool in tools {
            let name = tool["name"].as_str().unwrap_or_default();
            assert!(
                tool.get("inputSchema").is_some_and(Value::is_object),
                "tool {name} must expose camelCase inputSchema"
            );
            assert!(
                tool.get("input_schema").is_none(),
                "tool {name} must not use snake_case input_schema"
            );
            assert!(
                tool["inputSchema"].get("properties").is_some()
                    || tool["inputSchema"].get("$ref").is_some(),
                "tool {name} inputSchema must be a real schema"
            );
        }
    }

    assert_eq!(
        generated, committed,
        "mcp/memphant.tools.v1.json is stale — regenerate via `memphant-mcp --list-tools-json`"
    );
}

#[tokio::test]
async fn persistent_session_round_trips_retain_then_recall() {
    let store = InMemoryStore::default();
    let tenant = TenantId::new();
    let handler = dev_handler(store, tenant);

    // One duplex pipe carries the whole session — nothing is closed between
    // calls, proving the stdio session is persistent, not one-shot.
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let server = tokio::spawn(async move {
        handler
            .serve(server_io)
            .await
            .expect("server initializes")
            .waiting()
            .await
            .expect("server runs until client disconnect")
    });

    let client = ().serve(client_io).await.expect("client initialize handshake succeeds");

    let tools = client.list_all_tools().await.expect("tools/list");
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    for name in TOOL_NAMES {
        assert!(names.contains(&name), "tools/list missing {name}");
    }

    let scope = uuid::Uuid::new_v4();
    let actor = uuid::Uuid::new_v4();
    let retain_args = json!({
        "tenant_id": tenant.as_uuid(),
        "scope_id": scope,
        "actor_id": actor,
        "source_kind": "user",
        "source_trust": "trusted_user",
        "subject_hint": null,
        "body": "Release region is Taipei.",
        "compiler_version": null,
    });
    let retained = client
        .call_tool(
            CallToolRequestParams::new("retain")
                .with_arguments(retain_args.as_object().cloned().expect("args object")),
        )
        .await
        .expect("tools/call retain");
    assert_ne!(retained.is_error, Some(true), "retain succeeded");
    let structured = retained
        .structured_content
        .as_ref()
        .expect("retain returns structured content");
    assert!(structured["episode_id"].is_string());

    // Recall on the SAME session (stdin never closed): the degraded
    // read-your-own-writes path returns the un-reflected episode body.
    let recall_args = json!({
        "tenant_id": tenant.as_uuid(),
        "scope_id": scope,
        "actor_id": actor,
        "query": "Where is the release region?",
    });
    let recalled = client
        .call_tool(
            CallToolRequestParams::new("recall")
                .with_arguments(recall_args.as_object().cloned().expect("args object")),
        )
        .await
        .expect("tools/call recall");
    assert_ne!(recalled.is_error, Some(true), "recall succeeded");
    let structured = recalled
        .structured_content
        .as_ref()
        .expect("recall returns structured content");
    assert_eq!(
        structured["items"][0]["body"].as_str(),
        Some("Release region is Taipei.")
    );

    client.cancel().await.expect("client shuts down");
    server.await.expect("server task joins");
}

#[tokio::test]
async fn startup_refuses_without_api_key_or_dev_tenant() {
    // NOTE: env mutation — this is the only test in the binary touching
    // these variables and the round-trip tests never read them.
    unsafe {
        std::env::remove_var("MEMPHANT_API_KEY");
        std::env::remove_var("MEMPHANT_DEV_TENANT");
    }
    let store = AnyStore::Mem(InMemoryStore::default());
    let error = resolve_tenant(&store)
        .await
        .expect_err("no key + no dev tenant must refuse to start");
    assert!(
        error.contains("refusing to start"),
        "refusal is explicit: {error}"
    );

    // A revoked key must also refuse.
    let mem = InMemoryStore::default();
    let tenant = TenantId::new();
    mem.insert_api_key(ApiKeyRow {
        id: uuid::Uuid::new_v4(),
        tenant_id: tenant,
        key_hash: api_key_hash("mk_revoked"),
        label: "test".to_string(),
        max_trust: TrustLevel::TrustedUser,
        revoked: true,
    });
    unsafe {
        std::env::set_var("MEMPHANT_API_KEY", "mk_revoked");
    }
    let error = resolve_tenant(&AnyStore::Mem(mem))
        .await
        .expect_err("revoked key must refuse to start");
    assert!(error.contains("revoked"), "revocation is explicit: {error}");
    unsafe {
        std::env::remove_var("MEMPHANT_API_KEY");
    }
}
