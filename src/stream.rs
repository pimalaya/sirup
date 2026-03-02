use std::{
    fs,
    io::{self, Read, Write},
    net::TcpStream,
    path::Path,
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, Result};
use io_imap::{
    context::ImapContext,
    coroutines::{capability::*, greeting_with_capability::*, starttls::*},
};
use io_stream::runtimes::std::handle;
use log::{debug, info};
use rustls::crypto::CryptoProvider;
use rustls::{
    pki_types::{pem::PemObject, CertificateDer},
    ClientConfig, ClientConnection, StreamOwned,
};
use rustls_platform_verifier::{ConfigVerifierExt, Verifier};

pub enum Stream {
    Tcp(TcpStream),
    Rustls(StreamOwned<ClientConnection, TcpStream>),
}

impl Stream {
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Tcp(s) => s.set_read_timeout(dur),
            Self::Rustls(s) => s.get_ref().set_read_timeout(dur),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(s) => s.read(buf),
            Self::Rustls(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(s) => s.write(buf),
            Self::Rustls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tcp(s) => s.flush(),
            Self::Rustls(s) => s.flush(),
        }
    }
}

pub fn tcp(host: &str, port: u16) -> Result<(ImapContext, Stream)> {
    info!("connecting to {host}:{port} using TCP");

    let mut context = ImapContext::new();
    let mut tcp = TcpStream::connect((host, port))?;

    let mut coroutine = GetImapGreetingWithCapability::new(context);
    let mut arg = None;

    loop {
        match coroutine.resume(arg.take()) {
            GetImapGreetingWithCapabilityResult::Ok(out) => break context = out.context,
            GetImapGreetingWithCapabilityResult::Io(io) => arg = Some(handle(&mut tcp, io)?),
            GetImapGreetingWithCapabilityResult::Err(err) => Err(err)?,
        }
    }

    Ok((context, Stream::Tcp(tcp)))
}

pub fn rustls(
    host: &str,
    port: u16,
    starttls: bool,
    cert: Option<&Path>,
) -> Result<(ImapContext, Stream)> {
    info!(
        "connecting to {host}:{port} using {}",
        if starttls { "StartTLS" } else { "TLS" }
    );

    let mut context = ImapContext::new();
    let mut tcp = TcpStream::connect((host, port))?;

    if starttls {
        let mut coroutine = ImapStartTls::new(context);
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                ImapStartTlsResult::Ok(out) => break context = out.context,
                ImapStartTlsResult::Io(io) => arg = Some(handle(&mut tcp, io)?),
                ImapStartTlsResult::Err(err) => Err(err)?,
            }
        }
    }

    let mut config = if let Some(pem_path) = cert {
        debug!("using TLS cert at {}", pem_path.display());
        let pem = fs::read(pem_path)?;

        let Some(cert) = CertificateDer::pem_slice_iter(&pem).next() else {
            bail!("empty TLS cert at {}", pem_path.display())
        };

        let verifier = Verifier::new_with_extra_roots(vec![cert?], crypto_provider()?)?;

        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth()
    } else {
        debug!("using OS TLS certs");
        ClientConfig::with_platform_verifier()?
    };

    config.alpn_protocols = vec![b"imap".to_vec()];

    let server_name = host.to_string().try_into()?;
    let conn = ClientConnection::new(Arc::new(config), server_name)?;
    let mut tls = StreamOwned::new(conn, tcp);

    if starttls {
        let mut coroutine = GetImapCapability::new(context);
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                GetImapCapabilityResult::Ok { context: c } => break context = c,
                GetImapCapabilityResult::Io(io) => arg = Some(handle(&mut tls, io)?),
                GetImapCapabilityResult::Err { err, .. } => Err(err)?,
            }
        }
    } else {
        let mut coroutine = GetImapGreetingWithCapability::new(context);
        let mut arg = None;

        loop {
            match coroutine.resume(arg.take()) {
                GetImapGreetingWithCapabilityResult::Ok(out) => break context = out.context,
                GetImapGreetingWithCapabilityResult::Io(io) => arg = Some(handle(&mut tls, io)?),
                GetImapGreetingWithCapabilityResult::Err(err) => Err(err)?,
            }
        }
    };

    Ok((context, Stream::Rustls(tls)))
}

#[cfg(feature = "rustls-ring")]
pub fn crypto_provider() -> Result<Arc<CryptoProvider>> {
    Ok(Arc::new(rustls::crypto::ring::default_provider()))
}

#[cfg(not(feature = "rustls-ring"))]
#[cfg(feature = "rustls-aws")]
pub fn crypto_provider() -> Result<Arc<CryptoProvider>> {
    Ok(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
}

#[cfg(not(feature = "rustls-ring"))]
#[cfg(not(feature = "rustls-aws"))]
pub fn crypto_provider() -> Result<Arc<CryptoProvider>> {
    bail!("Missing one of `rustls-ring` or `rustls-aws` cargo feature")
}
