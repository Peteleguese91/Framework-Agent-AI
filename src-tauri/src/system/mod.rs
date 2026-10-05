use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    family: &'static str,
    os: &'static str,
    architecture: &'static str,
}

#[tauri::command]
pub fn platform_info() -> PlatformInfo {
    PlatformInfo {
        family: std::env::consts::FAMILY,
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
    }
}
