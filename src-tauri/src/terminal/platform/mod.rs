#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::{default_shell, kill_foreground, kill_tree, prepare_exec};
#[cfg(target_os = "windows")]
pub use windows::{default_shell, kill_foreground, kill_tree, prepare_exec};

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
compile_error!("GravityForge terminal currently supports Linux and Windows");
