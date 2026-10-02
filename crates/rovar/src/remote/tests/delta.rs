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
    let base = rovar_format::delta::Snapshot::from_bytes(&base_bytes, rovar_api::MAX_CONTENT_BYTES)
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
            rovar_api::MAX_CONTENT_BYTES,
        )
        .unwrap();
    let second = STANDARD
        .decode(saves[2]["content"].as_str().unwrap())
        .unwrap();
    let after_second = after_first
        .apply(
            &serde_json::from_slice(&second).unwrap(),
            rovar_api::MAX_CONTENT_BYTES,
        )
        .unwrap();
    let expected = rovar_format::delta::Snapshot::from_bytes(
        &std::fs::read(&path).unwrap(),
        rovar_api::MAX_CONTENT_BYTES,
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
