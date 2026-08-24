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

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
            commands::toggle_panel,
            commands::add_project,
            commands::rename_project,
            commands::remove_project,
            commands::move_project,
            commands::set_active_project,
            commands::reveal_project,
            commands::host_platform,
            commands::open_session,
            commands::write_session,
            commands::resize_session,
            commands::session_scrollback,
            commands::close_session,
            commands::restart_session,
            commands::stop_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Beacon");
}

/// Logs to stderr, filtered by `RUST_LOG` (default: our crates at `info`).
///
/// Nothing here ever logs file contents — `.env` values in particular must not
/// reach a log line.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("beacon_split_lib=info,beacon_core=info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
