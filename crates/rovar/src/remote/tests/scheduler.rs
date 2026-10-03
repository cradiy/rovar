use super::*;

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
