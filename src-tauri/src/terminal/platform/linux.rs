use portable_pty::MasterPty;
use std::{env, io, process::Command};

pub fn default_shell() -> String {
    env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty())
        .unwrap_or_else(|| "/bin/bash".into())
}

pub fn kill_foreground(master: &dyn MasterPty) {
    if let Some(group) = master.process_group_leader() {
        if group > 0 {
            unsafe {
                let _ = libc::killpg(group, libc::SIGKILL);
            }
        }
    }
}

pub fn kill_tree(pid: Option<u32>) {
    if let Some(pid) = pid {
        unsafe {
            let _ = libc::killpg(pid as i32, libc::SIGKILL);
        }
    }
}

pub fn prepare_exec(command: &mut Command) -> io::Result<()> {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}
