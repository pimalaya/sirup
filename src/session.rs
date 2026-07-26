#[cfg(feature = "smtp")]
use std::net::Ipv4Addr;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::{
    fs,
    io::{self, Read, Write},
    net::Shutdown,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
#[cfg(feature = "imap")]
use io_imap::{
    client::ImapClientStd,
    codec::{
        GreetingCodec,
        encode::{Encoder, Fragment},
    },
    types::{
        core::Vec1,
        response::{Capability, Code, Greeting},
    },
};
#[cfg(feature = "smtp")]
use io_smtp::{client::SmtpClientStd, rfc5321::SmtpEhloDomain};
use log::{info, warn};
use pimalaya_cli::spinner::Spinner;
#[cfg(any(feature = "imap", feature = "smtp"))]
use pimalaya_stream::std::stream::StreamStd;
use pimalaya_stream::{sasl::Sasl, tls::Tls};
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};
use url::Url;

pub enum Session {
    #[cfg(feature = "imap")]
    Imap {
        client: ImapClientStd,
        capability: Vec<Capability<'static>>,
    },
    #[cfg(feature = "smtp")]
    Smtp(SmtpClientStd),
    #[cfg(not(feature = "imap"))]
    #[cfg(not(feature = "smtp"))]
    Invalid,
}

impl Session {
    /// Sets the read timeout on the underlying authenticated stream.
    ///
    /// Both protocol clients box the stream as `Box<dyn ImapStream>` /
    /// `Box<dyn SmtpStream>`; we downcast back to [`StreamStd`] (the
    /// only concrete type Sirup ever stores) to reach the inherent
    /// [`StreamStd::set_read_timeout`] method.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        let stream: &mut StreamStd = match self {
            #[cfg(feature = "imap")]
            Self::Imap { client, .. } => client
                .stream
                .as_any_mut()
                .downcast_mut::<StreamStd>()
                .expect("Sirup IMAP stream is always StreamStd"),
            #[cfg(feature = "smtp")]
            Self::Smtp(client) => client
                .stream
                .as_any_mut()
                .downcast_mut::<StreamStd>()
                .expect("Sirup SMTP stream is always StreamStd"),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            Self::Invalid => return Ok(()),
        };
        stream.set_read_timeout(timeout)
    }

    /// Toggles non-blocking mode on the underlying authenticated stream,
    /// reaching it through the same [`StreamStd`] downcast as
    /// [`Self::set_read_timeout`].
    pub fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        let stream: &mut StreamStd = match self {
            #[cfg(feature = "imap")]
            Self::Imap { client, .. } => client
                .stream
                .as_any_mut()
                .downcast_mut::<StreamStd>()
                .expect("Sirup IMAP stream is always StreamStd"),
            #[cfg(feature = "smtp")]
            Self::Smtp(client) => client
                .stream
                .as_any_mut()
                .downcast_mut::<StreamStd>()
                .expect("Sirup SMTP stream is always StreamStd"),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            Self::Invalid => return Ok(()),
        };
        stream.set_nonblocking(nonblocking)
    }

    /// Sends a protocol-level NOOP to keep the upstream session alive.
    pub fn noop(&mut self) -> Result<()> {
        match self {
            #[cfg(feature = "imap")]
            Self::Imap { client, .. } => Ok(client.noop()?),
            #[cfg(feature = "smtp")]
            Self::Smtp(client) => Ok(client.noop()?),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            Self::Invalid => Ok(()),
        }
    }
}

impl Read for Session {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(feature = "imap")]
            Self::Imap { client, .. } => client.stream.read(buf),
            #[cfg(feature = "smtp")]
            Self::Smtp(client) => client.stream.read(buf),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            Self::Invalid => Ok(0),
        }
    }
}

impl Write for Session {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(feature = "imap")]
            Self::Imap { client, .. } => client.stream.write(buf),
            #[cfg(feature = "smtp")]
            Self::Smtp(client) => client.stream.write(buf),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            Self::Invalid => Ok(0),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(feature = "imap")]
            Self::Imap { client, .. } => client.stream.flush(),
            #[cfg(feature = "smtp")]
            Self::Smtp(client) => client.stream.flush(),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            Self::Invalid => Ok(()),
        }
    }
}

