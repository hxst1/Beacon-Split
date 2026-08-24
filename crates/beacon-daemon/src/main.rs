//! Beacon's session daemon.
//!
//! Owns every PTY so that closing the Beacon window does not end the work it
//! was showing. The window is a client: it attaches, renders what the daemon
//! has, and detaches. Nothing about a session depends on anyone watching it.

mod server;

use std::io::ErrorKind;
use std::os::unix::net::UnixListener;

use beacon_core::protocol::socket_dir;

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
    init_tracing();

    let dir = requested_dir();
    let listener = match bind(&dir) {
        Ok(listener) => listener,
        Err(err) => {
            tracing::error!(error = %err, "could not listen");
            std::process::exit(1);
        }
    };

    let socket = dir.join("daemon.sock");
    tracing::info!(socket = %socket.display(), pid = std::process::id(), "daemon started");
    server::serve(listener, socket);
    tracing::info!("daemon stopped");
}

/// Binds the socket, clearing a stale one left by a daemon that was killed.
///
/// A unix socket file outlives the process that made it, so its presence proves
/// nothing. Whether anything is listening is settled by trying to connect.
fn bind(dir: &std::path::Path) -> std::io::Result<UnixListener> {
    std::fs::create_dir_all(dir)?;
    restrict_to_owner(dir)?;

    let path = dir.join("daemon.sock");
    match UnixListener::bind(&path) {
        Ok(listener) => Ok(listener),
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                return Err(std::io::Error::new(
                    ErrorKind::AddrInUse,
                    "another daemon is already listening",
                ));
            }
            tracing::info!("clearing a socket left behind by a previous daemon");
            std::fs::remove_file(&path)?;
            UnixListener::bind(&path)
        }
        Err(err) => Err(err),
    }
}

/// The directory's permissions are the access control: on Linux the temporary
/// directory is shared between users, and a session is a shell.
fn restrict_to_owner(dir: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
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
