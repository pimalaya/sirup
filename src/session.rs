//! The socket-proxy daemon.
//!
//! [`open`] authenticates one upstream session, and [`serve`] binds a
//! socket per session, replaces each protocol greeting with a
//! pre-authenticated one, then proxies raw bytes both ways while issuing
//! a periodic NOOP to keep the upstream alive during idle.
//!
//! The two halves are separate because an account serves as many
//! protocols as it declares: they are opened one at a time, so their
//! progress reports do not interleave and a failure leaves no socket
//! bound, and only then does each get the thread running its own accept
//! loop.
//!
//! The [`Session`] enum wraps the concrete protocol client and exposes
//! the stream controls the proxy loop needs. [`test()`] reuses the same
//! connect path to validate an account without binding a socket, for the
//! wizard.

#[cfg(feature = "smtp")]
use std::net::Ipv4Addr;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::{
    any::Any,
    fs,
    io::{self, Read, Write},
    net::Shutdown,
    path::{Path, PathBuf},
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
    client::{ImapClient, ImapClientStd},
    codec::{
        GreetingCodec,
        encode::{Encoder, Fragment},
    },
    session::ImapSessionOpenOptions,
    types::{
        core::Vec1,
        response::{Capability, Code, Greeting},
    },
};
use io_sasl::mechanism::Sasl;
#[cfg(feature = "smtp")]
use io_smtp::{
    client::{SmtpClient, SmtpClientStd},
    rfc5321::SmtpEhloDomain,
    session::SmtpSessionOpenOptions,
};
use log::{info, warn};
use pimalaya_cli::spinner::Spinner;
use pimalaya_stream::{retry::Retry, stream::Stream, tls::Tls};
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};
use url::Url;

use crate::protocol::Protocol;

/// An authenticated upstream session, one variant per protocol.
///
/// Wraps the concrete protocol client behind the read, write and stream
/// controls the proxy loop drives. The `Invalid` variant only exists to
/// keep the type inhabited when neither protocol feature is enabled.
pub enum Session {
    /// An authenticated IMAP session and the capabilities the upstream
    /// advertised, replayed in the synthesized PREAUTH greeting.
    #[cfg(feature = "imap")]
    Imap {
        client: ImapClientStd,
        capability: Vec<Capability<'static>>,
    },
    /// An authenticated SMTP submission session.
    #[cfg(feature = "smtp")]
    Smtp(SmtpClientStd),
    /// Placeholder keeping the enum inhabited when no protocol feature is
    /// enabled.
    #[cfg(not(feature = "imap"))]
    #[cfg(not(feature = "smtp"))]
    Invalid,
}

impl Session {
    /// The concrete stream under the protocol client.
    ///
    /// Both clients box it as `Box<dyn ImapStream>` / `Box<dyn
    /// SmtpStream>` to stay transport-agnostic. Sirup opens every stream
    /// through pimalaya-stream, so the concrete type is always [`Stream`]
    /// and the downcast is infallible by construction.
    fn stream(&mut self) -> Option<&mut Stream> {
        let stream: &mut dyn Any = match self {
            #[cfg(feature = "imap")]
            Self::Imap { client, .. } => client.stream.as_any_mut(),
            #[cfg(feature = "smtp")]
            Self::Smtp(client) => client.stream.as_any_mut(),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            Self::Invalid => return None,
        };

        let stream = stream
            .downcast_mut::<Stream>()
            .expect("Sirup stream is always a pimalaya-stream Stream");

        Some(stream)
    }

