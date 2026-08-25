//! The desktop shell: window setup and the IPC surface.
//!
//! All behaviour lives in `beacon-core`. This crate stays deliberately thin so
//! that moving session management into a daemon later is a change of transport,
//! not a rewrite.

mod commands;
mod error;
mod state;

use beacon_core::Beacon;
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    // Registered only when this build was configured to update itself.
    //
    // The plugin refuses to initialise without an update key, and refusing to
    // initialise is refusing to start — so a copy somebody compiled themselves
    // would not open at all. Those update by pulling, which is the right answer
    // for them and no reason to break the application.
    let context = tauri::generate_context!();
    let updates_itself = context.config().plugins.0.contains_key("updater");

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let beacon = Beacon::load_default().map_err(|err| {
                tracing::error!(error = %err, "could not load Beacon state");
                err
            })?;
            app.manage(AppState::new(beacon, app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::create_workspace,
            commands::update_workspace,
            commands::delete_workspace,
            commands::set_active_workspace,
            commands::set_layout,
            commands::set_layout_preset,
            commands::layout_presets,
            commands::set_appearance,
            commands::set_binding,
            commands::reset_bindings,
            commands::toggle_panel,
            commands::add_project,
            commands::rename_project,
            commands::remove_project,
            commands::move_project,
            commands::reorder_project,
            commands::set_shell,
            commands::set_notifications,
            commands::mark_releases_seen,
            commands::set_release_notices,
            commands::release_notes,
            commands::set_active_project,
            commands::reveal_project,
            commands::host_platform,
            commands::report_frontend_error,
            commands::list_dir,
            commands::read_file,
            commands::file_revision,
            commands::write_file,
            commands::create_file,
            commands::create_dir,
            commands::rename_path,
            commands::duplicate_path,
            commands::copy_into,
            commands::trash_path,
            commands::reveal_path,
            commands::list_project_files,
            commands::read_env_file,
            commands::git_status,
            commands::git_diff,
            commands::git_stage,
            commands::git_unstage,
            commands::git_stage_all,
            commands::git_commit,
            commands::git_push,
            commands::git_pull,
            commands::open_session,
            commands::write_session,
            commands::resize_session,
            commands::session_scrollback,
            commands::close_session,
            commands::restart_session,
            commands::stop_project,
            commands::stop_session_slot,
            commands::list_sessions,
            commands::stop_daemon,
            commands::claude_hook_status,
            commands::claude_integration,
            commands::check_requirements,
            commands::daemon_available,
            commands::install_claude_status_line,
            commands::remove_claude_status_line,
            commands::session_usage,
            commands::claude_hook_command,
            commands::install_claude_hooks,
            commands::remove_claude_hooks,
        ]);

    let builder = if updates_itself {
        builder.plugin(tauri_plugin_updater::Builder::new().build())
    } else {
        tracing::info!("no update key in this build; it updates by pulling");
        builder
    };

    builder.run(context).expect("error while running Beacon");
}

/// Logs to stderr, filtered by `RUST_LOG` (default: our crates at `info`).
///
/// Nothing here ever logs file contents — `.env` values in particular must not
/// reach a log line.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("beacon_split_lib=info,beacon_core=info,frontend=info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
