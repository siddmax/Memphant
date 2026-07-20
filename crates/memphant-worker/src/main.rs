//! The reflect worker: claims queued jobs (SKIP LOCKED in Postgres) and
//! compiles them through the same `MemoryService` path the public reflect
//! verb uses. `MEMPHANT_WORKER_ONCE=1` runs one tick; `MEMPHANT_WORKER_DRAIN=1`
//! runs ticks to empty. Both exit deterministically.

use std::time::Duration;

const BATCH: usize = 16;
const TICK: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerMode {
    Daemon,
    Once,
    Drain,
}

fn worker_mode(once: bool, drain: bool) -> Result<WorkerMode, &'static str> {
    match (once, drain) {
        (false, false) => Ok(WorkerMode::Daemon),
        (true, false) => Ok(WorkerMode::Once),
        (false, true) => Ok(WorkerMode::Drain),
        (true, true) => {
            Err("MEMPHANT_WORKER_ONCE and MEMPHANT_WORKER_DRAIN are mutually exclusive")
        }
    }
}

#[tokio::main]
async fn main() {
    let mode = worker_mode(
        std::env::var("MEMPHANT_WORKER_ONCE").as_deref() == Ok("1"),
        std::env::var("MEMPHANT_WORKER_DRAIN").as_deref() == Ok("1"),
    )
    .unwrap_or_else(|error| panic!("memphant-worker: {error}"));
    let store = memphant_runtime::build_worker_store()
        .await
        .expect("memphant-worker: store construction failed");
    eprintln!("memphant-worker: store={}", store.name());
    let service = memphant_runtime::build_worker_service(store);

    if mode == WorkerMode::Once {
        let completed = service
            .run_worker_tick(BATCH)
            .await
            .expect("memphant-worker: tick failed");
        println!("memphant-worker: once completed={completed}");
        return;
    }
    if mode == WorkerMode::Drain {
        let mut total = 0;
        loop {
            let completed = service
                .run_worker_tick(BATCH)
                .await
                .expect("memphant-worker: drain tick failed");
            if completed == 0 {
                break;
            }
            total += completed;
        }
        println!("memphant-worker: drain completed={total}");
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

#[cfg(test)]
mod tests {
    use super::{WorkerMode, worker_mode};

    #[test]
    fn worker_modes_are_distinct_and_conflicts_fail() {
        assert_eq!(worker_mode(false, false).unwrap(), WorkerMode::Daemon);
        assert_eq!(worker_mode(true, false).unwrap(), WorkerMode::Once);
        assert_eq!(worker_mode(false, true).unwrap(), WorkerMode::Drain);
        assert!(worker_mode(true, true).is_err());
    }
}
