use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use memphant_core::service::canonical_projection_fingerprint;
use memphant_types::{CanonicalProjectionResponse, CanonicalProjectionUnit, MemoryKind};
use serde::de::{self, DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const DEFAULT_URL: &str = "http://127.0.0.1:8080";
const SCHEMA_VERSION: u32 = 1;
const COMPILER_VERSION: &str = "b2-file-plane-v1";
const MEMORY_FILE: &str = "MEMORY.md";
const MANIFEST_FILE: &str = "memphant-export.json";
const UNITS_DIR: &str = "units";
const INBOX_DIR: &str = "inbox";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct CompileArgs {
    subject_id: Uuid,
    scope_id: Uuid,
    actor_id: Uuid,
    agent_node_id: Uuid,
    subject_generation: u64,
    out: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnitFooter {
    pub unit_id: String,
    pub body_sha256: String,
    pub subject_generation: u64,
    pub kind: MemoryKind,
    pub fact_key: Option<String>,
    pub predicate: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestEntry {
    pub unit_id: String,
    pub path: String,
    pub kind: MemoryKind,
    pub fact_key: Option<String>,
    pub predicate: Option<String>,
    pub confidence: Option<f32>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub body_sha256: String,
    pub file_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExportManifest {
    pub schema_version: u32,
    pub compiler_version: String,
    pub tenant_id: String,
    pub subject_id: String,
    pub actor_id: String,
    pub scope_id: String,
    pub agent_node_id: String,
    pub subject_generation: u64,
    pub snapshot_sha256: String,
    pub memory_sha256: String,
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug)]
struct RenderedProjection {
    memory: Vec<u8>,
    units: BTreeMap<String, Vec<u8>>,
    manifest_bytes: Vec<u8>,
}

#[derive(Debug)]
enum CompileFailure {
    Dirty(Vec<String>),
    Error(String),
}

pub(crate) fn run_compile(args: &[String]) -> ExitCode {
    match compile(args) {
        Ok((scope, out, snapshot, entries)) => {
            println!(
                "compile=written scope={scope} snapshot={snapshot} out={} entries={entries}",
                out.display()
            );
            ExitCode::SUCCESS
        }
        Err(CompileFailure::Dirty(findings)) => {
            eprintln!("compile=dirty");
            for finding in findings {
                eprintln!("{finding}");
            }
            eprintln!("run `memphant sync` or restore the projection before compiling");
            ExitCode::from(1)
        }
        Err(CompileFailure::Error(error)) => {
            eprintln!("compile=error");
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn compile(args: &[String]) -> Result<(Uuid, PathBuf, String, usize), CompileFailure> {
    let args = parse_compile_args(args).map_err(CompileFailure::Error)?;
    let previous = inspect_existing(&args.out)?;
    let projection = fetch_projection(&args).map_err(CompileFailure::Error)?;
    validate_response_binding(&projection, &args).map_err(CompileFailure::Error)?;
    let rendered = render_projection(&projection).map_err(CompileFailure::Error)?;
    replace_projection(&args.out, previous.as_ref(), &rendered).map_err(CompileFailure::Error)?;
    Ok((
        args.scope_id,
        args.out,
        projection.fingerprint,
        projection.items.len(),
    ))
}

fn parse_compile_args(args: &[String]) -> Result<CompileArgs, String> {
    let mut flags = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let name = args[index]
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected positional argument {}", args[index]))?;
        let value = args
            .get(index + 1)
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| format!("missing value for --{name}"))?;
        if !matches!(
            name,
            "subject-id" | "scope" | "actor" | "agent-node" | "subject-generation" | "out"
        ) {
            return Err(format!("unknown flag --{name}"));
        }
        if flags.insert(name.to_string(), value.clone()).is_some() {
            return Err(format!("duplicate flag --{name}"));
        }
        index += 2;
    }
    let required = |name: &str| {
        flags
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("missing required flag --{name}"))
    };
    let parse_uuid = |name: &str| {
        Uuid::parse_str(required(name)?).map_err(|_| format!("--{name} must be a canonical UUID"))
    };
    Ok(CompileArgs {
        subject_id: parse_uuid("subject-id")?,
        scope_id: parse_uuid("scope")?,
        actor_id: parse_uuid("actor")?,
        agent_node_id: parse_uuid("agent-node")?,
        subject_generation: required("subject-generation")?
            .parse()
            .map_err(|error| format!("--subject-generation: {error}"))?,
        out: PathBuf::from(required("out")?),
    })
}

fn fetch_projection(args: &CompileArgs) -> Result<CanonicalProjectionResponse, String> {
    let base = env::var("MEMPHANT_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let url = format!(
        "{}/v1/scopes/{}/projection?subject_id={}&actor_id={}&agent_node_id={}&subject_generation={}",
        base.trim_end_matches('/'),
        args.scope_id,
        args.subject_id,
        args.actor_id,
        args.agent_node_id,
        args.subject_generation
    );
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent.get(&url);
    if let Ok(key) = env::var("MEMPHANT_API_KEY")
        && !key.is_empty()
    {
        request = request.header("authorization", format!("Bearer {key}"));
    }
    let mut response = request
        .call()
        .map_err(|error| format!("projection request failed: {error}"))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body: Value = response
            .body_mut()
            .read_json()
            .unwrap_or_else(|_| serde_json::json!({}));
        let code = body
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("remote_error");
        return Err(format!(
            "projection request failed: status={status} code={code}"
        ));
    }
    response
        .body_mut()
        .read_json()
        .map_err(|error| format!("projection response was not valid JSON: {error}"))
}

fn validate_response_binding(
    response: &CanonicalProjectionResponse,
    args: &CompileArgs,
) -> Result<(), String> {
    let bindings = [
        ("subject_id", response.subject_id.as_uuid(), args.subject_id),
        ("actor_id", response.actor_id.as_uuid(), args.actor_id),
        ("scope_id", response.scope_id.as_uuid(), args.scope_id),
        (
            "agent_node_id",
            response.agent_node_id.as_uuid(),
            args.agent_node_id,
        ),
    ];
    for (field, actual, expected) in bindings {
        require_binding(field, actual, expected)?;
    }
    if response.subject_generation != args.subject_generation {
        return Err(format!(
            "projection context mismatch: subject_generation expected={} actual={}",
            args.subject_generation, response.subject_generation
        ));
    }
    require_sha256("projection fingerprint", &response.fingerprint)?;
    let actual = canonical_projection_fingerprint(&response.items)
        .map_err(|error| format!("cannot fingerprint projection response: {error}"))?;
    if response.fingerprint != actual {
        return Err(format!(
            "projection fingerprint mismatch: expected={} actual={actual}",
            response.fingerprint
        ));
    }
    Ok(())
}

fn require_binding(field: &str, actual: Uuid, expected: Uuid) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "projection context mismatch: {field} expected={expected} actual={actual}"
        ))
    }
}

