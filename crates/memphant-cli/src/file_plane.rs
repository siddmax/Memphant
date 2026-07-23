use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use memphant_core::service::canonical_projection_fingerprint;
use memphant_types::{CanonicalProjectionResponse, CanonicalProjectionUnit, MemoryKind, UnitId};
use serde::de::{self, DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use cap_std::fs::Dir;
use rustix::fs::{
    AtFlags, CWD, FileType, Mode, OFlags, fstat, fsync, mkdirat, openat, renameat, statat, unlinkat,
};
use rustix::io::dup;

const DEFAULT_URL: &str = "http://127.0.0.1:8080";
const SCHEMA_VERSION: u32 = 1;
const COMPILER_VERSION: &str = "b2-file-plane-v1";
const MEMORY_FILE: &str = "MEMORY.md";
const MANIFEST_FILE: &str = "memphant-export.json";
const UNITS_DIR: &str = "units";
const INBOX_DIR: &str = "inbox";
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MANAGED_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 100_000;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

#[derive(Debug)]
struct TreeHandles {
    root: OwnedFd,
    units: OwnedFd,
    inbox: OwnedFd,
    root_identity: FileIdentity,
    units_identity: FileIdentity,
    inbox_identity: FileIdentity,
}

#[derive(Debug)]
struct ValidatedExport {
    manifest: ExportManifest,
    snapshot_sha256: String,
    handles: TreeHandles,
}

#[derive(Debug)]
struct CompileArgs {
    subject_id: Uuid,
    scope_id: Uuid,
    actor_id: Uuid,
    agent_node_id: Uuid,
    subject_generation: u64,
    out: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    if let Some(previous) = &previous {
        validate_manifest_binding(&previous.manifest, &args).map_err(CompileFailure::Dirty)?;
    }
    let projection = fetch_projection(&args).map_err(CompileFailure::Error)?;
    validate_response_binding(&projection, &args).map_err(CompileFailure::Error)?;
    if let Some(previous) = &previous {
        validate_manifest_response_context(&previous.manifest, &projection)
            .map_err(CompileFailure::Dirty)?;
    }
    let rendered = render_projection(&projection).map_err(CompileFailure::Error)?;
    if let Some(previous) = &previous {
        previous
            .revalidate(&args.out)
            .map_err(CompileFailure::Dirty)?;
    }
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

fn validate_manifest_binding(
    manifest: &ExportManifest,
    args: &CompileArgs,
) -> Result<(), Vec<String>> {
    let expected = [
        ("subject_id", &manifest.subject_id, args.subject_id),
        ("actor_id", &manifest.actor_id, args.actor_id),
        ("scope_id", &manifest.scope_id, args.scope_id),
        ("agent_node_id", &manifest.agent_node_id, args.agent_node_id),
    ];
    let mut findings = Vec::new();
    for (field, actual, expected) in expected {
        if actual != &expected.to_string() {
            findings.push(format!(
                "manifest context mismatch: {field} expected={expected} actual={actual}"
            ));
        }
    }
    if manifest.subject_generation != args.subject_generation {
        findings.push(format!(
            "manifest context mismatch: subject_generation expected={} actual={}",
            args.subject_generation, manifest.subject_generation
        ));
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err(findings)
    }
}

fn validate_manifest_response_context(
    manifest: &ExportManifest,
    response: &CanonicalProjectionResponse,
) -> Result<(), Vec<String>> {
    let expected = [
        (
            "tenant_id",
            &manifest.tenant_id,
            response.tenant_id.as_uuid(),
        ),
        (
            "subject_id",
            &manifest.subject_id,
            response.subject_id.as_uuid(),
        ),
        ("actor_id", &manifest.actor_id, response.actor_id.as_uuid()),
        ("scope_id", &manifest.scope_id, response.scope_id.as_uuid()),
        (
            "agent_node_id",
            &manifest.agent_node_id,
            response.agent_node_id.as_uuid(),
        ),
    ];
    let findings = expected
        .into_iter()
        .filter(|(_, actual, expected)| *actual != &expected.to_string())
        .map(|(field, actual, expected)| {
            format!(
                "server context differs from manifest: {field} expected={actual} actual={expected}"
            )
        })
        .collect::<Vec<_>>();
    if findings.is_empty() {
        Ok(())
    } else {
        Err(findings)
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

impl TreeHandles {
    fn open(root: &Path) -> Result<Self, String> {
        let root_fd =
            open_directory_at(CWD, root).map_err(|error| format!("output root: {error}"))?;
        Self::from_root(root_fd, root)
    }

    fn from_root(root_fd: OwnedFd, root: &Path) -> Result<Self, String> {
        let units = open_directory_at(&root_fd, UNITS_DIR)
            .map_err(|error| format!("{UNITS_DIR}: {error}"))?;
        let inbox = open_directory_at(&root_fd, INBOX_DIR)
            .map_err(|error| format!("{INBOX_DIR}: {error}"))?;
        let handles = Self {
            root_identity: identity(&root_fd)?,
            units_identity: identity(&units)?,
            inbox_identity: identity(&inbox)?,
            root: root_fd,
            units,
            inbox,
        };
        handles.ensure_bound(root)?;
        Ok(handles)
    }

    fn ensure_bound(&self, root_path: &Path) -> Result<(), String> {
        let root = identity_at(CWD, root_path)
            .map_err(|error| format!("output root path changed: {error}"))?;
        let units = identity_at(&self.root, UNITS_DIR)
            .map_err(|error| format!("{UNITS_DIR} path changed: {error}"))?;
        let inbox = identity_at(&self.root, INBOX_DIR)
            .map_err(|error| format!("{INBOX_DIR} path changed: {error}"))?;
        if root != self.root_identity {
            return Err("output root path no longer names the validated directory".to_string());
        }
        if units != self.units_identity {
            return Err("units path no longer names the validated directory".to_string());
        }
        if inbox != self.inbox_identity {
            return Err("inbox path no longer names the validated directory".to_string());
        }
        Ok(())
    }
}

fn create_output_tree(root: &Path) -> Result<TreeHandles, String> {
    if root
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("output path may not contain parent traversal".to_string());
    }
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(root)
    };
    let mut ancestor = absolute
        .parent()
        .ok_or_else(|| "output root may not be the filesystem root".to_string())?
        .to_path_buf();
    let mut components = vec![
        absolute
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "output path must end in a UTF-8 component".to_string())?
            .to_string(),
    ];
    while !ancestor.exists() {
        components.push(
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "output path must be UTF-8".to_string())?
                .to_string(),
        );
        ancestor = ancestor
            .parent()
            .ok_or_else(|| "output path has no existing ancestor".to_string())?
            .to_path_buf();
    }
    let ancestor = fs::canonicalize(&ancestor)
        .map_err(|error| format!("cannot resolve output parent: {error}"))?;
    let mut current = open_directory_at(CWD, &ancestor)?;
    for name in components.into_iter().rev() {
        current = match open_directory_at(&current, name.as_str()) {
            Ok(next) => next,
            Err(_) => {
                mkdirat(&current, name.as_str(), Mode::from_raw_mode(0o700))
                    .map_err(|error| format!("cannot create output component {name}: {error}"))?;
                open_directory_at(&current, name.as_str())?
            }
        };
    }
    for name in [UNITS_DIR, INBOX_DIR] {
        match mkdirat(&current, name, Mode::from_raw_mode(0o700)) {
            Ok(()) => {}
            Err(error) if error == rustix::io::Errno::EXIST => {}
            Err(error) => return Err(format!("cannot create {name}: {error}")),
        }
    }
    let handles = TreeHandles::from_root(current, root)?;
    let root_names = directory_names(&handles.root)?;
    if root_names != [INBOX_DIR.to_string(), UNITS_DIR.to_string()]
        || !directory_names(&handles.units)?.is_empty()
        || !directory_names(&handles.inbox)?.is_empty()
    {
        return Err("new output tree changed before initialization".to_string());
    }
    Ok(handles)
}

