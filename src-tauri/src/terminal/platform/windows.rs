use portable_pty::MasterPty;
use std::{
    env, io,
    path::PathBuf,
    process::{Command, Stdio},
};

pub fn default_shell() -> String {
    let system_root = env::var_os("SystemRoot").map(PathBuf::from);
    if let Some(path) = system_root.map(|root| {
        root.join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe")
    }) {
        if path.is_file() {
            return path.to_string_lossy().into_owned();
        }
    }
    env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into())
}

pub fn kill_foreground(_master: &dyn MasterPty) {}

pub fn kill_tree(pid: Option<u32>) {
    if let Some(pid) = pid {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

pub fn prepare_exec(_command: &mut Command) -> io::Result<()> {
    Ok(())
}
