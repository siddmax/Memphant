//! CLI memory verbs contract (Task 8): the `memphant` binary drives the real
//! axum app (in-process, in-memory store, dev-mode tenant binding) over HTTP:
//! retain → reflect → recall returns the body; forget → recall is empty.

use std::process::Command;

use memphant_server::AppState;
use memphant_types::TenantId;
use serde_json::Value;

const TENANT: &str = "00000000-0000-0000-0000-00000000c11a";
const SCOPE: &str = "00000000-0000-0000-0000-00000000c11b";
const ACTOR: &str = "00000000-0000-0000-0000-00000000c11c";

async fn spawn_server() -> String {
    let tenant = TenantId::from_u128(uuid::Uuid::parse_str(TENANT).unwrap().as_u128());
    let state = AppState::new_in_memory().with_dev_tenant(tenant);
    let app = memphant_server::app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server runs");
    });
    format!("http://{addr}")
}

fn cli(url: &str, args: &[&str]) -> (Value, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_memphant-cli"))
        .args(args)
        .env("MEMPHANT_URL", url)
        .env_remove("MEMPHANT_API_KEY")
        .output()
        .expect("cli runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!(
            "cli {args:?} must print JSON, got error {error}\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (value, output.status.success())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retain_reflect_recall_then_forget_round_trips_over_http() {
    let url = spawn_server().await;

    // retain (episode shape)
    let (retained, ok) = cli(
        &url,
        &[
            "retain",
            "--tenant",
            TENANT,
            "--scope",
            SCOPE,
            "--actor",
            ACTOR,
            "--body",
            "Release region is Taipei.",
        ],
    );
    assert!(ok, "retain exits zero");
    let episode_id = retained["episode_id"]
        .as_str()
        .expect("retain prints episode_id")
        .to_string();

    // reflect
    let (reflected, ok) = cli(
        &url,
        &[
            "reflect", "--tenant", TENANT, "--scope", SCOPE, "--actor", ACTOR,
        ],
    );
    assert!(ok, "reflect exits zero");
    assert!(reflected["episodes_consumed"].as_u64().unwrap_or(0) >= 1);

    // recall returns the body
    let (recalled, ok) = cli(
        &url,
        &[
            "recall",
            "--tenant",
            TENANT,
            "--scope",
            SCOPE,
            "--actor",
            ACTOR,
            "--query",
            "Where is the release region?",
        ],
    );
    assert!(ok, "recall exits zero");
    assert_eq!(
        recalled["items"][0]["body"].as_str(),
        Some("Release region is Taipei."),
        "recall returns the retained body: {recalled}"
    );

    // forget by episode id
    let (forgotten, ok) = cli(
        &url,
        &[
            "forget",
            "--tenant",
            TENANT,
            "--scope",
            SCOPE,
            "--actor",
            ACTOR,
            "--episode",
            &episode_id,
            "--reason",
            "cli-contract-test",
        ],
    );
    assert!(ok, "forget exits zero: {forgotten}");

    // recall is now empty
    let (recalled, ok) = cli(
        &url,
        &[
            "recall",
            "--tenant",
            TENANT,
            "--scope",
            SCOPE,
            "--actor",
            ACTOR,
            "--query",
            "Where is the release region?",
        ],
    );
    assert!(ok, "recall exits zero after forget");
    assert_eq!(
        recalled["items"].as_array().map(Vec::len),
        Some(0),
        "forgotten memory never recalls: {recalled}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resource_retain_and_trace_round_trip_over_http() {
    let url = spawn_server().await;

    let mut body_file = std::env::temp_dir();
    body_file.push(format!("memphant-cli-test-{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&body_file, "fn main() { println!(\"release: taipei\"); }")
        .expect("write body file");

    let (retained, ok) = cli(
        &url,
        &[
            "retain",
            "--tenant",
            TENANT,
            "--scope",
            SCOPE,
            "--actor",
            ACTOR,
            "--resource",
            "--uri",
            "repo://demo/src/main.rs",
            "--revision",
            "abc123",
            "--body-file",
            body_file.to_str().expect("utf-8 temp path"),
        ],
    );
    std::fs::remove_file(&body_file).ok();
    assert!(ok, "resource retain exits zero: {retained}");
    assert!(retained["resource_id"].is_string());
    assert_eq!(retained["enqueued"][0].as_str(), Some("reflect_resource"));

    cli(
        &url,
        &[
            "reflect", "--tenant", TENANT, "--scope", SCOPE, "--actor", ACTOR,
        ],
    );
    let (recalled, ok) = cli(
        &url,
        &[
            "recall",
            "--tenant",
            TENANT,
            "--scope",
            SCOPE,
            "--actor",
            ACTOR,
            "--query",
            "release taipei",
        ],
    );
    assert!(ok);
    let trace_id = recalled["trace_id"].as_str().expect("trace id").to_string();

    let (trace, ok) = cli(&url, &["trace", &trace_id]);
    assert!(ok, "trace exits zero");
    assert_eq!(trace["id"].as_str(), Some(trace_id.as_str()));
}
