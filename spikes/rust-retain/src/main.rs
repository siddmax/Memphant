use serde::Deserialize;
use std::env;
use std::fs;
use std::process::ExitCode;

#[derive(Debug, Deserialize)]
struct ExtractionPolicy {
    rules: Vec<ExtractionRule>,
}

#[derive(Debug, Deserialize)]
struct ExtractionRule {
    contains: String,
    subject: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct Episode {
    body: String,
}

#[derive(Debug, Deserialize)]
struct GoldenCase {
    id: String,
    episodes: Vec<Episode>,
    expected: Vec<String>,
}

fn extract(policy: &ExtractionPolicy, body: &str) -> Vec<String> {
    let lowered = body.to_lowercase();
    policy
        .rules
        .iter()
        .filter(|rule| lowered.contains(&rule.contains.to_lowercase()))
        .map(|rule| format!("{}:{}", rule.subject, rule.value))
        .collect()
}

fn run(policy_path: &str, golden_path: &str) -> Result<(), String> {
    let policy: ExtractionPolicy = serde_json::from_str(
        &fs::read_to_string(policy_path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let golden = fs::read_to_string(golden_path).map_err(|error| error.to_string())?;
    for line in golden.lines().filter(|line| !line.trim().is_empty()) {
        let case: GoldenCase = serde_json::from_str(line).map_err(|error| error.to_string())?;
        let extracted: Vec<String> = case
            .episodes
            .iter()
            .flat_map(|episode| extract(&policy, &episode.body))
            .collect();
        if extracted != case.expected {
            return Err(format!(
                "{}: expected {:?}, extracted {:?}",
                case.id, case.expected, extracted
            ));
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: memphant-rust-retain-spike <policy.json> <golden.jsonl>");
        return ExitCode::from(2);
    }
    match run(&args[1], &args[2]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
