mod computer;
mod filesystem;
mod local_runtime;
mod model;
mod security;
mod system;
mod terminal;
mod voice;
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
            app.manage(local_runtime::LocalRuntimeManager::default());
            app.manage(computer::ComputerController::default());
            app.manage(voice::VoiceEngine::default());
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window
                    .state::<local_runtime::LocalRuntimeManager>()
                    .shutdown();
                window.state::<computer::ComputerController>().stop();
                window.state::<voice::VoiceEngine>().stop();
            }
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
            local_runtime::local_runtime_hardware_profile,
            local_runtime::local_runtime_discovery,
            local_runtime::local_runtime_inspect_model,
            local_runtime::local_runtime_status,
            local_runtime::local_runtime_start,
            local_runtime::local_runtime_stop,
            local_runtime::local_runtime_restart,
            local_runtime::local_runtime_test_capabilities,
            model::model_set_api_key,
            model::model_list_models,
            model::model_health_check,
            model::model_chat,
            model::model_stream_chat,
            model::model_cancel,
            computer::computer_status,
            computer::computer_enable,
            computer::computer_disable,
            computer::computer_stop,
            computer::computer_execute,
            computer::computer_audit_log,
            voice::voice_status,
            voice::voice_devices,
            voice::voice_microphone_self_test,
            voice::voice_runtime_status,
            voice::voice_hardware_recommendation,
            voice::voice_runtime_load,
            voice::voice_runtime_unload,
            voice::voice_enable,
            voice::voice_disable,
            voice::voice_start_listening,
            voice::voice_stop_and_transcribe,
            voice::voice_capture_integrity_test,
            voice::voice_speak,
            voice::voice_interrupt
        ])
        .run(tauri::generate_context!())
        .expect("failed to run GravityForge");
}
