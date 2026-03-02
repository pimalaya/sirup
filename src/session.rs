use std::{
    fs,
    io::{self, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    time::Duration,
};

use anyhow::{bail, Result};
use io_imap::{
    codec::{
        encode::{Encoder, Fragment},
        GreetingCodec,
    },
    context::ImapContext,
    coroutines::{authenticate::*, authenticate_anonymous::*, authenticate_plain::*, login::*},
    types::{
        core::Vec1,
        response::{Capability, Code, Greeting},
    },
};
use io_stream::runtimes::std::handle;
use log::{info, warn};

use crate::{
    account::{Account, SaslCandidate, Tls},
    stream::{self, Stream},
};

pub fn start(mut account: Account) -> Result<()> {
    let (mut context, mut server) = connect(&account)?;

    let mut candidates = vec![];
    let ir = context.capability.contains(&Capability::SaslIr);
    for sasl in account.sasl.drain(..) {
        match sasl {
            SaslCandidate::Anonymous { message } => {
                candidates.push(ImapAuthenticateCandidate::Anonymous(
                    ImapAuthenticateAnonymousParams::new(message, ir),
                ));
            }
            SaslCandidate::Login { username, password } => {
                candidates.push(ImapAuthenticateCandidate::Login(ImapLoginParams::new(
                    username, password,
                )?));
            }
            SaslCandidate::Plain {
                authzid,
                authcid,
                passwd,
            } => {
                candidates.push(ImapAuthenticateCandidate::Plain(
                    ImapAuthenticatePlainParams::new(authzid, authcid, passwd, ir),
                ));
            }
        }
    }

    let mut arg = None;
    let mut coroutine = ImapAuthenticate::new(context, candidates);

    loop {
        match coroutine.resume(arg.take()) {
            ImapAuthenticateResult::Io(io) => arg = Some(handle(&mut server, io)?),
            ImapAuthenticateResult::Ok { context: c, .. } => break context = c,
            ImapAuthenticateResult::Err { err, .. } => bail!(err),
        }
    }

    let capability = Vec1::unvalidated(context.capability.into_iter().collect());

    // Remove stale socket file from a previous run
    if account.sock_path.exists() {
        fs::remove_file(&account.sock_path)?;
    }

    let listener = UnixListener::bind(&account.sock_path)?;

    for incoming in listener.incoming() {
        let mut client = incoming?;
        info!("client connected");

        // Send PREAUTH greeting

        let greeting = Greeting::preauth(
            Some(Code::Capability(capability.clone())),
            "Sirup IMAP session ready",
        )?;

        for fragment in GreetingCodec::new().encode(&greeting) {
            match fragment {
                Fragment::Line { data } => client.write_all(&data)?,
                Fragment::Literal { data, .. } => client.write_all(&data)?,
            }
        }

        client.flush()?;

        // Proxy bidirectionally between client and IMAP server
        match proxy(&mut server, &mut client) {
            Ok(()) => info!("client disconnected"),
            Err(err) => warn!("proxy error: {err}"),
        }
    }

    let _ = fs::remove_file(&account.sock_path);
    Ok(())
}

fn connect(account: &Account) -> Result<(ImapContext, Stream)> {
    match &account.tls {
        Tls::None => stream::tcp(&account.host, account.port),
        Tls::Rustls { starttls, cert } => {
            stream::rustls(&account.host, account.port, *starttls, cert.as_deref())
        }
    }
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
