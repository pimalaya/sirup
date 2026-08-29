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

use crate::{config::Connection, protocol::Protocol};
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
#[cfg(feature = "sieve")]
use io_managesieve::{
    client::{ManagesieveClient, ManagesieveClientStd},
    rfc5804::capability::ManagesieveCapabilities,
    session::ManagesieveSessionOpenOptions,
};
#[cfg(feature = "smtp")]
use io_smtp::{
    client::{SmtpClient, SmtpClientStd},
    rfc5321::SmtpEhloDomain,
    session::SmtpSessionOpenOptions,
};
use log::{info, warn};
use pimalaya_cli::spinner::Spinner;
use pimalaya_stream::{retry::Retry, stream::Stream};
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};

/// The tag the ManageSieve keepalive NOOP carries and expects echoed.
#[cfg(feature = "sieve")]
const KEEPALIVE_TAG: &str = "sirup-keepalive";

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
    /// An authenticated ManageSieve session and the capabilities the
    /// upstream last reported, replayed as the synthesized greeting.
    #[cfg(feature = "sieve")]
    Managesieve {
        client: ManagesieveClientStd,
        capabilities: ManagesieveCapabilities,
    },
    /// Placeholder keeping the enum inhabited when no protocol feature is
    /// enabled.
    #[cfg(not(feature = "imap"))]
    #[cfg(not(feature = "smtp"))]
    #[cfg(not(feature = "sieve"))]
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
            #[cfg(feature = "sieve")]
            Self::Managesieve { client, .. } => client.stream.as_any_mut(),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            #[cfg(not(feature = "sieve"))]
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
    ///
    /// The ManageSieve one carries a tag the server echoes back in the
    /// TAG response code, which no other reply on that stream can
    /// carry: an echo that does not match means the reply belongs to
    /// something else and the stream is out of step, so it fails rather
    /// than proxying a desynchronised session to the next client.
    /// Neither IMAP nor SMTP can promise as much.
    pub fn noop(&mut self) -> Result<()> {
        match self {
            #[cfg(feature = "imap")]
            Self::Imap { client, .. } => Ok(client.noop()?),
            #[cfg(feature = "smtp")]
            Self::Smtp(client) => Ok(client.noop()?),
            #[cfg(feature = "sieve")]
            Self::Managesieve { client, .. } => {
                // NOTE: only one NOOP is ever in flight, the keepalive
                // firing from the accept loop with no client attached,
                // so a constant tag identifies it as well as a counter.
                let echoed = client.noop(Some(String::from(KEEPALIVE_TAG)))?;

                match echoed.as_deref() {
                    Some(KEEPALIVE_TAG) | None => Ok(()),
                    Some(tag) => bail!("ManageSieve NOOP echoed tag `{tag}`, stream out of step"),
                }
            }
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            #[cfg(not(feature = "sieve"))]
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
            #[cfg(feature = "sieve")]
            Self::Managesieve { client, .. } => client.stream.read(buf),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            #[cfg(not(feature = "sieve"))]
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
            #[cfg(feature = "sieve")]
            Self::Managesieve { client, .. } => client.stream.write(buf),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            #[cfg(not(feature = "sieve"))]
            Self::Invalid => Ok(0),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(feature = "imap")]
            Self::Imap { client, .. } => client.stream.flush(),
            #[cfg(feature = "smtp")]
            Self::Smtp(client) => client.stream.flush(),
            #[cfg(feature = "sieve")]
            Self::Managesieve { client, .. } => client.stream.flush(),
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            #[cfg(not(feature = "sieve"))]
            Self::Invalid => Ok(()),
        }
    }
}

