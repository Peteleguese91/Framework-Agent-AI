mod platform;

use crate::{
    security::{canonical_workspace, resolve_existing_path},
    workspace::WorkspaceState,
};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TerminalActor {
    User,
    Agent,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub pid: Option<u32>,
    pub created_at: u64,
    pub status: TerminalStatus,
    pub exit_code: Option<u32>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalStatus {
    Starting,
    Running,
    Exited,
    Failed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputEvent {
    id: String,
    data: Vec<u8>,
}

struct Session {
    info: Mutex<TerminalInfo>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
}

#[derive(Default)]
pub struct TerminalManager {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_id: AtomicU64,
}

impl Drop for TerminalManager {
    fn drop(&mut self) {
        if let Ok(sessions) = self.sessions.get_mut() {
            for session in sessions.values() {
                let _ = kill_session(session);
            }
        }
    }
}
fn log_event(action: &str, id: &str, detail: &str) {
    eprintln!(
        "{}",
        serde_json::json!({"category":"TERMINAL","action":action,"id":id,"detail":detail})
    );
}

fn workspace_root(state: &State<'_, WorkspaceState>) -> Result<PathBuf, String> {
    state
        .0
        .lock()
        .map_err(|_| "Workspace lock failed".to_string())?
        .root()
        .map(Path::to_path_buf)
}

fn checked_cwd(root: &Path, cwd: Option<&str>) -> Result<PathBuf, String> {
    let path = match cwd {
        None | Some("") => root.to_path_buf(),
        Some(value) => {
            let requested = Path::new(value);
            if requested.is_absolute() {
                let canonical = canonical_workspace(requested).map_err(|e| e.to_string())?;
                if !canonical.starts_with(root) {
                    return Err("Terminal cwd must be inside the workspace".into());
                }
                canonical
            } else {
                resolve_existing_path(root, requested).map_err(|e| e.to_string())?
            }
        }
    };
    if !path.is_dir() {
        return Err("Terminal cwd is not a directory".into());
    }
    Ok(path)
}

fn get_session(manager: &TerminalManager, id: &str) -> Result<Arc<Session>, String> {
    manager
        .sessions
        .lock()
        .map_err(|_| "Terminal lock failed")?
        .get(id)
        .cloned()
        .ok_or_else(|| format!("Invalid terminal id: {id}"))
}

#[tauri::command]
pub fn terminal_create(
    workspace_root: String,
    actor: TerminalActor,
    shell: Option<String>,
    cwd: Option<String>,
    app: AppHandle,
    workspace: State<'_, WorkspaceState>,
    manager: State<'_, TerminalManager>,
) -> Result<TerminalInfo, String> {
    if !matches!(actor, TerminalActor::User) {
        return Err("Agent terminal sessions require an approval policy".into());
    }
    let root = workspace_root_from_state(&workspace)?;
    let supplied = canonical_workspace(Path::new(&workspace_root)).map_err(|e| e.to_string())?;
    if supplied != root {
        return Err("Workspace has changed; reopen the terminal".into());
    }
    let cwd = checked_cwd(&root, cwd.as_deref())?;
    let shell = shell
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(platform::default_shell);
    let id = manager.next_id.fetch_add(1, Ordering::Relaxed).to_string();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("PTY creation failed: {e}"))?;
    let mut command = CommandBuilder::new(&shell);
    command.cwd(&cwd);
    let mut child = pair.slave.spawn_command(command).map_err(|e| {
        log_event("spawn_error", &id, &e.to_string());
        format!("Shell spawn failed: {e}")
    })?;
    let pid = child.process_id();
    let mut reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            return Err(format!("PTY reader failed: {error}"));
        }
    };
    let writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(error) => {
            let _ = child.kill();
            return Err(format!("PTY writer failed: {error}"));
        }
    };
    let info = TerminalInfo {
        id: id.clone(),
        shell,
        cwd: cwd.to_string_lossy().into_owned(),
        pid,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        status: TerminalStatus::Running,
        exit_code: None,
    };
    let session = Arc::new(Session {
        info: Mutex::new(info.clone()),
        master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        child: Mutex::new(child),
    });
    manager
        .sessions
        .lock()
        .map_err(|_| "Terminal lock failed")?
        .insert(id.clone(), session.clone());
    log_event("spawn", &id, "running");

    let output_app = app.clone();
    let output_id = id.clone();
    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = output_app.emit(
                        "terminal-output",
                        OutputEvent {
                            id: output_id.clone(),
                            data: buffer[..n].to_vec(),
                        },
                    );
                }
                Err(error) => {
                    log_event("read_error", &output_id, &error.to_string());
                    break;
                }
            }
        }
    });

    thread::spawn(move || loop {
        let result = session
            .child
            .lock()
            .map_err(|_| "Terminal child lock failed".to_string())
            .and_then(|mut child| child.try_wait().map_err(|e| e.to_string()));
        match result {
            Ok(Some(exit)) => {
                if let Ok(mut info) = session.info.lock() {
                    info.status = TerminalStatus::Exited;
                    info.exit_code = Some(exit.exit_code());
                    let _ = app.emit("terminal-status", info.clone());
                }
                log_event("exit", &id, "exited");
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(150)),
            Err(error) => {
                if let Ok(mut info) = session.info.lock() {
                    info.status = TerminalStatus::Failed;
                    let _ = app.emit("terminal-status", info.clone());
                }
                log_event("wait_error", &id, &error);
                break;
            }
        }
    });
    Ok(info)
}

