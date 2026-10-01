use beacon_core::domain::{ProjectId, WorkspaceId};
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// Reveals a project in Finder / the desktop file manager.
#[tauri::command]
pub fn reveal_project(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<()> {
    let path = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|err| CommandError::from(err.to_string()))
}

/// Records something the frontend could not recover from.
///
/// A release build has no inspector to open, and an uncaught render error
/// unmounts React's entire tree — which looks like a blank window and says
/// nothing about why. This is how it says why.
#[tauri::command]
pub fn report_frontend_error(details: String) {
    tracing::error!(target: "frontend", "{details}");
}

/// Which platform we are on, so the frontend can label shortcuts `⌘` or `Ctrl`
/// without sniffing the user agent.
#[tauri::command]
pub fn host_platform() -> &'static str {
    std::env::consts::OS
}

/// The Windows build, for the terminal: xterm needs it to know whether the
/// pseudo-console reflows lines on resize itself (it does from 21376), and
/// guesses wrongly in both directions without it. `None` elsewhere.
#[tauri::command]
pub fn windows_build() -> Option<u32> {
    #[cfg(windows)]
    {
        windows_build_number()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Read from the registry, where Windows keeps it as text. The version APIs
/// answer whatever the application's manifest claims to support instead.
#[cfg(windows)]
fn windows_build_number() -> Option<u32> {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};

    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let value = wide("CurrentBuildNumber");

    let mut buffer = [0u16; 32];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }

    let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16(&buffer[..length])
        .ok()?
        .trim()
        .parse()
        .ok()
}
