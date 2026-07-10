//! The reflect worker: claims queued jobs (SKIP LOCKED in Postgres) and
//! compiles them through the same `MemoryService` path the public reflect
//! verb uses. `MEMPHANT_WORKER_ONCE=1` runs one tick and exits so tests and
//! the e2e probe are deterministic.

use std::time::Duration;

const BATCH: usize = 16;
const TICK: Duration = Duration::from_millis(500);

#[tokio::main]
async fn main() {
    let store = memphant_runtime::build_store()
        .await
        .expect("memphant-worker: store construction failed");
    eprintln!("memphant-worker: store={}", store.name());
    let service = memphant_runtime::build_service(store);

    if std::env::var("MEMPHANT_WORKER_ONCE").as_deref() == Ok("1") {
        let completed = service
            .run_worker_tick(BATCH)
            .await
            .expect("memphant-worker: tick failed");
        println!("memphant-worker: once completed={completed}");
        return;
    }

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install SIGTERM handler");
    loop {
        tokio::select! {
            _ = sigterm.recv() => {
                eprintln!("memphant-worker: SIGTERM — draining and shutting down");
                break;
            }
            _ = tokio::signal::ctrl_c() => {
                eprintln!("memphant-worker: interrupt — shutting down");
                break;
            }
            _ = tokio::time::sleep(TICK) => {
                match service.run_worker_tick(BATCH).await {
                    Ok(0) => {}
                    Ok(completed) => eprintln!("memphant-worker: completed={completed}"),
                    Err(error) => eprintln!("memphant-worker: tick error: {error}"),
                }
            }
        }
    }
}