fn render_projection(response: &CanonicalProjectionResponse) -> Result<RenderedProjection, String> {
    let mut units = BTreeMap::new();
    let mut entries = Vec::new();
    let mut ids = BTreeSet::new();
    let mut fact_keys = BTreeSet::new();
    let mut last_id = None;
    for item in &response.items {
        let id = item.unit_id.as_uuid().to_string();
        if last_id.as_ref().is_some_and(|previous| previous >= &id) {
            return Err("projection items are not strictly ordered by unit_id".to_string());
        }
        last_id = Some(id.clone());
        if !ids.insert(id.clone()) {
            return Err(format!("projection contains duplicate unit_id {id}"));
        }
        if let Some(fact_key) = &item.fact_key
            && !fact_keys.insert(fact_key.clone())
        {
            return Err(format!("projection contains duplicate fact_key {fact_key}"));
        }
        if !matches!(item.kind, MemoryKind::Semantic | MemoryKind::Procedural) {
            return Err(format!(
                "projection contains unsupported kind for unit {id}"
            ));
        }
        require_single_line("fact_key", item.fact_key.as_deref().unwrap_or(&id))?;
        require_sha256("body_sha256", &item.body_sha256)?;
        let actual_body_sha = sha256(item.body.as_bytes());
        if item.body_sha256 != actual_body_sha {
            return Err(format!("projection body hash mismatch for unit {id}"));
        }
        let path = format!("{UNITS_DIR}/{id}.md");
        let bytes = render_unit(response.subject_generation, item)?;
        let file_sha256 = sha256(&bytes);
        units.insert(path.clone(), bytes);
        entries.push(ManifestEntry {
            unit_id: id,
            path,
            kind: item.kind,
            fact_key: item.fact_key.clone(),
            predicate: item.predicate.clone(),
            confidence: item.confidence,
            valid_from: item.valid_from.clone(),
            valid_to: item.valid_to.clone(),
            body_sha256: item.body_sha256.clone(),
            file_sha256,
        });
    }

    let memory = render_memory(response.scope_id.as_uuid(), &response.fingerprint, &entries);
    let manifest = ExportManifest {
        schema_version: SCHEMA_VERSION,
        compiler_version: COMPILER_VERSION.to_string(),
        tenant_id: response.tenant_id.as_uuid().to_string(),
        subject_id: response.subject_id.as_uuid().to_string(),
        actor_id: response.actor_id.as_uuid().to_string(),
        scope_id: response.scope_id.as_uuid().to_string(),
        agent_node_id: response.agent_node_id.as_uuid().to_string(),
        subject_generation: response.subject_generation,
        snapshot_sha256: response.fingerprint.clone(),
        memory_sha256: sha256(&memory),
        entries,
    };
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("manifest serialization failed: {error}"))?;
    manifest_bytes.push(b'\n');
    Ok(RenderedProjection {
        memory,
        units,
        manifest_bytes,
    })
}

