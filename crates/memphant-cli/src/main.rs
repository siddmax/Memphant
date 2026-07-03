use std::env;
use std::process::ExitCode;
use std::str::FromStr;

use memphant_store_postgres::{Provider, lint_migrations};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
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
            eprintln!("usage: memphant db lint --provider <plain-postgres|supabase|neon>");
            ExitCode::from(2)
        }
    }
}
