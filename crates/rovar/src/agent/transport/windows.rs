use super::{Pending, serve};
use anyhow::{Context, ensure};
use futures_channel::mpsc;
use std::{
    io::{self, Read, Write},
    os::windows::io::{FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions},
    runtime::{Builder, Runtime},
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

const PREFIX: &str = r"\\.\pipe\rovar-";

pub(crate) struct Server {
    pub stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    _directory: tempfile::TempDir,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(crate) fn directory() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Rovar/agent")
}

fn runtime() -> io::Result<Arc<Runtime>> {
    Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map(Arc::new)
}

// Both ends use overlapped I/O. Timeouts cancel pending reads/writes instead of
// leaving blocking Win32 calls alive after a client stops responding.
trait Pipe: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Pipe for T {}

pub(crate) struct Stream {
    pipe: Box<dyn Pipe>,
    runtime: Arc<Runtime>,
    read_timeout: Duration,
}

impl Read for Stream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.runtime.block_on(async {
            tokio::time::timeout(self.read_timeout, self.pipe.read(bytes))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Pipe read timed out"))?
        })
    }
}

impl Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), self.pipe.write(bytes))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Pipe write timed out"))?
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn connect(path: &Path) -> anyhow::Result<Stream> {
    let name = path.to_str().context("Invalid pipe endpoint")?;
    ensure!(
        name.strip_prefix(PREFIX)
            .is_some_and(|suffix| !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')),
        "Expected a local Rovar pipe endpoint"
    );
    let runtime = runtime()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let pipe = loop {
        let result = {
            let _entered = runtime.enter();
            // Tokio uses SECURITY_IDENTIFICATION, preventing server impersonation.
            ClientOptions::new().open(path)
        };
        match result {
            Ok(pipe) => break pipe,
            Err(error) if error.raw_os_error() == Some(231) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error.into()),
        }
    };
    Ok(Stream {
        pipe: Box::new(pipe),
        runtime,
        read_timeout: Duration::from_secs(60),
    })
}

struct LocalAllocation(*mut core::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: These allocations are returned by the Windows conversion APIs.
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn create_pipe(path: &Path, first: bool) -> anyhow::Result<NamedPipeServer> {
    // SAFETY: All output buffers are initialized, correctly aligned and kept alive
    // through the calls. OwnedHandle/LocalAllocation release each Windows allocation.
    unsafe {
        let mut token = std::ptr::null_mut();
        ensure!(
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) != 0,
            "OpenProcessToken: {}",
            io::Error::last_os_error()
        );
        let _token = OwnedHandle::from_raw_handle(token);
        let mut size = 0;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
        ensure!(
            size > 0,
            "GetTokenInformation: {}",
            io::Error::last_os_error()
        );
        let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
        ensure!(
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                size,
                &mut size
            ) != 0,
            "GetTokenInformation: {}",
            io::Error::last_os_error()
        );
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut sid = std::ptr::null_mut();
        ensure!(
            ConvertSidToStringSidW(user.User.Sid, &mut sid) != 0,
            "ConvertSidToStringSid: {}",
            io::Error::last_os_error()
        );
        let _sid = LocalAllocation(sid.cast());
        let mut length = 0;
        while *sid.add(length) != 0 {
            length += 1;
        }
        let sid = String::from_utf16(std::slice::from_raw_parts(sid, length))?;
        // Protected DACL: only the current user gets access, without inherited ACEs.
        let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = std::ptr::null_mut();
        ensure!(
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut()
            ) != 0,
            "Invalid pipe security descriptor: {}",
            io::Error::last_os_error()
        );
        let _descriptor = LocalAllocation(descriptor);
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        Ok(ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                path,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
            )?)
    }
}

