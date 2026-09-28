use super::*;
use crate::workspace::tests::open;
use gpui::TestAppContext;

#[gpui::test]
fn paste_between_pages_does_not_reuse_a_matching_parent_id(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(
                Rect {
                    x: 100.,
                    y: 200.,
                    width: 400.,
                    height: 300.,
                },
                cx,
            );
            let board = this.boards[0].id;
            let id = this.next_id;
            this.next_id += 1;
            this.shapes.push(Shape::new(
                id,
                Some(board),
                ShapeKind::Rectangle,
                Rect {
                    x: 20.,
                    y: 30.,
                    width: 40.,
                    height: 50.,
                },
            ));
            this.select_shape(id, cx);
            this.copy_selection(cx);
            this.add_page(None, window, cx);
            this.add_artboard(
                Rect {
                    x: 900.,
                    y: 800.,
                    width: 400.,
                    height: 300.,
                },
                cx,
            );
            assert_eq!(this.boards[0].id, board);
            this.select(None, cx);
            this.paste_in_place(window, cx);
            let pasted = this.selected_shape().unwrap();
            assert_eq!(pasted.board, None);
            assert_eq!((pasted.rect.x, pasted.rect.y), (120., 230.));
        })
        .unwrap();
}

#[gpui::test]
fn page_controls_rename_duplicate_reorder_and_confirm_delete(cx: &mut TestAppContext) {
    use crate::workspace::tests::{click, draw};
    let window = open(cx);
    window
        .update(cx, |this, _, _| this.sidebar.collapsed = false)
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "add-page");
    let active = window
        .update(&mut visual.cx, |this, _, _| this.pages.active.clone())
        .unwrap();
    let row = window
        .update(&mut visual.cx, |this, _, _| {
            this.pages.row_bounds.borrow()[&active]
        })
        .unwrap();
    assert!(visual.debug_bounds("layers-tab").unwrap().bottom() < row.top());
    visual.simulate_mouse_down(row.center(), MouseButton::Right, Default::default());
    visual.simulate_mouse_up(row.center(), MouseButton::Right, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("page-context-glass-0").is_some());
    click(&mut visual, "page-rename");
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.pages.renaming.as_deref(), Some(active.as_str()));
            this.finish_page_rename(false, window, cx);
            assert_eq!(this.pages.entries.len(), 2);
            let second = this.pages.active.clone();
            this.undo_redo(false, window, cx);
            assert_eq!(this.pages.entries.len(), 1);
            this.undo_redo(true, window, cx);
            assert_eq!(this.pages.active, second);
            this.begin_page_rename(&second, window, cx);
            this.pages
                .input
                .update(cx, |input, cx| input.set_value("Details", cx));
            this.finish_page_rename(true, window, cx);
            assert_eq!(this.pages.current().page.name, "Details");
            this.add_page(Some(&second), window, cx);
            let duplicate = this.pages.active.clone();
            let first = this.pages.entries[0].page.id.clone();
            this.reorder_page(&duplicate, Some(&first), cx);
            assert_eq!(this.pages.entries[0].page.id, duplicate);
            this.undo_redo(false, window, cx);
            assert_eq!(this.pages.entries[0].page.id, first);
            this.undo_redo(true, window, cx);
            assert_eq!(this.pages.entries[0].page.id, duplicate);
            this.pages.delete = Some(duplicate);
        })
        .unwrap();
    draw(&mut visual);
    assert!(visual.debug_bounds("page-delete-dialog").is_some());
    click(&mut visual, "cancel-page-delete");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.pages.entries.len(), 3);
            this.pages.delete = Some(this.pages.active.clone());
        })
        .unwrap();
    click(&mut visual, "confirm-page-delete");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.pages.entries.len(), 2);
            assert!(this.pages.delete.is_none());
        })
        .unwrap();
}

