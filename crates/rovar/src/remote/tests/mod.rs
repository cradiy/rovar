use super::*;
mod colors;
use gpui::TestAppContext;
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

fn identity() -> Identity {
    Identity {
        server_id: "server".into(),
        user_id: "user".into(),
        username: "Alice".into(),
        api_version: rovar_api::VERSION,
        spaces: vec![rovar_api::Space {
            id: "personal".into(),
            name: "Personal".into(),
            kind: "personal".into(),
            role: "owner".into(),
        }],
        registration: rovar_api::RegistrationPolicy {
            personal: true,
            teams: true,
        },
    }
}

fn server(
    responses: Vec<(u16, serde_json::Value)>,
) -> (
    String,
    Arc<Mutex<Vec<serde_json::Value>>>,
    std::thread::JoinHandle<()>,
) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let saves = Arc::new(Mutex::new(Vec::new()));
    let observed = saves.clone();
    let thread = std::thread::spawn(move || {
        for (status, body) in responses {
            let deadline = std::time::Instant::now();
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && deadline.elapsed().as_secs() < 10 =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("Mock request did not arrive: {e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let (header_end, length) = loop {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|n| n.parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    break (end + 4, length);
                }
            };
            while request.len() < header_end + length {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
            }
            if request.starts_with(b"PUT ") {
                observed.lock().unwrap().push(
                    serde_json::from_slice(&request[header_end..header_end + length]).unwrap(),
                );
            }
            let body = body.to_string();
            write!(socket, "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (url, saves, thread)
}

fn sync(remote: &Entity<Remote>, cx: &mut TestAppContext) {
    // This test uses real loopback I/O on the transport's Tokio runtime.
    cx.executor().allow_parking();
    remote.update(cx, |r, cx| {
        r.retry(cx);
        r.sync(cx);
    });
    wait_sync(remote, cx);
}

fn wait_sync(remote: &Entity<Remote>, cx: &mut TestAppContext) {
    let start = std::time::Instant::now();
    loop {
        cx.run_until_parked();
        if cx.update(|cx| !remote.read(cx).busy) {
            break;
        }
        assert!(start.elapsed().as_secs() < 8, "Sync did not complete");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[gpui::test]
fn late_unauthorized_response_does_not_revoke_a_new_login(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (url, _, thread) = server(vec![(
        401,
        serde_json::json!({"code":"unauthorized", "message":"Old session expired"}),
    )]);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    std::fs::write(&path, b"keep me").unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let id = remote.update(cx, |r, cx| {
        let id = r
            .connect(url.clone(), identity(), "old".into(), cx)
            .unwrap();
        r.track(
            path.clone(),
            id.clone(),
            "Design".into(),
            Kind::Document,
            cx,
        );
        r.sync(cx);
        r.connect(url, identity(), "new".into(), cx).unwrap();
        id
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, _| {
        let connection = r.connection(&id).unwrap();
        assert!(connection.authenticated);
        assert_eq!(connection.token, "new");
        assert!(r.link(&path).unwrap().dirty);
        assert!(r.link(&path).unwrap().error.is_none());
    });
    thread.join().unwrap();
}

#[gpui::test]
fn failed_upload_survives_restart_and_replays_before_newer_edits(cx: &mut TestAppContext) {
    let identity = identity();
    let session = serde_json::to_value(&identity).unwrap();
    let object = |revision| serde_json::json!({"id":"object", "kind":"document", "title":"Design", "revision":revision,"created":1,"modified":2,"deleted":false});
    let (url, saves, thread) = server(vec![
        (200, session.clone()),
        (
            503,
            serde_json::json!({"code":"offline","message":"Unavailable"}),
        ),
        (200, session.clone()), // Restore credentials after restart.
        (200, session.clone()),
        (200, object(1)),
        (200, session),
        (200, object(2)),
    ]);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    std::fs::write(&path, b"first").unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let id = remote.update(cx, |r, cx| {
        let id = r.connect(url, identity, "token".into(), cx).unwrap();
        r.track(
            path.clone(),
            id.clone(),
            "Design".into(),
            Kind::Document,
            cx,
        );
        id
    });
    sync(&remote, cx);
    remote.update(cx, |r, _| {
        assert!(r.link(&path).unwrap().dirty);
        assert!(r.link(&path).unwrap().error.is_some());
    });
    // Recreate the worker from disk: auth is not persisted, pending request is.
    let restarted = cx.new(|_| Remote {
        root: root.path().into(),
        catalog: serde_json::from_slice(&std::fs::read(root.path().join("servers.json")).unwrap())
            .unwrap(),
        busy: false,
        error: None,
        libraries_changed: BTreeSet::new(),
        retry_at: web_time::Instant::now(),
        reconnect_at: BTreeMap::new(),
        refresh_at: BTreeMap::new(),
        auth_generation: 0,
    });
    restarted.update(cx, |r, cx| r.restore_credentials(&id, "token".into(), cx));
    sync(&restarted, cx);
    std::fs::write(&path, b"second").unwrap();
    restarted.update(cx, |r, cx| r.changed(&path, None, false, cx));
    sync(&restarted, cx);
    restarted.update(cx, |r, _| assert!(r.link(&path).unwrap().dirty));
    sync(&restarted, cx);
    restarted.update(cx, |r, _| {
        assert!(!r.link(&path).unwrap().dirty);
        assert_eq!(r.link(&path).unwrap().object.revision, 2);
    });
    thread.join().unwrap();
    let saves = saves.lock().unwrap();
    assert_eq!(saves.len(), 3);
    assert_eq!(
        saves[0], saves[1],
        "Retry must preserve request ID and bytes"
    );
    assert_ne!(saves[1]["request_id"], saves[2]["request_id"]);
    assert_eq!(saves[2]["base_revision"], 1);
    assert_eq!(
        STANDARD
            .decode(saves[2]["content"].as_str().unwrap())
            .unwrap(),
        b"second"
    );
}

#[gpui::test]
fn expired_session_pauses_uploads_and_conflict_forks_without_losing_local_bytes(
    cx: &mut TestAppContext,
) {
    let identity = identity();
    let session = serde_json::to_value(&identity).unwrap();
    let (url, _, thread) = server(vec![
        (
            401,
            serde_json::json!({"code":"unauthorized","message":"Expired"}),
        ),
        (200, session),
        (
            409,
            serde_json::json!({"code":"revision_conflict","message":"Conflict"}),
        ),
    ]);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    std::fs::write(&path, b"keep me").unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let id = remote.update(cx, |r, cx| {
        let id = r
            .connect(url.clone(), identity.clone(), "old".into(), cx)
            .unwrap();
        r.track(
            path.clone(),
            id.clone(),
            "Design".into(),
            Kind::Document,
            cx,
        );
        id
    });
    sync(&remote, cx);
    remote.update(cx, |r, cx| {
        assert!(!r.connection(&id).unwrap().authenticated);
        assert!(r.connection(&id).unwrap().token.is_empty());
        r.sync(cx);
        assert!(!r.busy);
        assert!(r.link(&path).unwrap().dirty);
        r.connect(url, identity, "new".into(), cx).unwrap();
    });
    sync(&remote, cx);
    remote.update(cx, |r, cx| {
        let original = r.link(&path).unwrap().object.id.clone();
        assert!(r.link(&path).unwrap().conflict);
        r.retry(cx);
        r.sync(cx);
        assert!(!r.busy);
        r.fork_conflict(&path, cx);
        let link = r.link(&path).unwrap();
        assert_ne!(link.object.id, original);
        assert_eq!(link.object.revision, 0);
        assert!(!link.conflict);
        assert!(link.dirty);
    });
    assert_eq!(std::fs::read(path).unwrap(), b"keep me");
    thread.join().unwrap();
}

#[gpui::test]
fn resolving_local_conflict_uploads_merged_bytes_against_reviewed_revision(
    cx: &mut TestAppContext,
) {
    let session = serde_json::to_value(identity()).unwrap();
    let conflict = serde_json::json!({"code":"revision_conflict", "message":"Conflict"});
    let (url, saves, thread) = server(vec![
        (200, session.clone()),
        (409, conflict.clone()),
        (200, session),
        (409, conflict),
    ]);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    std::fs::write(&path, b"before merging").unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    remote.update(cx, |r, cx| {
        let id = r.connect(url, identity(), "token".into(), cx).unwrap();
        r.track(path.clone(), id, "Design".into(), Kind::Document, cx);
    });
    sync(&remote, cx);
    std::fs::write(&path, b"merged with server objects").unwrap();
    remote.update(cx, |r, cx| {
        let mut reviewed = r.link(&path).unwrap().object.clone();
        reviewed.revision = 7;
        r.resolve_conflict(&path, &reviewed, None, cx).unwrap();
        assert!(!r.link(&path).unwrap().conflict);
        assert!(r.link(&path).unwrap().dirty);
    });
    // Someone saves again after the comparison. The normal revision check must
    // stop the upload rather than silently overwrite that newer version.
    sync(&remote, cx);
    remote.update(cx, |r, _| assert!(r.link(&path).unwrap().conflict));
    thread.join().unwrap();
    let saves = saves.lock().unwrap();
    assert_eq!(saves.len(), 2);
    assert_eq!(saves[1]["base_revision"], 7);
    assert_ne!(saves[0]["request_id"], saves[1]["request_id"]);
    assert_eq!(
        STANDARD
            .decode(saves[1]["content"].as_str().unwrap())
            .unwrap(),
        b"merged with server objects"
    );
    assert_eq!(std::fs::read(path).unwrap(), b"merged with server objects");
}

#[gpui::test]
fn resolving_with_server_snapshot_persists_clean_content_without_upload(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    std::fs::write(&path, b"local version").unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    remote.update(cx, |r, cx| {
        let id = r
            .connect("http://127.0.0.1:1".into(), identity(), "token".into(), cx)
            .unwrap();
        r.track(
            path.clone(),
            id.clone(),
            "Local title".into(),
            Kind::Document,
            cx,
        );
        let link = r.catalog.links.get_mut(&path).unwrap();
        link.conflict = true;
        let mut reviewed = link.object.clone();
        reviewed.revision = 4;
        reviewed.title = "Server title".into();
        let pending = root
            .path()
            .join("pending")
            .join(id)
            .join(format!("{}.json", reviewed.id));
        write_atomic(&pending, b"old rejected request").unwrap();
        r.resolve_conflict(&path, &reviewed, Some(b"server version"), cx)
            .unwrap();
        assert!(!pending.exists());
        r.sync(cx);
        assert!(
            !r.busy,
            "Accepting the server must not upload a new revision"
        );
        let catalog: Catalog =
            serde_json::from_slice(&std::fs::read(root.path().join("servers.json")).unwrap())
                .unwrap();
        let saved = catalog.links.get(&path).unwrap();
        assert!(!saved.dirty && !saved.conflict);
        assert_eq!(saved.object.revision, 4);
        assert_eq!(saved.object.title, "Server title");
        assert_eq!(
            saved.digest,
            digest(b"server version", "Server title", false)
        );
    });
    assert_eq!(std::fs::read(path).unwrap(), b"server version");
}
