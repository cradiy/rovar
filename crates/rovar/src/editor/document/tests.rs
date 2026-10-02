use super::*;
use crate::{
    app::Studio,
    editor::tests::{click, draw},
    i18n::{self, Language, t},
};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn sync_selection_follows_node_identity_when_handles_are_reassigned(cx: &mut TestAppContext) {
    let window = crate::editor::tests::open(cx);
    window
        .update(cx, |editor, window, cx| {
            let mut page = Page::empty("Design".into());
            let shape = Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 100.,
                    height: 50.,
                },
            );
            page.shapes.push(shape.clone());
            page.next_id = 2;
            let mut document = Document::single(page);
            let loaded = |document: &Document| Loaded {
                json: serde_json::to_vec(document).unwrap(),
                assets: Default::default(),
                needs_upgrade: false,
            };
            editor.load_document(loaded(&document), window, cx).unwrap();
            editor.select_shape(1, cx);
            let view = editor.sync_view(window, cx);
            document.pages[0].shapes[0].id = 2;
            let mut inserted = shape;
            inserted.uid = uuid::Uuid::new_v4();
            document.pages[0].shapes.push(inserted);
            document.pages[0].next_id = 3;
            editor.load_document(loaded(&document), window, cx).unwrap();
            editor.restore_sync_view(view, window, cx);
            assert_eq!(
                editor.selection_ids(),
                std::collections::BTreeSet::from([2])
            );
            let view = editor.sync_view(window, cx);
            document.pages[0].shapes[0].uid = uuid::Uuid::new_v4();
            editor.load_document(loaded(&document), window, cx).unwrap();
            editor.restore_sync_view(view, window, cx);
            assert!(editor.selection_ids().is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn workspace_language_menu_preserves_document_and_input_entities(cx: &mut TestAppContext) {
    i18n::set_language(Language::English).unwrap();
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-n");
    draw(&mut visual);
    let editor = window
        .update(&mut visual.cx, |studio, _, _| {
            studio.active_editor().unwrap()
        })
        .unwrap();
    editor.update(&mut visual.cx, |editor, cx| {
        editor.shapes.push(Shape::new(
            1,
            None,
            ShapeKind::Rectangle,
            Rect {
                x: 100.,
                y: 100.,
                width: 160.,
                height: 120.,
            },
        ));
        editor.shapes[0].name = "My 卡片".into();
        editor.next_id = 2;
        editor.select_shape(1, cx);
    });
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("invalid");
    draw(&mut visual);
    let before = editor.update(&mut visual.cx, |editor, cx| {
        assert_eq!(
            editor.inspector.fields[3].read(cx).value().as_ref(),
            "invalid"
        );
        (
            editor.shapes[0].clone(),
            editor.inspector.fields[3].entity_id(),
            editor.history.borrow().undo_len(),
        )
    });
    for (selector, language, fill) in [
        ("language-zh-CN", Language::Chinese, "填充"),
        ("language-en-US", Language::English, "Fill"),
    ] {
        click(&mut visual, "language-menu");
        assert!(visual.debug_bounds("language-system").is_some());
        let menu = visual.debug_bounds("language-options").unwrap();
        let trigger = visual.debug_bounds("language-menu").unwrap();
        assert!(menu.right() <= trigger.right());
        assert!(menu.left() >= px(0.));
        click(&mut visual, selector);
        assert!(visual.debug_bounds("language-options").is_none());
        editor.update(&mut visual.cx, |editor, cx| {
            assert_eq!(i18n::preference(), language);
            assert_eq!(t("fill"), fill);
            assert_eq!(editor.shapes[0], before.0);
            assert_eq!(editor.inspector.fields[3].entity_id(), before.1);
            assert_eq!(editor.inspector.fields[3].read(cx).value().as_ref(), "160");
            assert_eq!(editor.history.borrow().undo_len(), before.2);
        });
    }
    click(&mut visual, "language-menu");
    click(&mut visual, "language-system");
    assert_eq!(i18n::preference(), Language::System);
    i18n::set_language(Language::English).unwrap();
}
