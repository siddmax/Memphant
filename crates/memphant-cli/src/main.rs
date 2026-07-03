use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;

use memphant_store_postgres::{Provider, lint_migrations};
use memphant_types::{MemphantLock, VerifyReport};
use serde::{Deserialize, Serialize};

const DEFAULT_PROVIDER_PROFILE_DIR: &str = "deploy/provider-profiles";
const PITR_RETENTION_MARGIN_DAYS: u64 = 1;

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
        [db, command, provider_flag, provider]
            if db == "db" && command == "bootstrap-check" && provider_flag == "--provider" =>
        {
            bootstrap_check(provider, None)
        }
        [db, command, provider_flag, provider, profile_flag, profile]
            if db == "db"
                && command == "bootstrap-check"
                && provider_flag == "--provider"
                && profile_flag == "--profile" =>
        {
            bootstrap_check(provider, Some(Path::new(profile)))
        }
        _ => {
            eprintln!(
                "usage: memphant lock --out <path|-> | memphant verify --lock <path> [--export <dir>] | memphant compile --scope <scope> --out <dir> --source <json> | memphant db lint --provider <plain-postgres|supabase|neon> | memphant db bootstrap-check --provider <plain-postgres|supabase|neon> [--profile <env-file>]"
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

fn bootstrap_check(provider: &str, profile_path: Option<&Path>) -> ExitCode {
    let provider = match Provider::from_str(provider) {
        Ok(provider) => provider,
        Err(error) => {
            eprintln!("bootstrap_check=dirty provider={provider}");
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    let profile_path = profile_path.map(Path::to_path_buf).unwrap_or_else(|| {
        PathBuf::from(DEFAULT_PROVIDER_PROFILE_DIR).join(format!("{provider}.env.example"))
    });

    let mut findings = Vec::new();
    if let Err(error) = lint_migrations(provider) {
        findings.extend(
            error
                .findings()
                .iter()
                .map(|finding| format!("migration:{finding}")),
        );
    }

    match read_provider_profile(&profile_path) {
        Ok(profile) => findings.extend(validate_provider_profile(provider, &profile)),
        Err(error) => findings.push(format!("profile:unreadable:{error}")),
    }

    if findings.is_empty() {
        println!(
            "bootstrap_check=clean provider={provider} profile={}",
            profile_path.display()
        );
        println!("migration_lint=clean provider={provider}");
        return ExitCode::SUCCESS;
    }

    eprintln!(
        "bootstrap_check=dirty provider={provider} profile={}",
        profile_path.display()
    );
    for finding in findings {
        eprintln!("{finding}");
    }
    ExitCode::from(1)
}

fn read_provider_profile(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    parse_env_profile(&content)
}

fn parse_env_profile(content: &str) -> Result<BTreeMap<String, String>, String> {
    let mut profile = BTreeMap::new();
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            return Err(format!("line:{}:missing_equals", index + 1));
        };
        let key = key.trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        {
            return Err(format!("line:{}:invalid_key", index + 1));
        }
        profile.insert(key.to_string(), unquote_env_value(value.trim()).to_string());
    }
    Ok(profile)
}

fn unquote_env_value(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn validate_provider_profile(
    provider: Provider,
    profile: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut findings = Vec::new();
    let expected_provider = provider.to_string();
    if let Some(value) = require_key(profile, "MEMPHANT_PROVIDER", &mut findings)
        && value != expected_provider
    {
        findings.push(format!(
            "profile:provider_mismatch:expected={expected_provider}:actual={value}"
        ));
    }
    if let Some(schema) = require_key(profile, "MEMPHANT_SCHEMA", &mut findings)
        && schema != "memphant"
    {
        findings.push(format!("profile:schema_mismatch:actual={schema}"));
    }
    if let Some(database_url) = require_key(profile, "DATABASE_URL", &mut findings) {
        validate_database_url(provider, database_url, &mut findings);
    }
    validate_residency_and_retention(profile, &mut findings);

    match provider {
        Provider::PlainPostgres => {}
        Provider::Supabase => validate_supabase_profile(profile, &mut findings),
        Provider::Neon => validate_neon_profile(profile, &mut findings),
    }

    findings
}

fn validate_database_url(provider: Provider, database_url: &str, findings: &mut Vec<String>) {
    if !(database_url.starts_with("postgres://") || database_url.starts_with("postgresql://")) {
        findings.push("database_url:must_use_postgres_scheme".to_string());
    }
    if provider == Provider::Neon && !database_url.contains("sslmode=require") {
        findings.push("neon:database_url_missing_sslmode_require".to_string());
    }
    if database_url.contains("public.") || database_url.contains("syndai.") {
        findings.push("database_url:forbidden_schema_reference".to_string());
    }
}

fn validate_residency_and_retention(
    profile: &BTreeMap<String, String>,
    findings: &mut Vec<String>,
) {
    let pg_region = require_key(profile, "MEMPHANT_PG_REGION", findings).map(str::to_string);
    let object_region =
        require_key(profile, "MEMPHANT_OBJECT_STORE_REGION", findings).map(str::to_string);
    if let (Some(pg_region), Some(object_region)) = (pg_region, object_region)
        && pg_region != object_region
    {
        findings.push(format!(
            "residency:region_mismatch:pg={pg_region}:object_store={object_region}"
        ));
    }

    require_key(profile, "MEMPHANT_OBJECT_STORE", findings);
    require_key(profile, "MEMPHANT_OBJECT_STORE_BUCKET", findings);

    expect_true(profile, "MEMPHANT_OBJECT_VERSIONING_REQUIRED", findings);
    let pitr_days = parse_u64_key(profile, "MEMPHANT_PITR_WINDOW_DAYS", findings);
    let retention_days = parse_u64_key(profile, "MEMPHANT_OBJECT_RETENTION_DAYS", findings);
    if let (Some(pitr_days), Some(retention_days)) = (pitr_days, retention_days)
        && retention_days < pitr_days + PITR_RETENTION_MARGIN_DAYS
    {
        findings.push(format!(
            "restore_retention_floor_violation:pitr_days={pitr_days}:object_retention_days={retention_days}:required_min={}",
            pitr_days + PITR_RETENTION_MARGIN_DAYS
        ));
    }
}

fn validate_supabase_profile(profile: &BTreeMap<String, String>, findings: &mut Vec<String>) {
    if let Some(exposed) = require_key(profile, "MEMPHANT_SUPABASE_EXPOSED_SCHEMAS", findings) {
        let exposed_schemas = exposed
            .split(',')
            .map(|value| value.trim().to_ascii_lowercase())
            .collect::<Vec<_>>();
        if exposed_schemas.iter().any(|schema| schema == "memphant") {
            findings.push("supabase:memphant_schema_exposed_to_postgrest".to_string());
        }
    }
    expect_false(
        profile,
        "MEMPHANT_SUPABASE_ANON_HAS_MEMPHANT_ACCESS",
        findings,
    );
    expect_false(
        profile,
        "MEMPHANT_SUPABASE_AUTHENTICATED_HAS_MEMPHANT_ACCESS",
        findings,
    );
    expect_true(profile, "MEMPHANT_SUPABASE_ADVISORS_REQUIRED", findings);
    if let Some(command) = require_key(profile, "MEMPHANT_SUPABASE_LINT_COMMAND", findings) {
        let command = command.to_ascii_lowercase();
        for needle in ["supabase db lint", "--schema memphant", "--fail-on warning"] {
            if !command.contains(needle) {
                findings.push(format!("supabase:lint_command_missing:{needle}"));
            }
        }
    }
}

fn validate_neon_profile(profile: &BTreeMap<String, String>, findings: &mut Vec<String>) {
    require_key(profile, "MEMPHANT_NEON_BRANCH", findings);
    expect_true(profile, "MEMPHANT_NEON_BRANCHING_FOR_EVALS", findings);
}

fn require_key<'a>(
    profile: &'a BTreeMap<String, String>,
    key: &str,
    findings: &mut Vec<String>,
) -> Option<&'a str> {
    match profile.get(key) {
        Some(value) if !value.trim().is_empty() => Some(value.as_str()),
        _ => {
            findings.push(format!("profile:missing:{key}"));
            None
        }
    }
}

fn parse_u64_key(
    profile: &BTreeMap<String, String>,
    key: &str,
    findings: &mut Vec<String>,
) -> Option<u64> {
    let value = require_key(profile, key, findings)?;
    match value.parse::<u64>() {
        Ok(value) => Some(value),
        Err(_) => {
            findings.push(format!("profile:invalid_u64:{key}:{value}"));
            None
        }
    }
}

fn expect_true(profile: &BTreeMap<String, String>, key: &str, findings: &mut Vec<String>) {
    if let Some(value) = require_key(profile, key, findings)
        && value != "true"
    {
        findings.push(format!("profile:expected_true:{key}:actual={value}"));
    }
}

fn expect_false(profile: &BTreeMap<String, String>, key: &str, findings: &mut Vec<String>) {
    if let Some(value) = require_key(profile, key, findings)
        && value != "false"
    {
        findings.push(format!("profile:expected_false:{key}:actual={value}"));
    }
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
