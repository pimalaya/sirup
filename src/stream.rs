use std::{
    fs,
    io::{self, Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, Result};
#[cfg(feature = "imap")]
use io_imap::{
    context::ImapContext,
    coroutines::{
        authenticate::*, authenticate_anonymous::ImapAuthenticateAnonymousParams,
        authenticate_plain::ImapAuthenticatePlainParams, capability::*,
        greeting_with_capability::*, login::ImapLoginParams, starttls::*,
    },
    types::{auth::AuthMechanism, response::Capability as ImapCapability},
};
#[cfg(feature = "smtp")]
use io_smtp::{
    context::SmtpContext,
    coroutines::{authenticate_plain::*, ehlo::*, greeting::*, starttls::*},
    types::core::{Domain, EhloDomain},
};
use io_stream::runtimes::std::handle;
use log::{debug, info};
#[cfg(feature = "native-tls")]
use native_tls::TlsConnector;
#[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
use rustls::{
    crypto::{self, CryptoProvider},
    pki_types::{pem::PemObject, CertificateDer},
    ClientConfig, ClientConnection, StreamOwned,
};
#[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
use rustls_platform_verifier::{ConfigVerifierExt, Verifier};

use crate::config::{AccountConfig, RustlsCryptoConfig, TlsConfig, TlsProviderConfig};

pub enum Stream {
    Imap(TcpStream),
    #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
    Rustls(StreamOwned<ClientConnection, TcpStream>),
    #[cfg(feature = "native-tls")]
    NativeTls(native_tls::TlsStream<TcpStream>),
}

pub enum Context {
    #[cfg(feature = "imap")]
    Imap(ImapContext),
    #[cfg(feature = "smtp")]
    Smtp(SmtpContext),
}

#[cfg(feature = "imap")]
impl Context {
    pub fn imap_capability(&self) -> Vec<ImapCapability<'_>> {
        match self {
            Context::Imap(ctx) => ctx.capability.iter().cloned().collect(),
            #[cfg(feature = "smtp")]
            _ => Vec::new(),
        }
    }
}

impl Stream {
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.set_read_timeout(dur),
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
            Self::Plain(s) => s.read(buf),
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
            Self::Plain(s) => s.write(buf),
            #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
            Self::Rustls(s) => s.write(buf),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
            Self::Rustls(s) => s.flush(),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(s) => s.flush(),
        }
    }
}

/// Selects the TLS provider based on configuration and available features.
fn select_tls_provider(config: &TlsConfig) -> Result<TlsProviderConfig> {
    match config.provider {
        #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
        Some(TlsProviderConfig::Rustls) => Ok(TlsProviderConfig::Rustls),
        #[cfg(not(feature = "rustls-aws"))]
        #[cfg(not(feature = "rustls-ring"))]
        Some(TlsProviderConfig::Rustls) => {
            bail!("Required cargo feature: `rustls-aws` or `rustls-ring`")
        }
        #[cfg(feature = "native-tls")]
        Some(TlsProviderConfig::NativeTls) => Ok(TlsProviderConfig::NativeTls),
        #[cfg(not(feature = "native-tls"))]
        Some(TlsProviderConfig::NativeTls) => {
            bail!("Required cargo feature: `native-tls`")
        }
        #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
        None => Ok(TlsProviderConfig::Rustls),
        #[cfg(not(feature = "rustls-aws"))]
        #[cfg(not(feature = "rustls-ring"))]
        #[cfg(feature = "native-tls")]
        None => Ok(TlsProviderConfig::NativeTls),
        #[cfg(not(feature = "rustls-aws"))]
        #[cfg(not(feature = "rustls-ring"))]
        #[cfg(not(feature = "native-tls"))]
        None => {
            bail!("Required cargo feature: `rustls-aws`, `rustls-ring` or `native-tls`")
        }
    }
}

