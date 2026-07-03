use std::collections::HashMap;

use memphant_types::{ActorId, ScopeId, TenantId};
use serde_json::json;

#[test]
fn mcp_tools_publish_input_output_schemas_and_contract_annotations() {
    let tools = memphant_mcp::tool_specs();
    let by_name: HashMap<_, _> = tools
        .iter()
        .map(|tool| (tool.name.as_str(), tool))
        .collect();

    for name in [
        "retain", "recall", "reflect", "correct", "forget", "trace", "mark",
    ] {
        let tool = by_name
            .get(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert!(tool.input_schema.is_object(), "{name} input schema");
        assert!(tool.output_schema.is_object(), "{name} output schema");
    }

    assert!(by_name["recall"].annotations.read_only_hint);
    assert!(by_name["trace"].annotations.read_only_hint);
    assert!(!by_name["retain"].annotations.destructive_hint);
    assert!(by_name["retain"].annotations.idempotent_hint);
    assert!(!by_name["correct"].annotations.destructive_hint);
    assert!(by_name["correct"].annotations.idempotent_hint);
    assert!(by_name["forget"].annotations.destructive_hint);
    assert!(by_name["forget"].annotations.idempotent_hint);
    assert!(by_name["mark"].annotations.idempotent_hint);
}

#[tokio::test]
async fn mcp_tool_runtime_round_trips_retain_reflect_recall_and_trace() {
    let runtime = memphant_mcp::McpRuntime::new_in_memory();
    let listed = memphant_mcp::handle_json_rpc_value(
        &runtime,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        }),
    )
    .await;
    assert_eq!(listed["result"]["tools"][0]["name"], "retain");

    let tenant_id = TenantId::from_u128(91_000);
    let scope_id = ScopeId::from_u128(91_001);
    let actor_id = ActorId::from_u128(91_002);

    let retained = runtime
        .call_tool(
            "retain",
            json!({
                "tenant_id": tenant_id,
                "scope_id": scope_id,
                "actor_id": actor_id,
                "source_kind": "system",
                "source_trust": "trusted_system",
                "subject_hint": "callback token",
                "body": "Callback token is v2.",
                "compiler_version": null
            }),
        )
        .await
        .expect("retain tool succeeds");
    assert!(!retained.is_error);
    assert_eq!(
        retained.structured_content["enqueued"][0],
        "reflect_episode"
    );

    let reflected = runtime
        .call_tool(
            "reflect",
            json!({
                "tenant_id": tenant_id,
                "scope_id": scope_id,
                "actor_id": actor_id,
                "compiler_version": null
            }),
        )
        .await
        .expect("reflect tool succeeds");
    assert_eq!(reflected.structured_content["episodes_consumed"], 1);

    let recalled = runtime
        .call_tool(
            "recall",
            json!({
                "tenant_id": tenant_id,
                "scope_id": scope_id,
                "actor_id": actor_id,
                "allowed_scope_ids": [scope_id],
                "query": "Which callback token is current?",
                "limit": 4,
                "budget_tokens": 80,
                "mode": "fast",
                "include_beliefs": false,
                "include_trace": true
            }),
        )
        .await
        .expect("recall tool succeeds");
    assert_eq!(
        recalled.structured_content["items"][0]["body"],
        "Callback token is v2."
    );

    let trace_id = recalled.structured_content["trace_id"]
        .as_str()
        .expect("trace id");
    let trace = runtime
        .call_tool(
            "trace",
            json!({
                "tenant_id": tenant_id,
                "trace_id": trace_id
            }),
        )
        .await
        .expect("trace tool succeeds");
    assert_eq!(trace.structured_content["id"], trace_id);
}
