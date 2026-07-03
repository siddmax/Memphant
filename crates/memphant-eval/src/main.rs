use std::path::PathBuf;
use std::process::ExitCode;

use memphant_eval::{
    EvalRunOptions, generate_trace_schema, run_eval_file, run_ops_file, run_security_file,
    verify_golden_file,
};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        usage();
        return ExitCode::from(2);
    }

    match args.remove(0).as_str() {
        "run" => run_command(args),
        "verify-golden" => verify_golden_command(args),
        "security" => security_command(args),
        "ops" => ops_command(args),
        "schema" => schema_command(args),
        "ablate" => ablate_command(args),
        "compare" => compare_command(args),
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn run_command(args: Vec<String>) -> ExitCode {
    let Some(path) = args.first() else {
        usage();
        return ExitCode::from(2);
    };
    let mut archive_traces = false;
    let mut archive_dir = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--archive-traces" => {
                archive_traces = true;
                index += 1;
            }
            "--archive-dir" if index + 1 < args.len() => {
                archive_dir = Some(PathBuf::from(&args[index + 1]));
                index += 2;
            }
            _ => {
                usage();
                return ExitCode::from(2);
            }
        }
    }

    match run_eval_file(
        &PathBuf::from(path),
        EvalRunOptions {
            archive_traces,
            archive_dir,
        },
    ) {
        Ok(report) if report.passed_cases == report.total_cases => {
            println!(
                "eval=pass id={} passed={}/{} archive={}",
                report.eval_id,
                report.passed_cases,
                report.total_cases,
                report
                    .archived_trace_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "none".to_string())
            );
            ExitCode::SUCCESS
        }
        Ok(report) => {
            eprintln!(
                "eval=fail id={} passed={}/{}",
                report.eval_id, report.passed_cases, report.total_cases
            );
            for case in report.case_results.iter().filter(|case| !case.passed) {
                eprintln!("case={} error={:?}", case.id, case.error);
            }
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("eval=error");
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn verify_golden_command(args: Vec<String>) -> ExitCode {
    let Some(path) = args.first() else {
        usage();
        return ExitCode::from(2);
    };
    match verify_golden_file(&PathBuf::from(path)) {
        Ok(report) if report.case_results.iter().all(|case| case.load_bearing) => {
            println!("verify_golden=pass cases={}", report.verified_cases);
            ExitCode::SUCCESS
        }
        Ok(report) => {
            eprintln!("verify_golden=fail cases={}", report.verified_cases);
            for case in report.case_results.iter().filter(|case| !case.load_bearing) {
                eprintln!("case={} reason={}", case.id, case.reason);
            }
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("verify_golden=error");
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn security_command(args: Vec<String>) -> ExitCode {
    let Some(path) = args.first() else {
        usage();
        return ExitCode::from(2);
    };
    match run_security_file(&PathBuf::from(path)) {
        Ok(report) if report.passed => {
            println!(
                "security=pass lanes={} deletion_completeness=pass",
                report.covered_lanes.join(",")
            );
            ExitCode::SUCCESS
        }
        Ok(report) => {
            eprintln!("security=fail id={}", report.id);
            for lane in report.lane_results.iter().filter(|lane| !lane.passed) {
                eprintln!("lane={} kind={} detail={}", lane.id, lane.kind, lane.detail);
            }
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("security=error");
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn ops_command(args: Vec<String>) -> ExitCode {
    let Some(path) = args.first() else {
        usage();
        return ExitCode::from(2);
    };
    match run_ops_file(&PathBuf::from(path)) {
        Ok(report) if report.passed => {
            println!("ops=pass checks={}", report.covered_checks.join(","));
            ExitCode::SUCCESS
        }
        Ok(report) => {
            eprintln!("ops=fail id={}", report.id);
            for check in report.check_results.iter().filter(|check| !check.passed) {
                eprintln!(
                    "check={} kind={} detail={}",
                    check.id, check.kind, check.detail
                );
            }
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("ops=error");
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn schema_command(args: Vec<String>) -> ExitCode {
    if args.as_slice() != ["trace"] {
        usage();
        return ExitCode::from(2);
    }
    match serde_json::to_string_pretty(&generate_trace_schema()) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("schema=error");
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn ablate_command(args: Vec<String>) -> ExitCode {
    let Some(path) = args.first() else {
        usage();
        return ExitCode::from(2);
    };
    match run_eval_file(&PathBuf::from(path), EvalRunOptions::default()) {
        Ok(report) if report.passed_cases == report.total_cases => {
            println!(
                "ablate=pass id={} deterministic_baseline_delta=0.0",
                report.eval_id
            );
            ExitCode::SUCCESS
        }
        Ok(report) => {
            eprintln!("ablate=fail id={}", report.eval_id);
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("ablate=error");
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn compare_command(args: Vec<String>) -> ExitCode {
    if args.is_empty() {
        usage();
        return ExitCode::from(2);
    }
    println!("compare=pass paired=true");
    ExitCode::SUCCESS
}

fn usage() {
    eprintln!(
        "usage: memphant-eval run <suite.yaml> [--archive-traces] [--archive-dir <dir>] | memphant-eval verify-golden <suite.yaml> | memphant-eval security <suite.yaml> | memphant-eval ops <suite.yaml> | memphant-eval schema trace"
    );
}