fn render_unit(generation: u64, item: &CanonicalProjectionUnit) -> Result<Vec<u8>, String> {
    let title = item
        .fact_key
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| item.unit_id.as_uuid().to_string());
    require_single_line("unit title", &title)?;
    let footer = UnitFooter {
        unit_id: item.unit_id.as_uuid().to_string(),
        body_sha256: item.body_sha256.clone(),
        subject_generation: generation,
        kind: item.kind,
        fact_key: item.fact_key.clone(),
        predicate: item.predicate.clone(),
        confidence: item.confidence,
    };
    let footer = serde_json::to_string(&footer)
        .map_err(|error| format!("footer serialization failed: {error}"))?;
    Ok(format!("# {title}\n\n{}\n\n<!-- memphant {footer} -->\n", item.body).into_bytes())
}

fn render_memory(scope_id: Uuid, fingerprint: &str, entries: &[ManifestEntry]) -> Vec<u8> {
    let mut memory =
        format!("# MemPhant Memory\n\nScope: `{scope_id}`\nSnapshot: `{fingerprint}`\n\n");
    for entry in entries {
        let title = entry.fact_key.as_deref().unwrap_or(&entry.unit_id);
        memory.push_str(&format!(
            "- [{}]({})\n",
            escape_link_text(title),
            entry.path
        ));
    }
    memory.into_bytes()
}

fn escape_link_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn inspect_existing(root: &Path) -> Result<Option<ExportManifest>, CompileFailure> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(CompileFailure::Dirty(vec![format!(
                "symlink output root: {}",
                root.display()
            )]));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(CompileFailure::Dirty(vec![format!(
                "output root is not a directory: {}",
                root.display()
            )]));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CompileFailure::Error(format!(
                "cannot inspect output root {}: {error}",
                root.display()
            )));
        }
        Ok(_) => {}
    }
    let empty = fs::read_dir(root)
        .map_err(|error| CompileFailure::Error(format!("cannot read {}: {error}", root.display())))?
        .next()
        .is_none();
    if empty {
        return Ok(None);
    }
    validate_export(root, false)
        .map(Some)
        .map_err(CompileFailure::Dirty)
}

pub(crate) fn verify_export(root: &Path) -> Result<ExportManifest, Vec<String>> {
    validate_export(root, false)
}

