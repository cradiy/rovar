use super::*;

#[gpui::test]
fn queued_document_refreshes_resume_when_a_slot_opens_and_coalesce_duplicates(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let paths = [
        root.path().join("first.rovar"),
        root.path().join("second.rovar"),
    ];
    let objects: Vec<_> = (0..2)
        .map(|_| Object {
            id: uuid::Uuid::new_v4().to_string(),
            kind: Kind::Document,
            title: "Design".into(),
            revision: 1,
            created: 1,
            modified: 1,
            deleted: false,
        })
        .collect();
    let mut responses = Vec::new();
    for original in &objects {
        let latest = Object {
            revision: 2,
            ..original.clone()
        };
        responses.extend([
            (200, serde_json::to_value(identity()).unwrap()),
            (200, serde_json::to_value(&latest).unwrap()),
            (
                200,
                serde_json::to_value(Snapshot {
                    object: latest,
                    media: Vec::new(),
                    content: STANDARD.encode(b"server edit"),
                })
                .unwrap(),
            ),
        ]);
    }
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (url, _, server) = server_with_requests(responses, requests.clone());
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    for (path, object) in paths.iter().zip(&objects) {
        std::fs::write(path, b"base").unwrap();
        cx.update(|cx| confirmed_document(&remote, url.clone(), path.clone(), object.clone(), cx));
    }
    remote.update(cx, |r, cx| {
        for i in 0..super::super::scheduler::CONNECTIONS {
            r.start_network(&format!("occupied-{i}"));
        }
        for path in &paths {
            for _ in 0..3 {
                r.refresh_document(path.clone(), BTreeSet::new(), cx);
            }
            assert!(r.path_busy(path));
        }
        assert!(requests.lock().unwrap().is_empty());
        // Completion must start queued work without another autosave/sync tick.
        r.finish_network("occupied-0".into(), cx, |_, _| {});
    });
    pump(cx, |cx| {
        remote.read_with(cx, |r, _| !r.path_busy(&paths[1]))
    });
    server.join().unwrap();
    for path in &paths {
        assert_eq!(std::fs::read(path).unwrap(), b"server edit");
        remote.read_with(cx, |r, _| {
            assert_eq!(r.link(path).unwrap().object.revision, 2)
        });
    }
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 6);
    for object in objects {
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.contains(&format!("/{}/metadata", object.id)))
                .count(),
            1
        );
    }
}

#[gpui::test]
fn queued_refresh_preserves_edits_made_while_the_connection_is_busy(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    std::fs::write(&path, b"base").unwrap();
    let object = Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: Kind::Document,
        title: "Design".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    let latest = Object {
        revision: 2,
        ..object.clone()
    };
    let (url, _, server) = server(vec![
        (200, serde_json::to_value(identity()).unwrap()),
        (200, serde_json::to_value(latest).unwrap()),
    ]);
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let id = cx.update(|cx| confirmed_document(&remote, url, path.clone(), object, cx));
    remote.update(cx, |r, cx| {
        r.start_network(&id);
        r.refresh_document(path.clone(), BTreeSet::new(), cx);
        std::fs::write(&path, b"local edit while waiting").unwrap();
        r.changed(&path, None, false, cx);
        r.finish_network(id, cx, |_, _| {});
    });
    wait_sync(&remote, cx);
    server.join().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"local edit while waiting");
    remote.read_with(cx, |r, _| {
        let link = r.link(&path).unwrap();
        assert_eq!(link.object.revision, 1);
        assert!(link.dirty);
        assert!(r.error.is_none());
    });
}

#[gpui::test]
fn queued_refresh_is_discarded_after_sign_out_and_reauthentication(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    remote.update(cx, |r, cx| {
        let url = "http://127.0.0.1:1".to_owned();
        let id = r
            .connect(url.clone(), identity(), "old-token".into(), cx)
            .unwrap();
        r.busy = true;
        r.refresh(id.clone(), BTreeSet::new(), cx);
        r.sign_out(&id, cx);
        r.connect(url, identity(), "new-token".into(), cx).unwrap();
        r.busy = false;
        r.publish_completed(cx);
        assert!(
            !r.is_busy(),
            "The old request must not run in the new session"
        );
        assert!(r.scheduler.active.is_empty());
    });
}