fn workspace_root_from_state(state: &State<'_, WorkspaceState>) -> Result<PathBuf, String> {
    workspace_root(state)
}

#[tauri::command]
pub fn terminal_write(
    id: String,
    data: Vec<u8>,
    manager: State<'_, TerminalManager>,
) -> Result<(), String> {
    let session = get_session(&manager, &id)?;
    session
        .writer
        .lock()
        .map_err(|_| "Terminal writer lock failed")?
        .write_all(&data)
        .map_err(|e| format!("Terminal write failed: {e}"))
}

#[tauri::command]
pub fn terminal_resize(
    id: String,
    cols: u16,
    rows: u16,
    manager: State<'_, TerminalManager>,
) -> Result<(), String> {
    if cols == 0 || rows == 0 {
        return Err("Terminal size must be positive".into());
    }
    let session = get_session(&manager, &id)?;
    session
        .master
        .lock()
        .map_err(|_| "Terminal PTY lock failed")?
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Terminal resize failed: {e}"))
}

#[tauri::command]
pub fn terminal_interrupt(id: String, manager: State<'_, TerminalManager>) -> Result<(), String> {
    terminal_write(id, vec![3], manager)
}

fn kill_session(session: &Session) -> Result<(), String> {
    let pid = session
        .info
        .lock()
        .map_err(|_| "Terminal info lock failed")?
        .pid;
    if let Ok(master) = session.master.lock() {
        platform::kill_foreground(master.as_ref());
    }
    platform::kill_tree(pid);
    let mut child = session
        .child
        .lock()
        .map_err(|_| "Terminal child lock failed")?;
    if child.try_wait().map_err(|e| e.to_string())?.is_some() {
        return Ok(());
    }
    child
        .kill()
        .map_err(|e| format!("Terminal kill failed: {e}"))
}

#[tauri::command]
pub fn terminal_kill(id: String, manager: State<'_, TerminalManager>) -> Result<(), String> {
    let session = get_session(&manager, &id)?;
    kill_session(&session)?;
    log_event("kill", &id, "requested");
    Ok(())
}

#[tauri::command]
pub fn terminal_close(id: String, manager: State<'_, TerminalManager>) -> Result<(), String> {
    let session = get_session(&manager, &id)?;
    kill_session(&session)?;
    manager
        .sessions
        .lock()
        .map_err(|_| "Terminal lock failed")?
        .remove(&id);
    log_event("close", &id, "removed");
    Ok(())
}

#[tauri::command]
pub fn terminal_status(
    id: String,
    manager: State<'_, TerminalManager>,
) -> Result<TerminalInfo, String> {
    let session = get_session(&manager, &id)?;
    let info = session
        .info
        .lock()
        .map_err(|_| "Terminal info lock failed")?;
    Ok(info.clone())
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecRequest {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_ms: u64,
    pub env: HashMap<String, String>,
    pub actor: Option<TerminalActor>,
}

impl<'de> serde::Deserialize<'de> for TerminalActor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "USER" => Ok(Self::User),
            "AGENT" => Ok(Self::Agent),
            _ => Err(serde::de::Error::custom("Invalid terminal actor")),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub duration_ms: u64,
    pub timed_out: bool,
}

fn drain<R: Read>(mut reader: R) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    while let Ok(n) = reader.read(&mut buffer) {
        if n == 0 {
            break;
        }
        if output.len() < 8 * 1024 * 1024 {
            let remaining = 8 * 1024 * 1024 - output.len();
            output.extend_from_slice(&buffer[..n.min(remaining)]);
        }
    }
    output
}

