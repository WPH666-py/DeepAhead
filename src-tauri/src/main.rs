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
            // 上下文占用比例 + 压缩（自动 / 手动）
            commands::estimate_context_usage,
            commands::compress_context,
            commands::get_context_config,
            // 上下文压缩引擎（billion-context 移植）
            commands::context_engine_config,
            // 长期记忆协议（dsh-memory-protocol 移植）
            commands::get_memory_config,
            commands::set_memory_config,
            commands::memory_weave,
            commands::memory_ingest,
            commands::memory_search,
            commands::memory_status,
            commands::memory_recent,
            commands::memory_clear,
            // 插件体检（dsh-plugin-vet 移植）
            commands::vet_scan,
            commands::vet_write_health_record,
            commands::vet_list_records,
            // 规则引擎（dsh-rule-engine 移植）
            commands::rules_load,
            commands::rules_parse,
            commands::rules_audit,
            commands::rules_test_guard,
            commands::rules_get_config,
            commands::rules_set_toggles,
            commands::rules_guard_command,
            commands::rules_turn_cards,
            commands::rules_rate_turn_card,
            commands::rules_attach_turn_card,
            commands::rules_labels,
            commands::rules_fingerprint,
            // 执行许可档位 ↔ 规则引擎开关联动（开关不交给用户自选）
            commands::rules_link_mode,
            commands::rules_is_risk,
            // 回合裁决卡片：❌ 放行 → 一次性放行 → 自动「继续」
            commands::rules_resolve_pending,
            commands::rules_take_continue,
            commands::rules_pending_state,
            // 运行时日志（实时落盘到用户本机安装目录）
            commands::runtime_log_write,
            commands::runtime_log_status,
            commands::runtime_log_tail,
            commands::runtime_log_open_dir,
            // 费用统计（dsh-cost-meter 移植）
            commands::cost_snapshot,
            commands::cost_set_config,
            commands::cost_clear,
            // 执行许可（需逐步确认 / 仅确认风险操作 / 全流程开放）
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
            commands::git_log_graph,
            commands::git_commit_detail,
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
            // 自动更新（检测 Gitee 新版本 → 下载 → 卸载重装）
            commands::app_version,
            commands::check_update,
            commands::download_update,
            commands::cancel_update,
            commands::install_update,
            commands::uninstall_now,
            commands::update_helper_log,
            commands::quit_for_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DeepAhead");
}
