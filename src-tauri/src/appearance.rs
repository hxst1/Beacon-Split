//! Applying the parts of the look that CSS cannot reach.
//!
//! Opacity is a variable the frontend sets on the document, and that is where
//! it belongs. Frosting is not: `backdrop-filter` composites what is behind an
//! element *within the page*, and behind Beacon's chrome is a flat colour, so
//! it produced the same flat colour back. Blurring the desktop is something
//! only the window server can do, through a window effect.

use beacon_core::appearance::Appearance;
use tauri::{Manager, Runtime};

/// The label the main window is given in `tauri.conf.json`.
const MAIN_WINDOW: &str = "main";

/// Puts the window effect in the state this appearance asks for.
///
/// Best-effort by design. A window effect is decoration: a platform that has
/// none, or a call that fails, should leave a perfectly usable window rather
/// than stopping anything.
pub fn apply<R: Runtime>(app: &tauri::AppHandle<R>, appearance: &Appearance) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };

    let effects = appearance.frosted.then(|| {
        use tauri::utils::WindowEffect;
        use tauri::utils::config::WindowEffectsConfig;
        use tauri::window::EffectState;

        WindowEffectsConfig {
            // Frosts what is behind the whole window rather than tinting it as
            // a panel material would, which is what leaves the opacity setting
            // still meaning something: how much of that frosted desktop the
            // window's own background lets through.
            effects: vec![WindowEffect::UnderWindowBackground],
            // Frosted whether or not Beacon is the focused application. The
            // alternative follows focus, and a window that changes how it
            // looks every time you glance at another one is a distraction.
            state: Some(EffectState::Active),
            radius: None,
            color: None,
        }
    });

    if let Err(err) = window.set_effects(effects) {
        tracing::warn!(error = %err, frosted = appearance.frosted, "could not set the window effect");
    }
}
