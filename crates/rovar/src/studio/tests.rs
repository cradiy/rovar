use super::*;
use gpui::{TestAppContext, VisualTestContext, point, size};

#[gpui::test]
fn home_tabs_documents_and_reopening_preserve_independent_documents(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_owned();
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(root.clone(), window, cx)
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("home").is_some());
    visual.simulate_keystrokes("ctrl-n");
    visual.cx.run_until_parked();
    let first = window
        .update(&mut visual.cx, |this, window, cx| {
            assert!(!this.tabs[0].saving);
            let path = this.tabs[0].file.path.clone();
            assert!(path.exists());
            this.active_editor()
                .unwrap()
                .update(cx, |editor, _| editor.restore_view([123., 45., 2.]));
            this.new_document(window, cx);
            assert_eq!(this.tabs.len(), 2);
            assert_eq!(
                this.active_editor().unwrap().read(cx).view_state(),
                [0., 0., 1.]
            );
            this.select_tab(Some(1), window, cx);
            assert_eq!(
                this.active_editor().unwrap().read(cx).view_state(),
                [123., 45., 2.]
            );
            this.close_tab(1, window, cx);
            assert_eq!(this.active, None);
            path
        })
        .unwrap();
    visual.cx.run_until_parked();
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.open_path(first.clone(), window, cx)
        })
        .unwrap();
    visual.cx.run_until_parked();
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(this.tabs.iter().all(|tab| tab.error.is_none()));
            assert_eq!(
                this.active_editor().unwrap().read(cx).view_state(),
                [123., 45., 2.]
            );
        })
        .unwrap();
    let restored = visual
        .cx
        .open_window(size(px(1280.), px(800.)), |window, cx| {
            Studio::new(root, window, cx)
        });
    restored
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.active, None);
            assert_eq!(this.tabs.len(), 2);
            assert!(
                this.tabs
                    .iter()
                    .all(|tab| tab.editor.is_none() && !tab.loading)
            );
        })
        .unwrap();
    // Rendering the restored home must not trigger full document/media loads.
    let mut restored_visual = VisualTestContext::from_window(restored.into(), &visual.cx);
    restored_visual.update(|window, cx| window.draw(cx).clear());
    assert!(restored_visual.debug_bounds("home").is_some());
    let bounds = restored_visual.debug_bounds("home-new").unwrap();
    restored_visual.simulate_mouse_down(
        point(bounds.center().x, bounds.center().y),
        gpui::MouseButton::Left,
        Default::default(),
    );
    restored_visual.simulate_mouse_up(bounds.center(), gpui::MouseButton::Left, Default::default());
    restored_visual.cx.run_until_parked();
    restored
        .update(&mut restored_visual.cx, |this, _, _| {
            assert_eq!(this.tabs.len(), 3)
        })
        .unwrap();
}

#[gpui::test]
fn closing_during_first_save_flushes_newer_edits_and_recovers_without_session(
    cx: &mut TestAppContext,
) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    let path = window
        .update(cx, |studio, window, cx| {
            studio.new_document(window, cx);
            let tab = &studio.tabs[0];
            let editor = tab.editor.clone().unwrap();
            let json = editor
                .read(cx)
                .snapshot_document(&tab.document_id, cx)
                .unwrap()
                .0;
            let mut document = crate::document::Document::decode(&json).unwrap();
            document.shapes.push(crate::shape::Shape::new(
                1,
                None,
                crate::shape::ShapeKind::Ellipse,
                crate::artboard::Rect {
                    x: 10.,
                    y: 20.,
                    width: 200.,
                    height: 120.,
                },
            ));
            document.next_id = 2;
            editor
                .update(cx, |editor, cx| {
                    editor.load_document(
                        crate::document::Loaded {
                            json: serde_json::to_vec(&document).unwrap(),
                            assets: Default::default(),
                        },
                        window,
                        cx,
                    )
                })
                .unwrap();
            let path = tab.file.path.clone();
            studio.close_tab(1, window, cx);
            path
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |studio, _, _| assert!(studio.tabs.is_empty()))
        .unwrap();
    let loaded = crate::document::load(&path).unwrap();
    assert_eq!(
        crate::document::Document::decode(&loaded.json)
            .unwrap()
            .shapes
            .len(),
        1
    );
    std::fs::remove_file(directory.path().join("session.json")).unwrap();
    let recovered = files::recover_documents(directory.path().into());
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].path, path);
}