pub(crate) fn listen(base: &Path) -> anyhow::Result<(Server, mpsc::Receiver<Pending>)> {
    std::fs::create_dir_all(base)?;
    let directory = tempfile::Builder::new()
        .prefix("rovar-live-")
        .tempdir_in(base)?;
    let endpoint = PathBuf::from(format!(
        "{PREFIX}{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let runtime = runtime()?;
    let mut listener = {
        let _entered = runtime.enter();
        create_pipe(&endpoint, true)?
    };
    std::fs::write(
        directory.path().join("session.json"),
        serde_json::to_vec(&serde_json::json!({"pid":std::process::id(),"endpoint":endpoint}))?,
    )?;
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let (sender, receiver) = mpsc::channel(8);
    let worker = std::thread::Builder::new()
        .name("rovar-agent".into())
        .spawn(move || {
            let connections = Arc::new(AtomicUsize::new(0));
            while !stopping.load(Ordering::Acquire) && !sender.is_closed() {
                let accepted = runtime.block_on(async {
                    tokio::time::timeout(Duration::from_millis(20), listener.connect()).await
                });
                let connected = match accepted {
                    Err(_) => continue,
                    // Discovery can close its probe before ConnectNamedPipe runs.
                    // ERROR_NO_DATA belongs to that client, not to the listener.
                    Ok(Err(error)) if error.raw_os_error() == Some(232) => false,
                    Ok(Err(_)) => break,
                    Ok(Ok(())) => true,
                };
                // Create the next instance before handing off the connected one, so
                // the name remains reserved and concurrent clients can keep connecting.
                let next = {
                    let _entered = runtime.enter();
                    create_pipe(&endpoint, false)
                };
                let Ok(next) = next else {
                    break;
                };
                let pipe = std::mem::replace(&mut listener, next);
                if !connected || connections.load(Ordering::Acquire) >= 8 {
                    continue;
                }
                connections.fetch_add(1, Ordering::AcqRel);
                let count = connections.clone();
                let sender = sender.clone();
                let mut stream = Stream {
                    pipe: Box::new(pipe),
                    runtime: runtime.clone(),
                    read_timeout: Duration::from_secs(10),
                };
                if std::thread::Builder::new()
                    .name("rovar-agent-client".into())
                    .spawn(move || {
                        if serve(&mut stream, sender).is_ok() {
                            // Closing a server pipe can discard unread output. Keep
                            // it alive until the client consumes its reply and closes.
                            let _ = stream.read(&mut [0]);
                        }
                        count.fetch_sub(1, Ordering::AcqRel);
                    })
                    .is_err()
                {
                    connections.fetch_sub(1, Ordering::AcqRel);
                }
            }
        })?;
    Ok((
        Server {
            stop,
            worker: Some(worker),
            _directory: directory,
        },
        receiver,
    ))
}

pub(crate) fn discover(base: &Path) -> anyhow::Result<serde_json::Value> {
    let entries = match std::fs::read_dir(base) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(serde_json::json!([])),
        Err(error) => return Err(error.into()),
    };
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("rovar-live-")
            || !entry.file_type().is_ok_and(|kind| kind.is_dir())
        {
            continue;
        }
        let record = std::fs::File::open(entry.path().join("session.json"))
            .ok()
            .and_then(|file| serde_json::from_reader::<_, serde_json::Value>(file.take(4096)).ok());
        if let Some(record) = record
            && let (Some(pid), Some(endpoint)) =
                (record["pid"].as_u64(), record["endpoint"].as_str())
            && connect(Path::new(endpoint)).is_ok()
        {
            sessions.push(serde_json::json!({"pid":pid,"endpoint":endpoint}));
        }
    }
    sessions.sort_by(|a, b| a["endpoint"].as_str().cmp(&b["endpoint"].as_str()));
    Ok(serde_json::json!(sessions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    #[test]
    fn pipe_rejects_invalid_input_and_recovers_after_an_idle_client() {
        let root = tempfile::tempdir().unwrap();
        let (server, _requests) = listen(root.path()).unwrap();
        let sessions = discover(root.path()).unwrap();
        let endpoint = Path::new(sessions[0]["endpoint"].as_str().unwrap());
        let mut idle = connect(endpoint).unwrap();
        idle.read_timeout = Duration::from_millis(30);
        assert_eq!(
            idle.read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        drop(idle);
        for input in [b"invalid\n".as_slice(), b"{\"method\":\"unknown\"}\r\n"] {
            let mut stream = connect(endpoint).unwrap();
            stream.write_all(input).unwrap();
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).unwrap();
            let response: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(response["error"]["code"], "INVALID_ARGUMENT");
        }
        drop(server);
        assert_eq!(discover(root.path()).unwrap(), serde_json::json!([]));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn pipe_name_is_reserved_and_remote_endpoints_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let (_server, _requests) = listen(root.path()).unwrap();
        let sessions = discover(root.path()).unwrap();
        let endpoint = Path::new(sessions[0]["endpoint"].as_str().unwrap());
        let runtime = runtime().unwrap();
        let _entered = runtime.enter();
        assert!(create_pipe(endpoint, true).is_err());
        for path in [
            r"\\remote\pipe\rovar-test",
            r"\\.\pipe\other-app",
            r"C:\file",
        ] {
            assert!(connect(Path::new(path)).is_err());
        }
    }

    #[test]
    fn pipe_delivers_large_unicode_responses_before_closing() {
        let root = tempfile::tempdir().unwrap();
        let (_server, mut requests) = listen(root.path()).unwrap();
        let sessions = discover(root.path()).unwrap();
        let endpoint = PathBuf::from(sessions[0]["endpoint"].as_str().unwrap());
        let client = std::thread::spawn(move || {
            let mut stream = connect(&endpoint).unwrap();
            stream
                .write_all(b"{\"request_id\":\"large\",\"method\":\"session.describe\"}\r\n")
                .unwrap();
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).unwrap();
            serde_json::from_str::<serde_json::Value>(&line).unwrap()
        });
        let start = Instant::now();
        let pending = loop {
            if let Ok(pending) = requests.try_recv() {
                break pending;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(5));
        };
        let expected = serde_json::json!({"request_id":"large", "result":"设计🦀".repeat(100_000)});
        pending.reply.send(expected.clone()).unwrap();
        assert_eq!(client.join().unwrap(), expected);
    }
}
