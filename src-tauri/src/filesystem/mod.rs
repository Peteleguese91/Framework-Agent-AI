mod platform;

use crate::workspace::WorkspaceState;
use globset::Glob;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::State;

const MAX_READ: u64 = 2 * 1024 * 1024;
const MAX_RANGE: u64 = 256 * 1024;
const MAX_COPY: u64 = 128 * 1024 * 1024;
const MAX_WALK: usize = 10_000;
const MAX_RESULTS: usize = 200;
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".cache",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
];

type FsResult<T> = Result<T, FsError>;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolActor {
    User,
    Agent,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    Read,
    Write,
    Destructive,
    Dangerous,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FsErrorCode {
    InvalidInput,
    OutsideWorkspace,
    NotFound,
    PermissionDenied,
    AlreadyExists,
    NotFile,
    NotDirectory,
    BinaryFile,
    FileTooLarge,
    Conflict,
    Unsupported,
    IoError,
    Internal,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsError {
    pub code: FsErrorCode,
    pub message: String,
}

impl FsError {
    fn new(code: FsErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl From<io::Error> for FsError {
    fn from(error: io::Error) -> Self {
        let code = match error.kind() {
            io::ErrorKind::NotFound => FsErrorCode::NotFound,
            io::ErrorKind::PermissionDenied => FsErrorCode::PermissionDenied,
            io::ErrorKind::AlreadyExists => FsErrorCode::AlreadyExists,
            _ => FsErrorCode::IoError,
        };
        Self::new(code, error.to_string())
    }
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsToolRequest {
    pub actor: ToolActor,
    pub action: FsAction,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum FsAction {
    List {
        path: String,
        limit: usize,
    },
    Read {
        path: String,
    },
    ReadRange {
        path: String,
        start_byte: u64,
        length: u64,
    },
    Write {
        path: String,
        content: String,
        expected_content: String,
    },
    Patch {
        path: String,
        hunks: Vec<PatchHunk>,
    },
    Create {
        path: String,
        content: String,
    },
    Delete {
        path: String,
        recursive: bool,
    },
    Move {
        from: String,
        to: String,
    },
    Copy {
        from: String,
        to: String,
        recursive: bool,
    },
    Mkdir {
        path: String,
    },
    Exists {
        path: String,
    },
    Stat {
        path: String,
    },
    Search {
        query: String,
        mode: SearchMode,
        limit: usize,
    },
    Glob {
        pattern: String,
        limit: usize,
    },
}

impl FsAction {
    fn name(&self) -> &'static str {
        match self {
            Self::List { .. } => "list",
            Self::Read { .. } => "read",
            Self::ReadRange { .. } => "read_range",
            Self::Write { .. } => "write",
            Self::Patch { .. } => "patch",
            Self::Create { .. } => "create",
            Self::Delete { .. } => "delete",
            Self::Move { .. } => "move",
            Self::Copy { .. } => "copy",
            Self::Mkdir { .. } => "mkdir",
            Self::Exists { .. } => "exists",
            Self::Stat { .. } => "stat",
            Self::Search { .. } => "search",
            Self::Glob { .. } => "glob",
        }
    }
    fn risk(&self) -> RiskLevel {
        match self {
            Self::List { .. }
            | Self::Read { .. }
            | Self::ReadRange { .. }
            | Self::Exists { .. }
            | Self::Stat { .. }
            | Self::Search { .. }
            | Self::Glob { .. } => RiskLevel::Read,
            Self::Delete {
                recursive: true, ..
            } => RiskLevel::Dangerous,
            Self::Delete { .. } => RiskLevel::Destructive,
            _ => RiskLevel::Write,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchHunk {
    pub start_line: usize,
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Filename,
    Path,
    Content,
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum FsToolResult {
    List {
        entries: Vec<FsEntry>,
        truncated: bool,
    },
    Read {
        content: String,
        metadata: FsMetadata,
    },
    ReadRange {
        content: String,
        start_byte: u64,
        end_byte: u64,
        total_bytes: u64,
        has_more: bool,
    },
    Exists {
        exists: bool,
    },
    Stat {
        metadata: FsMetadata,
    },
    Search {
        matches: Vec<FsMatch>,
        truncated: bool,
    },
    Glob {
        entries: Vec<FsEntry>,
        truncated: bool,
    },
    Mutation {
        path: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsMetadata {
    pub kind: &'static str,
    pub size: u64,
    pub modified_at: Option<u64>,
    pub created_at: Option<u64>,
    pub read_only: bool,
    pub is_symlink: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsEntry {
    pub path: String,
    pub metadata: FsMetadata,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsMatch {
    pub path: String,
    pub line: Option<usize>,
    pub text: Option<String>,
}

fn relative(path: &str, allow_root: bool) -> FsResult<&Path> {
    if path.contains('\0') || (!allow_root && path.is_empty()) {
        return Err(FsError::new(
            FsErrorCode::InvalidInput,
            "A non-empty relative path is required",
        ));
    }
    let path = Path::new(path);
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(FsError::new(
            FsErrorCode::OutsideWorkspace,
            "Path must stay inside the workspace",
        ));
    }
    Ok(path)
}
fn existing(root: &Path, path: &str, allow_root: bool) -> FsResult<PathBuf> {
    let rel = relative(path, allow_root)?;
    let resolved = root.join(rel).canonicalize()?;
    if !resolved.starts_with(root) {
        return Err(FsError::new(
            FsErrorCode::OutsideWorkspace,
            "Path escapes the workspace",
        ));
    }
    Ok(resolved)
}
fn destination(root: &Path, path: &str) -> FsResult<PathBuf> {
    let rel = relative(path, false)?;
    let candidate = root.join(rel);
    let parent = candidate
        .parent()
        .ok_or_else(|| FsError::new(FsErrorCode::InvalidInput, "Invalid destination"))?
        .canonicalize()?;
    if !parent.starts_with(root) {
        return Err(FsError::new(
            FsErrorCode::OutsideWorkspace,
            "Path escapes the workspace",
        ));
    }
    let name = candidate
        .file_name()
        .ok_or_else(|| FsError::new(FsErrorCode::InvalidInput, "Invalid destination"))?;
    Ok(parent.join(name))
}
fn relative_display(root: &Path, path: &Path) -> FsResult<String> {
    Ok(path
        .strip_prefix(root)
        .map_err(|_| FsError::new(FsErrorCode::Internal, "Path display failed"))?
        .to_string_lossy()
        .replace('\\', "/"))
}
fn reject_final_symlink(path: &Path) -> FsResult<()> {
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(FsError::new(
            FsErrorCode::Unsupported,
            "Direct mutation of a symlink is unavailable",
        ));
    }
    Ok(())
}
fn mutable_existing(root: &Path, path: &str) -> FsResult<PathBuf> {
    let rel = relative(path, false)?;
    reject_final_symlink(&root.join(rel))?;
    let resolved = existing(root, path, false)?;
    if resolved == root {
        return Err(FsError::new(
            FsErrorCode::OutsideWorkspace,
            "Workspace root cannot be mutated",
        ));
    }
    Ok(resolved)
}
fn metadata(path: &Path) -> FsResult<FsMetadata> {
    let meta = fs::symlink_metadata(path)?;
    let millis = |time: Result<SystemTime, io::Error>| {
        time.ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_millis() as u64)
    };
    Ok(FsMetadata {
        kind: if meta.file_type().is_symlink() {
            "symlink"
        } else if meta.is_dir() {
            "directory"
        } else {
            "file"
        },
        size: meta.len(),
        modified_at: millis(meta.modified()),
        created_at: millis(meta.created()),
        read_only: meta.permissions().readonly(),
        is_symlink: meta.file_type().is_symlink(),
    })
}
fn read_text(path: &Path) -> FsResult<String> {
    let meta = fs::metadata(path)?;
    if !meta.is_file() {
        return Err(FsError::new(FsErrorCode::NotFile, "Path is not a file"));
    }
    if meta.len() > MAX_READ {
        return Err(FsError::new(
            FsErrorCode::FileTooLarge,
            "File exceeds the 2 MiB full-read limit",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_READ + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_READ {
        return Err(FsError::new(
            FsErrorCode::FileTooLarge,
            "File exceeds the 2 MiB full-read limit",
        ));
    }
    decode_text(bytes)
}
fn decode_text(bytes: Vec<u8>) -> FsResult<String> {
    if content_inspector::inspect(&bytes).is_binary() {
        return Err(FsError::new(
            FsErrorCode::BinaryFile,
            "Binary files cannot be read as text",
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| FsError::new(FsErrorCode::BinaryFile, "File is not valid UTF-8"))
}
fn read_range(root: &Path, path: &str, start_byte: u64, length: u64) -> FsResult<FsToolResult> {
    if length == 0 || length > MAX_RANGE {
        return Err(FsError::new(
            FsErrorCode::InvalidInput,
            "Range length must be 1 to 262144 bytes",
        ));
    }
    let file = existing(root, path, false)?;
    if !fs::metadata(&file)?.is_file() {
        return Err(FsError::new(FsErrorCode::NotFile, "Path is not a file"));
    }
    let mut handle = fs::File::open(&file)?;
    let total_bytes = handle.metadata()?.len();
    if start_byte > total_bytes {
        return Err(FsError::new(
            FsErrorCode::InvalidInput,
            "Range starts after end of file",
        ));
    }
    let mut sample = Vec::new();
    handle.by_ref().take(8192).read_to_end(&mut sample)?;
    if content_inspector::inspect(&sample).is_binary() {
        return Err(FsError::new(
            FsErrorCode::BinaryFile,
            "Binary files cannot be read as text",
        ));
    }
    handle.seek(SeekFrom::Start(start_byte))?;
    let count = length.min(total_bytes - start_byte);
    let mut bytes = Vec::new();
    handle.take(count).read_to_end(&mut bytes)?;
    let end_byte = start_byte + bytes.len() as u64;
    if content_inspector::inspect(&bytes).is_binary() {
        return Err(FsError::new(
            FsErrorCode::BinaryFile,
            "Binary files cannot be read as text",
        ));
    }
    let content = String::from_utf8(bytes).map_err(|_| {
        FsError::new(
            FsErrorCode::InvalidInput,
            "Range splits a UTF-8 character; adjust startByte or length",
        )
    })?;
    Ok(FsToolResult::ReadRange {
        content,
        start_byte,
        end_byte,
        total_bytes,
        has_more: end_byte < total_bytes,
    })
}
fn atomic_text(root: &Path, path: &str, content: &str, expected: Option<&str>) -> FsResult<String> {
    if content.len() as u64 > MAX_READ {
        return Err(FsError::new(
            FsErrorCode::FileTooLarge,
            "Content exceeds the 2 MiB write limit",
        ));
    }
    let target = destination(root, path)?;
    reject_final_symlink(&target)?;
    let previous_permissions = if let Some(expected) = expected {
        let current = mutable_existing(root, path)?;
        if !current.is_file() {
            return Err(FsError::new(FsErrorCode::NotFile, "Path is not a file"));
        }
        if read_text(&current)? != expected {
            return Err(FsError::new(
                FsErrorCode::Conflict,
                "File changed; expectedContent does not match",
            ));
        }
        Some(fs::metadata(current)?.permissions())
    } else {
        if fs::symlink_metadata(&target).is_ok() {
            return Err(FsError::new(
                FsErrorCode::AlreadyExists,
                "Target already exists",
            ));
        }
        None
    };
    let parent = target
        .parent()
        .ok_or_else(|| FsError::new(FsErrorCode::InvalidInput, "Invalid target"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(content.as_bytes())?;
    if let Some(permissions) = previous_permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary.as_file().sync_all()?;
    if expected.is_some() {
        temporary
            .persist(&target)
            .map_err(|error| FsError::from(error.error))?;
    } else {
        temporary
            .persist_noclobber(&target)
            .map_err(|error| FsError::from(error.error))?;
    }
    relative_display(root, &target)
}
fn patch_text(original: &str, hunks: &[PatchHunk]) -> FsResult<String> {
    if hunks.is_empty() || hunks.len() > 100 {
        return Err(FsError::new(
            FsErrorCode::InvalidInput,
            "Patch requires 1 to 100 hunks",
        ));
    }
    let ending = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let normalized = original.replace("\r\n", "\n");
    if normalized.contains('\r') {
        return Err(FsError::new(
            FsErrorCode::Unsupported,
            "Mixed or CR-only line endings are unsupported by patch",
        ));
    }
    let trailing = normalized.ends_with('\n');
    let mut lines: Vec<String> = normalized
        .split_terminator('\n')
        .map(str::to_owned)
        .collect();
    let mut order: Vec<&PatchHunk> = hunks.iter().collect();
    order.sort_by_key(|hunk| hunk.start_line);
    let mut next_available = 1usize;
    for hunk in &order {
        if hunk.start_line == 0
            || hunk.start_line > lines.len() + 1
            || hunk.start_line < next_available
            || hunk.old_lines.is_empty() && hunk.new_lines.is_empty()
        {
            return Err(FsError::new(
                FsErrorCode::InvalidInput,
                "Patch hunk position or overlap is invalid",
            ));
        }
        if hunk
            .old_lines
            .iter()
            .chain(hunk.new_lines.iter())
            .any(|line| line.contains('\n') || line.contains('\r'))
        {
            return Err(FsError::new(
                FsErrorCode::InvalidInput,
                "Patch lines must not include line endings",
            ));
        }
        let start = hunk.start_line - 1;
        let end = start
            .checked_add(hunk.old_lines.len())
            .ok_or_else(|| FsError::new(FsErrorCode::InvalidInput, "Patch range overflow"))?;
        if end > lines.len() || lines[start..end] != hunk.old_lines[..] {
            return Err(FsError::new(
                FsErrorCode::Conflict,
                "Patch expected lines do not match",
            ));
        }
        next_available = end.max(hunk.start_line) + 1;
    }
    for hunk in order.into_iter().rev() {
        let start = hunk.start_line - 1;
        lines.splice(
            start..start + hunk.old_lines.len(),
            hunk.new_lines.iter().cloned(),
        );
    }
    let mut result = lines.join(ending);
    if trailing && !lines.is_empty() {
        result.push_str(ending);
    }
    Ok(result)
}
fn walk(root: &Path) -> impl Iterator<Item = Result<ignore::DirEntry, ignore::Error>> {
    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry.file_type().is_some_and(|kind| {
                    kind.is_dir() && SKIP_DIRS.iter().any(|skip| entry.file_name() == *skip)
                })
        })
        .build()
}
fn checked_limit(limit: usize) -> FsResult<usize> {
    if (1..=MAX_RESULTS).contains(&limit) {
        Ok(limit)
    } else {
        Err(FsError::new(
            FsErrorCode::InvalidInput,
            "Limit must be 1 to 200",
        ))
    }
}
fn search(root: &Path, query: &str, mode: SearchMode, limit: usize) -> FsResult<FsToolResult> {
    let limit = checked_limit(limit)?;
    if query.is_empty() || query.len() > 256 {
        return Err(FsError::new(
            FsErrorCode::InvalidInput,
            "Query must contain 1 to 256 characters",
        ));
    }
    let mut matches = Vec::new();
    let mut scanned = 0;
    let mut truncated = false;
    for item in walk(root) {
        let entry = item.map_err(|error| FsError::new(FsErrorCode::IoError, error.to_string()))?;
        if entry.depth() == 0 || entry.file_type().is_some_and(|kind| kind.is_symlink()) {
            continue;
        }
        scanned += 1;
        if scanned > MAX_WALK {
            truncated = true;
            break;
        }
        let path = relative_display(root, entry.path())?;
        match mode {
            SearchMode::Filename | SearchMode::Path => {
                let candidate = if matches!(mode, SearchMode::Filename) {
                    entry.file_name().to_string_lossy().into_owned()
                } else {
                    path.clone()
                };
                if candidate.contains(query) {
                    matches.push(FsMatch {
                        path,
                        line: None,
                        text: None,
                    });
                }
            }
            SearchMode::Content => {
                if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                    continue;
                }
                let canonical = existing(root, &path, false)?;
                let text = match read_text(&canonical) {
                    Ok(text) => text,
                    Err(error)
                        if matches!(
                            error.code,
                            FsErrorCode::BinaryFile
                                | FsErrorCode::FileTooLarge
                                | FsErrorCode::PermissionDenied
                        ) =>
                    {
                        continue
                    }
                    Err(error) => return Err(error),
                };
                for (index, line) in text.lines().enumerate() {
                    if line.contains(query) {
                        matches.push(FsMatch {
                            path: path.clone(),
                            line: Some(index + 1),
                            text: Some(line.chars().take(300).collect()),
                        });
                        if matches.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }
        if matches.len() >= limit {
            truncated = true;
            break;
        }
    }
    Ok(FsToolResult::Search { matches, truncated })
}
fn glob(root: &Path, pattern: &str, limit: usize) -> FsResult<FsToolResult> {
    let limit = checked_limit(limit)?;
    if pattern.is_empty() || pattern.len() > 256 {
        return Err(FsError::new(
            FsErrorCode::InvalidInput,
            "Glob pattern must contain 1 to 256 characters",
        ));
    }
    relative(pattern, false)?;
    let glob = Glob::new(pattern)
        .map_err(|error| FsError::new(FsErrorCode::InvalidInput, error.to_string()))?
        .compile_matcher();
    let mut entries = Vec::new();
    let mut scanned = 0;
    let mut truncated = false;
    for item in walk(root) {
        let entry = item.map_err(|error| FsError::new(FsErrorCode::IoError, error.to_string()))?;
        if entry.depth() == 0 || entry.file_type().is_some_and(|kind| kind.is_symlink()) {
            continue;
        }
        scanned += 1;
        if scanned > MAX_WALK {
            truncated = true;
            break;
        }
        let path = relative_display(root, entry.path())?;
        if glob.is_match(&path) {
            entries.push(FsEntry {
                path,
                metadata: metadata(entry.path())?,
            });
            if entries.len() >= limit {
                truncated = true;
                break;
            }
        }
    }
    Ok(FsToolResult::Glob { entries, truncated })
}
fn copy_file(source: &Path, target: &Path, total: &mut u64) -> FsResult<()> {
    let meta = fs::metadata(source)?;
    let remaining = MAX_COPY.saturating_sub(*total);
    if meta.len() > remaining {
        return Err(FsError::new(
            FsErrorCode::FileTooLarge,
            "Copy exceeds the 128 MiB limit",
        ));
    }
    let input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let copied = io::copy(&mut input.take(remaining + 1), &mut output)?;
    if copied > remaining {
        return Err(FsError::new(
            FsErrorCode::FileTooLarge,
            "Copy exceeds the 128 MiB limit",
        ));
    }
    *total += copied;
    output.set_permissions(meta.permissions())?;
    output.sync_all()?;
    Ok(())
}
fn copy_tree(source: &Path, target: &Path, count: &mut usize, total: &mut u64) -> FsResult<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        *count += 1;
        if *count > MAX_WALK {
            return Err(FsError::new(
                FsErrorCode::FileTooLarge,
                "Copy exceeds the 10000 entry limit",
            ));
        }
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let meta = fs::symlink_metadata(&source_path)?;
        if meta.file_type().is_symlink() {
            return Err(FsError::new(
                FsErrorCode::Unsupported,
                "Copying symlinks is unavailable",
            ));
        }
        if meta.is_dir() {
            copy_tree(&source_path, &target_path, count, total)?;
        } else if meta.is_file() {
            copy_file(&source_path, &target_path, total)?;
        } else {
            return Err(FsError::new(
                FsErrorCode::Unsupported,
                "Special files cannot be copied",
            ));
        }
    }
    fs::set_permissions(target, fs::metadata(source)?.permissions())?;
    Ok(())
}
fn copy_path(root: &Path, from: &str, to: &str, recursive: bool) -> FsResult<String> {
    let source = mutable_existing(root, from)?;
    let target = destination(root, to)?;
    reject_final_symlink(&target)?;
    if fs::symlink_metadata(&target).is_ok() {
        return Err(FsError::new(
            FsErrorCode::AlreadyExists,
            "Destination already exists",
        ));
    }
    if source.is_dir() && (!recursive || target.starts_with(&source)) {
        return Err(FsError::new(
            FsErrorCode::InvalidInput,
            "Directory copy requires recursive=true and a destination outside the source",
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| FsError::new(FsErrorCode::InvalidInput, "Invalid destination"))?;
    if source.is_file() {
        let meta = fs::metadata(&source)?;
        if meta.len() > MAX_COPY {
            return Err(FsError::new(
                FsErrorCode::FileTooLarge,
                "Copy exceeds the 128 MiB limit",
            ));
        }
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        let input = fs::File::open(&source)?;
        let copied = io::copy(&mut input.take(MAX_COPY + 1), &mut temporary)?;
        if copied > MAX_COPY {
            return Err(FsError::new(
                FsErrorCode::FileTooLarge,
                "Copy exceeds the 128 MiB limit",
            ));
        }
        temporary.as_file().set_permissions(meta.permissions())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist_noclobber(&target)
            .map_err(|error| FsError::from(error.error))?;
    } else if source.is_dir() {
        let staging = tempfile::Builder::new()
            .prefix(".gravityforge-copy-")
            .tempdir_in(parent)?;
        let mut count = 0;
        let mut total = 0;
        for entry in fs::read_dir(&source)? {
            let entry = entry?;
            count += 1;
            if count > MAX_WALK {
                return Err(FsError::new(
                    FsErrorCode::FileTooLarge,
                    "Copy exceeds the 10000 entry limit",
                ));
            }
            let source_path = entry.path();
            let target_path = staging.path().join(entry.file_name());
            let meta = fs::symlink_metadata(&source_path)?;
            if meta.file_type().is_symlink() {
                return Err(FsError::new(
                    FsErrorCode::Unsupported,
                    "Copying symlinks is unavailable",
                ));
            }
            if meta.is_dir() {
                copy_tree(&source_path, &target_path, &mut count, &mut total)?;
            } else if meta.is_file() {
                copy_file(&source_path, &target_path, &mut total)?;
            } else {
                return Err(FsError::new(
                    FsErrorCode::Unsupported,
                    "Special files cannot be copied",
                ));
            }
        }
        if fs::symlink_metadata(&target).is_ok() {
            return Err(FsError::new(
                FsErrorCode::AlreadyExists,
                "Destination already exists",
            ));
        }
        fs::set_permissions(staging.path(), fs::metadata(&source)?.permissions())?;
        platform::move_no_replace(staging.path(), &target)?;
    } else {
        return Err(FsError::new(
            FsErrorCode::Unsupported,
            "Special files cannot be copied",
        ));
    }
    relative_display(root, &target)
}
fn delete_path(root: &Path, path: &str, recursive: bool) -> FsResult<String> {
    let source = mutable_existing(root, path)?;
    if source.is_dir() {
        if recursive {
            let mut count = 0;
            for item in WalkBuilder::new(&source)
                .hidden(false)
                .ignore(false)
                .git_ignore(false)
                .git_exclude(false)
                .git_global(false)
                .follow_links(false)
                .build()
            {
                let entry =
                    item.map_err(|error| FsError::new(FsErrorCode::IoError, error.to_string()))?;
                if entry.file_type().is_some_and(|kind| kind.is_symlink()) {
                    return Err(FsError::new(
                        FsErrorCode::Unsupported,
                        "Recursive deletion containing symlinks is unavailable",
                    ));
                }
                count += 1;
                if count > MAX_WALK {
                    return Err(FsError::new(
                        FsErrorCode::FileTooLarge,
                        "Deletion exceeds the 10000 entry limit",
                    ));
                }
            }
            fs::remove_dir_all(&source)?;
        } else {
            fs::remove_dir(&source)?;
        }
    } else {
        fs::remove_file(&source)?;
    }
    Ok(path.to_owned())
}
fn execute(root: &Path, action: FsAction) -> FsResult<FsToolResult> {
    match action {
        FsAction::List { path, limit } => {
            let limit = checked_limit(limit)?;
            let dir = existing(root, &path, true)?;
            if !dir.is_dir() {
                return Err(FsError::new(
                    FsErrorCode::NotDirectory,
                    "Path is not a directory",
                ));
            }
            let mut entries = Vec::new();
            for item in fs::read_dir(dir)? {
                let entry = item?;
                entries.push(FsEntry {
                    path: relative_display(root, &entry.path())?,
                    metadata: metadata(&entry.path())?,
                });
                if entries.len() > MAX_WALK {
                    return Err(FsError::new(
                        FsErrorCode::FileTooLarge,
                        "Directory exceeds the 10000 entry list limit",
                    ));
                }
            }
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            let truncated = entries.len() > limit;
            entries.truncate(limit);
            Ok(FsToolResult::List { entries, truncated })
        }
        FsAction::Read { path } => {
            let file = existing(root, &path, false)?;
            Ok(FsToolResult::Read {
                content: read_text(&file)?,
                metadata: metadata(&file)?,
            })
        }
        FsAction::ReadRange {
            path,
            start_byte,
            length,
        } => read_range(root, &path, start_byte, length),
        FsAction::Write {
            path,
            content,
            expected_content,
        } => Ok(FsToolResult::Mutation {
            path: atomic_text(root, &path, &content, Some(&expected_content))?,
        }),
        FsAction::Patch { path, hunks } => {
            let file = mutable_existing(root, &path)?;
            let original = read_text(&file)?;
            let updated = patch_text(&original, &hunks)?;
            Ok(FsToolResult::Mutation {
                path: atomic_text(root, &path, &updated, Some(&original))?,
            })
        }
        FsAction::Create { path, content } => Ok(FsToolResult::Mutation {
            path: atomic_text(root, &path, &content, None)?,
        }),
        FsAction::Delete { path, recursive } => Ok(FsToolResult::Mutation {
            path: delete_path(root, &path, recursive)?,
        }),
        FsAction::Move { from, to } => {
            let source = mutable_existing(root, &from)?;
            let target = destination(root, &to)?;
            reject_final_symlink(&target)?;
            if fs::symlink_metadata(&target).is_ok() {
                return Err(FsError::new(
                    FsErrorCode::AlreadyExists,
                    "Destination already exists",
                ));
            }
            if source.is_dir() && target.starts_with(&source) {
                return Err(FsError::new(
                    FsErrorCode::InvalidInput,
                    "Cannot move a directory into itself",
                ));
            }
            platform::move_no_replace(&source, &target)?;
            Ok(FsToolResult::Mutation {
                path: relative_display(root, &target)?,
            })
        }
        FsAction::Copy {
            from,
            to,
            recursive,
        } => Ok(FsToolResult::Mutation {
            path: copy_path(root, &from, &to, recursive)?,
        }),
        FsAction::Mkdir { path } => {
            let target = destination(root, &path)?;
            fs::create_dir(&target)?;
            Ok(FsToolResult::Mutation {
                path: relative_display(root, &target)?,
            })
        }
        FsAction::Exists { path } => {
            relative(&path, true)?;
            if path.is_empty() {
                return Ok(FsToolResult::Exists { exists: true });
            }
            match existing(root, &path, false) {
                Ok(_) => Ok(FsToolResult::Exists { exists: true }),
                Err(error) if error.code == FsErrorCode::NotFound => {
                    Ok(FsToolResult::Exists { exists: false })
                }
                Err(error) => Err(error),
            }
        }
        FsAction::Stat { path } => {
            relative(&path, true)?;
            let target = if path.is_empty() {
                root.to_path_buf()
            } else {
                destination(root, &path)?
            };
            Ok(FsToolResult::Stat {
                metadata: metadata(&target)?,
            })
        }
        FsAction::Search { query, mode, limit } => search(root, &query, mode, limit),
        FsAction::Glob { pattern, limit } => glob(root, &pattern, limit),
    }
}
fn authorize(actor: ToolActor) -> FsResult<()> {
    if matches!(actor, ToolActor::Agent) {
        Err(FsError::new(
            FsErrorCode::PermissionDenied,
            "Agent filesystem access requires an approval policy",
        ))
    } else {
        Ok(())
    }
}
fn log_tool(
    operation: &str,
    actor: ToolActor,
    risk: RiskLevel,
    result: &FsResult<FsToolResult>,
    started: Instant,
) {
    eprintln!(
        "{}",
        serde_json::json!({
            "category": "FS_TOOL", "operation": operation,
            "actor": match actor { ToolActor::User => "USER", ToolActor::Agent => "AGENT" },
            "risk": risk, "outcome": if result.is_ok() { "ok" } else { "error" },
            "errorCode": result.as_ref().err().map(|error| error.code),
            "durationMs": started.elapsed().as_millis(),
        })
    );
}
#[tauri::command]
pub async fn filesystem_tool(
    request: FsToolRequest,
    workspace: State<'_, WorkspaceState>,
) -> FsResult<FsToolResult> {
    let started = Instant::now();
    let operation = request.action.name();
    let risk = request.action.risk();
    if let Err(error) = authorize(request.actor) {
        let result = Err(error);
        log_tool(operation, request.actor, risk, &result, started);
        return result;
    }
    let root = workspace
        .0
        .lock()
        .map_err(|_| FsError::new(FsErrorCode::Internal, "Workspace lock failed"))
        .and_then(|state| {
            state
                .root()
                .map(Path::to_path_buf)
                .map_err(|message| FsError::new(FsErrorCode::InvalidInput, message))
        });
    let result = match root {
        Ok(root) => {
            match tauri::async_runtime::spawn_blocking(move || execute(&root, request.action)).await
            {
                Ok(result) => result,
                Err(error) => Err(FsError::new(FsErrorCode::Internal, error.to_string())),
            }
        }
        Err(error) => Err(error),
    };
    log_tool(operation, request.actor, risk, &result, started);
    result
}

#[cfg(test)]
mod tests;
