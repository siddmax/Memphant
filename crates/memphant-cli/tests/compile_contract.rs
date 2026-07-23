use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use memphant_core::MemoryStore;
use memphant_server::AppState;
use memphant_types::{
    ContextBindingAgentRef, ContextBindingEntityRef, ContextBindingRequest, ContextBindingResponse,
    ContextBindingScopeRef, MemoryKind, NewMemoryUnit, TenantId, TrustLevel, UnitState,
};

const TENANT: &str = "00000000-0000-0000-0000-00000000b203";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compile_is_a_deterministic_server_backed_projection_and_verify_is_complete() {
    let (url, binding, state) = spawn_server().await;
    let current_id = seed_unit(
        &state,
        &binding,
        UnitState::Active,
        "decision:queue",
        "Use LISTEN/NOTIFY.",
    )
    .await;
    let historical_id = seed_unit(
        &state,
        &binding,
        UnitState::Superseded,
        "decision:old-queue",
        "Poll every second.",
    )
    .await;
    let out = tempfile::tempdir().unwrap();

    let compiled = compile(&url, &binding, out.path(), &[]);
    assert_success(&compiled);
    assert!(String::from_utf8_lossy(&compiled.stdout).contains("compile=written"));

    let names = root_names(out.path());
    assert_eq!(
        names,
        ["MEMORY.md", "inbox", "memphant-export.json", "units"]
    );
    let unit_path = out
        .path()
        .join("units")
        .join(format!("{}.md", current_id.as_uuid()));
    assert!(unit_path.is_file());
    assert!(
        !out.path()
            .join("units")
            .join(format!("{}.md", historical_id.as_uuid()))
            .exists()
    );

    let unit = fs::read_to_string(&unit_path).unwrap();
    assert!(unit.starts_with("# decision:queue\n\nUse LISTEN/NOTIFY.\n\n"));
    let footer = unit
        .lines()
        .last()
        .unwrap()
        .strip_prefix("<!-- memphant ")
        .unwrap()
        .strip_suffix(" -->")
        .unwrap();
    let footer: serde_json::Value = serde_json::from_str(footer).unwrap();
    assert_eq!(footer["unit_id"], current_id.as_uuid().to_string());
    assert_eq!(footer["kind"], "semantic");
    assert_eq!(footer["fact_key"], "decision:queue");
    assert_eq!(footer["predicate"], "states");
    assert_eq!(footer["subject_generation"], 0);
    assert_eq!(footer["body_sha256"].as_str().unwrap().len(), 64);

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(out.path().join("memphant-export.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["snapshot_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(manifest["memory_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(manifest["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        manifest["entries"][0]["path"],
        format!("units/{}.md", current_id.as_uuid())
    );

    let verified = verify(out.path());
    assert_success(&verified);
    assert!(String::from_utf8_lossy(&verified.stdout).contains("export=clean"));

    let first = tree_bytes(out.path());
    let repeated = compile(&url, &binding, out.path(), &[]);
    assert_success(&repeated);
    assert_eq!(tree_bytes(out.path()), first);

    let legacy = compile(&url, &binding, out.path(), &["--source", "anything.json"]);
    assert!(!legacy.status.success());
    assert!(
        !repo_root()
            .join("examples/evals/compiled-memory-source.json")
            .exists()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compile_refuses_dirty_tampered_duplicate_traversal_and_unmanaged_trees() {
    let (url, binding, state) = spawn_server().await;
    let id = seed_unit(
        &state,
        &binding,
        UnitState::Active,
        "profile:city",
        "City is Taipei.",
    )
    .await;

    for mutation in [
        "body",
        "missing",
        "memory",
        "unexpected",
        "duplicate",
        "duplicate_json_key",
        "traversal",
        "footer",
        "generation",
    ] {
        let out = tempfile::tempdir().unwrap();
        assert_success(&compile(&url, &binding, out.path(), &[]));
        let unit = out
            .path()
            .join("units")
            .join(format!("{}.md", id.as_uuid()));
        match mutation {
            "body" => fs::write(
                &unit,
                fs::read_to_string(&unit)
                    .unwrap()
                    .replace("Taipei", "Kyoto"),
            )
            .unwrap(),
            "missing" => fs::remove_file(&unit).unwrap(),
            "memory" => fs::write(out.path().join("MEMORY.md"), "tampered\n").unwrap(),
            "unexpected" => fs::write(out.path().join("notes.md"), "unmanaged\n").unwrap(),
            "duplicate" => {
                let path = out.path().join("memphant-export.json");
                let mut manifest: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                let entry = manifest["entries"][0].clone();
                manifest["entries"].as_array_mut().unwrap().push(entry);
                fs::write(path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
            }
            "duplicate_json_key" => {
                let path = out.path().join("memphant-export.json");
                let manifest = fs::read_to_string(&path).unwrap();
                fs::write(
                    path,
                    manifest.replacen(
                        "\"schema_version\": 1,",
                        "\"schema_version\": 1,\n  \"schema_version\": 1,",
                        1,
                    ),
                )
                .unwrap();
            }
            "traversal" => {
                let path = out.path().join("memphant-export.json");
                let mut manifest: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                manifest["entries"][0]["path"] = serde_json::json!("../escape.md");
                fs::write(path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
            }
            "footer" => {
                let content = fs::read_to_string(&unit).unwrap();
                fs::write(
                    &unit,
                    content.replace("\"confidence\":1.0", "\"confidence\":0.5"),
                )
                .unwrap();
            }
            "generation" => {
                let content = fs::read_to_string(&unit).unwrap();
                fs::write(
                    &unit,
                    content.replace("\"subject_generation\":0", "\"subject_generation\":1"),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }

        let rejected = compile(&url, &binding, out.path(), &[]);
        assert!(!rejected.status.success(), "{mutation} was overwritten");
        let stderr = String::from_utf8_lossy(&rejected.stderr);
        assert!(stderr.contains("compile=dirty"), "{mutation}: {stderr}");
        assert!(
            stderr.contains("run `memphant sync` or restore"),
            "{mutation}: {stderr}"
        );
        let dirty = verify(out.path());
        assert!(!dirty.status.success(), "{mutation} verified clean");
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compile_and_verify_reject_managed_symlinks() {
    use std::os::unix::fs::symlink;

    let (url, binding, state) = spawn_server().await;
    let id = seed_unit(
        &state,
        &binding,
        UnitState::Active,
        "profile:city",
        "City is Taipei.",
    )
    .await;
    let out = tempfile::tempdir().unwrap();
    assert_success(&compile(&url, &binding, out.path(), &[]));
    let unit = out
        .path()
        .join("units")
        .join(format!("{}.md", id.as_uuid()));
    let target = out.path().join("outside.md");
    fs::write(&target, fs::read(&unit).unwrap()).unwrap();
    fs::remove_file(&unit).unwrap();
    symlink(&target, &unit).unwrap();

    let rejected = compile(&url, &binding, out.path(), &[]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("symlink"));
    assert!(!verify(out.path()).status.success());
}

async fn spawn_server() -> (
    String,
    ContextBindingResponse,
    AppState<memphant_core::InMemoryStore>,
) {
    let tenant = TenantId::from_u128(uuid::Uuid::parse_str(TENANT).unwrap().as_u128());
    let state = AppState::new_in_memory().with_dev_tenant(tenant);
    let binding = state
        .store()
        .resolve_context_binding(
            tenant,
            "b2-cli-contract".to_string(),
            ContextBindingRequest {
                subject: ContextBindingEntityRef {
                    external_ref: "b2-user".into(),
                    kind: "user".into(),
                },
                actor: ContextBindingEntityRef {
                    external_ref: "b2-user".into(),
                    kind: "user".into(),
                },
                scope: ContextBindingScopeRef {
                    external_ref: "b2-root".into(),
                    kind: "user_root".into(),
                    parent_external_ref: None,
                },
                agent_node: ContextBindingAgentRef {
                    external_ref: "b2-l0".into(),
                    parent_external_ref: None,
                },
                access_policies: Vec::new(),
            },
        )
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = memphant_server::app(state.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), binding, state)
}

async fn seed_unit(
    state: &AppState<memphant_core::InMemoryStore>,
    binding: &ContextBindingResponse,
    unit_state: UnitState,
    fact_key: &str,
    body: &str,
) -> memphant_types::UnitId {
    let tenant = TenantId::from_u128(uuid::Uuid::parse_str(TENANT).unwrap().as_u128());
    let context = state
        .store()
        .resolve_memory_context(
            tenant,
            binding.subject_id,
            binding.actor_id,
            binding.scope_id,
            binding.agent_node_id,
        )
        .await
        .unwrap();
    let mut transaction = state.store().begin(&context).await.unwrap();
    let id = state
        .store()
        .stage_memory_unit(
            &mut transaction,
            NewMemoryUnit {
                tenant_id: tenant,
                data_subject_id: binding.subject_id,
                scope_id: binding.scope_id,
                agent_node_id: binding.agent_node_id,
                subject_generation: binding.subject_generation,
                kind: MemoryKind::Semantic,
                state: unit_state,
                fact_key: Some(fact_key.into()),
                predicate: Some("states".into()),
                body: body.into(),
                confidence: Some(1.0),
                trust_level: TrustLevel::TrustedSystem,
                churn_class: None,
                freshness_due_at: None,
                actor_id: Some(binding.actor_id),
                source_kind: Some("test".into()),
                source_ref: format!("test:{fact_key}"),
                observed_at: "2026-07-22T00:00:00Z".into(),
                source_episode_id: None,
                source_resource_id: None,
                deletion_generation: None,
                contextual_chunks: Vec::new(),
                valid_from: None,
                valid_to: None,
                transaction_from: None,
                transaction_to: None,
            },
        )
        .await
        .unwrap();
    state.store().commit(transaction).await.unwrap();
    id
}

fn compile(url: &str, binding: &ContextBindingResponse, out: &Path, tail: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_memphant-cli"));
    command
        .env("MEMPHANT_URL", url)
        .env_remove("MEMPHANT_API_KEY")
        .args(["compile", "--agent-node"])
        .arg(binding.agent_node_id.as_uuid().to_string())
        .args(["--out"])
        .arg(out)
        .args(["--scope"])
        .arg(binding.scope_id.as_uuid().to_string())
        .args([
            "--subject-generation",
            &binding.subject_generation.to_string(),
        ])
        .args(["--actor"])
        .arg(binding.actor_id.as_uuid().to_string())
        .args(["--subject-id"])
        .arg(binding.subject_id.as_uuid().to_string())
        .args(tail)
        .output()
        .unwrap()
}

fn verify(export: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_memphant-cli"))
        .current_dir(repo_root())
        .args(["verify", "--lock", "memphant.lock", "--export"])
        .arg(export)
        .output()
        .unwrap()
}

fn root_names(root: &Path) -> Vec<String> {
    let mut names = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn tree_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, current: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
