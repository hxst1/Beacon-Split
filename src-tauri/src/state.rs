use std::sync::Mutex;

use beacon_core::Beacon;

/// Beacon's state is single-owner and synchronous; Tauri commands arrive on a
/// thread pool, so the lock lives here rather than inside the core.
///
/// Operations are short (a few file writes at most), so a plain mutex is the
/// right amount of machinery. When session management moves to a daemon this
/// becomes a client handle instead.
pub struct AppState(pub Mutex<Beacon>);

impl AppState {
    pub fn new(beacon: Beacon) -> Self {
        Self(Mutex::new(beacon))
    }
}

/// Recovers from a panic in another command rather than poisoning the app.
pub fn lock(state: &AppState) -> std::sync::MutexGuard<'_, Beacon> {
    state
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
