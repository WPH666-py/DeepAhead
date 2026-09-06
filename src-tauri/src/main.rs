#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use deepahead::{commands, cli, ApprovalGate, DeepSeekClient, UndoStore};

fn main() {
    let ds_client = DeepSeekClient::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(ds_client)
        .manage(UndoStore::new())
        .manage(Arc::new(ApprovalGate::new()))
        .invoke_handler(tauri::generate_handler![
            // 项目
            commands::create_project,
            commands::open_project,
            // 文件操作
            commands::list_directory,
            commands::read_file_content,
            commands::write_file_content,
            commands::delete_file,
            commands::is_binary_file,
            commands::smart_read_file,
            commands::open_file_with_default_app,
            // AI 模式 + 对话
            commands::list_ai_modes,
            commands::switch_ai_mode,
            commands::configure_deepseek,
            commands::send_ai_message,
            commands::send_ai_message_stream,
            commands::send_ai_message_with_tools,
            commands::check_deepseek_health,
            commands::get_run_undo_count,
            commands::undo_run_changes,
            commands::parse_context_file,
            commands::configure_vision,
            commands::get_vision_config,
            commands::analyze_image,
            commands::save_temp_image,
            // 执行许可（需分步确认 / 全流程开放）
            commands::respond_tool_approval,
            // Agent + 安全
            commands::list_agents,
            commands::send_agent_message,
            commands::run_safety_check,
            // Git
            commands::git_status,
            commands::git_diff,
            commands::git_log,
            commands::git_branches,
            commands::git_clone,
            commands::git_push,
            // SSH
            commands::ssh_test_connection,
            commands::ssh_exec,
            commands::ssh_read_file,
            commands::ssh_list_dir,
            // 终端
            commands::open_terminal,
            commands::run_command,
            commands::detect_runtimes,
            commands::run_file,
            commands::detect_runtimes_enhanced,
            // 文件预览/市场/退出
            commands::read_file_bytes,
            commands::preview_excel_as_markdown,
            commands::preview_csv_as_markdown,
            commands::search_vscode_marketplace,
            commands::exit_app,
            // 会话
            commands::save_session,
            commands::load_session,
            commands::list_sessions,
            commands::delete_session,
            // CLI 桥接
            cli::check_deepseek_cli,
            cli::run_cli_agent_task,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DeepAhead");
}
