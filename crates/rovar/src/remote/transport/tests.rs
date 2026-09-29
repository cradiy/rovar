use super::*;
use std::io::{Read, Write};

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
