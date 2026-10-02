use super::*;
use std::io::{Read, Write};

#[test]
fn chunked_media_downloads_reject_incomplete_oversized_and_corrupt_bodies() {
    use sha2::{Digest, Sha256};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for body in [b"abcdef".as_slice(), b"abc", b"abcdefg", b"abcxyz"] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            let size = socket.read(&mut request).unwrap();
            assert!(request[..size].starts_with(b"GET /api/v1/spaces/space/media/hash "));
            let mut response =
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                    .to_vec();
            for chunk in body.chunks(3) {
                write!(response, "{:x}\r\n", chunk.len()).unwrap();
                response.extend_from_slice(chunk);
                response.extend_from_slice(b"\r\n");
            }
            response.extend_from_slice(b"0\r\n\r\n");
            socket.write_all(&response).unwrap();
        }
        // An oversized proxy body must still revoke an expired session. The
        // advertised size is enough to reject it without buffering the body.
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let size = socket.read(&mut request).unwrap();
        assert!(request[..size].starts_with(b"GET /api/v1/spaces/space/media/hash "));
        socket
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 65537\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    });
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let root = tempfile::tempdir().unwrap();
        let client = Client::new(&url).unwrap();
        let item = rovar_api::Media {
            hash: hex::encode(Sha256::digest(b"abcdef")),
            length: 6,
        };
        let mut output = client
            .download_media(
                "spaces/space/media/hash",
                &item,
                &root.path().join("success"),
            )
            .await
            .unwrap();
        let mut bytes = Vec::new();
        output.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"abcdef");
        for (index, expected) in ["checksum", "declared length", "checksum"]
            .into_iter()
            .enumerate()
        {
            let error = client
                .download_media(
                    "spaces/space/media/hash",
                    &item,
                    &root.path().join(index.to_string()),
                )
                .await
                .unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
        let error = client
            .download_media(
                "spaces/space/media/hash",
                &item,
                &root.path().join("unauthorized"),
            )
            .await
            .unwrap_err();
        assert_eq!(error.downcast_ref::<HttpError>().unwrap().status, 401);
    });
    server.join().unwrap();
}

#[test]
fn logout_handles_expired_sessions_even_with_proxy_error_bodies() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for (status, body) in [
            (
                "401 Unauthorized",
                r#"{"code":"unauthorized","message":"Expired"}"#,
            ),
            ("401 Unauthorized", "<html>Session expired</html>"),
            ("503 Service Unavailable", "<html>Proxy unavailable</html>"),
            ("200 OK", "{}"),
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            let size = socket.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..size]).starts_with("POST /api/v1/logout "));
            write!(
                socket,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let client = Client::new(&url).unwrap();
        assert!(client.logout().await.is_ok());
        assert!(client.logout().await.is_ok());
        let error = client.logout().await.unwrap_err();
        assert_eq!(error.downcast_ref::<HttpError>().unwrap().status, 503);
        assert!(!error.to_string().contains("<html>"));
        assert!(client.logout().await.is_ok());
    });
    server.join().unwrap();
}