/// Gets the rustls crypto provider based on configuration and available features.
#[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
fn get_crypto_provider(config: &TlsConfig) -> Result<Arc<CryptoProvider>> {
    let crypto_config = match config.rustls.crypto {
        #[cfg(feature = "rustls-aws")]
        Some(RustlsCryptoConfig::Aws) => RustlsCryptoConfig::Aws,
        #[cfg(not(feature = "rustls-aws"))]
        Some(RustlsCryptoConfig::Aws) => {
            bail!("Required cargo feature: `rustls-aws`");
        }
        #[cfg(feature = "rustls-ring")]
        Some(RustlsCryptoConfig::Ring) => RustlsCryptoConfig::Ring,
        #[cfg(not(feature = "rustls-ring"))]
        Some(RustlsCryptoConfig::Ring) => {
            bail!("Required cargo feature: `rustls-ring`");
        }
        #[cfg(feature = "rustls-ring")]
        None => RustlsCryptoConfig::Ring,
        #[cfg(not(feature = "rustls-ring"))]
        #[cfg(feature = "rustls-aws")]
        None => RustlsCryptoConfig::Aws,
        #[cfg(not(feature = "rustls-aws"))]
        #[cfg(not(feature = "rustls-ring"))]
        None => {
            bail!("Required cargo feature: `rustls-aws` or `rustls-ring`");
        }
    };

    debug!("using rustls crypto provider: {crypto_config:?}");

    let provider = match crypto_config {
        #[cfg(feature = "rustls-aws")]
        RustlsCryptoConfig::Aws => crypto::aws_lc_rs::default_provider(),
        #[cfg(feature = "rustls-ring")]
        RustlsCryptoConfig::Ring => crypto::ring::default_provider(),
        #[allow(unreachable_patterns)]
        _ => unreachable!(),
    };

    Ok(match provider.install_default() {
        Ok(()) => CryptoProvider::get_default().unwrap().clone(),
        Err(provider) => provider,
    })
}

/// Wraps a TCP stream with TLS encryption.
fn wrap_with_tls(tcp: TcpStream, host: &str, tls_config: &TlsConfig) -> Result<Stream> {
    let tls_provider = select_tls_provider(tls_config)?;
    debug!("using TLS provider: {tls_provider:?}");

    match tls_provider {
        #[cfg(any(feature = "rustls-aws", feature = "rustls-ring"))]
        TlsProviderConfig::Rustls => {
            let crypto_provider = get_crypto_provider(tls_config)?;

            let client_config = if let Some(pem_path) = &tls_config.cert {
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

            let server_name = host.to_string().try_into()?;
            let conn = ClientConnection::new(Arc::new(client_config), server_name)?;
            Ok(Stream::Rustls(StreamOwned::new(conn, tcp)))
        }
        #[cfg(feature = "native-tls")]
        TlsProviderConfig::NativeTls => {
            let mut builder = TlsConnector::builder();

            if let Some(pem_path) = &tls_config.cert {
                debug!("using TLS cert at {}", pem_path.display());
                let pem = fs::read(pem_path)?;
                let cert = native_tls::Certificate::from_pem(&pem)?;
                builder.add_root_certificate(cert);
            }

            let connector = builder.build()?;
            Ok(Stream::NativeTls(connector.connect(host, tcp)?))
        }
        #[allow(unreachable_patterns)]
        _ => unreachable!(),
    }
}

#[cfg(feature = "smtp")]
fn make_ehlo_domain(host: &str) -> EhloDomain<'static> {
    Domain::try_from(host.to_string())
        .unwrap_or_else(|_| Domain::try_from("localhost".to_string()).unwrap())
        .into()
}

