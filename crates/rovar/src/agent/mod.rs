//! Local editor protocol. The transport never edits persisted document files.
pub(crate) mod protocol;
pub(crate) mod transport;

use clap::Parser;
use std::{
    io::{self, BufRead, Read, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(name = "rovar", version, about = "Rovar design editor")]
struct Options {
    /// List local running editor endpoints as JSON.
    #[arg(long, conflicts_with = "connect")]
    list_sessions: bool,
    /// Send JSON requests from stdin to a running editor; emit JSON responses.
    #[arg(long, value_name = "SOCKET")]
    connect: Option<PathBuf>,
}

pub(crate) fn run() {
    let options = Options::parse();
    let result = if options.list_sessions {
        transport::discover(&transport::directory()).map(|sessions| println!("{sessions}"))
    } else if let Some(socket) = options.connect {
        connect(&socket)
    } else {
        crate::app::start();
        Ok(())
    };
    if let Err(error) = result {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn connect(path: &std::path::Path) -> anyhow::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut failed = false;
    while let Some(line) = transport::read_line(&mut input)? {
        let _: protocol::Request = serde_json::from_slice(&line)?;
        // One connection per request keeps idle clients from occupying server slots.
        let mut stream = std::os::unix::net::UnixStream::connect(path)?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(60)))?;
        stream.set_write_timeout(Some(std::time::Duration::from_secs(10)))?;
        stream.write_all(&line)?;
        stream.write_all(b"\n")?;
        let mut response = io::BufReader::new(stream);
        let mut bytes = Vec::new();
        // A capture may carry a bounded PNG; responses are larger than requests.
        let count = response
            .by_ref()
            .take(48 * 1024 * 1024)
            .read_until(b'\n', &mut bytes)?;
        anyhow::ensure!(
            count > 0 && bytes.last() == Some(&b'\n'),
            "Incomplete agent response"
        );
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        failed |= value.get("error").is_some();
        output.write_all(&bytes)?;
        output.flush()?;
    }
    anyhow::ensure!(
        !failed,
        "One or more agent requests failed; see JSON responses"
    );
    Ok(())
}
pub(crate) mod imports;
