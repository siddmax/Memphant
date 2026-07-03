use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;

use memphant_store_postgres::{Provider, lint_migrations};
use memphant_types::{MemphantLock, VerifyReport};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct CompileSource {
    scope: String,
    entries: Vec<CompileEntry>,
}

#[derive(Debug, Deserialize)]
struct CompileEntry {
    id: String,
    title: String,
    body: String,
    #[serde(default)]
    citations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportMetadata {
    scope: String,
    lock: MemphantLock,
    source_path: String,
    source_hash: String,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [lock, out_flag, out] if lock == "lock" && out_flag == "--out" => emit_lock(out),
        [verify, lock_flag, path] if verify == "verify" && lock_flag == "--lock" => {
            verify_lock(path, None)
        }
        [verify, lock_flag, path, export_flag, export_dir]
            if verify == "verify" && lock_flag == "--lock" && export_flag == "--export" =>
        {
            verify_lock(path, Some(Path::new(export_dir)))
        }
        [
            compile,
            scope_flag,
            scope,
            out_flag,
            out,
            source_flag,
            source,
        ] if compile == "compile"
            && scope_flag == "--scope"
            && out_flag == "--out"
            && source_flag == "--source" =>
        {
            compile_markdown(scope, Path::new(out), Path::new(source))
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
                "usage: memphant lock --out <path|-> | memphant verify --lock <path> [--export <dir>] | memphant compile --scope <scope> --out <dir> --source <json> | memphant db lint --provider <plain-postgres|supabase|neon>"
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

fn verify_lock(path: &str, export_dir: Option<&Path>) -> ExitCode {
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
    if !report.ok {
        eprintln!("verify=dirty path={path}");
        for mismatch in report.mismatches {
            eprintln!(
                "{} expected={} actual={}",
                mismatch.key, mismatch.expected, mismatch.actual
            );
        }
        return ExitCode::from(1);
    }

    if let Some(export_dir) = export_dir
        && let Err(mismatches) = verify_export(export_dir)
    {
        eprintln!("verify=dirty path={path}");
        for mismatch in mismatches {
            eprintln!("{mismatch}");
        }
        return ExitCode::from(1);
    }

    println!("verify=clean path={path}");
    if let Some(export_dir) = export_dir {
        println!("export=clean path={}", export_dir.display());
    }
    ExitCode::SUCCESS
}

fn compile_markdown(scope: &str, out_dir: &Path, source_path: &Path) -> ExitCode {
    let source_bytes = match fs::read(source_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("compile=error source={}", source_path.display());
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    let source: CompileSource = match serde_json::from_slice(&source_bytes) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("compile=error source={}", source_path.display());
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    if source.scope != scope {
        eprintln!("compile=error scope={scope}");
        eprintln!(
            "source scope {} does not match requested scope {scope}",
            source.scope
        );
        return ExitCode::from(1);
    }
    if let Err(error) = fs::create_dir_all(out_dir) {
        eprintln!("compile=error out={}", out_dir.display());
        eprintln!("{error}");
        return ExitCode::from(1);
    }

    let mut index = format!("# MemPhant Export: {scope}\n\n");
    for entry in &source.entries {
        let file_name = format!("{}.md", safe_file_stem(&entry.id));
        index.push_str(&format!("- [{}]({file_name})\n", entry.title));
        let entry_markdown = render_entry(entry);
        let path = out_dir.join(file_name);
        if let Err(error) = fs::write(&path, entry_markdown) {
            eprintln!("compile=error path={}", path.display());
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    }
    if let Err(error) = fs::write(out_dir.join("index.md"), index) {
        eprintln!("compile=error path={}", out_dir.join("index.md").display());
        eprintln!("{error}");
        return ExitCode::from(1);
    }

    let metadata = ExportMetadata {
        scope: scope.to_string(),
        lock: MemphantLock::current(),
        source_path: absolute_or_original(source_path).display().to_string(),
        source_hash: stable_file_hash(&source_bytes),
    };
    let metadata_json = match serde_json::to_vec_pretty(&metadata) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("compile=error metadata=serialize");
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    if let Err(error) = fs::write(out_dir.join("memphant-export.json"), metadata_json) {
        eprintln!(
            "compile=error path={}",
            out_dir.join("memphant-export.json").display()
        );
        eprintln!("{error}");
        return ExitCode::from(1);
    }

    println!(
        "compile=written scope={scope} out={} entries={}",
        out_dir.display(),
        source.entries.len()
    );
    ExitCode::SUCCESS
}

fn verify_export(export_dir: &Path) -> Result<(), Vec<String>> {
    let metadata_path = export_dir.join("memphant-export.json");
    let content = fs::read_to_string(&metadata_path)
        .map_err(|error| vec![format!("export_metadata expected=readable actual={error}")])?;
    let metadata: ExportMetadata = serde_json::from_str(&content)
        .map_err(|error| vec![format!("export_metadata expected=json actual={error}")])?;
    let mut mismatches = Vec::new();
    for mismatch in metadata.lock.mismatches(&MemphantLock::current()) {
        mismatches.push(format!(
            "export_{} expected={} actual={}",
            mismatch.key, mismatch.expected, mismatch.actual
        ));
    }
    match fs::read(&metadata.source_path) {
        Ok(bytes) => {
            let actual_hash = stable_file_hash(&bytes);
            if metadata.source_hash != actual_hash {
                mismatches.push(format!(
                    "export_source_hash expected={} actual={actual_hash}",
                    metadata.source_hash
                ));
            }
        }
        Err(error) => mismatches.push(format!(
            "export_source_hash expected={} actual=unreadable:{error}",
            metadata.source_hash
        )),
    }
    if export_dir.join("index.md").is_file() {
        // The index is the root of the read-only view and is required even when a scope is empty.
    } else {
        mismatches.push("export_index expected=present actual=missing".to_string());
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(mismatches)
    }
}

fn render_entry(entry: &CompileEntry) -> String {
    let mut markdown = format!("# {}\n\n{}\n", entry.title, entry.body);
    if !entry.citations.is_empty() {
        markdown.push_str("\n## Citations\n\n");
        for citation in &entry.citations {
            markdown.push_str(&format!("- `{citation}`\n"));
        }
    }
    markdown
}

fn safe_file_stem(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn stable_file_hash(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(14_695_981_039_346_656_037, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211)
    });
    format!("fnv64:{hash:016x}")
}

fn absolute_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