    /// Sets the read timeout on the underlying authenticated stream.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        match self.stream() {
            Some(stream) => stream.set_read_timeout(timeout),
            None => Ok(()),
        }
    }

    /// Toggles non-blocking mode on the underlying authenticated stream,
    /// and the retry strategy along with it.
    ///
    /// The two are contradictory. A stream retries what a socket reports
    /// as not ready, for a minute by default, while non-blocking mode
    /// exists precisely to surface those failures: the proxy loop reads
    /// one as "nothing to relay this pass". Going back to blocking
    /// restores the default, the keepalive NOOP wanting a stalled read
    /// retried rather than handed back.
    pub fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        let Some(stream) = self.stream() else {
            return Ok(());
        };

        stream.retry = if nonblocking {
            Retry::Never
        } else {
            Retry::default()
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

/// Opens and authenticates the upstream session for `protocol`. Shared
/// by [`open`] and [`test()`].
fn connect(
    protocol: Protocol,
    url: Url,
    tls: Tls,
    starttls: bool,
    sasl: Option<Sasl>,
) -> Result<Session> {
    Ok(match protocol {
        #[cfg(feature = "imap")]
        #[cfg(any(
            feature = "rustls-ring",
            feature = "rustls-aws",
            feature = "native-tls"
        ))]
        Protocol::Imap => {
            let opts = ImapSessionOpenOptions {
                starttls,
                ..Default::default()
            };
            let (client, capability) = ImapClientStd::connect(&url, &tls, sasl, opts)?;
            Session::Imap { client, capability }
        }
        #[cfg(feature = "smtp")]
        #[cfg(any(
            feature = "rustls-ring",
            feature = "rustls-aws",
            feature = "native-tls"
        ))]
        Protocol::Smtp => {
            let domain: SmtpEhloDomain<'static> = Ipv4Addr::new(127, 0, 0, 1).into();
            let opts = SmtpSessionOpenOptions { starttls };
            let (client, _capabilities) = SmtpClientStd::connect(&url, &tls, domain, sasl, opts)?;
            Session::Smtp(client)
        }

        #[cfg(not(feature = "imap"))]
        Protocol::Imap => bail!("Missing cargo feature: `imap`"),
        #[cfg(not(feature = "smtp"))]
        Protocol::Smtp => bail!("Missing cargo feature: `smtp`"),
        #[cfg(not(feature = "rustls-aws"))]
        #[cfg(not(feature = "rustls-ring"))]
        #[cfg(not(feature = "native-tls"))]
        _ => {
            bail!("Missing cargo feature: `rustls-aws`, `rustls-ring` or `native-tls`")
        }
    })
}

/// Connects and authenticates once, then drops the session without
/// binding any socket. Used by the wizard to validate a freshly-built
/// account before handing it back.
#[cfg(discovery)]
pub fn test(
    protocol: Protocol,
    url: Url,
    tls: Tls,
    starttls: bool,
    sasl: Option<Sasl>,
) -> Result<()> {
    let _ = connect(protocol, url, tls, starttls, sasl)?;
    Ok(())
}

/// One protocol of an account, authenticated and waiting to be served.
pub struct Upstream {
    protocol: Protocol,
    sock_path: PathBuf,
    session: Session,
}

/// Opens and authenticates one upstream, without binding anything.
///
/// Opening is separate from serving so a `start` covering several
/// protocols can open them one at a time, reporting on each, and reach
/// [`serve`] with either every session up or none: a failure here leaves
/// no socket bound behind it.
pub fn open(
    protocol: Protocol,
    sock_path: PathBuf,
    url: Url,
    tls: Tls,
    starttls: bool,
    sasl: Option<Sasl>,
) -> Result<Upstream> {
    let spinner = Spinner::start(format!("Opening the {protocol} session"));

    let session = match connect(protocol, url, tls, starttls, sasl) {
        Ok(session) => session,
        Err(err) => {
            spinner.failure(format!("Cannot open the {protocol} session"));
            return Err(err);
        }
    };

    spinner.success(format!("Opened the {protocol} session"));

    Ok(Upstream {
        protocol,
        sock_path,
        session,
    })
}