fn pump(cx: &mut TestAppContext, mut ready: impl FnMut(&mut TestAppContext) -> bool) {
    let start = std::time::Instant::now();
    loop {
        cx.run_until_parked();
        if ready(cx) {
            return;
        }
        assert!(start.elapsed().as_secs() < 8, "Concurrent sync timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[gpui::test]
fn stalled_server_does_not_block_another_upload_and_publication_waits_for_local_work(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let slow_url = format!("http://{}", listener.local_addr().unwrap());
    let (arrived, arrival) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let slow = std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(start.elapsed().as_secs() < 8);
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let mut buffer = [0; 1024];
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
        }
        assert!(request.starts_with(b"GET /api/v1/session "));
        arrived.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(10)).unwrap();
        socket
            .write_all(
                b"HTTP/1.1 503 Unavailable\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            )
            .unwrap();
    });
    let object = Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: Kind::Document,
        title: "Fast".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    let (fast_url, saves, fast) = server(vec![
        (200, serde_json::to_value(identity()).unwrap()),
        (200, serde_json::to_value(&object).unwrap()),
    ]);
    let slow_path = root.path().join("a-slow.rovar");
    let fast_path = root.path().join("b-fast.rovar");
    let later_path = root.path().join("c-later.rovar");
    for path in [&slow_path, &fast_path, &later_path] {
        std::fs::write(path, b"document").unwrap();
    }
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let (slow_id, fast_id) = remote.update(cx, |r, cx| {
        let slow = r.connect(slow_url, identity(), "slow".into(), cx).unwrap();
        let fast = r.connect(fast_url, identity(), "fast".into(), cx).unwrap();
        r.track(
            slow_path.clone(),
            slow.clone(),
            "Slow".into(),
            Kind::Document,
            cx,
        );
        r.track(
            fast_path.clone(),
            fast.clone(),
            "Fast".into(),
            Kind::Document,
            cx,
        );
        r.catalog.links.get_mut(&fast_path).unwrap().object.id = object.id.clone();
        r.track(
            later_path.clone(),
            slow.clone(),
            "Later".into(),
            Kind::Document,
            cx,
        );
        r.sync(cx);
        (slow, fast)
    });
    pump(cx, |_| arrival.try_recv().is_ok());
    remote.update(cx, |r, cx| {
        assert!(r.connection_busy(&slow_id));
        assert!(!r.connection_busy(&fast_id));
        r.sync(cx);
        assert!(r.connection_busy(&fast_id));
        // Simulate a cache validation already holding the publication slot.
        r.busy = true;
    });
    pump(cx, |cx| {
        remote.read_with(cx, |r, _| !r.scheduler.completed.is_empty())
    });
    remote.update(cx, |r, cx| {
        assert_eq!(r.link(&fast_path).unwrap().object.revision, 0);
        assert!(!rovar_storage::exists(
            r.pending_path(r.link(&later_path).unwrap())
        ));
        r.busy = false;
        r.publish_completed(cx);
    });
    pump(cx, |cx| {
        remote.read_with(cx, |r, _| !r.connection_busy(&fast_id))
    });
    remote.read_with(cx, |r, _| {
        assert!(r.connection_busy(&slow_id));
        assert_eq!(r.link(&fast_path).unwrap().object.revision, 1);
        assert!(!r.link(&fast_path).unwrap().dirty);
        assert_eq!(r.link(&later_path).unwrap().object.revision, 0);
    });
    assert_eq!(saves.lock().unwrap().len(), 1);
    fast.join().unwrap();
    release.send(()).unwrap();
    wait_sync(&remote, cx);
    slow.join().unwrap();
    remote.read_with(cx, |r, _| {
        assert!(r.link(&slow_path).unwrap().error.is_some());
        assert!(!r.is_busy());
    });
}
