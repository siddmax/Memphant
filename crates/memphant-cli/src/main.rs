use std::env;
use std::fs;
use std::process::ExitCode;
use std::str::FromStr;

use memphant_store_postgres::{Provider, lint_migrations};
use memphant_types::{MemphantLock, VerifyReport};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [lock, out_flag, out] if lock == "lock" && out_flag == "--out" => emit_lock(out),
        [verify, lock_flag, path] if verify == "verify" && lock_flag == "--lock" => {
            verify_lock(path)
        }
        [db, lint, provider_flag, provider]
            if db == "db" && lint == "lint" && provider_flag == "--provider" =>
        {
            match Provider::from_str(provider).and_then(lint_migrations) {
                Ok(()) => {
                    println!("db_lint=clean provider={provider}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("db_lint=dirty provider={provider}");
                    eprintln!("{error}");
                    ExitCode::from(1)
                }
            }
        }
        _ => {
            eprintln!(
                "usage: memphant lock --out <path|-> | memphant verify --lock <path> | memphant db lint --provider <plain-postgres|supabase|neon>"
            );
            ExitCode::from(2)
        }
    }
}

fn emit_lock(out: &str) -> ExitCode {
    let json = match serde_json::to_string_pretty(&MemphantLock::current()) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("lock=error");
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };

    if out == "-" {
        println!("{json}");
        return ExitCode::SUCCESS;
    }

    match fs::write(out, format!("{json}\n")) {
        Ok(()) => {
            println!("lock=written path={out}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("lock=error path={out}");
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn verify_lock(path: &str) -> ExitCode {
    let lock = match fs::read_to_string(path)
        .map_err(|error| error.to_string())
        .and_then(|content| {
            serde_json::from_str::<MemphantLock>(&content).map_err(|error| error.to_string())
        }) {
        Ok(lock) => lock,
        Err(error) => {
            eprintln!("verify=error path={path}");
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };

    let report = VerifyReport::from_lock(lock);
    if report.ok {
        println!("verify=clean path={path}");
        ExitCode::SUCCESS
    } else {
        eprintln!("verify=dirty path={path}");
        for mismatch in report.mismatches {
            eprintln!(
                "{} expected={} actual={}",
                mismatch.key, mismatch.expected, mismatch.actual
            );
        }
        ExitCode::from(1)
    }
}
