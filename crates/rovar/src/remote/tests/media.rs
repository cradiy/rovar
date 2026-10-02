use super::*;

#[test]
fn media_upload_retries_resume_after_lost_chunk_and_completion_acknowledgements() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("media.rovar");
    let split = rovar_api::MEDIA_UPLOAD_CHUNK_BYTES;
    let bytes = vec![83; split + 17];
    let mut writer = rovar_format::Writer::create(&path).unwrap();
    writer.put_bytes("media", "media", &bytes).unwrap();
    writer.commit().unwrap();
    drop(writer);
    let block = rovar_format::Reader::open(&path)
        .unwrap()
        .block("media")
        .unwrap();
    let progress = |offset, complete| {
        serde_json::to_vec(&rovar_api::MediaUpload { offset, complete }).unwrap()
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (url, uploads, thread) = server_with_bodies(
        vec![
            (200, progress(0, false)),
            (503, b"{}".to_vec()), // The first part persisted, but its acknowledgement was lost.
            (200, progress(split as u64, false)),
            (200, progress(bytes.len() as u64, false)),
            (503, b"{}".to_vec()), // Completion committed, but its acknowledgement was lost.
            (200, progress(bytes.len() as u64, true)),
            (200, progress(3, false)), // A malformed server offset must not skip local bytes.
        ],
        requests.clone(),
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for _ in 0..2 {
            let client = Client::new(&url).unwrap();
            let error = client
                .upload_media("spaces/space/media/hash", block.clone())
                .await
                .unwrap_err();
            assert_eq!(error.downcast_ref::<HttpError>().unwrap().status, 503);
        }
        let client = Client::new(&url).unwrap();
        client
            .upload_media("spaces/space/media/hash", block.clone())
            .await
            .unwrap();
        assert!(
            client
                .upload_media("spaces/space/media/hash", block)
                .await
                .unwrap_err()
                .to_string()
                .contains("Invalid media upload progress")
        );
    });
    thread.join().unwrap();
    let uploads = uploads.lock().unwrap();
    assert_eq!(uploads.len(), 2);
    assert_eq!(
        uploads[0],
        serde_json::json!({ "length": split, "hash": hex::encode(Sha256::digest(&bytes[..split])) })
    );
    assert_eq!(
        uploads[1],
        serde_json::json!({ "length": 17, "hash": hex::encode(Sha256::digest(&bytes[split..])) })
    );
    let requests = requests.lock().unwrap();
    assert!(requests[1].contains("&offset=0 "));
    assert!(requests[3].contains(&format!("&offset={split} ")));
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST "))
            .count(),
        1
    );
}

#[gpui::test]
fn pending_snapshots_always_detach_media_before_upload(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    let bytes = b"existing media";
    let hash = hex::encode(Sha256::digest(bytes));
    let mut writer = rovar_format::Writer::create(&path).unwrap();
    writer.put_bytes("document", "json", b"{}").unwrap();
    writer
        .put_bytes(&format!("media/{hash}"), "media", bytes)
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let original = STANDARD.encode(std::fs::read(&path).unwrap());
    let object = Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: Kind::Document,
        title: "Design".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    let (url, saves, thread) = server(vec![
        (200, serde_json::to_value(identity()).unwrap()),
        (200, serde_json::json!([])),
        (200, serde_json::to_value(&object).unwrap()),
    ]);
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    remote.update(cx, |r, cx| {
        let connection = r.connect(url, identity(), "token".into(), cx).unwrap();
        r.track(
            path.clone(),
            connection,
            object.title.clone(),
            Kind::Document,
            cx,
        );
        let link = r.catalog.links.get_mut(&path).unwrap();
        link.object.id = object.id;
        let pending = r.pending_path(r.link(&path).unwrap());
        write_atomic(
            &pending,
            &serde_json::to_vec(&PendingSave {
                delta: None,
                input: Save {
                    kind: Kind::Document,
                    title: "Design".into(),
                    base_revision: 0,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    content: original.clone(),
                    media: vec![],
                    deleted: false,
                },
            })
            .unwrap(),
        )
        .unwrap();
    });
    sync(&remote, cx);
    thread.join().unwrap();
    let saves = saves.lock().unwrap();
    assert_eq!(saves.len(), 1);
    assert_ne!(saves[0]["content"], original);
    assert_eq!(
        saves[0]["media"],
        serde_json::json!([{ "hash": hash, "length": bytes.len() }])
    );
    remote.read_with(cx, |r, _| assert!(!r.link(&path).unwrap().dirty));
}

