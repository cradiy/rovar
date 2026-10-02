use super::*;
use crate::{
    document::{Document, Loaded, Page},
    scene::artboard::Rect,
    scene::shape::{Shape, ShapeKind},
};
use gpui::{TestAppContext, VisualTestContext, point, size};

fn loaded(document: &Document) -> Loaded {
    Loaded {
        json: serde_json::to_vec(document).unwrap(),
        assets: Default::default(),
        needs_upgrade: false,
    }
}
fn content(json: &[u8]) -> serde_json::Value {
    let mut value: serde_json::Value = serde_json::from_slice(json).unwrap();
    for page in value["pages"].as_array_mut().unwrap() {
        page.as_object_mut().unwrap().remove("next_id");
    }
    value
}
fn draw(visual: &mut VisualTestContext) {
    visual.cx.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
}
fn click(visual: &mut VisualTestContext, selector: &'static str) {
    draw(visual);
    let center = visual.debug_bounds(selector).unwrap().center();
    visual.simulate_mouse_move(center, None, Default::default());
    visual.simulate_mouse_down(center, gpui::MouseButton::Left, Default::default());
    visual.simulate_mouse_up(center, gpui::MouseButton::Left, Default::default());
    draw(visual);
}

#[gpui::test]
fn comparison_copies_server_objects_into_local_history_without_modifying_server(
    cx: &mut TestAppContext,
) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let snapshot = root.path().join("preview.rovar");
    std::fs::write(&snapshot, b"temporary preview").unwrap();
    let handle = cx.open_window(size(px(1100.), px(800.)), |window, cx| {
        Studio::new(root.path().into(), window, cx)
    });
    let (original, server_editor, before, server_before) = handle
        .update(cx, |studio, window, cx| {
            studio.new_document(window, cx);
            let original = studio.active_editor().unwrap();
            let mut page = Page::empty("Shared page".into());
            page.shapes.push(Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                Rect {
                    x: 100.,
                    y: 100.,
                    width: 180.,
                    height: 120.,
                },
            ));
            page.next_id = 2;
            let document = Document {
                colors: Default::default(),
                id: studio.tabs[0].document_id.clone(),
                pages: vec![page],
                components: Default::default(),
            };
            original.update(cx, |editor, cx| {
                editor.load_document(loaded(&document), window, cx).unwrap();
                editor.restore_view([0., 0., 1.]);
                editor.enable_preview(true);
            });
            let views = original.read(cx).page_views();
            let before = original
                .read(cx)
                .snapshot_document(&document.id, cx)
                .unwrap()
                .0;
            let mut pages: Vec<_> = document
                .pages
                .iter()
                .map(|p| (p.id.clone(), p.name.clone()))
                .collect();
            let local = Version {
                editor: original.clone(),
                pages: pages.iter().map(|p| p.0.clone()).collect(),
            };
            let mut remote_document = document;
            remote_document.pages[0].shapes[0].rect.width = 350.;
            remote_document
                .pages
                .push(Page::empty("Server only".into()));
            let (server, server_pages) = version(loaded(&remote_document), window, cx).unwrap();
            let server_editor = server.editor.clone();
            let server_before = server_editor
                .read(cx)
                .snapshot_document(&remote_document.id, cx)
                .unwrap()
                .0;
            pages.push(server_pages[1].clone());
            studio.comparison = Some(Panel {
                request: uuid::Uuid::new_v4(),
                path: studio.tabs[0].file.path.clone(),
                title: "Comparison".into(),
                local,
                server: Some(server),
                object: None,
                base_revision: 1,
                connection: String::new(),
                generation: 0,
                resolving: false,
                error: None,
                original_views: views,
                pages,
                page: 0,
                server_visible: false,
                view: [0., 0., 1.],
                loading: false,
                failed: false,
                _snapshot: Some(SnapshotFile(snapshot.clone())),
            });
            studio.focus_comparison(window, cx);
            (original, server_editor, before, server_before)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "compare-server");
    let bounds = visual.debug_bounds("comparison-canvas").unwrap();
    let target = bounds.origin + point(px(140.), px(140.));
    visual.simulate_mouse_move(target, None, Default::default());
    visual.simulate_mouse_down(target, gpui::MouseButton::Left, Default::default());
    visual.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    // The server can copy, but cut, paste, delete and undo must never mutate it.
    visual.simulate_keystrokes("ctrl-c ctrl-x ctrl-v delete ctrl-z");
    click(&mut visual, "compare-local");
    visual.simulate_keystrokes("ctrl-shift-v");
    draw(&mut visual);
    let merged = handle
        .update(&mut visual.cx, |studio, _, cx| {
            let id = &studio.tabs[0].document_id;
            let json = original.read(cx).snapshot_document(id, cx).unwrap().0;
            let document = Document::decode(&json).unwrap();
            assert_eq!(document.pages[0].shapes.len(), 2);
            assert_eq!(document.pages[0].shapes[1].rect.width, 350.);
            assert_eq!(
                server_editor.read(cx).snapshot_document(id, cx).unwrap().0,
                server_before
            );
            json
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    handle
        .update(&mut visual.cx, |studio, _, cx| {
            assert_eq!(
                content(
                    &original
                        .read(cx)
                        .snapshot_document(&studio.tabs[0].document_id, cx)
                        .unwrap()
                        .0
                ),
                content(&before)
            );
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    click(&mut visual, "compare-server");
    let start =
        visual.debug_bounds("comparison-canvas").unwrap().origin + point(px(500.), px(400.));
    let end = start + point(px(60.), px(40.));
    visual.simulate_mouse_down(start, gpui::MouseButton::Middle, Default::default());
    visual.simulate_mouse_move(end, Some(gpui::MouseButton::Middle), Default::default());
    visual.simulate_mouse_up(end, gpui::MouseButton::Middle, Default::default());
    click(&mut visual, "compare-local");
    handle
        .update(&mut visual.cx, |_, _, cx| {
            assert_eq!(original.read(cx).view_state(), [60., 40., 1.])
        })
        .unwrap();
    click(&mut visual, "compare-server");
    click(&mut visual, "compare-next-page");
    click(&mut visual, "compare-local");
    assert!(
        visual.debug_bounds("editor-area").is_none(),
        "Missing page must not display an unrelated local page"
    );
    click(&mut visual, "compare-prev-page");
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("compare-local").is_none());
    assert!(!snapshot.exists());
    handle
        .update(&mut visual.cx, |studio, _, cx| {
            assert_eq!(original.read(cx).view_state(), [0., 0., 1.]);
            assert_eq!(
                original
                    .read(cx)
                    .snapshot_document(&studio.tabs[0].document_id, cx)
                    .unwrap()
                    .0,
                merged
            );
        })
        .unwrap();
    // Returning to the normal editor retains the same history, not a replacement document.
    visual.simulate_keystrokes("ctrl-z");
    handle
        .update(&mut visual.cx, |studio, _, cx| {
            assert_eq!(
                content(
                    &original
                        .read(cx)
                        .snapshot_document(&studio.tabs[0].document_id, cx)
                        .unwrap()
                        .0
                ),
                content(&before)
            );
        })
        .unwrap();
}

#[gpui::test]
fn accepting_server_reloads_the_document_and_clears_conflict(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let documents = root.path().join("documents");
    std::fs::create_dir(&documents).unwrap();
    let path = documents.join(format!("{}.rovar", uuid::Uuid::new_v4()));
    let snapshot = root.path().join("snapshot.rovar");
    let local_document = Document::single(Page::empty("Local page".into()));
    let mut server_document = local_document.clone();
    server_document.pages[0].name = "Server page".into();
    let local_json = serde_json::to_vec(&local_document).unwrap();
    let server_json = serde_json::to_vec(&server_document).unwrap();
    cx.update(|cx| {
        crate::document::save_as(&path, &local_json, &[], cx.text_system()).unwrap();
        crate::document::save_as(&snapshot, &server_json, &[], cx.text_system()).unwrap();
    });
    let path = std::fs::canonicalize(path).unwrap();
    let space =
        serde_json::json!({"id":"personal", "name":"Personal", "kind":"personal", "role":"owner"});
    let identity = serde_json::json!({
        "server_id":"test", "user_id":"alice", "username":"Alice",
        "api_version":rovar_api::VERSION, "spaces":[space.clone()],
        "registration":{"personal":true,"teams":true}
    });
    let object = serde_json::json!({"id":"object", "kind":"document", "title":"Design", "revision":1, "created":1, "modified":1, "deleted":false});
    let catalog = serde_json::json!({
        "servers":{},
        "connections":[{"id":"connection", "url":"http://127.0.0.1:1", "identity":identity.clone(), "space":space}],
        "links":{path.to_string_lossy().as_ref(): {
            "connection":"connection", "object":object.clone(), "dirty":true, "conflict":true, "digest":""
        }}
    });
    std::fs::write(
        root.path().join("servers.json"),
        serde_json::to_vec(&catalog).unwrap(),
    )
    .unwrap();
    let handle = cx.open_window(size(px(680.), px(740.)), |window, cx| {
        Studio::new(root.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| {
            studio.remote.update(cx, |r, cx| {
                r.connect(
                    "http://127.0.0.1:1".into(),
                    serde_json::from_value(identity).unwrap(),
                    "token".into(),
                    cx,
                )
                .unwrap();
            });
            studio.open_path(path.clone(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    handle
        .update(cx, |studio, window, cx| {
            let editor = studio.active_editor().unwrap();
            // An unsaved local edit must finish saving before the server replaces it.
            let mut edited = local_document.clone();
            edited.pages[0].name = "Unsaved local edit".into();
            editor.update(cx, |e, cx| {
                e.load_document(loaded(&edited), window, cx).unwrap();
                e.enable_preview(true);
            });
            let views = editor.read(cx).page_views();
            let (server, pages) =
                version(crate::document::load(&snapshot).unwrap(), window, cx).unwrap();
            let mut reviewed: rovar_api::Object = serde_json::from_value(object).unwrap();
            reviewed.revision = 3;
            let generation = studio
                .remote
                .read(cx)
                .connection("connection")
                .unwrap()
                .generation;
            studio.comparison = Some(Panel {
                request: uuid::Uuid::new_v4(),
                path: path.clone(),
                title: "Design".into(),
                local: Version {
                    editor,
                    pages: pages.iter().map(|p| p.0.clone()).collect(),
                },
                server: Some(server),
                object: Some(reviewed),
                base_revision: 1,
                connection: "connection".into(),
                generation,
                resolving: false,
                error: None,
                original_views: views,
                pages,
                page: 0,
                server_visible: true,
                view: [0., 0., 1.],
                loading: false,
                failed: false,
                _snapshot: Some(SnapshotFile(snapshot.clone())),
            });
            cx.notify();
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "compare-resolve");
    for _ in 0..10 {
        visual
            .cx
            .executor()
            .advance_clock(std::time::Duration::from_millis(100));
        visual.cx.run_until_parked();
    }
    handle
        .update(&mut visual.cx, |studio, _, cx| {
            assert!(
                studio.comparison.is_none(),
                "Resolution should close the comparison: panel={:?}, tab={:?}, saving={}, revision={:?}/{:?}, remote_busy={}",
                studio.comparison.as_ref().map(|p| (p.resolving, &p.error)),
                studio.tabs[0].error, studio.tabs[0].saving, studio.tabs[0].saved_revision,
                studio.tabs[0].editor.as_ref().map(|e| e.read(cx).document_revision()),
                studio.remote.read(cx).busy
            );
            let tab = &studio.tabs[0];
            let editor = tab.editor.as_ref().unwrap();
            assert_eq!(
                editor
                    .read(cx)
                    .snapshot_document(&tab.document_id, cx)
                    .unwrap()
                    .0,
                server_json
            );
            assert_eq!(tab.last_saved, server_json);
            assert_eq!(
                tab.saved_revision,
                Some(editor.read(cx).document_revision())
            );
            let link = studio.remote.read(cx).link(&path).unwrap();
            assert!(!link.conflict && !link.dirty);
            assert_eq!(link.object.revision, 3);
        })
        .unwrap();
    assert_eq!(crate::document::load(&path).unwrap().json, server_json);
    assert!(!snapshot.exists());
}
