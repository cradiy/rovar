use super::*;
use crate::remote::tests::{confirmed_document, identity, server, wait_sync};
use base64::{Engine, engine::general_purpose::STANDARD};
use gpui::{Focusable, TestAppContext, size};

#[gpui::test]
fn open_editor_loads_a_merged_dirty_cache_before_the_next_upload(cx: &mut TestAppContext) {
    cx.update(uic::init);
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("documents");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{}.rovar", uuid::Uuid::new_v4()));
    let base = crate::document::Document::single(crate::document::Page::empty("Base".into()));
    let text_system = cx.update(|cx| cx.text_system().clone());
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&base).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let window = cx.open_window(size(px(1000.), px(800.)), |window, cx| {
        Studio::new(root.path().into(), window, cx)
    });
    window
        .update(cx, |studio, window, cx| {
            studio.open_path(path.clone(), window, cx)
        })
        .unwrap();
    let start = std::time::Instant::now();
    loop {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(50));
        if window
            .update(cx, |studio, _, _| studio.tabs[0].editor.is_some())
            .unwrap()
        {
            break;
        }
        assert!(start.elapsed().as_secs() < 8);
        std::thread::sleep(Duration::from_millis(5));
    }
    let object = rovar_api::Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: rovar_api::Kind::Document,
        title: "Design".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    let mut server_document = base.clone();
    server_document
        .pages
        .push(crate::document::Page::empty("Remote page".into()));
    let server_path = root.path().join("server.rovar");
    crate::document::save_as(
        &server_path,
        &serde_json::to_vec(&server_document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let session = (200, serde_json::to_value(identity()).unwrap());
    let (url, _, thread) = server(vec![
        session.clone(),
        (
            409,
            serde_json::json!({"code":"revision_conflict", "message":"Conflict"}),
        ),
        session,
        (
            200,
            serde_json::to_value(rovar_api::Snapshot {
                object: rovar_api::Object {
                    revision: 2,
                    ..object.clone()
                },
                content: STANDARD.encode(std::fs::read(server_path).unwrap()),
                media: vec![],
            })
            .unwrap(),
        ),
    ]);
    let (remote, old_editor) = window
        .update(cx, |studio, window, cx| {
            let remote = studio.remote.clone();
            confirmed_document(&remote, url, path.clone(), object, cx);
            studio.tabs[0].remote_baseline = remote.read(cx).link(&path).unwrap().baseline.clone();
            let mut local = base;
            local.pages[0].name = "Local name".into();
            crate::document::save_as(
                &path,
                &serde_json::to_vec(&local).unwrap(),
                &[],
                &text_system,
            )
            .unwrap();
            let loaded = crate::document::load(&path).unwrap();
            let tab = &mut studio.tabs[0];
            tab.last_saved = loaded.json.clone();
            let editor = tab.editor.as_ref().unwrap().clone();
            editor.update(cx, |editor, cx| {
                editor.load_document(loaded, window, cx).unwrap();
                editor.restore_view([40., 60., 2.]);
            });
            tab.saved_revision = Some(editor.read(cx).document_revision());
            remote.update(cx, |r, cx| r.changed(&path, None, false, cx));
            (remote, editor.entity_id())
        })
        .unwrap();
    crate::remote::tests::sync(&remote, cx);
    thread.join().unwrap();
    cx.run_until_parked();
    window
        .update(cx, |studio, _, cx| {
            let tab = &studio.tabs[0];
            let editor = tab.editor.as_ref().unwrap();
            assert_ne!(editor.entity_id(), old_editor);
            let (json, _) = editor
                .read(cx)
                .snapshot_document(&tab.document_id, cx)
                .unwrap();
            let merged = crate::document::Document::decode(&json).unwrap();
            assert_eq!(merged.pages[0].name, "Local name");
            assert_eq!(merged.pages[1].name, "Remote page");
            assert_eq!(tab.last_saved, crate::document::load(&path).unwrap().json);
            assert_eq!(editor.read(cx).view_state(), [40., 60., 2.]);
            assert!(remote.read(cx).link(&path).unwrap().dirty);
            assert!(studio.protected_paths(cx).is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn opening_a_clean_cache_fetches_new_revision_and_open_clean_editors_follow_updates(
    cx: &mut TestAppContext,
) {
    cx.update(uic::init);
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let documents = root.path().join("documents");
    std::fs::create_dir_all(&documents).unwrap();
    let path = documents.join(format!("{}.rovar", uuid::Uuid::new_v4()));
    let mut document =
        crate::document::Document::single(crate::document::Page::empty("Version 1".into()));
    let mut object = rovar_api::Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: rovar_api::Kind::Document,
        title: "Design".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    cx.update(|cx| {
        crate::document::save_as(
            &path,
            &serde_json::to_vec(&document).unwrap(),
            &[],
            cx.text_system(),
        )
        .unwrap()
    });
    let session = serde_json::to_value(identity()).unwrap();
    let mut responses = Vec::new();
    for revision in [2, 3, 4] {
        let snapshot = root.path().join(format!("snapshot-{revision}.rovar"));
        document.pages[0].name = format!("Version {revision}");
        cx.update(|cx| {
            crate::document::save_as(
                &snapshot,
                &serde_json::to_vec(&document).unwrap(),
                &[],
                cx.text_system(),
            )
            .unwrap()
        });
        object.revision = revision;
        responses.push((200, session.clone()));
        let listing = if revision == 2 {
            serde_json::to_value(&object).unwrap()
        } else {
            serde_json::to_value(rovar_api::Changes {
                objects: vec![object.clone()],
                cursor: revision,
                has_more: false,
            })
            .unwrap()
        };
        responses.push((200, listing));
        if revision != 4 {
            responses.push((
                200,
                serde_json::to_value(rovar_api::Snapshot {
                    media: Vec::new(),
                    object: object.clone(),
                    content: STANDARD.encode(std::fs::read(snapshot).unwrap()),
                })
                .unwrap(),
            ));
        }
    }
    object.revision = 1;
    let (url, saves, thread) = server(responses);
    let window = cx.open_window(size(px(1000.), px(800.)), |window, cx| {
        Studio::new(root.path().into(), window, cx)
    });
    let (remote, connection) = window
        .update(cx, |studio, window, cx| {
            let remote = studio.remote.clone();
            let id = confirmed_document(&remote, url, path.clone(), object, cx);
            remote.update(cx, |remote, _| remote.busy = true);
            studio.open_path(path.clone(), window, cx);
            assert_eq!(studio.tabs.len(), 1, "Opening during sync must be queued");
            (remote, id)
        })
        .unwrap();
    cx.run_until_parked();
    remote.update(cx, |remote, _| remote.busy = false);
    let start = std::time::Instant::now();
    loop {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(50));
        if window
            .update(cx, |studio, _, _| studio.tabs[0].editor.is_some())
            .unwrap()
        {
            break;
        }
        assert!(start.elapsed().as_secs() < 8);
        std::thread::sleep(Duration::from_millis(5));
    }
    let assert_version = |cx: &mut TestAppContext, version: &str| {
        window
            .update(cx, |studio, _, cx| {
                let tab = &studio.tabs[0];
                let (json, _) = tab
                    .editor
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .snapshot_document(&tab.document_id, cx)
                    .unwrap();
                let doc = crate::document::Document::decode(&json).unwrap();
                assert_eq!(doc.pages[0].name, version);
                let link = studio.remote.read(cx).link(&path).unwrap();
                assert!(!link.dirty && !link.conflict);
            })
            .unwrap();
    };
    assert_version(cx, "Version 2");
    window
        .update(cx, |studio, window, cx| {
            studio.tabs[0]
                .editor
                .as_ref()
                .unwrap()
                .update(cx, |editor, _| editor.restore_view([40., 60., 2.]));
            studio.search.focus_handle(cx).focus(window, cx);
            studio.refresh_connection(connection.clone(), cx)
        })
        .unwrap();
    wait_sync(&remote, cx);
    cx.run_until_parked();
    assert_version(cx, "Version 3");
    window
        .update(cx, |studio, window, cx| {
            assert!(studio.search.focus_handle(cx).is_focused(window));
            assert_eq!(
                studio.tabs[0]
                    .editor
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .view_state(),
                [40., 60., 2.]
            );
        })
        .unwrap();
    // Unsaved editor changes must protect both the on-disk cache and editor.
    window
        .update(cx, |studio, window, cx| {
            studio.tabs[0]
                .editor
                .as_ref()
                .unwrap()
                .update(cx, |editor, cx| {
                    let mut loaded = crate::document::load(&path).unwrap();
                    let mut local = crate::document::Document::decode(&loaded.json).unwrap();
                    local.pages[0].name = "Local edit".into();
                    loaded.json = serde_json::to_vec(&local).unwrap();
                    editor.load_document(loaded, window, cx).unwrap();
                });
            studio.refresh_connection(connection.clone(), cx);
        })
        .unwrap();
    wait_sync(&remote, cx);
    thread.join().unwrap();
    assert!(saves.lock().unwrap().is_empty());
    assert_eq!(
        crate::document::load(&path)
            .unwrap()
            .into_document()
            .unwrap()
            .pages[0]
            .name,
        "Version 3"
    );
}
