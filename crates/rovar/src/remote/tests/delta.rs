use super::*;
use crate::document::{Document, Page};
use crate::scene::{
    artboard::Rect,
    shape::{Shape, ShapeKind},
};

fn document() -> Document {
    let mut page = Page::empty("Design".into());
    for id in 1..=120 {
        page.shapes.push(Shape::new(
            id,
            None,
            ShapeKind::Rectangle,
            Rect {
                x: id as f32,
                y: 0.,
                width: 100.,
                height: 50.,
            },
        ));
    }
    page.next_id = 121;
    Document::single(page)
}

fn object(revision: i64) -> Object {
    Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: Kind::Document,
        title: "Design".into(),
        revision,
        created: 1,
        modified: revision as u64,
        deleted: false,
    }
}

fn confirm(
    remote: &Entity<Remote>,
    url: String,
    path: &Path,
    object: Object,
    cx: &mut TestAppContext,
) {
    cx.update(|cx| confirmed_document(remote, url, path.into(), object.clone(), cx));
    remote.update(cx, |r, _| {
        let bytes = std::fs::read(path).unwrap();
        let key = r
            .store_snapshot_baseline(
                &object,
                &super::super::baseline::content(path, false).unwrap(),
                &bytes,
            )
            .unwrap();
        r.catalog.links.get_mut(path).unwrap().baseline = Some(key);
        r.persist();
    });
}

#[gpui::test]
fn delta_retry_keeps_the_exact_request_and_later_edits_use_the_acknowledged_baseline(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    let text_system = cx.update(|cx| cx.text_system().clone());
    let mut document = document();
    let write = |doc: &Document| {
        crate::document::save_as(&path, &serde_json::to_vec(doc).unwrap(), &[], &text_system)
            .unwrap()
    };
    write(&document);
    let base_bytes = std::fs::read(&path).unwrap();
    let base =
        rovar_format::delta::Snapshot::from_bytes(&base_bytes, rovar_api::MAX_METADATA_BYTES)
            .unwrap();
    let original = object(1);
    let second = Object {
        revision: 2,
        ..original.clone()
    };
    let third = Object {
        revision: 3,
        ..original.clone()
    };
    let session = (200, serde_json::to_value(identity()).unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (url, saves, thread) = server_with_requests(
        vec![
            session.clone(),
            (
                500,
                serde_json::json!({"code":"internal_error","message":"acknowledgement unavailable"}),
            ),
            session.clone(),
            (200, serde_json::to_value(second).unwrap()),
            session,
            (200, serde_json::to_value(third).unwrap()),
        ],
        requests.clone(),
    );
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    confirm(&remote, url, &path, original, cx);
    document.pages[0].shapes[0].rect.x = 500.;
    write(&document);
    remote.update(cx, |r, cx| r.changed(&path, None, false, cx));
    let sent_size = std::fs::metadata(&path).unwrap().len();
    sync(&remote, cx);
    document.pages[0].shapes[1].rect.y = 400.;
    write(&document);
    remote.update(cx, |r, cx| r.changed(&path, None, false, cx));
    sync(&remote, cx);
    remote.read_with(cx, |r, _| assert!(r.link(&path).unwrap().dirty));
    sync(&remote, cx);
    thread.join().unwrap();
    remote.read_with(cx, |r, _| {
        assert!(!r.link(&path).unwrap().dirty);
        assert_eq!(r.link(&path).unwrap().object.revision, 3);
    });
    let saves = saves.lock().unwrap();
    assert_eq!(saves.len(), 3);
    assert_eq!(saves[0], saves[1]);
    assert_ne!(saves[1]["request_id"], saves[2]["request_id"]);
    assert_eq!(saves[2]["base_revision"], 2);
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("PUT"))
            .all(|r| r.contains("/delta "))
    );
    let first = STANDARD
        .decode(saves[0]["content"].as_str().unwrap())
        .unwrap();
    assert!(
        (first.len() as u64) * 4 < sent_size,
        "{} byte patch versus {sent_size} byte document",
        first.len()
    );
    let after_first = base
        .apply(
            &serde_json::from_slice(&first).unwrap(),
            rovar_api::MAX_METADATA_BYTES,
        )
        .unwrap();
    let second = STANDARD
        .decode(saves[2]["content"].as_str().unwrap())
        .unwrap();
    let after_second = after_first
        .apply(
            &serde_json::from_slice(&second).unwrap(),
            rovar_api::MAX_METADATA_BYTES,
        )
        .unwrap();
    let expected = rovar_format::delta::Snapshot::from_bytes(
        &std::fs::read(&path).unwrap(),
        rovar_api::MAX_METADATA_BYTES,
    )
    .unwrap();
    assert_eq!(after_second.hash().unwrap(), expected.hash().unwrap());
}

