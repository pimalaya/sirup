#[cfg(feature = "imap")]
use pimalaya_toolbox::stream::imap::ImapSession;
#[cfg(feature = "smtp")]
use pimalaya_toolbox::stream::smtp::SmtpSession;
use pimalaya_toolbox::{sasl::Sasl, stream::Tls};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    time::Duration,
};
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};

use anyhow::{bail, Result};
#[cfg(feature = "imap")]
use io_imap::{
    codec::{
        encode::{Encoder, Fragment},
        GreetingCodec,
    },
    types::{
        core::Vec1,
        response::{Code, Greeting},
    },
};
use log::{info, warn};
use url::Url;

#[derive(Debug)]
pub enum Session {
    #[cfg(feature = "imap")]
    Imap(ImapSession),
    #[cfg(feature = "smtp")]
    Smtp(SmtpSession),
}

impl Session {
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            #[cfg(feature = "imap")]
            Self::Imap(conn) => conn.stream.set_read_timeout(timeout),
            #[cfg(feature = "smtp")]
            Self::Smtp(conn) => conn.stream.set_read_timeout(timeout),
        }
    }
}

impl Read for Session {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(feature = "imap")]
            Self::Imap(conn) => conn.stream.read(buf),
            #[cfg(feature = "smtp")]
            Self::Smtp(conn) => conn.stream.read(buf),
        }
    }
}

impl Write for Session {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(feature = "imap")]
            Self::Imap(conn) => conn.stream.write(buf),
            #[cfg(feature = "smtp")]
            Self::Smtp(conn) => conn.stream.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(feature = "imap")]
            Self::Imap(conn) => conn.stream.flush(),
            #[cfg(feature = "smtp")]
            Self::Smtp(conn) => conn.stream.flush(),
        }
    }
}

pub fn start(sock_path: PathBuf, url: Url, tls: Tls, starttls: bool, sasl: Sasl) -> Result<()> {
    let mut conn = match url.scheme() {
        #[cfg(feature = "imap")]
        scheme if scheme.eq_ignore_ascii_case("imap") || scheme.eq_ignore_ascii_case("imaps") => {
            Session::Imap(ImapSession::new(url, tls, starttls, sasl)?)
        }
        #[cfg(not(feature = "imap"))]
        scheme if scheme.eq_ignore_ascii_case("imap") || scheme.eq_ignore_ascii_case("imaps") => {
            bail!("Missing cargo feature: `imap`");
        }
        #[cfg(feature = "smtp")]
        scheme if scheme.eq_ignore_ascii_case("smtp") || scheme.eq_ignore_ascii_case("smtps") => {
            Session::Smtp(SmtpSession::new(url, tls, starttls, sasl)?)
        }
        #[cfg(not(feature = "smtp"))]
        scheme if scheme.eq_ignore_ascii_case("smtp") || scheme.eq_ignore_ascii_case("smtps") => {
            bail!("Missing cargo feature: `smtp`");
        }
        scheme => bail!("Invalid URL scheme {scheme}"),
    };

    // Remove stale socket file from a previous run
    if sock_path.exists() {
        fs::remove_file(&sock_path)?;
    }

    if let Some(sock_dir) = sock_path.parent() {
        fs::create_dir_all(sock_dir)?;
    }

    let listener = UnixListener::bind(&sock_path)?;

    for incoming in listener.incoming() {
        let mut client = incoming?;
        info!("client connected");

        // Send protocol-specific greeting
        match &conn {
            #[cfg(feature = "imap")]
            Session::Imap(conn) => {
                let capability =
                    Vec1::unvalidated(conn.context.capability.clone().into_iter().collect());
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
        }

        client.flush()?;

        // Proxy bidirectionally between client and server
        match proxy(&mut conn, &mut client) {
            Ok(()) => info!("client disconnected"),
            Err(err) => warn!("proxy error: {err}"),
        }
    }

    let _ = fs::remove_file(&sock_path);
    Ok(())
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
