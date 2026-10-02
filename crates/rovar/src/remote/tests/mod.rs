use super::*;
mod baseline;
mod colors;
mod delta;
mod directory;
mod media;
mod merge;
use gpui::TestAppContext;

#[gpui::test]
fn metadata_refresh_preserves_session_generation_but_auth_changes_invalidate_it(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    remote.update(cx, |remote, cx| {
        let url = "https://example.test".to_owned();
        let id = remote
            .connect(url.clone(), identity(), "token".into(), cx)
            .unwrap();
        let first = remote.connection(&id).unwrap().generation;
        let mut refreshed = identity();
        refreshed.spaces[0].name = "Renamed workspace".into();
        remote
            .connect(url.clone(), refreshed.clone(), "token".into(), cx)
            .unwrap();
        assert_eq!(remote.connection(&id).unwrap().generation, first);
        assert_eq!(
            remote.connection(&id).unwrap().space.name,
            "Renamed workspace"
        );
        refreshed.spaces[0].role = "viewer".into();
        remote
            .connect(url.clone(), refreshed, "token".into(), cx)
            .unwrap();
        assert_ne!(remote.connection(&id).unwrap().generation, first);
        let changed = remote.connection(&id).unwrap().generation;
        remote
            .connect(url.clone(), identity(), "new-token".into(), cx)
            .unwrap();
        assert_ne!(remote.connection(&id).unwrap().generation, changed);
        let changed = remote.connection(&id).unwrap().generation;
        remote.sign_out(&id, cx);
        remote
            .connect(url, identity(), "new-token".into(), cx)
            .unwrap();
        assert_ne!(remote.connection(&id).unwrap().generation, changed);
    });
}
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

pub(crate) fn identity() -> Identity {
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

pub(crate) fn confirmed_document(
    remote: &Entity<Remote>,
    url: String,
    path: PathBuf,
    object: Object,
    cx: &mut gpui::App,
) -> String {
    remote.update(cx, |r, cx| {
        let id = r.connect(url, identity(), "token".into(), cx).unwrap();
        r.track(
            path.clone(),
            id.clone(),
            object.title.clone(),
            Kind::Document,
            cx,
        );
        let bytes = rovar_storage::fs::read(&path).unwrap();
        let key = r
            .store_baseline(&object, &super::baseline::content(&path, false).unwrap())
            .unwrap();
        let link = r.catalog.links.get_mut(&path).unwrap();
        link.digest = digest(&bytes, &object.title, false);
        link.baseline = Some(key);
        link.object = object;
        link.dirty = false;
        r.persist();
        id
    })
}

pub(crate) fn server(
    responses: Vec<(u16, serde_json::Value)>,
) -> (
    String,
    Arc<Mutex<Vec<serde_json::Value>>>,
    std::thread::JoinHandle<()>,
) {
    server_with_requests(responses, Arc::new(Mutex::new(Vec::new())))
}

fn server_with_requests(
    responses: Vec<(u16, serde_json::Value)>,
    requests: Arc<Mutex<Vec<String>>>,
) -> (
    String,
    Arc<Mutex<Vec<serde_json::Value>>>,
    std::thread::JoinHandle<()>,
) {
    server_with_bodies(
        responses
            .into_iter()
            .map(|(status, body)| (status, serde_json::to_vec(&body).unwrap()))
            .collect(),
        requests,
    )
}

fn server_with_bodies(
    responses: Vec<(u16, Vec<u8>)>,
    requests: Arc<Mutex<Vec<String>>>,
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
            requests.lock().unwrap().push(
                String::from_utf8_lossy(&request[..header_end])
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned(),
            );
            if request.starts_with(b"PUT ") {
                let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
                let bytes = &request[header_end..header_end + length];
                observed.lock().unwrap().push(
                    if headers.contains("content-type: application/octet-stream") {
                        serde_json::json!({ "length": bytes.len(), "hash": hex::encode(Sha256::digest(bytes)) })
                    } else {
                        serde_json::from_slice(bytes).unwrap()
                    },
                );
            }
            write!(
                socket,
                "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        }
    });
    (url, saves, thread)
}

pub(crate) fn sync(remote: &Entity<Remote>, cx: &mut TestAppContext) {
    // This test uses real loopback I/O on the transport's Tokio runtime.
    cx.executor().allow_parking();
    remote.update(cx, |r, cx| {
        r.retry(cx);
        r.sync(cx);
    });
    wait_sync(remote, cx);
}

pub(crate) fn wait_sync(remote: &Entity<Remote>, cx: &mut TestAppContext) {
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
        cleanup_at: None,
        baseline_cleanup: cleanup::Cleanup::default(),
        merge_pending: BTreeSet::new(),
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
        write_atomic(
            &pending,
            &serde_json::to_vec(&PendingSave {
                delta: None,
                input: Save {
                    kind: Kind::Document,
                    title: "Design".into(),
                    base_revision: 0,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    content: STANDARD.encode(b"local version"),
                    media: vec![],
                    deleted: false,
                },
            })
            .unwrap(),
        )
        .unwrap();
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
