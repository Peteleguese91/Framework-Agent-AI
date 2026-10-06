mod filesystem;
mod model;
mod security;
mod system;
mod terminal;
mod workspace;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(workspace::initial_state(app.handle()));
            app.manage(terminal::TerminalManager::default());
            app.manage(model::ModelState::default());
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
            workspace::search_workspace_files,
            terminal::terminal_create,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_interrupt,
            terminal::terminal_kill,
            terminal::terminal_close,
            terminal::terminal_status,
            terminal::terminal_exec,
            filesystem::filesystem_tool,
            model::model_set_api_key,
            model::model_list_models,
            model::model_health_check,
            model::model_chat,
            model::model_stream_chat,
            model::model_cancel
        ])
        .run(tauri::generate_context!())
        .expect("failed to run GravityForge");
}
