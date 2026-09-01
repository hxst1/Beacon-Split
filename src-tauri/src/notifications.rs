//! Notifications, spoken to macOS directly.
//!
//! `tauri-plugin-notification` is still what Linux and Windows use, but on
//! macOS it cannot do the two things that matter here. Its desktop
//! implementation answers `Granted` to every permission question without asking
//! the system (`desktop.rs:61-66`), so Beacon can never tell an allowed app
//! from a silenced one; and it posts through `NSUserNotificationCenter`, which
//! Apple deprecated in macOS 11 and which never raises the authorisation
//! prompt. The result is a notification that goes nowhere and an application
//! that believes it was delivered.
//!
//! `UserNotifications.framework` is the supported path: it reports the real
//! authorisation state, raises the system prompt once, and remembers the answer
//! across launches. See ADR-059.

use serde::Serialize;

/// What macOS will do with a notification from Beacon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// Never asked. The prompt is still available, and asking is the only way
    /// to get one — macOS raises it once per application, ever.
    NotDetermined,
    /// Asked and refused, or switched off later in System Settings. Nothing
    /// Beacon does can raise the prompt again; only the user can, from there.
    Denied,
    Authorized,
    /// Delivered quietly to Notification Centre, without a banner. Beacon never
    /// asks for this, but a user can end up here from System Settings.
    Provisional,
    /// There is no application for macOS to attribute a notification to.
    ///
    /// This is every `tauri dev` run: the binary is executed straight out of
    /// `target/`, with no bundle and therefore no identity. Reported honestly
    /// rather than as `denied`, because the two call for opposite advice.
    Unavailable,
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSBundle, NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
        UNNotificationRequest, UNNotificationSettings, UNNotificationSound,
        UNUserNotificationCenter,
    };

    use super::Permission;

    /// How long to wait for macOS to answer a question about itself.
    ///
    /// The completion handler arrives on one of the framework's own queues, in
    /// single-digit milliseconds. The timeout exists so that a wedged
    /// notification daemon degrades to "unknown" instead of hanging a command.
    const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);

    /// Whether this process is an application at all.
    ///
    /// `UNUserNotificationCenter::currentNotificationCenter` raises an
    /// Objective-C exception — not a `nil` — when there is no bundle, and an
    /// ObjC exception through Rust frames is undefined behaviour rather than a
    /// panic we could catch. So it is checked before, never rescued after.
    fn bundled() -> bool {
        NSBundle::mainBundle().bundleIdentifier().is_some()
    }

    pub fn permission() -> Permission {
        if !bundled() {
            return Permission::Unavailable;
        }

        let center = UNUserNotificationCenter::currentNotificationCenter();
        let (tx, rx) = mpsc::channel();
        let handler = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            let status = unsafe { settings.as_ref() }.authorizationStatus();
            let _ = tx.send(match status {
                UNAuthorizationStatus::Authorized => Permission::Authorized,
                UNAuthorizationStatus::Denied => Permission::Denied,
                UNAuthorizationStatus::Provisional => Permission::Provisional,
                _ => Permission::NotDetermined,
            });
        });
        center.getNotificationSettingsWithCompletionHandler(&handler);

        rx.recv_timeout(ANSWER_TIMEOUT)
            .unwrap_or(Permission::NotDetermined)
    }

    /// Raises the system prompt, and does not wait for an answer.
    ///
    /// The prompt is a window in front of a person, so the honest upper bound
    /// on the reply is "however long they take". A command that blocks for that
    /// is a frozen settings screen, so this returns as soon as macOS has been
    /// asked and the frontend reads the outcome by asking again.
    pub fn request() {
        if !bundled() {
            return;
        }

        let center = UNUserNotificationCenter::currentNotificationCenter();
        let options = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound;
        let handler = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            let error = (!error.is_null()).then(|| unsafe { (*error).localizedDescription() });
            tracing::info!(
                granted = granted.as_bool(),
                error = error.map(|e| e.to_string()),
                "asked macOS for notification permission"
            );
        });
        center.requestAuthorizationWithOptions_completionHandler(options, &handler);
    }

    pub fn post(title: &str, body: &str) -> Result<(), String> {
        if !bundled() {
            return Err("this build is not a bundled application".into());
        }

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));

        // Unique per notification, so macOS stacks them instead of replacing
        // the previous one — two projects finishing is two things to know.
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NSString::from_str(&format!(
            "beacon-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));

        let request =
            UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);

        let (tx, rx) = mpsc::channel();
        let handler = RcBlock::new(move |error: *mut NSError| {
            let _ = tx.send(
                (!error.is_null()).then(|| unsafe { (*error).localizedDescription() }.to_string()),
            );
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&request, Some(&handler));

        match rx.recv_timeout(ANSWER_TIMEOUT) {
            Ok(Some(error)) => Err(error),
            // Delivered, or macOS took longer than it ever takes to say so.
            Ok(None) | Err(_) => Ok(()),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::Permission;

    /// Elsewhere the plugin is honest enough: X11 and Windows have no
    /// per-application notification permission to report.
    pub fn permission() -> Permission {
        Permission::Authorized
    }

    pub fn request() {}

    pub fn post(_title: &str, _body: &str) -> Result<(), String> {
        Err("posting is handled by tauri-plugin-notification on this platform".into())
    }
}

pub use platform::{permission, post, request};

/// Where in System Settings a person can undo a refusal.
///
/// Only reachable by hand once the answer is `denied`: macOS raises its prompt
/// exactly once per application, so from then on this deep link is the whole
/// remedy.
#[cfg(target_os = "macos")]
pub fn settings_url(bundle_id: &str) -> String {
    format!("x-apple.systempreferences:com.apple.preference.notifications?id={bundle_id}")
}

#[cfg(not(target_os = "macos"))]
pub fn settings_url(_bundle_id: &str) -> String {
    String::new()
}
