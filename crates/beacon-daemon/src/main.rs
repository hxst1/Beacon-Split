//! Beacon's session daemon.
//!
//! Owns every PTY so that closing the Beacon window does not end the work it
//! was showing. The window is a client: it attaches, renders what the daemon
//! has, and detaches. Nothing about a session depends on anyone watching it.

mod hook;
mod mcp;
mod server;
mod statusline;

use std::io::ErrorKind;

use beacon_core::protocol::socket_dir;
use beacon_core::transport::{LocalListener, LocalStream, SOCKET_FILE};

/// Where to listen, when told.
///
/// An explicit socket makes a second, isolated Beacon possible — which is what
/// the tests need, so that running them cannot reach into a daemon somebody is
/// actually using.
fn requested_dir() -> std::path::PathBuf {
    std::env::args()
        .nth(1)
        .map(Into::into)
        .unwrap_or_else(socket_dir)
}

fn main() {
    // Running as a Claude Code hook rather than as the daemon. Checked before
    // anything else, including logging: a hook must be silent and quick.
    match std::env::args().nth(1).as_deref() {
        Some("hook") => hook::run(),
        // Serving MCP to the Claude session that started us. Checked here for
        // the same reason as the hook: it must not touch the daemon's logging,
        // which writes to stderr — and an MCP client reads stdout, not stderr,
        // so a stray log line is survivable but a stray print is not.
        Some("mcp") => mcp::run(),
        // A second argument is the status line Beacon took the slot from; it
        // still runs, and its output is still what Claude Code shows.
        Some("statusline") => statusline::run(std::env::args().nth(2)),
        _ => {}
    }

    init_tracing();

    let dir = requested_dir();
    let listener = match bind(&dir) {
        Ok(listener) => listener,
        Err(err) => {
            tracing::error!(error = %err, "could not listen");
            std::process::exit(1);
        }
    };

    let socket = dir.join(SOCKET_FILE);
    tracing::info!(socket = %socket.display(), pid = std::process::id(), "daemon started");
    server::serve(listener, socket);
    tracing::info!("daemon stopped");
}

/// Binds the socket, clearing a stale one left by a daemon that was killed.
///
/// A unix socket file outlives the process that made it, so its presence proves
/// nothing. Whether anything is listening is settled by trying to connect.
fn bind(dir: &std::path::Path) -> std::io::Result<LocalListener> {
    std::fs::create_dir_all(dir)?;
    restrict_to_owner(dir)?;

    let path = dir.join(SOCKET_FILE);
    match LocalListener::bind(&path) {
        Ok(listener) => Ok(listener),
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            if LocalStream::connect(&path).is_ok() {
                return Err(std::io::Error::new(
                    ErrorKind::AddrInUse,
                    "another daemon is already listening",
                ));
            }
            tracing::info!("clearing a socket left behind by a previous daemon");
            std::fs::remove_file(&path)?;
            LocalListener::bind(&path)
        }
        Err(err) => Err(err),
    }
}

/// The directory's permissions are the access control: on Linux the temporary
/// directory is shared between users, and a session is a shell.
#[cfg(unix)]
fn restrict_to_owner(dir: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// Nothing to restrict on Windows: the temporary directory lives inside the
/// user's profile, and a directory made there inherits an ACL that already
/// admits only that user (and the system).
#[cfg(windows)]
fn restrict_to_owner(_dir: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("beacon_daemon=info,beacon_core=info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
