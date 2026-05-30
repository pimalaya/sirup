// This file is part of Sirup, a CLI to spawn pre-authenticated IMAP/SMTP
// sessions and expose them via Unix sockets.
//
// Copyright (C) 2026  soywod <pimalaya.org@posteo.net>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

#[cfg(feature = "smtp")]
use std::net::Ipv4Addr;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
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
use io_smtp::{client::SmtpClientStd, rfc5321::types::ehlo_domain::EhloDomain};
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

pub fn start(
    sock_path: PathBuf,
    url: Url,
    tls: Tls,
    starttls: bool,
    sasl: Option<Sasl>,
) -> Result<()> {
    let s = Spinner::start("Starting remote session");
    let mut conn = match url.scheme() {
        #[cfg(feature = "imap")]
        #[cfg(any(
            feature = "rustls-ring",
            feature = "rustls-aws",
            feature = "native-tls"
        ))]
        "imap" | "imaps" => {
            let (client, capability) = ImapClientStd::connect(&url, &tls, starttls, sasl)?;
            Session::Imap { client, capability }
        }
        #[cfg(feature = "smtp")]
        #[cfg(any(
            feature = "rustls-ring",
            feature = "rustls-aws",
            feature = "native-tls"
        ))]
        "smtp" | "smtps" => {
            let domain: EhloDomain<'static> = Ipv4Addr::new(127, 0, 0, 1).into();
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
    };
    s.success("Starting remote session");

    let s = Spinner::start("Binding local socket");

    // Remove stale socket file from a previous run
    if sock_path.exists() {
        fs::remove_file(&sock_path)?;
    }

    if let Some(sock_dir) = sock_path.parent() {
        fs::create_dir_all(sock_dir)?;
    }

    let listener = UnixListener::bind(&sock_path)?;
    listener.set_nonblocking(true)?;
    s.success("Binding local socket");

    // NOOP cadence: under both the IMAP 30 min server-side minimum
    // (RFC 3501 §5.4) and the SMTP 5 min receiver timeout (RFC 5321
    // §4.5.3.2.7), with margin for slow round-trips.
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

        // Send protocol-specific greeting
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
                // SMTP greeting: 220 ready
                client.write_all(b"220 Sirup SMTP pre-auth session ready\r\n")?;
            }
            #[cfg(not(feature = "imap"))]
            #[cfg(not(feature = "smtp"))]
            Session::Invalid => (),
        }

        client.flush()?;

        // Proxy bidirectionally between client and server
        match proxy(&mut conn, &mut client) {
            Ok(()) => {
                s.failure("Disconnected");
                s = Spinner::start("Waiting for connection");
                info!("client disconnected")
            }
            Err(err) => warn!("proxy error: {err}"),
        }

        // Real client traffic counts as keepalive
        last_keepalive = Instant::now();
    }
}

fn proxy(server: &mut Session, client: &mut UnixStream) -> Result<()> {
    let timeout = Some(Duration::from_millis(50));
    server.set_read_timeout(timeout)?;
    client.set_read_timeout(timeout)?;

    let mut buf = [0; 1024 * 8];

    loop {
        // Client -> Server
        match client.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                server.write_all(&buf[..n])?;
                server.flush()?;
            }
            Err(ref e) if is_timeout(e) => {}
            Err(e) => return Err(e.into()),
        }

        // Server -> Client
        match server.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                client.write_all(&buf[..n])?;
                client.flush()?;
            }
            Err(ref e) if is_timeout(e) => {}
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