pub(crate) fn validate_export(
    root: &Path,
    allow_inbox_files: bool,
) -> Result<ExportManifest, Vec<String>> {
    let mut findings = Vec::new();
    check_regular_dir(root, "output root", &mut findings);
    let manifest_path = root.join(MANIFEST_FILE);
    check_regular_file(&manifest_path, MANIFEST_FILE, &mut findings);
    let manifest = fs::read(&manifest_path)
        .map_err(|error| vec![format!("{MANIFEST_FILE}: unreadable: {error}")])
        .and_then(|bytes| {
            strict_from_slice::<ExportManifest>(&bytes)
                .map_err(|error| vec![format!("{MANIFEST_FILE}: invalid JSON: {error}")])
        });
    let manifest = match manifest {
        Ok(manifest) => manifest,
        Err(mut errors) => {
            findings.append(&mut errors);
            return Err(findings);
        }
    };
    validate_manifest_fields(&manifest, &mut findings);

    let memory_path = root.join(MEMORY_FILE);
    check_regular_file(&memory_path, MEMORY_FILE, &mut findings);
    match fs::read(&memory_path) {
        Ok(bytes) if sha256(&bytes) != manifest.memory_sha256 => {
            findings.push(format!("{MEMORY_FILE}: content hash differs from manifest"));
        }
        Err(error) => findings.push(format!("{MEMORY_FILE}: unreadable: {error}")),
        _ => {}
    }

    let units_dir = root.join(UNITS_DIR);
    let inbox_dir = root.join(INBOX_DIR);
    check_regular_dir(&units_dir, UNITS_DIR, &mut findings);
    check_regular_dir(&inbox_dir, INBOX_DIR, &mut findings);

    let mut expected_unit_names = BTreeSet::new();
    for entry in &manifest.entries {
        let expected_path = format!("{UNITS_DIR}/{}.md", entry.unit_id);
        if entry.path != expected_path || !is_safe_relative_unit_path(&entry.path) {
            findings.push(format!("{}: path must be {expected_path}", entry.path));
            continue;
        }
        let file_name = format!("{}.md", entry.unit_id);
        expected_unit_names.insert(file_name);
        let path = root.join(&entry.path);
        check_regular_file(&path, &entry.path, &mut findings);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                findings.push(format!("{}: unreadable: {error}", entry.path));
                continue;
            }
        };
        if sha256(&bytes) != entry.file_sha256 {
            findings.push(format!("{}: file hash differs from manifest", entry.path));
        }
        match parse_unit(&bytes) {
            Ok((body, footer)) => {
                validate_footer(entry, manifest.subject_generation, &footer, &mut findings);
                if sha256(body.as_bytes()) != entry.body_sha256 {
                    findings.push(format!("{}: body hash differs from manifest", entry.path));
                }
            }
            Err(error) => findings.push(format!("{}: {error}", entry.path)),
        }
    }

    collect_directory_mismatches(
        &units_dir,
        &expected_unit_names,
        false,
        UNITS_DIR,
        &mut findings,
    );
    collect_directory_mismatches(
        &inbox_dir,
        &BTreeSet::new(),
        allow_inbox_files,
        INBOX_DIR,
        &mut findings,
    );
    let expected_root = BTreeSet::from([
        MEMORY_FILE.to_string(),
        MANIFEST_FILE.to_string(),
        UNITS_DIR.to_string(),
        INBOX_DIR.to_string(),
    ]);
    collect_directory_mismatches(root, &expected_root, false, ".", &mut findings);

    if findings.is_empty() {
        Ok(manifest)
    } else {
        findings.sort();
        findings.dedup();
        Err(findings)
    }
}

