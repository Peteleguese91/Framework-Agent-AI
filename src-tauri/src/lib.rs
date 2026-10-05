mod security;
mod system;
mod workspace;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(workspace::initial_state(app.handle()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            system::platform_info,
            workspace::workspace_status,
            workspace::open_workspace,
            workspace::refresh_workspace,
            workspace::list_directory,
            workspace::read_workspace_file,
            workspace::write_workspace_file,
            workspace::file_stat,
            workspace::rename_workspace_path,
            workspace::delete_workspace_path,
            workspace::search_workspace_files
        ])
        .run(tauri::generate_context!())
        .expect("failed to run GravityForge");
}