fn execute(root: &Path, request: ExecRequest) -> Result<ExecResult, String> {
    if request.command.trim().is_empty() {
        return Err("Command is empty".into());
    }
    if !(1..=3_600_000).contains(&request.timeout_ms) {
        return Err("timeoutMs must be between 1 and 3600000".into());
    }
    let cwd = checked_cwd(root, request.cwd.as_deref())?;
    let start = Instant::now();
    let mut command = Command::new(&request.command);
    command
        .args(&request.args)
        .current_dir(cwd)
        .envs(&request.env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    platform::prepare_exec(&mut command).map_err(|error| error.to_string())?;
    let mut child = command
        .spawn()
        .map_err(|e| format!("Exec spawn failed: {e}"))?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or("Missing stdout pipe")?;
    let stderr = child.stderr.take().ok_or("Missing stderr pipe")?;
    let out_thread = thread::spawn(move || drain(stdout));
    let err_thread = thread::spawn(move || drain(stderr));
    let deadline = start + Duration::from_millis(request.timeout_ms);
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break (status, false);
        }
        if Instant::now() >= deadline {
            platform::kill_tree(Some(pid));
            let _ = child.kill();
            break (child.wait().map_err(|e| e.to_string())?, true);
        }
        thread::sleep(Duration::from_millis(20));
    };
    let stdout = out_thread.join().map_err(|_| "stdout reader failed")?;
    let stderr = err_thread.join().map_err(|_| "stderr reader failed")?;
    Ok(ExecResult {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code: status.code(),
        success: status.success() && !timed_out,
        duration_ms: start.elapsed().as_millis() as u64,
        timed_out,
    })
}

#[tauri::command]
pub async fn terminal_exec(
    request: ExecRequest,
    workspace: State<'_, WorkspaceState>,
) -> Result<ExecResult, String> {
    if !matches!(request.actor, Some(TerminalActor::User)) {
        return Err("Agent terminal execution requires an approval policy".into());
    }
    let root = workspace_root_from_state(&workspace)?;
    tauri::async_runtime::spawn_blocking(move || execute(&root, request))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(command: &str, args: &[&str]) -> ExecRequest {
        ExecRequest {
            command: command.into(),
            args: args.iter().map(|s| (*s).into()).collect(),
            cwd: None,
            timeout_ms: 2000,
            env: HashMap::new(),
            actor: Some(TerminalActor::User),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn exec_success_failure_cwd_env_and_invalid_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        assert_eq!(
            execute(&root, request("pwd", &[])).unwrap().stdout.trim(),
            root.to_string_lossy()
        );
        assert_eq!(
            execute(&root, request("sh", &["-c", "exit 7"]))
                .unwrap()
                .exit_code,
            Some(7)
        );
        let mut req = request("sh", &["-c", "printf %s \"$GF_TEST\""]);
        req.env.insert("GF_TEST".into(), "works".into());
        assert_eq!(execute(&root, req).unwrap().stdout, "works");
        let mut bad = request("pwd", &[]);
        bad.cwd = Some("../".into());
        assert!(execute(&root, bad).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn exec_timeout_terminates_process() {
        let dir = tempfile::tempdir().unwrap();
        let result = execute(
            dir.path(),
            ExecRequest {
                timeout_ms: 50,
                ..request("sh", &["-c", "sleep 5"])
            },
        )
        .unwrap();
        assert!(result.timed_out);
        assert!(!result.success);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pty_streams_input_and_resizes() {
        use std::sync::mpsc;

        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        pair.master
            .resize(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let size = pair.master.get_size().unwrap();
        assert_eq!((size.rows, size.cols), (30, 100));

        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "read value; printf 'reply:%s\\n' \"$value\""]);
        let mut child = pair.slave.spawn_command(command).unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        drop(pair.slave);

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0u8; 1024];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                output.extend_from_slice(&buffer[..count]);
                if output
                    .windows(b"reply:hello".len())
                    .any(|part| part == b"reply:hello")
                {
                    break;
                }
            }
            let _ = tx.send(output);
        });

        writer.write_all(b"hello\n").unwrap();
        let output = rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|error| {
                let _ = child.kill();
                panic!("PTY output did not arrive: {error}");
            });
        assert!(String::from_utf8_lossy(&output).contains("reply:hello"));
        assert!(child.wait().unwrap().success());
    }
}