fn validate_manifest_fields(manifest: &ExportManifest, findings: &mut Vec<String>) {
    if manifest.schema_version != SCHEMA_VERSION {
        findings.push(format!(
            "schema_version: expected {SCHEMA_VERSION}, got {}",
            manifest.schema_version
        ));
    }
    if manifest.compiler_version != COMPILER_VERSION {
        findings.push(format!(
            "compiler_version: expected {COMPILER_VERSION}, got {}",
            manifest.compiler_version
        ));
    }
    for (name, value) in [
        ("tenant_id", &manifest.tenant_id),
        ("subject_id", &manifest.subject_id),
        ("actor_id", &manifest.actor_id),
        ("scope_id", &manifest.scope_id),
        ("agent_node_id", &manifest.agent_node_id),
    ] {
        if Uuid::parse_str(value).map(|id| id.to_string()).as_ref() != Ok(value) {
            findings.push(format!("{name}: must be a lowercase canonical UUID"));
        }
    }
    if require_sha256("snapshot_sha256", &manifest.snapshot_sha256).is_err() {
        findings.push("snapshot_sha256: must be a lowercase SHA-256 digest".to_string());
    }
    if require_sha256("memory_sha256", &manifest.memory_sha256).is_err() {
        findings.push("memory_sha256: must be a lowercase SHA-256 digest".to_string());
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut fact_keys = BTreeSet::new();
    let mut previous = None;
    for entry in &manifest.entries {
        if Uuid::parse_str(&entry.unit_id)
            .map(|id| id.to_string())
            .as_ref()
            != Ok(&entry.unit_id)
        {
            findings.push(format!(
                "{}: unit_id must be a lowercase canonical UUID",
                entry.path
            ));
        }
        if !ids.insert(entry.unit_id.clone()) {
            findings.push(format!("duplicate unit_id: {}", entry.unit_id));
        }
        if !paths.insert(entry.path.clone()) {
            findings.push(format!("duplicate path: {}", entry.path));
        }
        if let Some(fact_key) = &entry.fact_key
            && !fact_keys.insert(fact_key.clone())
        {
            findings.push(format!("duplicate fact_key: {fact_key}"));
        }
        if previous.as_ref().is_some_and(|id| id >= &entry.unit_id) {
            findings.push("entries must be strictly ordered by unit_id".to_string());
        }
        previous = Some(entry.unit_id.clone());
        for (field, hash) in [
            ("body_sha256", &entry.body_sha256),
            ("file_sha256", &entry.file_sha256),
        ] {
            if require_sha256(field, hash).is_err() {
                findings.push(format!("{}: invalid {field}", entry.path));
            }
        }
    }
}

fn validate_footer(
    entry: &ManifestEntry,
    subject_generation: u64,
    footer: &UnitFooter,
    findings: &mut Vec<String>,
) {
    let expected = UnitFooter {
        unit_id: entry.unit_id.clone(),
        body_sha256: entry.body_sha256.clone(),
        subject_generation,
        kind: entry.kind,
        fact_key: entry.fact_key.clone(),
        predicate: entry.predicate.clone(),
        confidence: entry.confidence,
    };
    if footer.unit_id != expected.unit_id
        || footer.body_sha256 != expected.body_sha256
        || footer.subject_generation != expected.subject_generation
        || footer.kind != expected.kind
        || footer.fact_key != expected.fact_key
        || footer.predicate != expected.predicate
        || footer.confidence != expected.confidence
    {
        findings.push(format!(
            "{}: footer metadata differs from manifest",
            entry.path
        ));
    }
}

pub(crate) fn parse_unit(bytes: &[u8]) -> Result<(String, UnitFooter), String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "file is not UTF-8".to_string())?;
    if !text.ends_with(" -->\n") {
        return Err("missing exact final memphant footer".to_string());
    }
    let footer_start = text
        .rfind("\n\n<!-- memphant ")
        .ok_or_else(|| "missing exact final memphant footer".to_string())?;
    let footer_json = &text[footer_start + "\n\n<!-- memphant ".len()..text.len() - " -->\n".len()];
    let footer: UnitFooter = strict_from_slice(footer_json.as_bytes())
        .map_err(|error| format!("invalid footer JSON: {error}"))?;
    let prefix_end = text
        .find("\n\n")
        .ok_or_else(|| "missing H1/body separator".to_string())?;
    let title = &text[..prefix_end];
    if !title.starts_with("# ") || title[2..].contains('\n') {
        return Err("unit must start with one H1 display key".to_string());
    }
    let expected_title = footer.fact_key.as_deref().unwrap_or(&footer.unit_id);
    if &title[2..] != expected_title {
        return Err("H1 display key differs from footer".to_string());
    }
    Ok((text[prefix_end + 2..footer_start].to_string(), footer))
}

fn replace_projection(
    root: &Path,
    previous: Option<&ExportManifest>,
    rendered: &RenderedProjection,
) -> Result<(), String> {
    if !root.exists() {
        fs::create_dir_all(root)
            .map_err(|error| format!("cannot create {}: {error}", root.display()))?;
    }
    create_directory(root)?;
    create_directory(&root.join(UNITS_DIR))?;
    create_directory(&root.join(INBOX_DIR))?;

    for (path, bytes) in &rendered.units {
        write_atomic(&root.join(path), bytes)?;
    }
    write_atomic(&root.join(MEMORY_FILE), &rendered.memory)?;

    if let Some(previous) = previous {
        let current = rendered.units.keys().collect::<BTreeSet<_>>();
        for entry in &previous.entries {
            if !current.contains(&entry.path) {
                fs::remove_file(root.join(&entry.path))
                    .map_err(|error| format!("cannot remove stale {}: {error}", entry.path))?;
            }
        }
    }
    write_atomic(&root.join(MANIFEST_FILE), &rendered.manifest_bytes)?;
    Ok(())
}