/// Binds every upstream's socket, then serves them all until one fails.
///
/// The sockets are bound before any of them is served, so a bind failure
/// leaves no half-served daemon behind, and each upstream then gets the
/// thread running its own accept loop and its own keepalive cadence.
///
/// The first failure ends the whole run: a daemon that kept serving its
/// other protocols would leave its supervisor reading the unit as healthy
/// while a part of it is dead, with nothing left to restart it.
pub fn serve(upstreams: Vec<Upstream>) -> Result<()> {
    let mut bound = Vec::with_capacity(upstreams.len());

    for upstream in upstreams {
        let spinner = Spinner::start(format!("Binding the {} socket", upstream.protocol));

        let listener = match bind(&upstream.sock_path) {
            Ok(listener) => listener,
            Err(err) => {
                spinner.failure(format!("Cannot bind the {} socket", upstream.protocol));
                return Err(err);
            }
        };

        spinner.success(format!(
            "Serving {} on {}",
            upstream.protocol,
            upstream.sock_path.display(),
        ));

        bound.push((upstream, listener));
    }

    let running = AtomicBool::new(true);

    thread::scope(|scope| {
        let workers: Vec<_> = bound
            .into_iter()
            .map(|(upstream, listener)| {
                scope.spawn(|| {
                    let outcome = serve_one(upstream, listener, &running);
                    // NOTE: whichever protocol fails first ends the run,
                    // so the others stop polling and the process exits
                    // rather than serving half an account.
                    running.store(false, Ordering::Relaxed);
                    outcome
                })
            })
            .collect();

        workers
            .into_iter()
            .map(|worker| worker.join().unwrap_or(Ok(())))
            .find(Result::is_err)
            .unwrap_or(Ok(()))
    })
}

/// Removes a stale socket, creates the directory holding it, then binds
/// a non-blocking listener on it.
fn bind(sock_path: &Path) -> Result<UnixListener> {
    if sock_path.exists() {
        fs::remove_file(sock_path)?;
    }

    if let Some(sock_dir) = sock_path.parent() {
        fs::create_dir_all(sock_dir)?;
    }

    let listener = UnixListener::bind(sock_path)?;
    listener.set_nonblocking(true)?;

    Ok(listener)
}

/// Serves one upstream: accepts clients one at a time, replaces the
/// protocol greeting with a pre-authenticated one and proxies bytes,
/// keeping the session warm with a NOOP while idle.
///
/// It returns when the upstream fails or when `running` is cleared,
/// which is how a sibling protocol's failure ends this one too.
fn serve_one(mut upstream: Upstream, listener: UnixListener, running: &AtomicBool) -> Result<()> {
    // NOTE: NOOP cadence sits under both the IMAP 30 min server-side
    // minimum (RFC 3501 §5.4) and the SMTP 5 min receiver timeout (RFC
    // 5321 §4.5.3.2.7), with margin for slow round-trips.
    const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(4 * 60);
    const ACCEPT_POLL: Duration = Duration::from_millis(200);

    let protocol = upstream.protocol;
    let conn = &mut upstream.session;
    let mut last_keepalive = Instant::now();

    while running.load(Ordering::Relaxed) {
        let (mut client, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                if last_keepalive.elapsed() >= KEEPALIVE_INTERVAL {
                    conn.set_read_timeout(None)?;
                    if let Err(err) = conn.noop() {
                        warn!("{protocol} keepalive NOOP failed: {err}");
                        bail!(err);
                    }
                    last_keepalive = Instant::now();
                }
                thread::sleep(ACCEPT_POLL);
                continue;
            }
            Err(e) => return Err(e.into()),
        };

        info!("{protocol} client connected");

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

        match proxy(conn, &mut client) {
            Ok(()) => info!("{protocol} client disconnected"),
            Err(err) => warn!("{protocol} proxy error: {err}"),
        }

        // NOTE: real client traffic counts as keepalive, resetting the
        // idle NOOP timer.
        last_keepalive = Instant::now();
    }

    Ok(())
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
    // NOTE: the client is accepted from a non-blocking listener; pin it to
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

    // NOTE: restore blocking for the idle keepalive NOOP, then wake the
    // client reader parked on its blocking read by shutting the socket.
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