impl ValidatedExport {
    fn revalidate(&self, root: &Path) -> Result<(), Vec<String>> {
        let (manifest, snapshot_sha256) = validate_export_handles(root, &self.handles, false)?;
        if manifest != self.manifest || snapshot_sha256 != self.snapshot_sha256 {
            return Err(vec![
                "projection changed while the canonical snapshot was being fetched".to_string(),
            ]);
        }
        Ok(())
    }
}

fn open_directory_at<Fd: AsFd>(dir: Fd, path: impl rustix::path::Arg) -> Result<OwnedFd, String> {
    openat(
        dir,
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("refusing symlink or non-directory: {error}"))
}

fn identity<Fd: AsFd>(fd: Fd) -> Result<FileIdentity, String> {
    let stat = fstat(fd).map_err(|error| error.to_string())?;
    if !FileType::from_raw_mode(stat.st_mode).is_dir() {
        return Err("expected directory".to_string());
    }
    Ok(FileIdentity {
        device: stat.st_dev as u64,
        inode: stat.st_ino as u64,
    })
}

fn identity_at<Fd: AsFd>(dir: Fd, path: impl rustix::path::Arg) -> Result<FileIdentity, String> {
    let stat = statat(dir, path, AtFlags::SYMLINK_NOFOLLOW).map_err(|error| error.to_string())?;
    if !FileType::from_raw_mode(stat.st_mode).is_dir() {
        return Err("expected a non-symlink directory".to_string());
    }
    Ok(FileIdentity {
        device: stat.st_dev as u64,
        inode: stat.st_ino as u64,
    })
}