/// Opens and authenticates the upstream session, selecting the protocol
/// from the URL scheme (`imap`/`imaps` or `smtp`/`smtps`). Shared by
/// [`start`] and [`test`].
fn connect(url: Url, tls: Tls, starttls: bool, sasl: Option<Sasl>) -> Result<Session> {
    Ok(match url.scheme() {
        #[cfg(feature = "imap")]
        #[cfg(any(
            feature = "rustls-ring",
            feature = "rustls-aws",
            feature = "native-tls"
        ))]
        "imap" | "imaps" => {
            let (client, capability) = ImapClientStd::connect(&url, &tls, starttls, sasl, None)?;
            Session::Imap { client, capability }
        }
        #[cfg(feature = "smtp")]
        #[cfg(any(
            feature = "rustls-ring",
            feature = "rustls-aws",
            feature = "native-tls"
        ))]
        "smtp" | "smtps" => {
            let domain: SmtpEhloDomain<'static> = Ipv4Addr::new(127, 0, 0, 1).into();
            Session::Smtp(SmtpClientStd::connect(&url, &tls, starttls, domain, sasl)?)
        }

        #[cfg(not(feature = "imap"))]
        "imap" | "imaps" => bail!("Missing cargo feature: `imap`"),
        #[cfg(not(feature = "smtp"))]
        "smtp" | "smtps" => bail!("Missing cargo feature: `smtp`"),
        #[cfg(not(feature = "rustls-aws"))]
        #[cfg(not(feature = "rustls-ring"))]
        #[cfg(not(feature = "native-tls"))]
        _ => {
            bail!("Missing cargo feature: `rustls-aws`, `rustls-ring` or `native-tls`")
        }

        s => bail!("Unknown scheme `{s}`, expects `imap(s)` or `smtp(s)`"),
    })
}

/// Connects and authenticates once, then drops the session without
/// binding any socket. Used by the wizard to validate a freshly-built
/// account before printing it.
#[cfg(all(feature = "imap", feature = "smtp"))]
#[cfg(any(
    feature = "rustls-ring",
    feature = "rustls-aws",
    feature = "native-tls"
))]
pub fn test(url: Url, tls: Tls, starttls: bool, sasl: Option<Sasl>) -> Result<()> {
    let _ = connect(url, tls, starttls, sasl)?;
    Ok(())
}