#[gpui::test]
fn confirmed_delta_base_rejection_falls_back_with_a_new_persisted_request_id(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    let text_system = cx.update(|cx| cx.text_system().clone());
    let mut document = document();
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let original = object(1);
    let session = (200, serde_json::to_value(identity()).unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (url, saves, thread) = server_with_requests(
        vec![
            session.clone(),
            (
                409,
                serde_json::json!({"code":"delta_base_mismatch", "message":"base unavailable"}),
            ),
            (
                500,
                serde_json::json!({"code":"internal_error", "message":"lost full-snapshot acknowledgement"}),
            ),
            session,
            (
                200,
                serde_json::to_value(Object {
                    revision: 2,
                    ..original.clone()
                })
                .unwrap(),
            ),
        ],
        requests.clone(),
    );
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    confirm(&remote, url, &path, original, cx);
    document.pages[0].shapes[0].rect.x = 321.;
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let content = STANDARD.encode(std::fs::read(&path).unwrap());
    remote.update(cx, |r, cx| r.changed(&path, None, false, cx));
    sync(&remote, cx);
    remote.read_with(cx, |r, _| {
        let link = r.link(&path).unwrap();
        assert!(!link.conflict && link.dirty);
        let pending: PendingSave =
            serde_json::from_slice(&std::fs::read(r.pending_path(link)).unwrap()).unwrap();
        assert!(pending.delta.is_none());
        assert_eq!(
            pending.input.request_id,
            saves.lock().unwrap()[1]["request_id"]
        );
    });
    sync(&remote, cx);
    thread.join().unwrap();
    let saves = saves.lock().unwrap();
    assert_eq!(saves.len(), 3);
    assert_ne!(saves[0]["request_id"], saves[1]["request_id"]);
    assert_eq!(saves[1], saves[2]);
    assert_eq!(saves[1]["content"], content);
    let requests = requests.lock().unwrap();
    let puts: Vec<_> = requests.iter().filter(|r| r.starts_with("PUT")).collect();
    assert!(puts[0].contains("/delta "));
    assert!(!puts[1].contains("/delta "));
    remote.read_with(cx, |r, _| assert!(!r.link(&path).unwrap().dirty));
}

fn download_response(
    object: Object,
    content: &[u8],
    delta: bool,
    media: Vec<rovar_api::Media>,
) -> (u16, serde_json::Value) {
    (
        200,
        serde_json::to_value(rovar_api::Transfer {
            snapshot: Snapshot {
                object,
                content: STANDARD.encode(content),
                media,
            },
            encoding: if delta {
                rovar_api::TransferEncoding::Delta
            } else {
                rovar_api::TransferEncoding::Full
            },
        })
        .unwrap(),
    )
}

#[gpui::test]
fn downloaded_deltas_advance_the_baseline_and_repair_a_missing_cache(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    let text_system = cx.update(|cx| cx.text_system().clone());
    let mut document = document();
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let base_bytes = std::fs::read(&path).unwrap();
    let base =
        rovar_format::delta::Snapshot::from_bytes(&base_bytes, rovar_api::MAX_METADATA_BYTES)
            .unwrap();
    document.pages[0].shapes[0].rect.x = 250.;
    let next_path = root.path().join("next.rovar");
    crate::document::save_as(
        &next_path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let next = rovar_format::delta::Snapshot::from_bytes(
        &std::fs::read(next_path).unwrap(),
        rovar_api::MAX_METADATA_BYTES,
    )
    .unwrap();
    let original = object(1);
    let updated = Object {
        revision: 2,
        ..original.clone()
    };
    let session = (200, serde_json::to_value(identity()).unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (url, _, thread) = server_with_requests(
        vec![
            session.clone(),
            (200, serde_json::to_value(&updated).unwrap()),
            download_response(
                updated.clone(),
                &serde_json::to_vec(&base.difference(&next).unwrap()).unwrap(),
                true,
                vec![],
            ),
            session,
            (200, serde_json::to_value(&updated).unwrap()),
            download_response(
                updated,
                &serde_json::to_vec(&next.difference(&next).unwrap()).unwrap(),
                true,
                vec![],
            ),
        ],
        requests.clone(),
    );
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    confirm(&remote, url, &path, original, cx);
    for missing in [false, true] {
        if missing {
            std::fs::remove_file(&path).unwrap();
        }
        remote.update(cx, |r, cx| {
            r.refresh_document(path.clone(), BTreeSet::new(), cx)
        });
        wait_sync(&remote, cx);
        let loaded = crate::document::load(&path)
            .unwrap()
            .into_document()
            .unwrap();
        assert_eq!(loaded.pages[0].shapes[0].rect.x, 250.);
        remote.read_with(cx, |r, _| {
            let link = r.link(&path).unwrap();
            assert_eq!(link.object.revision, 2);
            assert!(!link.dirty && !link.conflict);
            assert_eq!(
                r.read_baseline(link)
                    .unwrap()
                    .unwrap()
                    .transfer
                    .unwrap()
                    .hash()
                    .unwrap(),
                next.hash().unwrap()
            );
        });
    }
    thread.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST") && r.contains("/transfer "))
            .count(),
        2
    );
    assert!(
        !requests
            .iter()
            .any(|r| r.starts_with("GET") && r.contains("/transfer "))
    );
}

#[gpui::test]
fn damaged_download_delta_fetches_full_content_before_touching_the_local_cache(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    let text_system = cx.update(|cx| cx.text_system().clone());
    let mut document = document();
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let base_bytes = std::fs::read(&path).unwrap();
    let base =
        rovar_format::delta::Snapshot::from_bytes(&base_bytes, rovar_api::MAX_METADATA_BYTES)
            .unwrap();
    document.pages[0].shapes[0].rect.y = 400.;
    let next_path = root.path().join("next.rovar");
    crate::document::save_as(
        &next_path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let next_bytes = std::fs::read(next_path).unwrap();
    let next =
        rovar_format::delta::Snapshot::from_bytes(&next_bytes, rovar_api::MAX_METADATA_BYTES)
            .unwrap();
    let mut patch: serde_json::Value =
        serde_json::to_value(base.difference(&next).unwrap()).unwrap();
    patch["result"][0] = ((patch["result"][0].as_u64().unwrap() + 1) % 256).into();
    let original = object(1);
    let updated = Object {
        revision: 2,
        ..original.clone()
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let session = (200, serde_json::to_value(identity()).unwrap());
    let (url, _, thread) = server_with_requests(
        vec![
            session.clone(),
            (200, serde_json::to_value(&updated).unwrap()),
            download_response(
                updated.clone(),
                &serde_json::to_vec(&patch).unwrap(),
                true,
                vec![],
            ),
            (
                500,
                serde_json::json!({"code":"internal_error", "message":"temporarily unavailable"}),
            ),
            session,
            (200, serde_json::to_value(&updated).unwrap()),
            download_response(updated, &next_bytes, false, vec![]),
        ],
        requests.clone(),
    );
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    confirm(&remote, url, &path, original, cx);
    remote.update(cx, |r, cx| {
        r.refresh_document(path.clone(), BTreeSet::new(), cx)
    });
    wait_sync(&remote, cx);
    assert_eq!(std::fs::read(&path).unwrap(), base_bytes);
    remote.read_with(cx, |r, _| {
        assert_eq!(r.link(&path).unwrap().object.revision, 1)
    });
    remote.update(cx, |r, cx| {
        r.refresh_document(path.clone(), BTreeSet::new(), cx)
    });
    wait_sync(&remote, cx);
    thread.join().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), next_bytes);
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST") && r.contains("/transfer "))
            .count(),
        2
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("GET") && r.contains("/transfer "))
            .count(),
        1
    );
}

