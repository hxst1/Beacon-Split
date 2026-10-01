//! The local socket the window, the daemon and Claude's hooks talk over.
//!
//! One name for it, so nothing above this module knows which platform it is
//! on. Everywhere it is reached through a file in a per-user directory, with
//! the same blocking API: `connect`, `bind`, `incoming`, `try_clone` and read
//! timeouts.
//!
//! On unix that file *is* the socket, and this is the standard library's unix
//! socket under another name.
//!
//! On Windows it is a loopback TCP connection, and the file says where to find
//! it. Windows has AF_UNIX sockets too, and they were the first thing tried —
//! but they go through Winsock's provider chain, and on a machine whose network
//! software has a provider in that chain that does not know them, `connect`
//! fails with `WSAEINVAL` even to a socket the same process just bound. That is
//! a common setup on company laptops, which are where a tool like this runs.
//! Named pipes avoid Winsock but would need overlapped I/O to read and write
//! one connection from two threads, and an access list of their own. Loopback
//! TCP has neither problem; what it lacks is a filesystem permission saying who
//! may connect, so the file carries a secret alongside the port, and a
//! connection that cannot say it is turned away. See ADR-072.

#[cfg(unix)]
pub use std::os::unix::net::{UnixListener as LocalListener, UnixStream as LocalStream};

#[cfg(windows)]
pub use loopback::{LocalListener, LocalStream};

/// The name of the file a daemon is reached through, in its runtime directory.
///
/// Named for what it is: on Windows it holds an address rather than being a
/// socket, and a file left there by an AF_UNIX build cannot always be removed.
pub const SOCKET_FILE: &str = if cfg!(windows) {
    "daemon.endpoint"
} else {
    "daemon.sock"
};

#[cfg(windows)]
mod loopback {
    use std::io::{self, ErrorKind, Read, Write};
    use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    /// What a client says first, before the token.
    const GREETING: &str = "BEACON";
    /// What the daemon answers once it has checked the token.
    const WELCOME: &str = "BEACON OK";
    /// How long either side waits for the other's half of the handshake.
    ///
    /// Short, because both ends are on this machine, and because the daemon
    /// checks each connection on its accept loop: something that connects and
    /// says nothing holds up the next connection by at most this long.
    const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
    /// How long a connection gets to be accepted.
    ///
    /// Far shorter than the handshake, because of how Windows refuses one: a
    /// connection to a loopback port nobody listens on is not turned away at
    /// once but retried, for two seconds. A live daemon's kernel accepts in
    /// well under a millisecond, so this only ever shortens the wait on a dead
    /// one.
    const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
    /// A line longer than this is not a handshake.
    const MAX_LINE: usize = 256;

    /// A connection to the daemon, already past the handshake.
    #[derive(Debug)]
    pub struct LocalStream(TcpStream);

    impl LocalStream {
        /// Connects to whatever is listening at `path`, by the address and
        /// token written there.
        ///
        /// A file left behind by a daemon that was killed names a port that is
        /// closed, or that something else has since taken; either way the
        /// handshake fails, which is what makes "can I connect" a reliable test
        /// of "is a daemon there", as it is for a unix socket.
        ///
        /// The file also names the daemon's process, and a process that is gone
        /// is answered at once. Without that, the first start after a reboot —
        /// which leaves the temporary directory, and the file, where they were —
        /// would wait out Windows' retries twice: once in the window, and once
        /// in the daemon checking it is not about to start a second copy.
        pub fn connect<P: AsRef<Path>>(path: P) -> io::Result<LocalStream> {
            let endpoint = read_endpoint(path.as_ref())?;
            if endpoint.pid.is_some_and(|pid| !process_is_running(pid)) {
                return Err(io::Error::new(
                    ErrorKind::ConnectionRefused,
                    "the daemon that wrote this endpoint is no longer running",
                ));
            }
            let (port, token) = (endpoint.port, endpoint.token);
            let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
            let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
            stream.set_nodelay(true)?;

            stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
            (&stream).write_all(format!("{GREETING} {token}\n").as_bytes())?;
            if read_line(&stream)? != WELCOME {
                return Err(io::Error::new(
                    ErrorKind::ConnectionRefused,
                    "something other than the Beacon daemon answered",
                ));
            }
            stream.set_read_timeout(None)?;
            Ok(LocalStream(stream))
        }

        pub fn try_clone(&self) -> io::Result<LocalStream> {
            self.0.try_clone().map(LocalStream)
        }

        pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            self.0.set_read_timeout(timeout)
        }
    }

    impl Read for LocalStream {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.0.read(buffer)
        }
    }

    impl Write for LocalStream {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.0.write(buffer)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }

    /// Where the daemon listens: a loopback port, named in the file at `path`.
    #[derive(Debug)]
    pub struct LocalListener {
        listener: TcpListener,
        token: String,
    }

    impl LocalListener {
        /// Listens, and writes where to the file at `path`.
        ///
        /// Refuses with `AddrInUse` when the file is already there, as binding
        /// a unix socket does — the caller decides whether that file belongs to
        /// a live daemon or a dead one by trying to connect through it.
        pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<LocalListener> {
            let path = path.as_ref();
            if path.exists() {
                return Err(io::Error::new(
                    ErrorKind::AddrInUse,
                    "an endpoint file is already there",
                ));
            }

            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
            let port = listener.local_addr()?.port();
            let token = new_token();
            write_endpoint(path, port, &token)?;
            Ok(LocalListener { listener, token })
        }

        /// Connections that passed the handshake. Any that did not are closed
        /// and skipped, never handed out.
        pub fn incoming(&self) -> impl Iterator<Item = io::Result<LocalStream>> + '_ {
            std::iter::from_fn(move || {
                loop {
                    let stream = match self.listener.accept() {
                        Ok((stream, _)) => stream,
                        Err(err) => return Some(Err(err)),
                    };
                    match self.admit(stream) {
                        Some(stream) => return Some(Ok(stream)),
                        None => continue,
                    }
                }
            })
        }

        fn admit(&self, stream: TcpStream) -> Option<LocalStream> {
            let _ = stream.set_nodelay(true);
            stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT)).ok()?;

            let line = read_line(&stream).ok()?;
            let offered = line.strip_prefix(GREETING)?.trim();
            if !same(offered.as_bytes(), self.token.as_bytes()) {
                tracing::warn!("turned away a connection that did not know the daemon's token");
                return None;
            }

            (&stream)
                .write_all(format!("{WELCOME}\n").as_bytes())
                .ok()?;
            stream.set_read_timeout(None).ok()?;
            Some(LocalStream(stream))
        }
    }

    /// A secret only someone who can read the endpoint file knows.
    ///
    /// Two v4 UUIDs: 244 random bits from the system's generator, through a
    /// crate Beacon already has.
    fn new_token() -> String {
        format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )
    }

    /// Compares without stopping at the first difference, so the time a
    /// refusal takes says nothing about how much of a guess was right.
    fn same(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }

    /// Reads one line a byte at a time.
    ///
    /// Not through a `BufReader`: whatever follows the handshake belongs to
    /// the protocol, and a buffer would swallow the start of it.
    fn read_line(mut stream: &TcpStream) -> io::Result<String> {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            if stream.read(&mut byte)? == 0 {
                return Err(ErrorKind::UnexpectedEof.into());
            }
            match byte[0] {
                b'\n' => break,
                other => line.push(other),
            }
            if line.len() > MAX_LINE {
                return Err(io::Error::new(ErrorKind::InvalidData, "handshake too long"));
            }
        }
        String::from_utf8(line)
            .map(|line| line.trim_end_matches('\r').to_string())
            .map_err(|_| io::Error::new(ErrorKind::InvalidData, "handshake is not text"))
    }

    /// What the endpoint file says: `port token pid`.
    struct Endpoint {
        port: u16,
        token: String,
        /// Absent from a file that predates it, which is then simply tried.
        pid: Option<u32>,
    }

    fn read_endpoint(path: &Path) -> io::Result<Endpoint> {
        let text = std::fs::read_to_string(path)?;
        let mut words = text.split_whitespace();
        let port = words.next().and_then(|port| port.parse().ok());
        let token = words.next().map(str::to_string);
        let pid = words.next().and_then(|pid| pid.parse().ok());
        match (port, token) {
            (Some(port), Some(token)) => Ok(Endpoint { port, token, pid }),
            _ => Err(io::Error::new(
                ErrorKind::InvalidData,
                "the endpoint file does not say where the daemon is",
            )),
        }
    }

    /// Written through a temporary file, so a client can never read half of it.
    ///
    /// The directory is in the user's profile, so the file inherits an access
    /// list that admits only them — which is what keeps the token secret.
    fn write_endpoint(path: &Path, port: u16, token: &str) -> io::Result<()> {
        let temporary: PathBuf = path.with_extension("tmp");
        let pid = std::process::id();
        std::fs::write(&temporary, format!("{port} {token} {pid}\n"))?;
        std::fs::rename(&temporary, path)
    }

    /// Whether a process with this id is running.
    ///
    /// "Yes" when unsure: an id reused by something else since, or one that
    /// cannot be opened, falls through to connecting, and the handshake is
    /// what decides. Only a definite "gone" skips it.
    fn process_is_running(pid: u32) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                // No such process — or one we may not look at, which a daemon
                // of ours never is.
                return std::io::Error::last_os_error().raw_os_error() == Some(5);
            }
            let mut code = 0u32;
            let known = GetExitCodeProcess(process, &mut code) != 0;
            CloseHandle(process);
            !known || code == STILL_ACTIVE as u32
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::io::{BufRead, BufReader, Write};

    use super::{LocalListener, LocalStream};

    /// The first start after a reboot finds the last session's file still
    /// there. Waiting out Windows' two seconds of retries on its port, twice,
    /// is how a cold start would have felt slow for no visible reason.
    #[test]
    fn a_file_whose_daemon_has_exited_is_refused_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.sock");

        let mut gone = std::process::Command::new("cmd")
            .args(["/c", "exit"])
            .spawn()
            .unwrap();
        let pid = gone.id();
        gone.wait().unwrap();
        std::fs::write(&path, format!("1 some-token {pid}\n")).unwrap();

        let started = std::time::Instant::now();
        assert!(LocalStream::connect(&path).is_err());
        assert!(
            started.elapsed() < std::time::Duration::from_millis(300),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_client_that_knows_the_file_gets_through_and_can_talk_both_ways() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.sock");
        let listener = LocalListener::bind(&path).unwrap();

        let server = std::thread::spawn(move || {
            let stream = listener.incoming().next().unwrap().unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).unwrap();
            writer.write_all(format!("echo {line}").as_bytes()).unwrap();
        });

        let mut client = LocalStream::connect(&path).unwrap();
        client.write_all(b"hello\n").unwrap();
        let mut answer = String::new();
        BufReader::new(client).read_line(&mut answer).unwrap();
        assert_eq!(answer, "echo hello\n");
        server.join().unwrap();
    }

    #[test]
    fn a_second_listener_on_the_same_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.sock");
        let _first = LocalListener::bind(&path).unwrap();
        let second = LocalListener::bind(&path).unwrap_err();
        assert_eq!(second.kind(), std::io::ErrorKind::AddrInUse);
    }

    #[test]
    fn a_file_left_by_a_dead_daemon_does_not_connect() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.sock");
        drop(LocalListener::bind(&path).unwrap());
        assert!(path.exists());
        assert!(LocalStream::connect(&path).is_err());
    }

    #[test]
    fn a_connection_without_the_token_is_turned_away() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.sock");
        let listener = LocalListener::bind(&path).unwrap();
        let port: u16 = std::fs::read_to_string(&path)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();

        let server = std::thread::spawn(move || {
            // The impostor is skipped; the next real client is what comes out.
            listener.incoming().next().unwrap().is_ok()
        });

        let mut impostor = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        impostor.write_all(b"BEACON guess\n").unwrap();
        let mut reply = String::new();
        let _ = BufReader::new(impostor).read_line(&mut reply);
        assert_eq!(reply, "", "an impostor should be closed without a welcome");

        let _real = LocalStream::connect(&path).unwrap();
        assert!(server.join().unwrap());
    }

    /// How the window and the daemon both use a connection: one thread blocked
    /// reading it for as long as it lives, others writing to a clone of it. On
    /// a transport where a pending read held up a write, every request would
    /// wait for the next event to arrive.
    #[test]
    fn a_connection_is_written_while_another_thread_is_blocked_reading_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.sock");
        let listener = LocalListener::bind(&path).unwrap();

        // An echo server, line by line.
        std::thread::spawn(move || {
            let stream = listener.incoming().next().unwrap().unwrap();
            let mut writer = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                writeln!(writer, "{line}").unwrap();
            }
        });

        let client = LocalStream::connect(&path).unwrap();
        let reader = client.try_clone().unwrap();
        let (sender, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(reader).lines().map_while(Result::ok) {
                let _ = sender.send(line);
            }
        });

        // The reader is blocked by now; every write must still go through,
        // including one larger than any socket buffer.
        let large = "x".repeat(256 * 1024);
        let mut writer = client;
        for message in ["first", "second", large.as_str()] {
            writeln!(writer, "{message}").unwrap();
            let echoed = received
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("the echo never came back");
            assert_eq!(echoed.len(), message.len());
        }
    }

    /// A port left in a dead daemon's file and since taken by something else
    /// entirely. Connecting must fail — not attach the window to a stranger.
    #[test]
    fn something_else_listening_on_the_old_port_is_not_mistaken_for_the_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.sock");

        let stranger = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = stranger.local_addr().unwrap().port();
        std::fs::write(&path, format!("{port} some-old-token\n")).unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = stranger.accept() {
                let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
            }
        });

        // How it fails depends on whether the stranger has hung up yet — a
        // wrong answer, or a reset. That it fails is the point.
        assert!(LocalStream::connect(&path).is_err());
    }
}
