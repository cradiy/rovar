use super::*;
use std::net::TcpListener;

const ROUTE: &str = "spaces/workspace/media/hash";

fn item() -> rovar_api::Media {
    rovar_api::Media {
        hash: hex::encode(Sha256::digest(b"abcdef")),
        length: 6,
    }
}

fn server(responses: Vec<(Option<u64>, String)>) -> (Client, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = Client::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let thread = std::thread::spawn(move || {
        for (range, response) in responses {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "Missing download request"
                        );
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() < 8192);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            if let Some(offset) = range {
                assert!(
                    request.contains(&format!("\r\nrange: bytes={offset}-\r\n")),
                    "{request}"
                );
                assert!(
                    request.contains(&format!("\r\nif-range: \"{}\"\r\n", item().hash)),
                    "{request}"
                );
            } else {
                assert!(!request.contains("\r\nrange:"), "{request}");
            }
            socket.write_all(response.as_bytes()).unwrap();
        }
    });
    (client, thread)
}

fn full(body: &str) -> String {
    format!("HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\n{body}")
}

fn suffix(range: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nContent-Range: {range}\r\nETag: \"{}\"\r\nConnection: close\r\n\r\n{body}",
        item().hash
    )
}

fn contents(mut file: fs::File) -> Vec<u8> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}

#[test]
fn interrupted_download_resumes_after_reopen_and_reuses_verified_cache() {
    let (client, thread) = server(vec![
        (None, full("abc")),
        (Some(3), suffix("bytes 3-5/6", "def")),
    ]);
    let root = tempfile::tempdir().unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        assert!(
            client
                .download_media(ROUTE, &item(), root.path())
                .await
                .is_err()
        );
        // A process can die after writing data but before checkpointing it.
        let partial = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().is_some_and(|ext| ext == "part"))
            .unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(partial)
            .unwrap()
            .write_all(b"unconfirmed")
            .unwrap();
        let reopened = Client::new(&client.url).unwrap();
        assert_eq!(
            contents(
                reopened
                    .download_media(ROUTE, &item(), root.path())
                    .await
                    .unwrap()
            ),
            b"abcdef"
        );
        thread.join().unwrap();
        // The listener is gone: this must be served from the verified cache.
        assert_eq!(
            contents(
                client
                    .download_media(ROUTE, &item(), root.path())
                    .await
                    .unwrap()
            ),
            b"abcdef"
        );
    });
}

#[test]
fn resume_rejects_wrong_ranges_and_handles_full_response_or_416() {
    for rejection in [false, true] {
        let mut responses = vec![(None, full("abc")), (Some(3), suffix("bytes 2-4/6", "def"))];
        if rejection {
            responses.push((Some(3), "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()));
            responses.push((None, full("abcdef")));
        } else {
            responses.push((Some(3), full("abcdef")));
        }
        let (client, thread) = server(responses);
        let root = tempfile::tempdir().unwrap();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            assert!(
                client
                    .download_media(ROUTE, &item(), root.path())
                    .await
                    .is_err()
            );
            let error = client
                .download_media(ROUTE, &item(), root.path())
                .await
                .unwrap_err();
            assert!(error.to_string().contains("range response"));
            assert_eq!(
                contents(
                    client
                        .download_media(ROUTE, &item(), root.path())
                        .await
                        .unwrap()
                ),
                b"abcdef"
            );
        });
        thread.join().unwrap();
    }
}

#[test]
fn corrupt_prefix_and_changed_account_restart_without_reusing_partial_bytes() {
    for changed_account in [false, true] {
        let (mut client, thread) = server(vec![(None, full("abc")), (None, full("abcdef"))]);
        let root = tempfile::tempdir().unwrap();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            assert!(
                client
                    .download_media(ROUTE, &item(), root.path())
                    .await
                    .is_err()
            );
            if changed_account {
                client.token = "another-session".into();
            } else {
                let partial = fs::read_dir(root.path())
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| path.extension().is_some_and(|ext| ext == "part"))
                    .unwrap();
                fs::write(partial, b"xyz").unwrap();
            }
            assert_eq!(
                contents(
                    client
                        .download_media(ROUTE, &item(), root.path())
                        .await
                        .unwrap()
                ),
                b"abcdef"
            );
        });
        thread.join().unwrap();
    }
}

#[test]
fn corrupt_completion_discards_partial_state_before_retry() {
    let (client, thread) = server(vec![
        (None, full("abc")),
        (Some(3), suffix("bytes 3-5/6", "xyz")),
        (None, full("abcdef")),
    ]);
    let root = tempfile::tempdir().unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        assert!(
            client
                .download_media(ROUTE, &item(), root.path())
                .await
                .is_err()
        );
        let error = client
            .download_media(ROUTE, &item(), root.path())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("checksum"));
        assert_eq!(
            contents(
                client
                    .download_media(ROUTE, &item(), root.path())
                    .await
                    .unwrap()
            ),
            b"abcdef"
        );
    });
    thread.join().unwrap();
}