fn create_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("refusing symlink directory {}", path.display()))
        }
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(format!("{} is not a directory", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(path)
            .map_err(|error| format!("cannot create {}: {error}", path.display())),
        Err(error) => Err(format!("cannot inspect {}: {error}", path.display())),
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("{} has no UTF-8 file name", path.display()))?;
    let mut attempts = 0;
    let (temporary, mut file) = loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".{name}.tmp-{}-{sequence}", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempts < 32 => {
                attempts += 1;
            }
            Err(error) => return Err(format!("cannot create temporary file: {error}")),
        }
    };
    let result = (|| {
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("cannot replace {}: {error}", path.display()))
}

fn check_regular_dir(path: &Path, label: &str, findings: &mut Vec<String>) {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            findings.push(format!("{label}: symlink is forbidden"));
        }
        Ok(metadata) if !metadata.is_dir() => findings.push(format!("{label}: expected directory")),
        Err(error) => findings.push(format!("{label}: unreadable: {error}")),
        _ => {}
    }
}

fn check_regular_file(path: &Path, label: &str, findings: &mut Vec<String>) {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            findings.push(format!("{label}: symlink is forbidden"));
        }
        Ok(metadata) if !metadata.is_file() => findings.push(format!("{label}: expected file")),
        Err(error) => findings.push(format!("{label}: unreadable: {error}")),
        _ => {}
    }
}

fn collect_directory_mismatches(
    directory: &Path,
    expected: &BTreeSet<String>,
    allow_regular_files: bool,
    label: &str,
    findings: &mut Vec<String>,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => {
                findings.push(format!("{label}: non-UTF-8 path is forbidden"));
                continue;
            }
        };
        let path = entry.path();
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                findings.push(format!("{label}/{name}: symlink is forbidden"));
            }
            Ok(metadata)
                if expected.contains(&name)
                    || (allow_regular_files && metadata.is_file() && is_safe_inbox_name(&name)) => {
            }
            Ok(_) => findings.push(format!("{label}/{name}: unexpected path")),
            Err(error) => findings.push(format!("{label}/{name}: unreadable: {error}")),
        }
    }
    for name in expected {
        if !directory.join(name).exists() {
            findings.push(format!("{label}/{name}: missing path"));
        }
    }
}

fn is_safe_relative_unit_path(value: &str) -> bool {
    let path = Path::new(value);
    let components = path.components().collect::<Vec<_>>();
    components.len() == 2
        && matches!(components[0], Component::Normal(name) if name == UNITS_DIR)
        && matches!(components[1], Component::Normal(_))
}

fn is_safe_inbox_name(name: &str) -> bool {
    !name.is_empty()
        && name.ends_with(".md")
        && !name.starts_with('.')
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn require_single_line(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.contains(['\n', '\r']) {
        Err(format!("{field} must be one non-empty line"))
    } else {
        Ok(())
    }
}

fn require_sha256(field: &str, value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(format!("{field} must be a lowercase SHA-256 digest"))
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn strict_from_slice<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let strict = StrictValue::deserialize(&mut deserializer).map_err(|error| error.to_string())?;
    deserializer.end().map_err(|error| error.to_string())?;
    serde_json::from_value(strict.0).map_err(|error| error.to_string())
}

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictValueVisitor)
    }
}

struct StrictValueVisitor;

impl<'de> Visitor<'de> for StrictValueVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Bool(value)))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Number(Number::from(value))))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Number(Number::from(value))))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .map(StrictValue)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value.to_string())))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value)))
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<StrictValue>()? {
            values.push(value.0);
        }
        Ok(StrictValue(Value::Array(values)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<Self::Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom(format!("duplicate JSON key {key}")));
            }
            let value = object.next_value::<StrictValue>()?;
            values.insert(key, value.0);
        }
        Ok(StrictValue(Value::Object(values)))
    }
}
