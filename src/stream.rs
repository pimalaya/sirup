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
#[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
use rustls::{
    crypto::CryptoProvider,
    pki_types::{pem::PemObject, CertificateDer},
    ClientConfig, ClientConnection, StreamOwned,
};
#[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
use rustls_platform_verifier::{ConfigVerifierExt, Verifier};

pub enum Stream {
    Tcp(TcpStream),
    #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
    Rustls(StreamOwned<ClientConnection, TcpStream>),
    #[cfg(feature = "native-tls")]
    NativeTls(native_tls::TlsStream<TcpStream>),
}

impl Stream {
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Tcp(s) => s.set_read_timeout(dur),
            #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
            Self::Rustls(s) => s.get_ref().set_read_timeout(dur),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(s) => s.get_ref().set_read_timeout(dur),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(s) => s.read(buf),
            #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
            Self::Rustls(s) => s.read(buf),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(s) => s.write(buf),
            #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
            Self::Rustls(s) => s.write(buf),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tcp(s) => s.flush(),
            #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
            Self::Rustls(s) => s.flush(),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(s) => s.flush(),
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

#[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
pub fn rustls(
    host: &str,
    port: u16,
    starttls: bool,
    cert: Option<&Path>,
    crypto_provider: CryptoProvider,
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

    let crypto_provider = match crypto_provider.install_default() {
        Ok(()) => CryptoProvider::get_default().unwrap().clone(),
        Err(crypto_provider) => crypto_provider,
    };

    let mut config = if let Some(pem_path) = cert {
        debug!("using TLS cert at {}", pem_path.display());
        let pem = fs::read(pem_path)?;

        let Some(cert) = CertificateDer::pem_slice_iter(&pem).next() else {
            bail!("empty TLS cert at {}", pem_path.display())
        };

        let verifier = Verifier::new_with_extra_roots(vec![cert?], crypto_provider)?;

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

#[cfg(feature = "native-tls")]
pub fn native_tls(
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

    let mut builder = native_tls::TlsConnector::builder();

    if let Some(pem_path) = cert {
        debug!("using TLS cert at {}", pem_path.display());
        let pem = fs::read(pem_path)?;
        let cert = native_tls::Certificate::from_pem(&pem)?;
        builder.add_root_certificate(cert);
    }

    let connector = builder.build()?;
    let mut tls = connector.connect(host, tcp)?;

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

    Ok((context, Stream::NativeTls(tls)))
}
