use super::*;
use gpui::{Focusable, TestAppContext, VisualTestContext, WindowHandle, size};

fn open(root: &std::path::Path, cx: &mut TestAppContext) -> WindowHandle<Studio> {
    cx.update(|cx| {
        uic::init(cx);
        init(cx);
    });
    cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(root.into(), window, cx)
    })
}

fn draw(visual: &mut VisualTestContext) {
    visual.cx.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
}

fn add_rectangle(studio: &mut Studio, window: &mut Window, cx: &mut Context<Studio>) {
    let tab = studio
        .tabs
        .iter()
        .find(|tab| Some(tab.token) == studio.active)
        .unwrap();
    let editor = tab.editor.clone().unwrap();
    let (json, _) = editor
        .read(cx)
        .snapshot_document(&tab.document_id, cx)
        .unwrap();
    let mut document = crate::document::Document::decode(&json).unwrap();
    document.pages[0].shapes.push(crate::shape::Shape::new(
        document.pages[0].next_id,
        None,
        crate::shape::ShapeKind::Rectangle,
        crate::artboard::Rect {
            x: 10.,
            y: 20.,
            width: 100.,
            height: 80.,
        },
    ));
    document.pages[0].next_id += 1;
    editor.update(cx, |editor, cx| {
        editor
            .load_document(
                crate::document::Loaded {
                    needs_upgrade: false,
                    json: serde_json::to_vec(&document).unwrap(),
                    assets: Default::default(),
                },
                window,
                cx,
            )
            .unwrap();
    });
}

fn shape_count(handle: WindowHandle<Studio>, cx: &mut TestAppContext) -> usize {
    handle
        .update(cx, |studio, _, cx| {
            let tab = studio
                .tabs
                .iter()
                .find(|tab| Some(tab.token) == studio.active)
                .unwrap();
            let (json, _) = tab
                .editor
                .as_ref()
                .unwrap()
                .read(cx)
                .snapshot_document(&tab.document_id, cx)
                .unwrap();
            crate::document::Document::decode(&json).unwrap().pages[0]
                .shapes
                .len()
        })
        .unwrap()
}

#[gpui::test]
fn menu_actions_follow_canvas_and_text_input_focus(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let handle = open(root.path(), cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    visual.update(|window, cx| assert!(!window.is_action_available(&SaveDocument, cx)));
    visual.dispatch_action(NewDocument);
    draw(&mut visual);
    handle.update(&mut visual.cx, add_rectangle).unwrap();
    draw(&mut visual);
    visual.dispatch_action(SelectAll);
    visual.dispatch_action(Copy);
    visual.dispatch_action(Paste);
    draw(&mut visual);
    assert_eq!(shape_count(handle, &mut visual.cx), 2);
    visual.dispatch_action(Undo);
    draw(&mut visual);
    assert_eq!(shape_count(handle, &mut visual.cx), 1);
    visual.dispatch_action(Redo);
    draw(&mut visual);
    assert_eq!(shape_count(handle, &mut visual.cx), 2);
    visual.dispatch_action(SaveDocument);
    visual.dispatch_action(CloseDocument);
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |studio, window, cx| {
            assert!(studio.tabs.is_empty());
            studio
                .search
                .update(cx, |input, cx| input.set_value("菜单 clipboard", cx));
            studio.search.focus_handle(cx).focus(window, cx);
        })
        .unwrap();
    draw(&mut visual);
    visual.dispatch_action(SelectAll);
    visual.dispatch_action(Cut);
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |studio, _, cx| {
            assert_eq!(studio.search.read(cx).value().as_ref(), "");
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("菜单 clipboard")
            );
        })
        .unwrap();
    visual.dispatch_action(Paste);
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |studio, _, cx| {
            assert_eq!(studio.search.read(cx).value().as_ref(), "菜单 clipboard");
        })
        .unwrap();
}

#[gpui::test]
fn cmd_q_waits_for_storage_in_all_windows_and_preserves_the_session(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let first = open(root.path(), cx);
    let session = first
        .update(cx, |studio, _, _| studio.session.clone())
        .unwrap();
    let second = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        let mut studio = Studio::new(root.path().into(), window, cx);
        studio.session = session;
        studio
    });
    let mut paths = Vec::new();
    for handle in [first, second] {
        paths.push(
            handle
                .update(cx, |studio, window, cx| {
                    studio.new_document(window, cx);
                    // Change the document while its initial save is still in flight.
                    add_rectangle(studio, window, cx);
                    studio.tabs[0].file.path.clone()
                })
                .unwrap(),
        );
    }
    first
        .update(cx, |studio, window, cx| studio.open_settings(window, cx))
        .unwrap();
    let mut visual = VisualTestContext::from_window(first.into(), cx);
    draw(&mut visual);
    first
        .update(&mut visual.cx, |studio, _, cx| {
            studio.library.update(cx, |library, _| library.busy = true);
        })
        .unwrap();
    visual.simulate_keystrokes("cmd-q");
    visual.cx.run_until_parked();
    for handle in [first, second] {
        handle
            .update(&mut visual.cx, |studio, _, _| {
                assert!(studio.closing && studio.quit_requested && studio.awaiting_library);
            })
            .unwrap();
    }
    first
        .update(&mut visual.cx, |studio, _, cx| {
            studio.library.update(cx, |library, cx| {
                library.busy = false;
                cx.notify();
            });
        })
        .unwrap();
    visual.cx.run_until_parked();
    assert!(first.update(&mut visual.cx, |_, _, _| ()).is_err());
    assert!(second.update(&mut visual.cx, |_, _, _| ()).is_err());
    for path in &paths {
        let loaded = crate::document::load(path).unwrap();
        assert_eq!(
            crate::document::Document::decode(&loaded.json)
                .unwrap()
                .pages[0]
                .shapes
                .len(),
            1
        );
    }
    let session: Session =
        serde_json::from_slice(&std::fs::read(root.path().join("session.json")).unwrap()).unwrap();
    for path in paths {
        assert!(session.open.contains(&path));
    }
}

#[gpui::test]
fn quit_menu_keeps_window_open_when_document_save_fails(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let handle = open(root.path(), cx);
    handle
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    cx.run_until_parked();
    handle
        .update(cx, |studio, window, cx| {
            add_rectangle(studio, window, cx);
            // A directory at the destination makes atomic document replacement fail.
            let path = studio.tabs[0].file.path.clone();
            std::fs::remove_file(&path).unwrap();
            std::fs::create_dir(&path).unwrap();
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    visual.dispatch_action(Quit);
    visual.cx.run_until_parked();
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert!(!studio.closing && !studio.quit_requested);
            assert!(studio.error.is_some());
            assert!(studio.tabs[0].editor.is_some());
        })
        .unwrap();
}
