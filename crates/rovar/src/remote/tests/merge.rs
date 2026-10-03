use super::*;
use crate::document::{Document, Page};
use crate::scene::{
    artboard::Rect,
    shape::{Shape, ShapeKind},
};

fn document() -> Document {
    let mut page = Page::empty("Design".into());
    page.shapes.push(Shape::new(
        1,
        None,
        ShapeKind::Rectangle,
        Rect {
            x: 0.,
            y: 0.,
            width: 100.,
            height: 50.,
        },
    ));
    page.next_id = 2;
    Document::single(page)
}

fn run_merge(cx: &mut TestAppContext, divergent: bool) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    let server_path = root.path().join("server.rovar");
    let text_system = cx.update(|cx| cx.text_system().clone());
    let base = document();
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&base).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let original = Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: Kind::Document,
        title: "Design".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    let server_object = Object {
        revision: 2,
        modified: 2,
        ..original.clone()
    };
    let mut local = base.clone();
    local.pages[0].shapes[0].rect.x = 15.;
    let mut server_document = base;
    if divergent {
        server_document.pages[0].shapes[0].rect.x = 40.;
    } else {
        server_document.pages[0].shapes[0].rect.y = 25.;
    }
    crate::document::save_as(
        &server_path,
        &serde_json::to_vec(&server_document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let session = (200, serde_json::to_value(identity()).unwrap());
    let mut responses = vec![
        session.clone(),
        (
            409,
            serde_json::json!({"code":"revision_conflict", "message":"Conflict"}),
        ),
        session.clone(),
        (
            200,
            serde_json::to_value(Snapshot {
                object: server_object.clone(),
                content: STANDARD.encode(std::fs::read(&server_path).unwrap()),
                media: Vec::new(),
            })
            .unwrap(),
        ),
    ];
    if !divergent {
        responses.extend([
            session,
            (
                200,
                serde_json::to_value(Object {
                    revision: 3,
                    modified: 3,
                    title: "Local title".into(),
                    ..server_object
                })
                .unwrap(),
            ),
        ]);
    }
    let (url, saves, thread) = server(responses);
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    cx.update(|cx| confirmed_document(&remote, url, path.clone(), original, cx));
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&local).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let local_bytes = std::fs::read(&path).unwrap();
    remote.update(cx, |r, cx| {
        r.changed(&path, Some("Local title".into()), false, cx)
    });
    sync(&remote, cx);
    remote.update(cx, |r, _| {
        let link = r.link(&path).unwrap();
        assert!(link.dirty);
        assert_eq!(link.conflict, divergent, "{:?}", link.error);
        assert_eq!(link.object.revision, if divergent { 1 } else { 2 });
        assert_eq!(rovar_storage::exists(r.pending_path(link)), divergent);
        if !divergent {
            let confirmed = r.read_baseline(link).unwrap().unwrap();
            let semantic: serde_json::Value =
                serde_json::from_slice(&STANDARD.decode(confirmed.content).unwrap()).unwrap();
            assert_eq!(semantic["pages"][0]["shapes"][0]["rect"]["x"], 0.);
            assert_eq!(semantic["pages"][0]["shapes"][0]["rect"]["y"], 25.);
        }
    });
    if divergent {
        assert_eq!(std::fs::read(&path).unwrap(), local_bytes);
        assert_eq!(saves.lock().unwrap().len(), 1);
    } else {
        let merged = crate::document::load(&path)
            .unwrap()
            .into_document()
            .unwrap();
        assert_eq!(merged.pages[0].shapes[0].rect.x, 15.);
        assert_eq!(merged.pages[0].shapes[0].rect.y, 25.);
        sync(&remote, cx);
        remote.update(cx, |r, _| {
            let link = r.link(&path).unwrap();
            assert!(!link.dirty && !link.conflict);
            assert_eq!(link.object.revision, 3);
            assert!(!rovar_storage::exists(r.pending_path(link)));
        });
        let saves = saves.lock().unwrap();
        assert_eq!(saves.len(), 2);
        assert_eq!(saves[1]["base_revision"], 2);
        assert_ne!(saves[0]["request_id"], saves[1]["request_id"]);
        assert_eq!(saves[1]["title"], "Local title");
    }
    thread.join().unwrap();
}

#[gpui::test]
fn rejected_upload_merges_disjoint_edits_and_uploads_against_the_new_baseline(
    cx: &mut TestAppContext,
) {
    run_merge(cx, false);
}

#[gpui::test]
fn divergent_property_edits_preserve_the_cache_and_rejected_request(cx: &mut TestAppContext) {
    run_merge(cx, true);
}

#[gpui::test]
fn concurrent_image_insertions_keep_both_embedded_assets_after_handle_remapping(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let base = document();
    let text_system = cx.update(|cx| cx.text_system().clone());
    let mut snapshots = Vec::new();
    let mut hashes = BTreeSet::new();
    for (index, color) in [[255, 0, 0, 255], [0, 0, 255, 255]].into_iter().enumerate() {
        let image_path = root.path().join(format!("image-{index}.png"));
        image::RgbaImage::from_pixel(3, 2, image::Rgba(color))
            .save(&image_path)
            .unwrap();
        let asset = crate::media::MediaAsset::load(image_path.clone()).unwrap();
        let mut edited = base.clone();
        let mut shape = edited.pages[0].shapes[0].clone();
        shape.uid = uuid::Uuid::new_v4();
        shape.id = 2;
        shape.kind = ShapeKind::Image;
        edited.pages[0].shapes.push(shape);
        edited.pages[0].next_id = 3;
        edited.pages[0].assets.push(crate::document::AssetUse {
            object: 2,
            fill: false,
            hash: asset.hash.clone(),
        });
        let source = crate::document::AssetSource {
            hash: asset.hash.clone(),
            name: asset.name(),
            path: asset.source.clone(),
            size: [asset.width, asset.height],
        };
        hashes.insert(asset.hash.clone());
        let snapshot = root.path().join(format!("snapshot-{index}.rovar"));
        crate::document::save_as(
            &snapshot,
            &serde_json::to_vec(&edited).unwrap(),
            &[source],
            &text_system,
        )
        .unwrap();
        snapshots.push(std::fs::read(snapshot).unwrap());
        std::fs::remove_file(image_path).unwrap();
    }
    let merged = super::super::merge::assemble(
        &serde_json::to_vec(&base).unwrap(),
        &snapshots[0],
        &snapshots[1],
        &text_system,
    )
    .unwrap();
    let path = root.path().join("merged.rovar");
    std::fs::write(&path, merged).unwrap();
    let loaded = crate::document::load(&path).unwrap();
    assert_eq!(
        loaded.assets.keys().cloned().collect::<BTreeSet<_>>(),
        hashes
    );
    let document = loaded.into_document().unwrap();
    let images: Vec<_> = document.pages[0]
        .shapes
        .iter()
        .filter(|s| s.kind == ShapeKind::Image)
        .collect();
    assert_eq!(images.len(), 2);
    assert_ne!(images[0].id, images[1].id);
    assert_eq!(
        images
            .iter()
            .map(|s| s.media.as_ref().unwrap().hash.clone())
            .collect::<BTreeSet<_>>(),
        hashes
    );
}
