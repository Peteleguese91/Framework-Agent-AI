#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::move_no_replace;
#[cfg(target_os = "windows")]
pub use windows::move_no_replace;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
compile_error!("GravityForge filesystem tools currently support Linux and Windows");
