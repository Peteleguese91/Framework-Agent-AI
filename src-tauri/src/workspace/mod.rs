use crate::security::{canonical_workspace, resolve_existing_path, resolve_write_path};
use ignore::{
    gitignore::{Gitignore, GitignoreBuilder},
    WalkBuilder,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};

const IGNORES: &[&str] = &[
    "node_modules",
    ".git",
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
const LIMIT: usize = 50_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub name: String,
    pub root: String,
    pub project_info: ProjectInfo,
    pub project_map: ProjectMap,
}
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    pub project_types: Vec<String>,
    pub languages: Vec<String>,
    pub detected_frameworks: Vec<String>,
    pub package_manager: Option<String>,
    pub has_git: bool,
    pub main_config_files: Vec<String>,
    pub package: Option<PackageJsonSummary>,
}
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageJsonSummary {
    pub name: Option<String>,
    pub version: Option<String>,
    pub scripts: BTreeMap<String, String>,
    pub dependencies: Vec<String>,
    pub dev_dependencies: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMap {
    pub main_directories: Vec<String>,
    pub config_files: Vec<String>,
    pub files_by_extension: BTreeMap<String, usize>,
    pub entry_points: Vec<String>,
    pub has_tests: bool,
    pub total_files: usize,
    pub truncated: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTreeNode {
    pub name: String,
    pub relative_path: String,
    pub kind: NodeKind,
    pub extension: Option<String>,
    pub size: u64,
    pub modified_at: Option<u64>,
    pub is_ignored: bool,
    pub is_symlink: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    File,
    Directory,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum FileContent {
    Text { content: String },
    Binary,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub size: u64,
    pub modified_at: u64,
    pub read_only: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub display_name: String,
    pub path: String,
    pub last_opened: u64,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Persisted {
    workspace_root: Option<String>,
    recent_projects: Vec<RecentProject>,
}
pub struct WorkspaceManager {
    root: Option<PathBuf>,
    store: PathBuf,
    recent: Vec<RecentProject>,
    info: Option<WorkspaceInfo>,
}
pub struct WorkspaceState(pub Mutex<WorkspaceManager>);

impl WorkspaceManager {
    pub fn load(store: PathBuf) -> Self {
        let p: Persisted = fs::read(&store)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut s = Self {
            root: None,
            store,
            recent: p.recent_projects,
            info: None,
        };
        if let Some(r) = p.workspace_root {
            let _ = s.open(Path::new(&r));
        }
        s
    }
    fn root(&self) -> Result<&Path, String> {
        self.root
            .as_deref()
            .ok_or_else(|| "No workspace is open".into())
    }
    fn open(&mut self, path: &Path) -> Result<WorkspaceInfo, String> {
        let root = canonical_workspace(path).map_err(|e| e.to_string())?;
        let info = build_info(&root)?;
        self.root = Some(root.clone());
        self.info = Some(info.clone());
        self.recent.retain(|r| r.path != root.to_string_lossy());
        self.recent.insert(
            0,
            RecentProject {
                display_name: info.name.clone(),
                path: root.to_string_lossy().into_owned(),
                last_opened: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            },
        );
        self.recent.truncate(10);
        self.persist()?;
        Ok(info)
    }
    fn persist(&self) -> Result<(), String> {
        if let Some(p) = self.store.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let p = Persisted {
            workspace_root: self.root.as_ref().map(|p| p.to_string_lossy().into_owned()),
            recent_projects: self.recent.clone(),
        };
        fs::write(
            &self.store,
            serde_json::to_vec_pretty(&p).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
}
pub fn initial_state(app: &AppHandle) -> WorkspaceState {
    let p = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("workspace.json");
    WorkspaceState(Mutex::new(WorkspaceManager::load(p)))
}
#[tauri::command]
pub fn workspace_status(
    state: State<'_, WorkspaceState>,
) -> Result<(Option<WorkspaceInfo>, Vec<RecentProject>), String> {
    let s = state.0.lock().map_err(|_| "Workspace lock failed")?;
    Ok((s.info.clone(), s.recent.clone()))
}
#[tauri::command]
pub fn open_workspace(
    path: String,
    state: State<'_, WorkspaceState>,
) -> Result<WorkspaceInfo, String> {
    state
        .0
        .lock()
        .map_err(|_| "Workspace lock failed")?
        .open(Path::new(&path))
}
#[tauri::command]
pub fn refresh_workspace(state: State<'_, WorkspaceState>) -> Result<WorkspaceInfo, String> {
    let mut s = state.0.lock().map_err(|_| "Workspace lock failed")?;
    let r = s.root()?.to_path_buf();
    s.open(&r)
}
#[tauri::command]
pub fn list_directory(
    relative_path: String,
    state: State<'_, WorkspaceState>,
) -> Result<Vec<ProjectTreeNode>, String> {
    let s = state.0.lock().map_err(|_| "Workspace lock failed")?;
    let root = s.root()?;
    let dir = if relative_path.is_empty() {
        root.to_path_buf()
    } else {
        resolve_existing_path(root, Path::new(&relative_path)).map_err(|e| e.to_string())?
    };
    if !dir.is_dir() {
        return Err("Not a directory".into());
    }
    let ignore = ignore_rules(root);
    let mut out = Vec::new();
    for e in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let p = e.path();
        let rel = p.strip_prefix(root).map_err(|e| e.to_string())?;
        let m = fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
        if ignored(root, rel, m.is_dir(), &ignore) {
            continue;
        }
        out.push(ProjectTreeNode {
            name: e.file_name().to_string_lossy().into_owned(),
            relative_path: rel.to_string_lossy().into_owned(),
            kind: if m.is_dir() {
                NodeKind::Directory
            } else {
                NodeKind::File
            },
            extension: p.extension().map(|v| v.to_string_lossy().into_owned()),
            size: m.len(),
            modified_at: m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            is_ignored: false,
            is_symlink: m.file_type().is_symlink(),
        });
    }
    out.sort_by(|a, b| {
        matches!(a.kind, NodeKind::File)
            .cmp(&matches!(b.kind, NodeKind::File))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}
#[tauri::command]
pub fn read_workspace_file(
    relative_path: String,
    state: State<'_, WorkspaceState>,
) -> Result<FileContent, String> {
    let s = state.0.lock().map_err(|_| "Workspace lock failed")?;
    let p =
        resolve_existing_path(s.root()?, Path::new(&relative_path)).map_err(|e| e.to_string())?;
    if !p.is_file() {
        return Err("Not a file".into());
    }
    let b = fs::read(p).map_err(|e| e.to_string())?;
    if content_inspector::inspect(&b).is_binary() {
        Ok(FileContent::Binary)
    } else {
        String::from_utf8(b)
            .map(|content| FileContent::Text { content })
            .map_err(|_| "File is not valid UTF-8".into())
    }
}

fn stat_path(path: &Path) -> Result<FileStat, String> {
    let m = fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(FileStat {
        size: m.len(),
        modified_at: m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        read_only: m.permissions().readonly(),
    })
}

fn atomic_write(root: &Path, relative: &Path, content: &str) -> Result<FileStat, String> {
    let target = resolve_write_path(root, relative).map_err(|e| e.to_string())?;
    if target.exists() {
        resolve_existing_path(root, relative).map_err(|e| e.to_string())?;
    }
    let parent = target.parent().ok_or("Invalid target")?;
    let temp = parent.join(format!(
        ".gravityforge-{}.tmp",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    {
        use std::io::Write;
        let mut file = fs::File::create(&temp).map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    fs::rename(&temp, &target).map_err(|e| {
        let _ = fs::remove_file(&temp);
        e.to_string()
    })?;
    stat_path(&target)
}
fn rename_path(root: &Path, relative: &Path, new_name: &str) -> Result<String, String> {
    if new_name.is_empty() || Path::new(new_name).components().count() != 1 {
        return Err("New name must be a single path component".into());
    }
    let old = resolve_existing_path(root, relative).map_err(|e| e.to_string())?;
    let rel = relative.parent().unwrap_or(Path::new("")).join(new_name);
    let new = resolve_write_path(root, &rel).map_err(|e| e.to_string())?;
    if new.exists() {
        return Err("Destination already exists".into());
    }
    fs::rename(old, new).map_err(|e| e.to_string())?;
    Ok(rel.to_string_lossy().into_owned())
}
fn delete_path(root: &Path, relative: &Path) -> Result<(), String> {
    let path = resolve_existing_path(root, relative).map_err(|e| e.to_string())?;
    if path == root {
        return Err("Cannot delete workspace root".into());
    }
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_workspace_file(
    relative_path: String,
    content: String,
    state: State<'_, WorkspaceState>,
) -> Result<FileStat, String> {
    let s = state.0.lock().map_err(|_| "Workspace lock failed")?;
    atomic_write(s.root()?, Path::new(&relative_path), &content)
}

#[tauri::command]
pub fn file_stat(
    relative_path: String,
    state: State<'_, WorkspaceState>,
) -> Result<FileStat, String> {
    let s = state.0.lock().map_err(|_| "Workspace lock failed")?;
    let p =
        resolve_existing_path(s.root()?, Path::new(&relative_path)).map_err(|e| e.to_string())?;
    stat_path(&p)
}

#[tauri::command]
pub fn rename_workspace_path(
    relative_path: String,
    new_name: String,
    state: State<'_, WorkspaceState>,
) -> Result<String, String> {
    let s = state.0.lock().map_err(|_| "Workspace lock failed")?;
    rename_path(s.root()?, Path::new(&relative_path), &new_name)
}

#[tauri::command]
pub fn delete_workspace_path(
    relative_path: String,
    state: State<'_, WorkspaceState>,
) -> Result<(), String> {
    let s = state.0.lock().map_err(|_| "Workspace lock failed")?;
    delete_path(s.root()?, Path::new(&relative_path))
}

#[tauri::command]
pub fn search_workspace_files(
    query: String,
    state: State<'_, WorkspaceState>,
) -> Result<Vec<String>, String> {
    let s = state.0.lock().map_err(|_| "Workspace lock failed")?;
    let q = query.to_lowercase();
    let mut out = Vec::new();
    for e in WalkBuilder::new(s.root()?)
        .hidden(false)
        .git_ignore(true)
        .filter_entry(|e| !IGNORES.iter().any(|i| e.file_name() == *i))
        .build()
        .flatten()
    {
        if e.file_type().is_some_and(|t| t.is_file()) {
            let rel = e
                .path()
                .strip_prefix(s.root()?)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into_owned();
            if q.is_empty() || rel.to_lowercase().contains(&q) {
                out.push(rel);
                if out.len() >= 200 {
                    break;
                }
            }
        }
    }
    Ok(out)
}

fn package(path: &Path) -> Option<PackageJsonSummary> {
    let v: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let keys = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_object())
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    };
    Some(PackageJsonSummary {
        name: v.get("name").and_then(|x| x.as_str()).map(str::to_owned),
        version: v.get("version").and_then(|x| x.as_str()).map(str::to_owned),
        scripts: v
            .get("scripts")
            .and_then(|x| x.as_object())
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.into())))
                    .collect()
            })
            .unwrap_or_default(),
        dependencies: keys("dependencies"),
        dev_dependencies: keys("devDependencies"),
    })
}
fn build_info(root: &Path) -> Result<WorkspaceInfo, String> {
    let name = root
        .file_name()
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Project".into());
    let mut p = ProjectInfo {
        name: name.clone(),
        has_git: root.join(".git").exists(),
        package: package(&root.join("package.json")),
        ..Default::default()
    };
    for (f, t, l) in [
        ("package.json", "Node.js", None),
        ("Cargo.toml", "Rust", Some("Rust")),
        ("project.godot", "Godot", Some("GDScript")),
        ("pyproject.toml", "Python", Some("Python")),
        ("requirements.txt", "Python", Some("Python")),
        ("Dockerfile", "Docker", None),
    ] {
        if root.join(f).exists() {
            p.main_config_files.push(f.into());
            p.project_types.push(t.into());
            if let Some(l) = l {
                p.languages.push(l.into());
            }
        }
    }
    let names: Vec<_> = fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    if names.iter().any(|n| n.starts_with("vite.config.")) {
        p.detected_frameworks.push("Vite".into());
    }
    if names.iter().any(|n| n.starts_with("next.config.")) {
        p.detected_frameworks.push("Next.js".into());
    }
    if root.join("tsconfig.json").exists() {
        p.languages.push("TypeScript".into());
        p.main_config_files.push("tsconfig.json".into());
    }
    if root.join("src-tauri").is_dir() {
        p.detected_frameworks.push("Tauri".into());
    }
    if let Some(pkg) = &p.package {
        if pkg
            .dependencies
            .iter()
            .chain(&pkg.dev_dependencies)
            .any(|d| d == "react")
        {
            p.detected_frameworks.push("React".into());
        }
    }
    p.package_manager = if root.join("pnpm-lock.yaml").exists() {
        Some("pnpm".into())
    } else if root.join("yarn.lock").exists() {
        Some("yarn".into())
    } else if root.join("package-lock.json").exists() {
        Some("npm".into())
    } else {
        None
    };
    let map = project_map(root, &p);
    Ok(WorkspaceInfo {
        name,
        root: root.to_string_lossy().into_owned(),
        project_info: p,
        project_map: map,
    })
}
fn project_map(root: &Path, info: &ProjectInfo) -> ProjectMap {
    let mut ext = BTreeMap::new();
    let mut dirs = BTreeSet::new();
    let mut entry = Vec::new();
    let mut total = 0;
    let mut tests = false;
    let mut truncated = false;
    for item in WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .filter_entry(|e| !IGNORES.iter().any(|i| e.file_name() == *i))
        .build()
        .flatten()
    {
        let p = item.path();
        if p == root {
            continue;
        }
        let Ok(rel) = p.strip_prefix(root) else {
            continue;
        };
        if item.file_type().is_some_and(|t| t.is_dir()) {
            if rel.components().count() == 1 {
                dirs.insert(rel.to_string_lossy().into_owned());
            }
            continue;
        }
        total += 1;
        if total > LIMIT {
            truncated = true;
            break;
        }
        let s = rel.to_string_lossy();
        tests |= s.contains("test") || s.contains("spec");
        if let Some(x) = p.extension() {
            *ext.entry(x.to_string_lossy().to_lowercase()).or_insert(0) += 1;
        }
        if [
            "src/main.tsx",
            "src/main.ts",
            "src/index.ts",
            "src/main.rs",
            "main.py",
            "app.py",
            "project.godot",
        ]
        .contains(&s.as_ref())
        {
            entry.push(s.into_owned());
        }
    }
    ProjectMap {
        main_directories: dirs.into_iter().collect(),
        config_files: info.main_config_files.clone(),
        files_by_extension: ext,
        entry_points: entry,
        has_tests: tests,
        total_files: total,
        truncated,
    }
}
fn ignore_rules(root: &Path) -> Gitignore {
    let mut b = GitignoreBuilder::new(root);
    for x in IGNORES {
        let _ = b.add_line(None, &format!("{x}/"));
    }
    b.add(root.join(".gitignore"));
    b.build().unwrap_or_else(|_| Gitignore::empty())
}
fn ignored(root: &Path, rel: &Path, dir: bool, g: &Gitignore) -> bool {
    IGNORES
        .iter()
        .any(|i| rel.components().next().is_some_and(|c| c.as_os_str() == *i))
        || g.matched_path_or_any_parents(root.join(rel), dir)
            .is_ignore()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_and_detection() {
        let d = tempfile::tempdir().unwrap();
        fs::write(
            d.path().join("package.json"),
            r#"{"name":"demo","scripts":{"build":"vite build"},"dependencies":{"react":"1"}}"#,
        )
        .unwrap();
        fs::write(d.path().join("package-lock.json"), "{}").unwrap();
        fs::write(d.path().join("vite.config.ts"), "").unwrap();
        let p = build_info(d.path()).unwrap().project_info;
        assert_eq!(p.package.unwrap().name.as_deref(), Some("demo"));
        assert!(p.detected_frameworks.contains(&"React".into()));
        assert_eq!(p.package_manager.as_deref(), Some("npm"));
    }
    #[test]
    fn binary() {
        assert!(content_inspector::inspect(&[0, 159, 0]).is_binary());
    }
    #[test]
    fn ignores_defaults() {
        let d = tempfile::tempdir().unwrap();
        assert!(ignored(
            d.path(),
            Path::new("node_modules/x"),
            true,
            &ignore_rules(d.path())
        ));
    }

    #[test]
    fn persists_and_restores_the_last_workspace() {
        let project = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        fs::write(project.path().join("README.md"), "hello").unwrap();
        let store = config.path().join("workspace.json");
        let mut manager = WorkspaceManager::load(store.clone());
        manager.open(project.path()).unwrap();
        assert!(store.exists());
        let restored = WorkspaceManager::load(store);
        assert_eq!(
            restored.root().unwrap(),
            project.path().canonicalize().unwrap()
        );
        assert_eq!(restored.recent.len(), 1);
    }

    #[test]
    fn atomic_write_rename_delete_and_boundary() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        atomic_write(&root, Path::new("note.txt"), "first").unwrap();
        atomic_write(&root, Path::new("note.txt"), "second").unwrap();
        assert_eq!(fs::read_to_string(root.join("note.txt")).unwrap(), "second");
        assert!(!fs::read_dir(&root).unwrap().flatten().any(|e| e
            .file_name()
            .to_string_lossy()
            .starts_with(".gravityforge-")));
        assert_eq!(
            rename_path(&root, Path::new("note.txt"), "renamed.txt").unwrap(),
            "renamed.txt"
        );
        delete_path(&root, Path::new("renamed.txt")).unwrap();
        assert!(!root.join("renamed.txt").exists());
        assert!(atomic_write(&root, Path::new("../escape"), "no").is_err());
    }
}
