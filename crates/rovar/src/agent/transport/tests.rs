use super::*;
use futures_util::StreamExt;

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