fn read_regular_at<Fd: AsFd>(dir: Fd, name: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    let fd = openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("refusing symlink or unreadable file: {error}"))?;
    let stat = fstat(&fd).map_err(|error| error.to_string())?;
    if !FileType::from_raw_mode(stat.st_mode).is_file() {
        return Err("expected a regular file".to_string());
    }
    if stat.st_size < 0 || stat.st_size as u64 > max_bytes {
        return Err(format!("file exceeds {max_bytes} bytes"));
    }
    let mut bytes = Vec::with_capacity(stat.st_size as usize);
    File::from(fd)
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!("file exceeds {max_bytes} bytes"));
    }
    Ok(bytes)
}

fn directory_names<Fd: AsFd>(fd: Fd) -> Result<Vec<String>, String> {
    let fd = dup(fd).map_err(|error| error.to_string())?;
    let directory = Dir::from_std_file(File::from(fd));
    let mut names = Vec::new();
    for entry in directory.entries().map_err(|error| error.to_string())? {
        let name = entry
            .map_err(|error| error.to_string())?
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF-8 path is forbidden".to_string())?;
        names.push(name);
        if names.len() > MAX_DIRECTORY_ENTRIES {
            return Err(format!("directory exceeds {MAX_DIRECTORY_ENTRIES} entries"));
        }
    }
    names.sort();
    Ok(names)
}

fn inspect_existing(root: &Path) -> Result<Option<ValidatedExport>, CompileFailure> {
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
    let root_fd = open_directory_at(CWD, root).map_err(|error| {
        CompileFailure::Dirty(vec![format!(
            "cannot safely open {}: {error}",
            root.display()
        )])
    })?;
    if directory_names(&root_fd)
        .map_err(|error| CompileFailure::Dirty(vec![format!("cannot read output root: {error}")]))?
        .is_empty()
    {
        return Ok(None);
    }
    validate_export_anchored(root, false)
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
    validate_export_anchored(root, allow_inbox_files).map(|validated| validated.manifest)
}

fn validate_export_anchored(
    root: &Path,
    allow_inbox_files: bool,
) -> Result<ValidatedExport, Vec<String>> {
    let handles = TreeHandles::open(root).map_err(|error| vec![error])?;
    let (manifest, snapshot_sha256) = validate_export_handles(root, &handles, allow_inbox_files)?;
    Ok(ValidatedExport {
        manifest,
        snapshot_sha256,
        handles,
    })
}