#[gpui::test]
fn media_import_stays_on_its_page_and_survives_transfer(cx: &mut TestAppContext) {
    use crate::workspace::tests::{click, draw};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.png");
    image::RgbaImage::from_pixel(8, 4, image::Rgba([30, 100, 190, 255]))
        .save(&path)
        .unwrap();
    let window = open(cx);
    let first = window
        .update(cx, |this, _, _| this.pages.active.clone())
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "import-media");
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.add_page(None, window, cx)
        })
        .unwrap();
    visual
        .cx
        .simulate_path_prompt_response(|_| Some(vec![path]));
    draw(&mut visual);
    let transfer = window
        .update(&mut visual.cx, |this, window, cx| {
            assert!(this.shapes.is_empty());
            assert_eq!(this.pages.entries[0].page.shapes.len(), 1);
            this.undo_redo(false, window, cx);
            assert_eq!(this.pages.active, first);
            assert!(this.shapes.is_empty());
            this.undo_redo(true, window, cx);
            assert_eq!(this.shapes.len(), 1);
            let second = this.pages.entries[1].page.id.clone();
            this.switch_page(&second, window, cx);
            this.transfer(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap()
        })
        .unwrap();
    let other = open(&mut visual.cx);
    other
        .update(&mut visual.cx, |this, window, cx| {
            this.receive_transfer(transfer, window, cx).unwrap();
            this.switch_page(&first, window, cx);
            assert!(this.shapes[0].media.is_some());
        })
        .unwrap();
}

#[gpui::test]
fn pages_isolate_content_views_and_replay_across_pages(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            let first = this.pages.active.clone();
            this.add_artboard(
                Rect {
                    x: 10.,
                    y: 20.,
                    width: 100.,
                    height: 80.,
                },
                cx,
            );
            this.restore_view([30., 40., 2.]);
            let board = this.boards[0].id;
            this.set_selection(std::collections::BTreeSet::from([board]), cx);
            this.add_page(None, window, cx);
            let second = this.pages.active.clone();
            assert!(this.boards.is_empty());
            this.add_artboard(
                Rect {
                    x: 300.,
                    y: 20.,
                    width: 50.,
                    height: 60.,
                },
                cx,
            );
            this.switch_page(&first, window, cx);
            assert_eq!(this.boards.len(), 1);
            assert_eq!(this.boards[0].rect.x, 10.);
            assert_eq!(this.view_state(), [30., 40., 2.]);
            assert!(this.selection_ids().contains(&board));
            this.undo_redo(false, window, cx);
            assert_eq!(this.pages.active, second);
            assert!(this.boards.is_empty());
            this.undo_redo(true, window, cx);
            assert_eq!(this.boards[0].rect.x, 300.);
            let (json, _) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            let document = crate::document::Document::decode(&json).unwrap();
            assert_eq!(document.pages.len(), 2);
            assert_eq!(document.first_page().id, first);
            assert_eq!(document.pages[0].boards[0].rect.x, 10.);
        })
        .unwrap();
}

#[gpui::test]
fn page_deletion_and_cross_page_move_are_atomic_and_undoable(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            let first = this.pages.active.clone();
            this.add_artboard(
                Rect {
                    x: 10.,
                    y: 20.,
                    width: 100.,
                    height: 80.,
                },
                cx,
            );
            let board = this.boards[0].id;
            this.add_page(None, window, cx);
            let second = this.pages.active.clone();
            this.switch_page(&first, window, cx);
            this.set_selection(std::collections::BTreeSet::from([board]), cx);
            let steps = this.history.borrow().undo_len();
            this.move_selection_to_page(&second, window, cx);
            assert_eq!(this.history.borrow().undo_len(), steps + 1);
            assert_eq!(this.boards.len(), 1);
            this.switch_page(&first, window, cx);
            assert!(this.boards.is_empty());
            this.undo_redo(false, window, cx);
            assert_eq!(this.pages.active, first);
            assert_eq!(this.boards.len(), 1);
            this.switch_page(&second, window, cx);
            assert!(this.boards.is_empty());
            this.delete_page(&first, window, cx);
            assert_eq!(this.pages.entries.len(), 1);
            assert_eq!(this.pages.entries[0].page.id, second);
            this.undo_redo(false, window, cx);
            assert_eq!(this.pages.entries.len(), 2);
            assert_eq!(this.pages.entries[0].page.id, first);
            this.switch_page(&first, window, cx);
            assert_eq!(this.boards.len(), 1);
            this.delete_page(&second, window, cx);
            this.delete_page(&first, window, cx);
            assert_eq!(this.pages.entries.len(), 1);
        })
        .unwrap();
}