pub fn connect(config: &mut AccountConfig) -> Result<(Context, Stream)> {
    info!("connecting to server using {}", config.url);

    let host = config.url.host_str().unwrap_or("127.0.0.1");

    let (mut context, mut stream) = match config.url.scheme() {
        #[cfg(feature = "imap")]
        scheme if scheme.eq_ignore_ascii_case("imap") => {
            let mut context = ImapContext::new();
            let port = config.url.port().unwrap_or(143);
            let mut stream = TcpStream::connect((host, port))?;

            let mut coroutine = GetImapGreetingWithCapability::new(context);
            let mut arg = None;

            loop {
                match coroutine.resume(arg.take()) {
                    GetImapGreetingWithCapabilityResult::Io { io } => {
                        arg = Some(handle(&mut stream, io)?)
                    }
                    GetImapGreetingWithCapabilityResult::Ok { context: c } => break context = c,
                    GetImapGreetingWithCapabilityResult::Err { err, .. } => Err(err)?,
                }
            }

            (Context::Imap(context), Stream::Plain(stream))
        }
        #[cfg(feature = "imap")]
        scheme if scheme.eq_ignore_ascii_case("imaps") => {
            let mut context = ImapContext::new();
            let port = config.url.port().unwrap_or(993);
            let mut tcp = TcpStream::connect((host, port))?;

            if config.starttls {
                let mut coroutine = ImapStartTls::new(context);
                let mut arg = None;

                loop {
                    match coroutine.resume(arg.take()) {
                        ImapStartTlsResult::Io { io } => arg = Some(handle(&mut tcp, io)?),
                        ImapStartTlsResult::Ok { context: c } => break context = c,
                        ImapStartTlsResult::Err { err, .. } => Err(err)?,
                    }
                }
            }

            let mut stream = wrap_with_tls(tcp, host, &config.tls)?;

            if config.starttls {
                let mut coroutine = GetImapCapability::new(context);
                let mut arg = None;

                loop {
                    match coroutine.resume(arg.take()) {
                        GetImapCapabilityResult::Io { io } => arg = Some(handle(&mut stream, io)?),
                        GetImapCapabilityResult::Ok { context: c } => break context = c,
                        GetImapCapabilityResult::Err { err, .. } => Err(err)?,
                    }
                }
            } else {
                let mut coroutine = GetImapGreetingWithCapability::new(context);
                let mut arg = None;

                loop {
                    match coroutine.resume(arg.take()) {
                        GetImapGreetingWithCapabilityResult::Io { io } => {
                            arg = Some(handle(&mut stream, io)?)
                        }
                        GetImapGreetingWithCapabilityResult::Ok { context: c } => {
                            break context = c
                        }
                        GetImapGreetingWithCapabilityResult::Err { err, .. } => Err(err)?,
                    }
                }
            }

            (Context::Imap(context), stream)
        }
        #[cfg(feature = "smtp")]
        scheme if scheme.eq_ignore_ascii_case("smtp") => {
            let mut context = SmtpContext::new();
            let port = config.url.port().unwrap_or(25);
            let mut stream = TcpStream::connect((host, port))?;

            // Get greeting
            let mut coroutine = GetSmtpGreeting::new(context);
            let mut arg = None;

            loop {
                match coroutine.resume(arg.take()) {
                    GetSmtpGreetingResult::Io { io } => arg = Some(handle(&mut stream, io)?),
                    GetSmtpGreetingResult::Ok { context: c, .. } => break context = c,
                    GetSmtpGreetingResult::Err { err, .. } => Err(err)?,
                }
            }

            // Send EHLO
            let domain = make_ehlo_domain(host);
            let mut coroutine = SmtpEhlo::new(context, domain);
            let mut arg = None;

            loop {
                match coroutine.resume(arg.take()) {
                    SmtpEhloResult::Io { io } => arg = Some(handle(&mut stream, io)?),
                    SmtpEhloResult::Ok { context: c, .. } => break context = c,
                    SmtpEhloResult::Err { err, .. } => Err(err)?,
                }
            }

            (Context::Smtp(context), Stream::Plain(stream))
        }
        #[cfg(feature = "smtp")]
        scheme if scheme.eq_ignore_ascii_case("smtps") => {
            let mut context = SmtpContext::new();
            let port = config.url.port().unwrap_or(465);
            let mut tcp = TcpStream::connect((host, port))?;

            if config.starttls {
                // Get greeting first
                let mut coroutine = GetSmtpGreeting::new(context);
                let mut arg = None;

                loop {
                    match coroutine.resume(arg.take()) {
                        GetSmtpGreetingResult::Io { io } => arg = Some(handle(&mut tcp, io)?),
                        GetSmtpGreetingResult::Ok { context: c, .. } => break context = c,
                        GetSmtpGreetingResult::Err { err, .. } => Err(err)?,
                    }
                }

                // Send EHLO before STARTTLS
                let domain = make_ehlo_domain(host);
                let mut coroutine = SmtpEhlo::new(context, domain);
                let mut arg = None;

                loop {
                    match coroutine.resume(arg.take()) {
                        SmtpEhloResult::Io { io } => arg = Some(handle(&mut tcp, io)?),
                        SmtpEhloResult::Ok { context: c, .. } => break context = c,
                        SmtpEhloResult::Err { err, .. } => Err(err)?,
                    }
                }

                // STARTTLS
                let mut coroutine = SmtpStartTls::new(context);
                let mut arg = None;

                loop {
                    match coroutine.resume(arg.take()) {
                        SmtpStartTlsResult::Io { io } => arg = Some(handle(&mut tcp, io)?),
                        SmtpStartTlsResult::Ok { context: c } => break context = c,
                        SmtpStartTlsResult::Err { err, .. } => Err(err)?,
                    }
                }
            }

            let mut stream = wrap_with_tls(tcp, host, &config.tls)?;

            if config.starttls {
                // Send EHLO again after TLS
                let domain = make_ehlo_domain(host);
                let mut coroutine = SmtpEhlo::new(context, domain);
                let mut arg = None;

                loop {
                    match coroutine.resume(arg.take()) {
                        SmtpEhloResult::Io { io } => arg = Some(handle(&mut stream, io)?),
                        SmtpEhloResult::Ok { context: c, .. } => break context = c,
                        SmtpEhloResult::Err { err, .. } => Err(err)?,
                    }
                }
            } else {
                // Get greeting
                let mut coroutine = GetSmtpGreeting::new(context);
                let mut arg = None;

                loop {
                    match coroutine.resume(arg.take()) {
                        GetSmtpGreetingResult::Io { io } => arg = Some(handle(&mut stream, io)?),
                        GetSmtpGreetingResult::Ok { context: c, .. } => break context = c,
                        GetSmtpGreetingResult::Err { err, .. } => Err(err)?,
                    }
                }

                // Send EHLO
                let domain = make_ehlo_domain(host);
                let mut coroutine = SmtpEhlo::new(context, domain);
                let mut arg = None;

                loop {
                    match coroutine.resume(arg.take()) {
                        SmtpEhloResult::Io { io } => arg = Some(handle(&mut stream, io)?),
                        SmtpEhloResult::Ok { context: c, .. } => break context = c,
                        SmtpEhloResult::Err { err, .. } => Err(err)?,
                    }
                }
            }

            (Context::Smtp(context), stream)
        }
        scheme if scheme.eq_ignore_ascii_case("unix") => {
            todo!()
        }
        scheme => {
            bail!("Unknown scheme {scheme}, expected imap, imaps, smtp, smtps or unix");
        }
    };

    // Protocol-specific authentication
    match &mut context {
        #[cfg(feature = "imap")]
        Context::Imap(ref mut imap_ctx) => {
            if !imap_ctx.authenticated {
                let mut candidates = vec![];

                let ir = imap_ctx.capability.contains(&ImapCapability::SaslIr);

                for mechanism in config.sasl.mechanisms.drain(..) {
                    match mechanism {
                        crate::config::SaslMechanismConfig::Login => {
                            let Some(auth) = config.sasl.login.take() else {
                                debug!("missing SASL LOGIN configuration, skipping it");
                                continue;
                            };

                            if imap_ctx.capability.contains(&ImapCapability::LoginDisabled) {
                                debug!("SASL LOGIN disabled by the server, skipping it");
                                continue;
                            }

                            let login = ImapCapability::Auth(AuthMechanism::Login);
                            if !imap_ctx.capability.contains(&login) {
                                debug!("SASL LOGIN disabled by the server, skipping it");
                                continue;
                            }

                            candidates.push(ImapAuthenticateCandidate::Login(
                                ImapLoginParams::new(auth.username, auth.password.get()?)?,
                            ));
                        }
                        crate::config::SaslMechanismConfig::Plain => {
                            let Some(auth) = config.sasl.plain.take() else {
                                debug!("missing SASL PLAIN configuration, skipping it");
                                continue;
                            };

                            let plain = ImapCapability::Auth(AuthMechanism::Plain);
                            if !imap_ctx.capability.contains(&plain) {
                                debug!("SASL PLAIN disabled by the server, skipping it");
                                continue;
                            }

                            candidates.push(ImapAuthenticateCandidate::Plain(
                                ImapAuthenticatePlainParams::new(
                                    auth.authzid,
                                    auth.authcid,
                                    auth.passwd.get()?,
                                    ir,
                                ),
                            ));
                        }
                        crate::config::SaslMechanismConfig::Anonymous => {
                            let message = config
                                .sasl
                                .anonymous
                                .take()
                                .and_then(|auth| auth.message)
                                .unwrap_or_default();

                            candidates.push(ImapAuthenticateCandidate::Anonymous(
                                ImapAuthenticateAnonymousParams::new(message, ir),
                            ));
                        }
                    };
                }

                if !candidates.is_empty() {
                    let mut arg = None;
                    let ctx = std::mem::replace(imap_ctx, ImapContext::new());
                    let mut coroutine = ImapAuthenticate::new(ctx, candidates);

                    loop {
                        match coroutine.resume(arg.take()) {
                            ImapAuthenticateResult::Io { io } => {
                                arg = Some(handle(&mut stream, io)?)
                            }
                            ImapAuthenticateResult::Ok { context: c, .. } => {
                                *imap_ctx = c;
                                break;
                            }
                            ImapAuthenticateResult::Err { err, .. } => bail!(err),
                        }
                    }
                }
            }
        }
        #[cfg(feature = "smtp")]
        Context::Smtp(ref mut smtp_ctx) => {
            if !smtp_ctx.authenticated {
                // Try SASL PLAIN authentication if configured
                if let Some(auth) = config.sasl.plain.take() {
                    debug!("attempting SMTP AUTH PLAIN");

                    let ctx = std::mem::replace(smtp_ctx, SmtpContext::new());
                    let mut coroutine =
                        SmtpAuthenticatePlain::new(ctx, &auth.authcid, &auth.passwd.get()?);
                    let mut arg = None;

                    loop {
                        match coroutine.resume(arg.take()) {
                            SmtpAuthenticatePlainResult::Io { io } => {
                                arg = Some(handle(&mut stream, io)?)
                            }
                            SmtpAuthenticatePlainResult::Ok { context: c } => {
                                *smtp_ctx = c;
                                break;
                            }
                            SmtpAuthenticatePlainResult::Err { err, .. } => bail!(err),
                        }
                    }
                } else if let Some(auth) = config.sasl.login.take() {
                    // Fallback to LOGIN-style auth using PLAIN mechanism
                    debug!("attempting SMTP AUTH PLAIN with login credentials");

                    let ctx = std::mem::replace(smtp_ctx, SmtpContext::new());
                    let mut coroutine =
                        SmtpAuthenticatePlain::new(ctx, &auth.username, &auth.password.get()?);
                    let mut arg = None;

                    loop {
                        match coroutine.resume(arg.take()) {
                            SmtpAuthenticatePlainResult::Io { io } => {
                                arg = Some(handle(&mut stream, io)?)
                            }
                            SmtpAuthenticatePlainResult::Ok { context: c } => {
                                *smtp_ctx = c;
                                break;
                            }
                            SmtpAuthenticatePlainResult::Err { err, .. } => bail!(err),
                        }
                    }
                }
            }
        }
    }

    Ok((context, stream))
}