fn validate_export_handles(
    root: &Path,
    handles: &TreeHandles,
    allow_inbox_files: bool,
) -> Result<(ExportManifest, String), Vec<String>> {
    let mut findings = Vec::new();
    if let Err(error) = handles.ensure_bound(root) {
        return Err(vec![error]);
    }
    let manifest_bytes = read_regular_at(&handles.root, MANIFEST_FILE, MAX_MANIFEST_BYTES)
        .map_err(|error| vec![format!("{MANIFEST_FILE}: {error}")])?;
    let manifest = strict_from_slice::<ExportManifest>(&manifest_bytes)
        .map_err(|error| vec![format!("{MANIFEST_FILE}: invalid JSON: {error}")]);
    let manifest = match manifest {
        Ok(manifest) => manifest,
        Err(mut errors) => {
            findings.append(&mut errors);
            return Err(findings);
        }
    };
    validate_manifest_fields(&manifest, &mut findings);

    let memory_bytes = match read_regular_at(&handles.root, MEMORY_FILE, MAX_MANAGED_FILE_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => {
            findings.push(format!("{MEMORY_FILE}: {error}"));
            Vec::new()
        }
    };
    if sha256(&memory_bytes) != manifest.memory_sha256 {
        findings.push(format!("{MEMORY_FILE}: content hash differs from manifest"));
    }

    let mut expected_unit_names = BTreeSet::new();
    let mut canonical_items = Vec::new();
    let mut unit_bytes = BTreeMap::new();
    for entry in &manifest.entries {
        let expected_path = format!("{UNITS_DIR}/{}.md", entry.unit_id);
        if entry.path != expected_path || !is_safe_relative_unit_path(&entry.path) {
            findings.push(format!("{}: path must be {expected_path}", entry.path));
            continue;
        }
        let file_name = format!("{}.md", entry.unit_id);
        expected_unit_names.insert(file_name.clone());
        let bytes = match read_regular_at(&handles.units, &file_name, MAX_MANAGED_FILE_BYTES) {
            Ok(bytes) => bytes,
            Err(error) => {
                findings.push(format!("{}: {error}", entry.path));
                continue;
            }
        };
        unit_bytes.insert(file_name, bytes.clone());
        if sha256(&bytes) != entry.file_sha256 {
            findings.push(format!("{}: file hash differs from manifest", entry.path));
        }
        match parse_unit(&bytes) {
            Ok((body, footer)) => {
                validate_footer(entry, manifest.subject_generation, &footer, &mut findings);
                if sha256(body.as_bytes()) != entry.body_sha256 {
                    findings.push(format!("{}: body hash differs from manifest", entry.path));
                }
                if let Ok(unit_id) = Uuid::parse_str(&entry.unit_id) {
                    canonical_items.push(CanonicalProjectionUnit {
                        unit_id: UnitId::from_u128(unit_id.as_u128()),
                        kind: entry.kind,
                        fact_key: entry.fact_key.clone(),
                        predicate: entry.predicate.clone(),
                        body,
                        confidence: entry.confidence,
                        valid_from: entry.valid_from.clone(),
                        valid_to: entry.valid_to.clone(),
                        body_sha256: entry.body_sha256.clone(),
                    });
                }
            }
            Err(error) => findings.push(format!("{}: {error}", entry.path)),
        }
    }

    let units_names = directory_names(&handles.units).unwrap_or_else(|error| {
        findings.push(format!("{UNITS_DIR}: {error}"));
        Vec::new()
    });
    collect_name_mismatches(
        &units_names,
        &expected_unit_names,
        false,
        UNITS_DIR,
        &mut findings,
    );
    let inbox_names = directory_names(&handles.inbox).unwrap_or_else(|error| {
        findings.push(format!("{INBOX_DIR}: {error}"));
        Vec::new()
    });
    collect_name_mismatches(
        &inbox_names,
        &BTreeSet::new(),
        allow_inbox_files,
        INBOX_DIR,
        &mut findings,
    );
    if allow_inbox_files {
        for name in &inbox_names {
            if is_safe_inbox_name(name)
                && let Err(error) = read_regular_at(&handles.inbox, name, MAX_MANAGED_FILE_BYTES)
            {
                findings.push(format!("{INBOX_DIR}/{name}: {error}"));
            }
        }
    }
    let expected_root = BTreeSet::from([
        MEMORY_FILE.to_string(),
        MANIFEST_FILE.to_string(),
        UNITS_DIR.to_string(),
        INBOX_DIR.to_string(),
    ]);
    let root_names = directory_names(&handles.root).unwrap_or_else(|error| {
        findings.push(format!("output root: {error}"));
        Vec::new()
    });
    collect_name_mismatches(&root_names, &expected_root, false, ".", &mut findings);

    if let Ok(scope_id) = Uuid::parse_str(&manifest.scope_id) {
        let expected_memory = render_memory(scope_id, &manifest.snapshot_sha256, &manifest.entries);
        if memory_bytes != expected_memory {
            findings.push(format!(
                "{MEMORY_FILE}: content does not match the canonical manifest index"
            ));
        }
    }
    if canonical_items.len() == manifest.entries.len() {
        match canonical_projection_fingerprint(&canonical_items) {
            Ok(actual) if actual != manifest.snapshot_sha256 => findings.push(format!(
                "snapshot_sha256: canonical entries hash to {actual}, not {}",
                manifest.snapshot_sha256
            )),
            Err(error) => findings.push(format!("snapshot_sha256: cannot fingerprint: {error}")),
            _ => {}
        }
    }

    if let Err(error) = handles.ensure_bound(root) {
        findings.push(error);
    }

    if findings.is_empty() {
        let snapshot_sha256 = tree_snapshot_sha256(
            handles,
            &root_names,
            &units_names,
            &inbox_names,
            &manifest_bytes,
            &memory_bytes,
            &unit_bytes,
        );
        Ok((manifest, snapshot_sha256))
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
        if !matches!(entry.kind, MemoryKind::Semantic | MemoryKind::Procedural) {
            findings.push(format!("{}: unsupported memory kind", entry.path));
        }
        match entry.fact_key.as_deref() {
            Some(value) if require_single_line("fact_key", value).is_ok() => {}
            _ => findings.push(format!(
                "{}: fact_key must be one non-empty line",
                entry.path
            )),
        }
        match entry.predicate.as_deref() {
            Some(value) if require_single_line("predicate", value).is_ok() => {}
            _ => findings.push(format!(
                "{}: predicate must be one non-empty line",
                entry.path
            )),
        }
        match entry.confidence {
            Some(value) if value.is_finite() && (0.0..=1.0).contains(&value) => {}
            _ => findings.push(format!(
                "{}: confidence must be finite and within 0..=1",
                entry.path
            )),
        }
        let valid_from =
            validate_timestamp(entry.valid_from.as_deref(), "valid_from", entry, findings);
        let valid_to = validate_timestamp(entry.valid_to.as_deref(), "valid_to", entry, findings);
        if let (Some(valid_from), Some(valid_to)) = (valid_from, valid_to)
            && valid_from >= valid_to
        {
            findings.push(format!(
                "{}: valid_from must be earlier than valid_to",
                entry.path
            ));
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

fn validate_timestamp(
    value: Option<&str>,
    field: &str,
    entry: &ManifestEntry,
    findings: &mut Vec<String>,
) -> Option<jiff::Timestamp> {
    let value = value?;
    if !(value.ends_with('Z') || value.ends_with("+00:00")) {
        findings.push(format!("{}: {field} must use UTC", entry.path));
        return None;
    }
    match value.parse() {
        Ok(timestamp) => Some(timestamp),
        Err(_) => {
            findings.push(format!("{}: {field} must be RFC3339", entry.path));
            None
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
    previous: Option<&ValidatedExport>,
    rendered: &RenderedProjection,
) -> Result<(), String> {
    let owned_handles = if previous.is_none() {
        Some(create_output_tree(root)?)
    } else {
        None
    };
    let handles = previous
        .map(|validated| &validated.handles)
        .or(owned_handles.as_ref())
        .expect("existing or newly opened output handles");
    handles.ensure_bound(root)?;

    for (path, bytes) in &rendered.units {
        let name = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("invalid rendered unit path {path}"))?;
        write_atomic_at(&handles.units, name, bytes)?;
    }
    write_atomic_at(&handles.root, MEMORY_FILE, &rendered.memory)?;

    if let Some(previous) = previous {
        let current = rendered.units.keys().collect::<BTreeSet<_>>();
        for entry in &previous.manifest.entries {
            if !current.contains(&entry.path) {
                let name = Path::new(&entry.path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| format!("invalid stale unit path {}", entry.path))?;
                unlinkat(&handles.units, name, AtFlags::empty())
                    .map_err(|error| format!("cannot remove stale {}: {error}", entry.path))?;
            }
        }
    }
    fsync(&handles.units).map_err(|error| format!("cannot sync {UNITS_DIR}: {error}"))?;
    write_atomic_at(&handles.root, MANIFEST_FILE, &rendered.manifest_bytes)?;
    fsync(&handles.root).map_err(|error| format!("cannot sync output root: {error}"))?;
    handles.ensure_bound(root)?;
    Ok(())
}

fn write_atomic_at<Fd: AsFd>(directory: Fd, name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut attempts = 0;
    let (temporary, fd) = loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = format!(".{name}.tmp-{}-{sequence}", std::process::id());
        match openat(
            &directory,
            temporary.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        ) {
            Ok(fd) => break (temporary, fd),
            Err(error) if error == rustix::io::Errno::EXIST && attempts < 32 => {
                attempts += 1;
            }
            Err(error) => return Err(format!("cannot create temporary file: {error}")),
        }
    };
    let mut file = File::from(fd);
    let result = (|| {
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        renameat(&directory, temporary.as_str(), &directory, name)
            .map_err(|error| error.to_string())?;
        fsync(&directory).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = unlinkat(&directory, temporary.as_str(), AtFlags::empty());
    }
    result.map_err(|error| format!("cannot replace {name}: {error}"))
}

fn collect_name_mismatches(
    names: &[String],
    expected: &BTreeSet<String>,
    allow_regular_files: bool,
    label: &str,
    findings: &mut Vec<String>,
) {
    for name in names {
        if !(expected.contains(name) || allow_regular_files && is_safe_inbox_name(name)) {
            findings.push(format!("{label}/{name}: unexpected path"));
        }
    }
    for name in expected {
        if !names.contains(name) {
            findings.push(format!("{label}/{name}: missing path"));
        }
    }
}

fn tree_snapshot_sha256(
    handles: &TreeHandles,
    root_names: &[String],
    units_names: &[String],
    inbox_names: &[String],
    manifest: &[u8],
    memory: &[u8],
    units: &BTreeMap<String, Vec<u8>>,
) -> String {
    let mut hasher = Sha256::new();
    for identity in [
        handles.root_identity,
        handles.units_identity,
        handles.inbox_identity,
    ] {
        hasher.update(identity.device.to_be_bytes());
        hasher.update(identity.inode.to_be_bytes());
    }
    for (label, names) in [
        ("root", root_names),
        ("units", units_names),
        ("inbox", inbox_names),
    ] {
        hasher.update(label.as_bytes());
        for name in names {
            hasher.update((name.len() as u64).to_be_bytes());
            hasher.update(name.as_bytes());
        }
    }
    for (name, bytes) in [(MANIFEST_FILE, manifest), (MEMORY_FILE, memory)] {
        hasher.update(name.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    for (name, bytes) in units {
        hasher.update(name.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    format!("{:x}", hasher.finalize())
}

fn is_safe_relative_unit_path(value: &str) -> bool {
    let path = Path::new(value);
    let components = path.components().collect::<Vec<_>>();
    components.len() == 2
        && matches!(components[0], Component::Normal(name) if name == UNITS_DIR)
        && matches!(components[1], Component::Normal(_))
}

fn is_safe_inbox_name(name: &str) -> bool {
    let lowercase = name.to_ascii_lowercase();
    let stem = lowercase.split('.').next().unwrap_or_default();
    let windows_device = matches!(stem, "con" | "prn" | "aux" | "nul" | "clock$")
        || stem.strip_prefix("com").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || stem.strip_prefix("lpt").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        });
    !name.is_empty()
        && name.ends_with(".md")
        && !name.starts_with('.')
        && !matches!(lowercase.as_str(), "memory.md" | "memphant-export.json")
        && !windows_device
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

#[cfg(test)]
mod tests {
    use super::is_safe_inbox_name;

    #[test]
    fn inbox_names_reject_reserved_and_device_components() {
        assert!(is_safe_inbox_name("queue-decision.md"));
        for name in [
            "MEMORY.md",
            "memphant-export.json",
            "CON.md",
            "nul.md",
            "COM1.md",
            "lpt9.md",
            ".hidden.md",
            "nested/name.md",
        ] {
            assert!(!is_safe_inbox_name(name), "reserved inbox name {name}");
        }
    }
}
