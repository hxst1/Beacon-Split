//! What the webview has to be told before it starts, on machines where its
//! defaults do not draw a window.
//!
//! On Linux with NVIDIA's driver, WebKitGTK's DMA-BUF renderer breaks the
//! Wayland connection the moment the first frame is handed over: GDK logs
//! `Error 71 (Protocol error) dispatching to Wayland display`, the process
//! exits, and what the user saw was a transparent window that never filled in.
//! Turning that renderer off falls back to shared memory, which every driver
//! does correctly. See ADR-096.
//!
//! From WebKitGTK 2.54 it is the other way round: DMA-BUF draws on NVIDIA, and
//! the shared-memory path hands the compositor frames holding only what changed
//! — a pulsing dot, a blinking cursor — so the window flashes empty several
//! times a second. There the renderer is left alone. See ADR-100.

/// Sets the webview's environment, and says whether it changed anything. Must
/// run before anything starts a thread, because the environment is process-wide
/// and GTK reads it on startup — which is also why it cannot log for itself.
pub fn prepare() -> bool {
    #[cfg(target_os = "linux")]
    return linux::prepare();
    #[cfg(not(target_os = "linux"))]
    false
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::Path;

    const DMABUF: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

    pub fn prepare() -> bool {
        // Somebody who set it either way knows something about this machine
        // that a check for a kernel module does not.
        if std::env::var_os(DMABUF).is_some() || !nvidia_driver() || dmabuf_draws() {
            return false;
        }
        // SAFETY: called first thing in `run`, before tracing, Tauri or GTK
        // has started a thread that could be reading the environment.
        unsafe { std::env::set_var(DMABUF, "1") };
        true
    }

    /// Whether NVIDIA's proprietary driver is loaded — the open-source nouveau
    /// one renders through Mesa and does not have the problem.
    fn nvidia_driver() -> bool {
        Path::new("/sys/module/nvidia_drm").exists()
            || Path::new("/proc/driver/nvidia/version").exists()
    }

    /// Whether the WebKitGTK loaded is one whose DMA-BUF renderer works with
    /// NVIDIA's driver — and whose fallback no longer does. Safe before GTK has
    /// started: the version is a constant of the library, not of a display.
    fn dmabuf_draws() -> bool {
        // SAFETY: both functions take nothing and return a compiled-in number.
        let version = unsafe {
            (
                webkit2gtk::ffi::webkit_get_major_version(),
                webkit2gtk::ffi::webkit_get_minor_version(),
            )
        };
        version >= (2, 54)
    }
}
