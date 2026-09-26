use super::*;
use gpui::{TestAppContext, VisualTestContext, point, size};

#[gpui::test]
fn moving_a_tab_between_windows_preserves_dirty_document_history_and_view(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let source = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    source
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(source.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear());
    visual.simulate_keystrokes("r");
    visual.simulate_mouse_down(
        point(px(400.), px(200.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_move(
        point(px(590.), px(350.)),
        Some(gpui::MouseButton::Left),
        Default::default(),
    );
    visual.simulate_mouse_up(
        point(px(590.), px(350.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    let (drag, session, original) = source
        .update(&mut visual.cx, |studio, window, cx| {
            let editor = studio.active_editor().unwrap();
            editor.update(cx, |editor, _| editor.restore_view([73., 51., 2.]));
            let json = editor
                .read(cx)
                .snapshot_document(&studio.tabs[0].document_id, cx)
                .unwrap()
                .0;
            assert_eq!(
                crate::document::Document::decode(&json)
                    .unwrap()
                    .shapes
                    .len(),
                1
            );
            let drag = studio.drag_payload(1, window, cx);
            studio.begin_tab_drag(drag.clone(), window, cx);
            (drag, studio.session.clone(), json)
        })
        .unwrap();
    let target = visual
        .cx
        .open_window(size(px(1280.), px(800.)), |window, cx| {
            let mut studio = Studio::new(directory.path().into(), window, cx);
            studio.tabs.clear();
            studio.session = session;
            studio
        });
    target
        .update(&mut visual.cx, |studio, window, cx| {
            studio.receive_tab(&drag, None, window, cx);
            let tab = &studio.tabs[0];
            let editor = tab.editor.clone().unwrap();
            assert_eq!(editor.read(cx).view_state(), [73., 51., 2.]);
            assert_eq!(
                editor
                    .read(cx)
                    .snapshot_document(&tab.document_id, cx)
                    .unwrap()
                    .0,
                original
            );
            editor.update(cx, |editor, cx| editor.undo_redo(false, window, cx));
            let undone = editor
                .read(cx)
                .snapshot_document(&tab.document_id, cx)
                .unwrap()
                .0;
            assert!(
                crate::document::Document::decode(&undone)
                    .unwrap()
                    .shapes
                    .is_empty()
            );
            editor.update(cx, |editor, cx| editor.undo_redo(true, window, cx));
            assert_eq!(
                editor
                    .read(cx)
                    .snapshot_document(&tab.document_id, cx)
                    .unwrap()
                    .0,
                original
            );
            studio.save_tab(studio.tabs[0].token, window, cx);
        })
        .unwrap();
    visual.cx.run_until_parked();
    source
        .update(&mut visual.cx, |studio, _, _| {
            assert!(studio.tabs.is_empty());
            assert_eq!(studio.active, None);
        })
        .unwrap();
    target
        .update(&mut visual.cx, |studio, _, _| {
            assert!(studio.tabs[0].error.is_none());
            assert_eq!(
                crate::document::load(&studio.tabs[0].file.path)
                    .unwrap()
                    .json,
                original
            );
        })
        .unwrap();
    assert!(drag.transaction.borrow().owner.is_some());
    let tearoff = target
        .update(&mut visual.cx, |studio, window, cx| {
            let token = studio.tabs[0].token;
            let drag = studio.drag_payload(token, window, cx);
            studio.begin_tab_drag(drag.clone(), window, cx);
            drag.transaction.borrow_mut().native = true;
            assert!(studio.detach_owned_tab(&drag, window, cx));
            drag
        })
        .unwrap();
    visual
        .cx
        .update(|cx| Studio::finish_tab_drag(tearoff.clone(), gpui::DragEnd::Unaccepted, cx));
    visual.cx.run_until_parked();
    assert!(tearoff.transaction.borrow().owner.is_some());
    let detached = visual.cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|w| w.window_id() != source.window_id() && w.window_id() != target.window_id())
            .unwrap()
            .downcast::<Studio>()
            .unwrap()
    });
    detached
        .update(&mut visual.cx, |studio, _, cx| {
            assert_eq!(studio.tabs.len(), 1);
            assert_eq!(
                studio
                    .active_editor()
                    .unwrap()
                    .read(cx)
                    .snapshot_document(&studio.tabs[0].document_id, cx)
                    .unwrap()
                    .0,
                original
            );
        })
        .unwrap();
    visual
        .cx
        .executor()
        .advance_clock(Duration::from_millis(32));
    visual.cx.run_until_parked();
    assert!(target.update(&mut visual.cx, |_, _, _| ()).is_err());
}
