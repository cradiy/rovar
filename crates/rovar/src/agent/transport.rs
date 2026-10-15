use super::protocol::{Error, Request, response};
use anyhow::{Context, ensure};
use futures_channel::mpsc;
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc as sync,
    },
    time::{Duration, Instant},
};

pub(crate) const MAX_REQUEST_BYTES: usize = 24 * 1024 * 1024;
pub(crate) struct Pending {
    pub request: Request,
    pub deadline: Instant,
    pub reply: sync::SyncSender<serde_json::Value>,
}

pub(crate) struct Server {
    pub stop: Arc<AtomicBool>,
    path: PathBuf,
    inode: u64,
    _directory: tempfile::TempDir,
}
impl gpui::Global for Server {}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if std::fs::symlink_metadata(&self.path).is_ok_and(|m| m.ino() == self.inode) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub(crate) fn directory() -> PathBuf {
    dirs::runtime_dir().unwrap_or_else(std::env::temp_dir)
}

pub(crate) fn listen(base: &Path) -> anyhow::Result<(Server, mpsc::Receiver<Pending>)> {
    let directory = tempfile::Builder::new()
        .prefix("rovar-live-")
        .tempdir_in(base)?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    let path = directory.path().join("editor.sock");
    // Never unlink an existing path: it may belong to another running editor.
    let listener = UnixListener::bind(&path)?;
    let guard = Server {
        stop: Arc::new(AtomicBool::new(false)),
        inode: std::fs::symlink_metadata(&path)?.ino(),
        path,
        _directory: directory,
    };
    std::fs::set_permissions(&guard.path, std::fs::Permissions::from_mode(0o600))?;
    std::fs::write(
        guard._directory.path().join("session.json"),
        serde_json::to_vec(&serde_json::json!({"pid":std::process::id(),"socket":guard.path}))?,
    )?;
    listener.set_nonblocking(true)?;
    let stop = guard.stop.clone();
    let (sender, receiver) = mpsc::channel(8);
    let connections = Arc::new(AtomicUsize::new(0));
    std::thread::Builder::new()
        .name("rovar-agent".into())
        .spawn(move || {
            while !stop.load(Ordering::Acquire) && !sender.is_closed() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if connections.load(Ordering::Acquire) >= 8 {
                            continue;
                        }
                        connections.fetch_add(1, Ordering::AcqRel);
                        let count = connections.clone();
                        let sender = sender.clone();
                        let spawned = std::thread::Builder::new()
                            .name("rovar-agent-client".into())
                            .spawn(move || {
                                let _ = serve(stream, sender);
                                count.fetch_sub(1, Ordering::AcqRel);
                            });
                        if spawned.is_err() {
                            connections.fetch_sub(1, Ordering::AcqRel);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(_) => break,
                }
            }
        })?;
    Ok((guard, receiver))
}

pub(crate) fn discover(base: &Path) -> anyhow::Result<serde_json::Value> {
    let mut sessions = Vec::new();
    let entries = match std::fs::read_dir(base) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(serde_json::json!([]));
        }
        Err(error) => return Err(error.into()),
    };
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("rovar-live-")
        {
            continue;
        }
        let path = entry.path();
        if !std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir() && m.mode() & 0o077 == 0) {
            continue;
        }
        let socket = path.join("editor.sock");
        let record = std::fs::File::open(path.join("session.json"))
            .ok()
            .and_then(|file| serde_json::from_reader::<_, serde_json::Value>(file.take(4096)).ok());
        if let Some(pid) = record.and_then(|value| value["pid"].as_u64())
            && UnixStream::connect(&socket).is_ok()
        {
            sessions.push(serde_json::json!({"pid":pid,"socket":socket}));
        }
    }
    sessions.sort_by(|a, b| a["socket"].as_str().cmp(&b["socket"].as_str()));
    Ok(serde_json::json!(sessions))
}

pub(crate) fn read_line(reader: &mut impl BufRead) -> anyhow::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    let length = reader
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if length == 0 {
        return Ok(None);
    }
    ensure!(length <= MAX_REQUEST_BYTES, "Request exceeds 24 MiB");
    ensure!(
        bytes.last() == Some(&b'\n'),
        "Request must end with a newline"
    );
    bytes.pop();
    Ok(Some(bytes))
}

fn serve(mut stream: UnixStream, mut sender: mpsc::Sender<Pending>) -> anyhow::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let line = read_line(&mut BufReader::new(stream.try_clone()?))?.context("Empty request")?;
    let value = match serde_json::from_slice::<Request>(&line) {
        Err(error) => serde_json::json!({"request_id":null,"error":{
            "code":"INVALID_ARGUMENT","message":error.to_string()}}),
        Ok(request) => {
            let (reply, receive) = sync::sync_channel(1);
            if sender
                .try_send(Pending {
                    request: request.clone(),
                    deadline: Instant::now() + Duration::from_secs(45),
                    reply,
                })
                .is_err()
            {
                response(
                    &request,
                    Err(Error::new("BUSY", "Editor request queue is unavailable")),
                )
            } else {
                receive
                    .recv_timeout(Duration::from_secs(50))
                    .unwrap_or_else(|_| {
                        response(
                            &request,
                            Err(Error::new(
                                "TIMEOUT",
                                "Outcome unknown; retry the identical request_id and body",
                            )),
                        )
                    })
            }
        }
    };
    serde_json::to_writer(&mut stream, &value)?;
    stream.write_all(b"\n")?;
    Ok(())
}