#[gpui::test]
fn download_reconstruction_ignores_local_edits_and_fetches_only_missing_media(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    let target = root.path().join("remote.rovar");
    let text_system = cx.update(|cx| cx.text_system().clone());
    let mut sources = Vec::new();
    let mut media = Vec::new();
    let mut image_bytes = Vec::new();
    for (index, color) in [[255, 0, 0, 255], [0, 0, 255, 255]].into_iter().enumerate() {
        let original = root.path().join(format!("image-{index}.png"));
        image::RgbaImage::from_pixel(3, 2, image::Rgba(color))
            .save(&original)
            .unwrap();
        let asset = crate::media::MediaAsset::load(original.clone()).unwrap();
        let bytes = std::fs::read(&original).unwrap();
        media.push(rovar_api::Media {
            hash: asset.hash.clone(),
            length: bytes.len() as u64,
        });
        image_bytes.push(bytes);
        sources.push(crate::document::AssetSource {
            hash: asset.hash.clone(),
            name: asset.name(),
            path: asset.source.clone(),
            size: [asset.width, asset.height],
        });
    }
    let mut base_document = document();
    base_document.pages[0].shapes[0].kind = ShapeKind::Image;
    base_document.pages[0]
        .assets
        .push(crate::document::AssetUse {
            object: 1,
            fill: false,
            hash: media[0].hash.clone(),
        });
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&base_document).unwrap(),
        &sources,
        &text_system,
    )
    .unwrap();
    let base = rovar_format::delta::Snapshot::from_bytes(
        &std::fs::read(&path).unwrap(),
        rovar_api::MAX_METADATA_BYTES,
    )
    .unwrap();
    let mut next_document = base_document.clone();
    next_document.pages[0].shapes[1].kind = ShapeKind::Image;
    next_document.pages[0]
        .assets
        .push(crate::document::AssetUse {
            object: 2,
            fill: false,
            hash: media[1].hash.clone(),
        });
    crate::document::save_as(
        &target,
        &serde_json::to_vec(&next_document).unwrap(),
        &sources,
        &text_system,
    )
    .unwrap();
    let next = rovar_format::delta::Snapshot::from_bytes(
        &std::fs::read(target).unwrap(),
        rovar_api::MAX_METADATA_BYTES,
    )
    .unwrap();
    let original = object(1);
    let updated = Object {
        revision: 2,
        ..original.clone()
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let response = download_response(
        updated.clone(),
        &serde_json::to_vec(&base.difference(&next).unwrap()).unwrap(),
        true,
        media.clone(),
    );
    let (url, _, thread) = server_with_bodies(
        vec![
            (response.0, serde_json::to_vec(&response.1).unwrap()),
            (200, image_bytes[1].clone()),
        ],
        requests.clone(),
    );
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    confirm(&remote, url, &path, original, cx);
    base_document.pages[0].shapes[0].rect.x = 999.;
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&base_document).unwrap(),
        &sources,
        &text_system,
    )
    .unwrap();
    let local = std::fs::read(&path).unwrap();
    let result = Arc::new(Mutex::new(None));
    let output = result.clone();
    let cache = path.clone();
    remote.update(cx, |r, cx| {
        let link = r.link(&cache).unwrap();
        let baseline = r.read_baseline(link).unwrap();
        let connection = r.connection(&link.connection).unwrap();
        let client = connection.client();
        let space = connection.space.id.clone();
        let executor = cx.background_executor().clone();
        r.busy = true;
        cx.spawn(async move |this, cx| {
            *output.lock().unwrap() = Some(
                super::super::delta::receive(
                    &client, &space, &updated, baseline, &cache, &executor,
                )
                .await,
            );
            this.update(cx, |r, _| r.busy = false).unwrap();
        })
        .detach();
    });
    wait_sync(&remote, cx);
    thread.join().unwrap();
    let (_, bytes) = result.lock().unwrap().take().unwrap().unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        local,
        "Downloading cannot mutate local edits"
    );
    let downloaded = root.path().join("downloaded.rovar");
    std::fs::write(&downloaded, bytes).unwrap();
    let loaded = crate::document::load(&downloaded).unwrap();
    assert_eq!(loaded.assets.len(), 2);
    let document = loaded.into_document().unwrap();
    assert_eq!(
        document.pages[0].shapes[0].rect.x, 1.,
        "Apply to the confirmed base, not x=999 from the local editor"
    );
    assert_eq!(
        document.pages[0].shapes[1].media.as_ref().unwrap().hash,
        media[1].hash
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("POST") && requests[0].contains("/transfer "));
    assert!(requests[1].contains(&format!("/media/{} ", media[1].hash)));
}
