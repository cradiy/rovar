use super::*;
use futures_util::StreamExt;

#[test]
fn startup_removes_stale_registrations_and_preserves_live_instances() {
    let root = tempfile::tempdir().unwrap();
    let endpoint = root.path().join("disconnected.sock");
    drop(UnixListener::bind(&endpoint).unwrap());
    let stale = register(root.path(), &endpoint).unwrap().keep();
    let (server, _requests) = listen(root.path()).unwrap();
    assert!(!stale.exists());
    let registration = server._registration.path().to_owned();
    let runtime_directory = server._directory.path().to_owned();
    let sessions = discover(root.path()).unwrap();
    assert_eq!(sessions.as_array().unwrap().len(), 1);
    assert_eq!(
        sessions[0]["endpoint"],
        server.path.to_string_lossy().as_ref()
    );
    assert!(registration.exists());
    drop(server);
    assert!(!registration.exists());
    assert!(!runtime_directory.exists());
}

#[test]
fn unix_endpoint_delivers_large_responses_before_closing() {
    let root = tempfile::tempdir().unwrap();
    let (server, mut requests) = listen(root.path()).unwrap();
    let mut client = connect(&server.path).unwrap();
    client
        .write_all(b"{\"request_id\":\"large\",\"method\":\"session.describe\"}\n")
        .unwrap();
    let expected = serde_json::json!({
        "request_id": "large",
        "result": {"text": "设计🦀".repeat(128 * 1024)},
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let pending = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), requests.next())
            .await
            .unwrap()
            .unwrap()
    });
    pending.reply.send(expected.clone()).unwrap();
    let mut bytes = Vec::new();
    client.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'), "Received {} bytes", bytes.len());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        expected
    );
}