/// Opens and authenticates the upstream session for `protocol`. Shared
/// by [`open`] and [`test()`].
fn connect(protocol: Protocol, connection: Connection) -> Result<Session> {
    Ok(match protocol {
        #[cfg(feature = "imap")]
        #[cfg(any(
            feature = "rustls-ring",
            feature = "rustls-aws",
            feature = "native-tls"
        ))]
        Protocol::Imap => {
            let opts = ImapSessionOpenOptions {
                starttls: connection.starttls,
                ..Default::default()
            };
            let (client, capability) =
                ImapClientStd::connect(&connection.url, &connection.tls, connection.sasl, opts)?;
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
            let opts = SmtpSessionOpenOptions {
                starttls: connection.starttls,
            };
            let (client, _capabilities) = SmtpClientStd::connect(
                &connection.url,
                &connection.tls,
                domain,
                connection.sasl,
                opts,
            )?;
            Session::Smtp(client)
        }
        #[cfg(feature = "sieve")]
        #[cfg(any(
            feature = "rustls-ring",
            feature = "rustls-aws",
            feature = "native-tls"
        ))]
        Protocol::Sieve => {
            let opts = ManagesieveSessionOpenOptions {
                starttls: connection.starttls,
                allow_cleartext_auth: connection.allow_cleartext_auth,
            };
            let (client, capabilities) = ManagesieveClientStd::connect(
                &connection.url,
                &connection.tls,
                connection.sasl,
                opts,
            )?;
            Session::Managesieve {
                client,
                capabilities,
            }
        }

        #[cfg(not(feature = "imap"))]
        Protocol::Imap => bail!("Missing cargo feature: `imap`"),
        #[cfg(not(feature = "smtp"))]
        Protocol::Smtp => bail!("Missing cargo feature: `smtp`"),
        #[cfg(not(feature = "sieve"))]
        Protocol::Sieve => bail!("Missing cargo feature: `sieve`"),
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
pub fn test(protocol: Protocol, connection: Connection) -> Result<()> {
    let _ = connect(protocol, connection)?;
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
pub fn open(protocol: Protocol, sock_path: PathBuf, connection: Connection) -> Result<Upstream> {
    let spinner = Spinner::start(format!("Opening the {protocol} session"));

    let session = match connect(protocol, connection) {
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

/// Renders the ManageSieve greeting an attached client reads: the
/// capabilities the upstream last reported, then an `OK` completion.
///
/// On ManageSieve the greeting *is* the capability response ([RFC 5804
/// section 1.7]), so what a client reads here is the real thing rather
/// than the invented ready line SMTP has to make do with. The upstream
/// greeting was consumed during connect and is never forwarded.
///
/// STARTTLS and SASL are dropped from the set. Neither is reachable
/// across the socket, the connection being already encrypted and already
/// authenticated, and advertising either invites an attached client to
/// attempt it. OWNER stays: it is how a client reads back the identity
/// the upstream settled on.
///
/// [RFC 5804 section 1.7]: https://www.rfc-editor.org/rfc/rfc5804#section-1.7
#[cfg(feature = "sieve")]
fn managesieve_greeting(capabilities: &ManagesieveCapabilities) -> Vec<u8> {
    const UNREACHABLE: [&str; 2] = ["STARTTLS", "SASL"];

    let mut greeting = Vec::new();

    for capability in &capabilities.capabilities {
        if UNREACHABLE
            .iter()
            .any(|name| capability.name.eq_ignore_ascii_case(name))
        {
            continue;
        }

        greeting.extend_from_slice(&quote(&capability.name));

        if let Some(value) = &capability.value {
            greeting.push(b' ');
            greeting.extend_from_slice(&quote(value));
        }

        greeting.extend_from_slice(b"\r\n");
    }

    greeting.extend_from_slice(b"OK \"Sirup ManageSieve pre-auth session ready\"\r\n");
    greeting
}

/// Renders `value` as a ManageSieve quoted string ([RFC 5804 section
/// 4]), escaping the backslash and the double quote.
///
/// A capability name or value travels on one line by construction, the
/// response grammar being line-based, so the literal form the
/// specification also allows is never needed here. A CR or an LF that
/// reached this far anyway is dropped rather than allowed to forge a
/// line of its own.
///
/// [RFC 5804 section 4]: https://www.rfc-editor.org/rfc/rfc5804#section-4
#[cfg(feature = "sieve")]
fn quote(value: &str) -> Vec<u8> {
    let mut quoted = Vec::with_capacity(value.len() + 2);
    quoted.push(b'"');

    for byte in value.bytes() {
        match byte {
            b'\r' | b'\n' => continue,
            b'\\' | b'"' => quoted.push(b'\\'),
            _ => {}
        }

        quoted.push(byte);
    }

    quoted.push(b'"');
    quoted
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
            #[cfg(feature = "sieve")]
            Session::Managesieve { capabilities, .. } => {
                client.write_all(&managesieve_greeting(capabilities))?;
            }
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            #[cfg(not(feature = "sieve"))]
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

#[cfg(all(test, feature = "sieve"))]
mod tests {
    use io_managesieve::rfc5804::capability::ManagesieveCapability;

    use super::*;

    fn capability(name: &str, value: Option<&str>) -> ManagesieveCapability {
        ManagesieveCapability {
            name: String::from(name),
            value: value.map(String::from),
        }
    }

    fn capabilities() -> ManagesieveCapabilities {
        ManagesieveCapabilities {
            capabilities: vec![
                capability("IMPLEMENTATION", Some("Dovecot Pigeonhole")),
                capability("SIEVE", Some("fileinto reject envelope")),
                capability("SASL", Some("PLAIN SCRAM-SHA-256")),
                capability("STARTTLS", None),
                capability("OWNER", Some("user@example.com")),
                capability("VERSION", Some("1.0")),
            ],
        }
    }

    #[test]
    fn the_greeting_replays_the_capabilities_as_a_response() {
        let greeting = managesieve_greeting(&capabilities());
        let greeting = String::from_utf8(greeting).expect("the greeting is UTF-8");

        assert_eq!(
            greeting,
            concat!(
                "\"IMPLEMENTATION\" \"Dovecot Pigeonhole\"\r\n",
                "\"SIEVE\" \"fileinto reject envelope\"\r\n",
                "\"OWNER\" \"user@example.com\"\r\n",
                "\"VERSION\" \"1.0\"\r\n",
                "OK \"Sirup ManageSieve pre-auth session ready\"\r\n",
            )
        );
    }

    #[test]
    fn the_greeting_drops_what_the_socket_cannot_reach() {
        // NOTE: the connection is already encrypted and already
        // authenticated, so advertising either would only invite an
        // attached client to attempt it. OWNER is the opposite case: it
        // is how a client reads back the identity that was settled on.
        let greeting = managesieve_greeting(&capabilities());
        let greeting = String::from_utf8(greeting).expect("the greeting is UTF-8");

        assert!(!greeting.contains("STARTTLS"));
        assert!(!greeting.contains("SASL"));
        assert!(greeting.contains("OWNER"));
    }

    #[test]
    fn a_capability_cannot_forge_a_line_of_its_own() {
        let capabilities = ManagesieveCapabilities {
            capabilities: vec![capability(
                "IMPLEMENTATION",
                Some("ev\"il\r\nOK \"hijacked"),
            )],
        };
        let greeting = managesieve_greeting(&capabilities);
        let greeting = String::from_utf8(greeting).expect("the greeting is UTF-8");

        // NOTE: the quote is escaped and the CRLF dropped, so the value
        // stays one token of one line and the OK below it is the only
        // completion an attached client reads.
        assert_eq!(
            greeting,
            concat!(
                "\"IMPLEMENTATION\" \"ev\\\"ilOK \\\"hijacked\"\r\n",
                "OK \"Sirup ManageSieve pre-auth session ready\"\r\n",
            )
        );
        assert_eq!(greeting.lines().count(), 2);
    }
}
