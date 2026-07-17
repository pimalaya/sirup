use std::path::PathBuf;

use anyhow::{Result, bail};
use url::Url;

pub fn start(sock_path: PathBuf, url: Url) -> Result<()> {
    match url.scheme() {
        #[cfg(feature = "imap")]
        "imap" | "imaps" => imap::start(sock_path),
        #[cfg(not(feature = "imap"))]
        "imap" | "imaps" => bail!("Missing cargo feature: `imap`"),
        #[cfg(feature = "smtp")]
        "smtp" | "smtps" => smtp::start(sock_path),
        #[cfg(not(feature = "smtp"))]
        "smtp" | "smtps" => bail!("Missing cargo feature: `smtp`"),
        s => bail!("Unknown scheme `{s}`, expects `imap(s)` or `smtp(s)`"),
    }
}

#[cfg(feature = "imap")]
pub mod imap {
    #[cfg(unix)]
    use std::os::unix::net::UnixStream;
    use std::{
        io::{Read, Write, stdin, stdout},
        path::PathBuf,
    };

    use anyhow::Result;
    use io_imap::{
        codec::fragmentizer::{FragmentInfo, Fragmentizer},
        types::core::TagGenerator,
    };
    #[cfg(windows)]
    use uds_windows::UnixStream;

    pub fn start(sock_path: PathBuf) -> Result<()> {
        let mut stream = UnixStream::connect(&sock_path)?;

        let mut buf = vec![0; 1024 * 8];
        let mut fragmentizer = Fragmentizer::without_max_message_size();
        let mut tag = TagGenerator::new();

        let stdin = stdin();
        let mut stdout = stdout();

        loop {
            let n = stream.read(&mut buf)?;

            if n == 0 {
                break;
            }

            fragmentizer.enqueue_bytes(&buf[..n]);

            while let Some(FragmentInfo::Line { .. }) = fragmentizer.progress() {
                let line = String::from_utf8_lossy(fragmentizer.message_bytes());
                print!("S: {line}");
            }

            let tag = tag.generate();

            println!();
            print!("C: {} ", tag.inner());
            stdout.flush()?;

            let mut input = String::new();
            stdin.read_line(&mut input)?;
            let input = input.trim_end();

            stream.write_all(tag.inner().as_bytes())?;
            stream.write_all(b" ")?;
            stream.write_all(input.as_bytes())?;
            stream.write_all(b"\r\n")?;
            stream.flush()?;
        }

        Ok(())
    }
}

#[cfg(feature = "smtp")]
pub mod smtp {
    #[cfg(unix)]
    use std::os::unix::net::UnixStream;
    use std::{
        io::{BufRead, BufReader, Write, stdin, stdout},
        path::PathBuf,
    };

    use anyhow::Result;
    #[cfg(windows)]
    use uds_windows::UnixStream;

    pub fn start(sock_path: PathBuf) -> Result<()> {
        let stream = UnixStream::connect(&sock_path)?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut writer = stream;

        let stdin = stdin();
        let mut stdout = stdout();

        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line)?;
            if n == 0 {
                return Ok(());
            }
            print!("S: {line}");

            // NOTE: a space after the 3-digit code ("220 ") marks the final
            // reply line; a dash ("220-") marks a continuation line.
            if line.len() >= 4 && line.chars().nth(3) == Some(' ') {
                break;
            }
        }

        loop {
            println!();
            print!("C: ");
            stdout.flush()?;

            let mut input = String::new();
            if stdin.read_line(&mut input)? == 0 {
                break;
            }
            let input = input.trim_end();

            if input.is_empty() {
                continue;
            }

            writer.write_all(input.as_bytes())?;
            writer.write_all(b"\r\n")?;
            writer.flush()?;

            loop {
                let mut line = String::new();
                let n = reader.read_line(&mut line)?;
                if n == 0 {
                    return Ok(());
                }
                print!("S: {line}");

                if line.len() >= 4 && line.chars().nth(3) == Some(' ') {
                    break;
                }
            }
        }

        Ok(())
    }
}