pub fn start(
    sock_path: PathBuf,
    url: Url,
    tls: Tls,
    starttls: bool,
    sasl: Option<Sasl>,
) -> Result<()> {
    let s = Spinner::start("Starting remote session");
    let mut conn = connect(url, tls, starttls, sasl)?;
    s.success("Starting remote session");

    let s = Spinner::start("Binding local socket");

    if sock_path.exists() {
        fs::remove_file(&sock_path)?;
    }

    if let Some(sock_dir) = sock_path.parent() {
        fs::create_dir_all(sock_dir)?;
    }

    let listener = UnixListener::bind(&sock_path)?;
    listener.set_nonblocking(true)?;
    s.success("Binding local socket");

    // NOTE: NOOP cadence sits under both the IMAP 30 min server-side
    // minimum (RFC 3501 §5.4) and the SMTP 5 min receiver timeout (RFC
    // 5321 §4.5.3.2.7), with margin for slow round-trips.
    const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(4 * 60);
    const ACCEPT_POLL: Duration = Duration::from_millis(200);
    let mut last_keepalive = Instant::now();

    let mut s = Spinner::start("Waiting for connection");
    loop {
        let (mut client, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                if last_keepalive.elapsed() >= KEEPALIVE_INTERVAL {
                    conn.set_read_timeout(None)?;
                    if let Err(err) = conn.noop() {
                        warn!("keepalive NOOP failed: {err}");
                        bail!(err);
                    }
                    last_keepalive = Instant::now();
                }
                thread::sleep(ACCEPT_POLL);
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        info!("client connected");
        s.success("Connection established");
        s = Spinner::start("Holding connection");

        match &conn {
            #[cfg(feature = "imap")]
            Session::Imap { capability, .. } => {
                let capability = Vec1::unvalidated(capability.clone());
                let greeting = Greeting::preauth(
                    Some(Code::Capability(capability)),
                    "Sirup IMAP pre-auth session ready",
                )?;

                for fragment in GreetingCodec::new().encode(&greeting) {
                    match fragment {
                        Fragment::Line { data } => client.write_all(&data)?,
                        Fragment::Literal { data, .. } => client.write_all(&data)?,
                    }
                }
            }
            #[cfg(feature = "smtp")]
            Session::Smtp(_) => {
                client.write_all(b"220 Sirup SMTP pre-auth session ready\r\n")?;
            }
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            Session::Invalid => (),
        }

        client.flush()?;

        match proxy(&mut conn, &mut client) {
            Ok(()) => {
                s.failure("Disconnected");
                s = Spinner::start("Waiting for connection");
                info!("client disconnected")
            }
            Err(err) => warn!("proxy error: {err}"),
        }

        // NOTE: real client traffic counts as keepalive, resetting the
        // idle NOOP timer.
        last_keepalive = Instant::now();
    }
}

/// Relays bytes both ways between the client socket and the upstream
/// session until either side closes.
///
/// The upstream is a single stream whose TLS state cannot be touched by
/// two threads at once, so exactly one thread owns it: the pump, which
/// multiplexes non-blocking upstream reads with writes drained from a
/// channel. A second thread only reads the client socket and feeds that
/// channel. No shared lock means neither direction can starve or park the
/// other. The upstream is non-blocking so an idle read never blocks the
/// pump (a TLS read timeout is not reliably surfaced). Scoped threads keep
/// the borrow of the long-lived `server` session local to this call.
fn proxy(server: &mut Session, client: &mut UnixStream) -> Result<()> {
    server.set_nonblocking(true)?;
    // The client is accepted from a non-blocking listener; pin it to
    // blocking so its read parks instead of spinning.
    client.set_nonblocking(false)?;

    let running = AtomicBool::new(true);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let client_reader = client.try_clone()?;
    let client_writer = client.try_clone()?;

    thread::scope(|scope| {
        let reader = scope.spawn(|| client_to_channel(client_reader, tx, &running));
        let pump = upstream_pump(server, rx, client_writer, &running);
        let reader = reader.join().unwrap_or(Ok(()));
        pump.and(reader)
    })
}

/// Reads the client socket (blocking) and forwards each chunk to the pump
/// over `tx`. On close it flips `running`; the pump wakes it back up by
/// shutting the socket down when the upstream closes.
fn client_to_channel(
    mut client: UnixStream,
    tx: mpsc::Sender<Vec<u8>>,
    running: &AtomicBool,
) -> Result<()> {
    let mut buf = [0; 1024 * 8];

    while running.load(Ordering::Relaxed) {
        match client.read(&mut buf) {
            Ok(0) => break,
            Ok(n) if tx.send(buf[..n].to_vec()).is_ok() => {}
            Ok(_) => break,
            Err(ref e) if is_timeout(e) => {}
            Err(e) => {
                running.store(false, Ordering::Relaxed);
                return Err(e.into());
            }
        }
    }

    running.store(false, Ordering::Relaxed);
    Ok(())
}

/// Owns the upstream. Each pass drains any pending upstream bytes to the
/// client, then writes any channel-buffered client bytes to the upstream,
/// sleeping briefly only when both are idle.
fn upstream_pump(
    server: &mut Session,
    rx: mpsc::Receiver<Vec<u8>>,
    mut client: UnixStream,
    running: &AtomicBool,
) -> Result<()> {
    let mut buf = [0; 1024 * 8];
    let mut outcome = Ok(());

    'pump: while running.load(Ordering::Relaxed) {
        let mut idle = true;

        // Upstream -> client (non-blocking; drain what is ready).
        loop {
            match server.read(&mut buf) {
                Ok(0) => break 'pump,
                Ok(n) => match client.write_all(&buf[..n]).and_then(|()| client.flush()) {
                    Ok(()) => idle = false,
                    Err(e) => {
                        outcome = Err(e.into());
                        break 'pump;
                    }
                },
                Err(ref e) if is_timeout(e) => break,
                Err(e) => {
                    outcome = Err(e.into());
                    break 'pump;
                }
            }
        }

        // Client -> upstream (drain the channel).
        loop {
            match rx.try_recv() {
                Ok(chunk) => {
                    if let Err(e) = write_upstream(server, &chunk) {
                        outcome = Err(e);
                        break 'pump;
                    }
                    idle = false;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break 'pump,
            }
        }

        if idle {
            thread::sleep(Duration::from_millis(2));
        }
    }

    // Restore blocking for the idle keepalive NOOP, then wake the client
    // reader parked on its blocking read.
    let _ = server.set_nonblocking(false);
    running.store(false, Ordering::Relaxed);
    let _ = client.shutdown(Shutdown::Both);
    outcome
}

/// Writes `data` to the non-blocking upstream, retrying the `WouldBlock`
/// that a full socket send buffer can raise mid-write.
fn write_upstream(server: &mut Session, mut data: &[u8]) -> Result<()> {
    while !data.is_empty() {
        match server.write(data) {
            Ok(0) => bail!("upstream write returned 0"),
            Ok(n) => data = &data[n..],
            Err(ref e) if is_timeout(e) => thread::sleep(Duration::from_millis(1)),
            Err(e) => return Err(e.into()),
        }
    }

    loop {
        match server.flush() {
            Ok(()) => return Ok(()),
            Err(ref e) if is_timeout(e) => thread::sleep(Duration::from_millis(1)),
            Err(e) => return Err(e.into()),
        }
    }
}

fn is_timeout(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}
