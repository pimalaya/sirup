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

use anyhow::Result;
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

use crate::{
    config::AccountConfig,
    stream::{self, Context, Stream},
};

pub fn start(mut config: AccountConfig, sock_path: PathBuf) -> Result<()> {
    let (context, mut server) = stream::connect(&mut config)?;

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
        match &context {
            #[cfg(feature = "imap")]
            Context::Imap(_) => {
                let capability = Vec1::unvalidated(context.imap_capability().into_iter().collect());
                let greeting = Greeting::preauth(
                    Some(Code::Capability(capability)),
                    "Sirup IMAP session ready",
                )?;

                for fragment in GreetingCodec::new().encode(&greeting) {
                    match fragment {
                        Fragment::Line { data } => client.write_all(&data)?,
                        Fragment::Literal { data, .. } => client.write_all(&data)?,
                    }
                }
            }
            #[cfg(feature = "smtp")]
            Context::Smtp(_) => {
                // SMTP greeting: 220 ready
                client.write_all(b"220 Sirup SMTP session ready\r\n")?;
            }
        }

        client.flush()?;

        // Proxy bidirectionally between client and server
        match proxy(&mut server, &mut client) {
            Ok(()) => info!("client disconnected"),
            Err(err) => warn!("proxy error: {err}"),
        }
    }

    let _ = fs::remove_file(&sock_path);
    Ok(())
}

fn proxy(server: &mut Stream, client: &mut UnixStream) -> Result<()> {
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
