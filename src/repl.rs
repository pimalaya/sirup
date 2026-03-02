use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(windows)]
use uds_windows::UnixStream;

use anyhow::Result;
use io_imap::{
    codec::fragmentizer::{FragmentInfo, Fragmentizer},
    types::core::TagGenerator,
};

use crate::account::Account;

pub fn start(account: Account) -> Result<()> {
    let mut stream = UnixStream::connect(&account.sock_path)?;

    let mut buf = vec![0; 1024 * 8];
    let mut fragmentizer = Fragmentizer::without_max_message_size();
    let mut tag = TagGenerator::new();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

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
