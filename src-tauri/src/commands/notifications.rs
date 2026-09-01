use tauri_plugin_opener::OpenerExt;

use crate::error::{CommandError, CommandResult};
use crate::notifications::{self, Permission};

/// What macOS will currently do with a notification from Beacon.
///
/// Asked rather than remembered. The answer lives in System Settings, where it
/// can change while Beacon is running and without telling it, so a cached copy
/// would be wrong exactly when it mattered.
#[tauri::command]
pub async fn notification_permission() -> Permission {
    notifications::permission()
}

/// Raises the system prompt, once, and returns without waiting for the answer.
///
/// The caller learns the outcome by asking again — see
/// [`notification_permission`].
#[tauri::command]
pub async fn request_notification_permission() {
    notifications::request();
}

#[tauri::command]
pub async fn send_notification(
    app: tauri::AppHandle,
    title: String,
    body: String,
) -> CommandResult<()> {
    #[cfg(target_os = "macos")]
    {
        let _ = &app;
        notifications::post(&title, &body).map_err(CommandError::from)
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|err| CommandError::from(err.to_string()))
    }
}

/// Opens the row for Beacon in System Settings → Notifications.
///
/// The remedy for a refusal, which is otherwise a dead end: macOS raises its
/// prompt once per application and never again.
#[tauri::command]
pub async fn open_notification_settings(app: tauri::AppHandle) -> CommandResult<()> {
    let url = notifications::settings_url(&app.config().identifier);
    if url.is_empty() {
        return Ok(());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| CommandError::from(err.to_string()))
}
