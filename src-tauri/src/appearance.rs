//! Applying the parts of the look that CSS cannot reach.
//!
//! Opacity is a variable the frontend sets on the document, and that is where
//! it belongs. Frosting is not: `backdrop-filter` composites what is behind an
//! element *within the page*, and behind Beacon's chrome is a flat colour, so
//! it produced the same flat colour back. Blurring the desktop is something
//! only the window server can do.

use beacon_core::appearance::Appearance;
use tauri::{Manager, Runtime};

/// The label the main window is given in `tauri.conf.json`.
const MAIN_WINDOW: &str = "main";

/// Puts the window effect in the state this appearance asks for.
///
/// Best-effort: a window effect is decoration, and a platform without one
/// should still get a perfectly usable window.
pub fn apply<R: Runtime>(app: &tauri::AppHandle<R>, appearance: &Appearance) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        tracing::warn!("no main window to put the frosting on");
        return;
    };
    set_frosted(&window, appearance.frosted);
}

/// Adds or removes the frosted material behind the window.
///
/// `WebviewWindow::set_effects` would be the obvious way to do this and is the
/// wrong one twice over. On macOS it only ever *adds*: handing it `None` runs
/// a branch that exists for Windows alone, so the effect applied at startup
/// could never be taken off again and the switch appeared dead. And it drops
/// the result inside a closure it schedules on the main thread, so what comes
/// back says the work was queued, not that it worked — which is why an earlier
/// look at the log seemed to confirm something it had never been asked.
///
/// So: the underlying calls, both directions, and the outcome written down.
#[cfg(target_os = "macos")]
fn set_frosted<R: Runtime>(window: &tauri::WebviewWindow<R>, frosted: bool) {
    use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState};

    let outcome = if frosted {
        window_vibrancy::apply_vibrancy(
            window,
            // Frosts what is behind the window without tinting it the way a
            // panel material would, which is what leaves opacity still meaning
            // something: how much of that frosted desktop shows through.
            NSVisualEffectMaterial::UnderWindowBackground,
            // Frosted whether or not Beacon is the focused application. The
            // alternative follows focus, and a window that changes how it looks
            // every time you glance at another one is a distraction.
            Some(NSVisualEffectState::Active),
            None,
        )
        .map(|()| true)
    } else {
        window_vibrancy::clear_vibrancy(window)
    };

    match outcome {
        Ok(changed) => tracing::info!(frosted, changed, "window frosting set"),
        Err(err) => tracing::warn!(error = %err, frosted, "could not set the window frosting"),
    }
}

/// No window server here offers this yet.
#[cfg(not(target_os = "macos"))]
fn set_frosted<R: Runtime>(_window: &tauri::WebviewWindow<R>, frosted: bool) {
    if frosted {
        tracing::info!("frosting is not available on this platform; leaving the window sharp");
    }
}