#[test]
fn media_is_uploaded_once_reused_on_retry_and_verified_when_downloaded() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    let bytes = vec![73; 256 * 1024 + 7];
    let hash = hex::encode(Sha256::digest(&bytes));
    let mut writer = rovar_format::Writer::create(&path).unwrap();
    writer
        .put_bytes("document", "json", br#"{"name":"Design"}"#)
        .unwrap();
    writer
        .put_bytes(&format!("media/{hash}"), "media", &bytes)
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let original = std::fs::read(&path).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (url, uploads, thread) = server_with_bodies(
        vec![
            (200, serde_json::to_vec(&serde_json::json!([hash])).unwrap()),
            (
                200,
                serde_json::to_vec(&serde_json::json!({"offset":0,"complete":false})).unwrap(),
            ),
            (
                200,
                serde_json::to_vec(&serde_json::json!({"offset":bytes.len(),"complete":false}))
                    .unwrap(),
            ),
            (
                200,
                serde_json::to_vec(&serde_json::json!({"offset":bytes.len(),"complete":true}))
                    .unwrap(),
            ),
            (200, b"[]".to_vec()),
            (200, bytes.clone()),
            (200, b"corrupt response".to_vec()),
        ],
        requests.clone(),
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let client = Client::new(&url).unwrap();
        let input = Save {
            kind: Kind::Document,
            title: "Design".into(),
            base_revision: 0,
            request_id: uuid::Uuid::new_v4().to_string(),
            content: STANDARD.encode(&original),
            media: vec![],
            deleted: false,
        };
        let transfer = super::super::media::prepare(&client, "personal", &input)
            .await
            .unwrap();
        assert_eq!(transfer.media.len(), 1);
        assert_eq!(transfer.media[0].hash, hash);
        assert!(STANDARD.decode(&transfer.content).unwrap().len() < original.len() / 8);
        let retry = super::super::media::prepare(&client, "personal", &input)
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(&retry).unwrap(),
            serde_json::to_value(&transfer).unwrap(),
            "Retry fingerprints must be deterministic even after the media upload succeeded"
        );
        let snapshot = Snapshot {
            object: Object {
                id: uuid::Uuid::new_v4().to_string(),
                kind: Kind::Document,
                title: input.title,
                revision: 1,
                created: 1,
                modified: 1,
                deleted: false,
            },
            content: transfer.content,
            media: transfer.media,
        };
        let reused = super::super::media::hydrate(&client, "personal", &snapshot, &path)
            .await
            .unwrap();
        let output = root.path().join("download.rovar");
        std::fs::write(&output, reused).unwrap();
        let reader = rovar_format::Reader::open(&output).unwrap();
        assert_eq!(
            reader
                .read(&format!("media/{hash}"), bytes.len() as u64)
                .unwrap(),
            bytes
        );
        assert_eq!(
            requests.lock().unwrap().len(),
            5,
            "Local media should not be downloaded again"
        );
        let missing = root.path().join("missing.rovar");
        let downloaded = super::super::media::hydrate(&client, "personal", &snapshot, &missing)
            .await
            .unwrap();
        std::fs::write(&output, downloaded).unwrap();
        rovar_format::Reader::open(&output)
            .unwrap()
            .verify()
            .unwrap();
        assert!(
            super::super::media::hydrate(&client, "personal", &snapshot, &missing)
                .await
                .unwrap_err()
                .to_string()
                .contains("checksum")
        );
        assert!(
            !missing.exists(),
            "Failed hydration must never replace the local file"
        );
    });
    thread.join().unwrap();
    assert_eq!(uploads.lock().unwrap().len(), 1);
    assert_eq!(
        uploads.lock().unwrap()[0],
        serde_json::json!({"length": bytes.len(), "hash": hash})
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}
