use super::*;
use crate::workspace::tests::{click, draw, open};
use gpui::{EntityInputHandler, Modifiers, TestAppContext, VisualTestContext, WindowHandle};

#[gpui::test]
fn repeated_multiselection_copy_keeps_relative_geometry_and_cancelled_motion(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            this.set_selection(BTreeSet::from([1, 2]), cx);
            this.duplicate_selection(window, cx);
            assert_eq!(this.selection_ids(), BTreeSet::from([4, 5]));
            this.batch_before = this.before_geometry();
            this.move_selection(point(-100., 0.));
            this.finish_selection_move(cx);
            this.duplicate_selection(window, cx);
            assert_eq!(this.selection_ids(), BTreeSet::from([6, 7]));
            assert_eq!(this.world_rect(6).unwrap().x, -60.);
            assert_eq!(this.world_rect(6).unwrap().y, 140.);
            assert_eq!(
                this.world_rect(7).unwrap().x - this.world_rect(6).unwrap().x,
                120.
            );
            let original = this.world_rect(6).unwrap();
            this.batch_before = this.before_geometry();
            this.begin(
                GestureKind::SelectionMove,
                point(px(100.), px(100.)),
                MouseButton::Left,
                window,
                cx,
            );
            this.move_gesture(point(px(160.), px(120.)), false, cx);
            this.cancel_gesture(window, cx);
            assert_eq!(this.world_rect(6), Some(original));
            this.duplicate_selection(window, cx);
            assert_eq!(this.world_rect(8).unwrap().x, -140.);
            assert_eq!(this.world_rect(8).unwrap().y, 160.);
        })
        .unwrap();
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}
fn fixture(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.zoom = 0.8;
            this.view.pan = point(30., 20.);
            this.shapes.push(Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                rect(100., 100., 60., 50.),
            ));
            this.shapes.push(Shape::new(
                2,
                None,
                ShapeKind::Ellipse,
                rect(220., 150., 90., 60.),
            ));
            let text = this.make_text(3, None, rect(100., 270., 180., 80.), window, cx);
            text.editor.update(cx, |e, cx| {
                e.replace_text_in_range(None, "多选文字", window, cx)
            });
            this.texts.push(text);
            this.next_id = 4;
            this.select(None, cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    window
}
fn screen(visual: &mut VisualTestContext, p: Point<f32>) -> Point<Pixels> {
    visual.update(|window, cx| {
        let root = window.root::<Workspace>().unwrap().unwrap();
        let this = root.read(cx);
        let p = this.view.screen(p);
        this.bounds.get().origin + point(px(p.x), px(p.y))
    })
}

#[gpui::test]
fn duplicate_repeats_adjusted_displacement_and_resets_after_selection_changes(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            this.select_shape(1, cx);
            this.duplicate_selection(window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = visual.debug_bounds("shape-4").unwrap().center();
    let end = start + point(px(80.), px(-16.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-d");
    visual.simulate_keystrokes("ctrl-d");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            for (id, x) in [(4, 220.), (5, 340.), (6, 460.)] {
                let rect = this.world_rect(id).unwrap();
                assert!((rect.x - x).abs() < 0.001, "{id}: {rect:?}");
                assert!((rect.y - 100.).abs() < 0.001);
            }
            this.select_shape(1, cx);
            this.select_shape(6, cx);
            this.duplicate_selection(window, cx);
            assert!((this.world_rect(7).unwrap().x - 480.).abs() < 0.001);
            assert!((this.world_rect(7).unwrap().y - 120.).abs() < 0.001);
            this.replay_history(false, window, cx);
            assert!(this.world_rect(7).is_none());
            assert!(this.duplicate.is_none());
        })
        .unwrap();
}
fn pointer_click(visual: &mut VisualTestContext, p: Point<f32>, shift: bool) {
    let p = screen(visual, p);
    visual.simulate_click(
        p,
        Modifiers {
            shift,
            ..Default::default()
        },
    );
    draw(visual);
}
fn pointer_drag(visual: &mut VisualTestContext, a: Point<f32>, b: Point<f32>, cancel: bool) {
    let a = screen(visual, a);
    let b = screen(visual, b);
    visual.simulate_mouse_down(a, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(b, MouseButton::Left, Default::default());
    draw(visual);
    if cancel {
        visual.simulate_keystrokes("escape");
    }
    visual.simulate_mouse_up(b, MouseButton::Left, Default::default());
    draw(visual);
}

#[gpui::test]
fn pointer_multiselect_marquee_and_layer_selection_share_inspector(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    pointer_click(&mut visual, point(120., 120.), false);
    pointer_click(&mut visual, point(250., 175.), true);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([1, 2]))
        })
        .unwrap();
    assert!(visual.debug_bounds("multi-properties").is_some());
    assert!(visual.debug_bounds("multi-selection-bounds").is_some());
    for (selector, expected) in [
        ("multi-selection-object-1", rect(100., 100., 60., 50.)),
        ("multi-selection-object-2", rect(220., 150., 90., 60.)),
    ] {
        let bounds = visual.debug_bounds(selector).unwrap();
        assert_eq!(
            bounds.origin,
            screen(&mut visual, point(expected.x, expected.y))
        );
        assert_eq!(
            bounds.size,
            gpui::size(px(expected.width * 0.8), px(expected.height * 0.8))
        );
    }
    pointer_click(&mut visual, point(120., 120.), true);
    assert!(visual.debug_bounds("multi-selection-object-1").is_none());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([2]))
        })
        .unwrap();
    pointer_drag(&mut visual, point(80., 80.), point(320., 370.), false);
    assert!(visual.debug_bounds("multi-selection-object-1").is_some());
    assert!(visual.debug_bounds("multi-selection-object-2").is_some());
    assert!(visual.debug_bounds("multi-selection-object-3").is_some());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([1, 2, 3]))
        })
        .unwrap();
    pointer_drag(&mut visual, point(400., 400.), point(450., 450.), true);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([1, 2, 3]))
        })
        .unwrap();
    click(&mut visual, "toggle-layers");
    let p = visual.debug_bounds("layer-2").unwrap().center();
    visual.simulate_click(
        p,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([1, 3]))
        })
        .unwrap();
    click(&mut visual, "layer-lock-1");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([3]))
        })
        .unwrap();
}

#[gpui::test]
fn overlapping_multiselection_keeps_each_outline_and_allows_dragging(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, _| {
            this.shapes[0].rect = rect(100., 100., 160., 120.);
            this.shapes[1].rect = rect(120., 120., 40., 30.);
        })
        .unwrap();
    draw(&mut visual);
    pointer_click(&mut visual, point(110., 110.), false);
    pointer_click(&mut visual, point(140., 135.), true);
    let outer = visual.debug_bounds("multi-selection-object-1").unwrap();
    let inner = visual.debug_bounds("multi-selection-object-2").unwrap();
    assert!(outer.contains(&inner.origin));
    pointer_drag(&mut visual, point(110., 110.), point(140., 140.), false);
    for (selector, before) in [
        ("multi-selection-object-1", outer),
        ("multi-selection-object-2", inner),
    ] {
        let after = visual.debug_bounds(selector).unwrap();
        assert_eq!(after.origin, before.origin + point(px(24.), px(24.)));
        assert_eq!(after.size, before.size);
    }
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([1, 2]));
            assert!(this.shapes.iter().all(|shape| !shape.stroke.enabled));
        })
        .unwrap();
}

#[gpui::test]
fn group_drag_reparents_without_jumps_and_cancel_preserves_redo(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let before = window
        .update(&mut visual.cx, |this, _, cx| {
            this.add_artboard(rect(350., 90., 300., 230.), cx);
            this.set_selection(BTreeSet::from([1, 2]), cx);
            this.history.borrow().undo_len()
        })
        .unwrap();
    draw(&mut visual);
    pointer_drag(&mut visual, point(120., 120.), point(420., 150.), false);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].board, Some(4));
            assert_eq!(this.shapes[1].board, Some(4));
            assert_eq!(this.world_rect(1).unwrap(), rect(400., 130., 60., 50.));
            assert_eq!(this.world_rect(2).unwrap(), rect(520., 180., 90., 60.));
            assert_eq!(this.history.borrow().undo_len(), before + 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].board, None);
            assert_eq!(this.shapes[0].rect.x, 100.);
            assert_eq!(this.selection_ids(), BTreeSet::from([1, 2]));
        })
        .unwrap();
    pointer_drag(&mut visual, point(120., 120.), point(180., 150.), true);
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.world_rect(1).unwrap().x, 400.)
        })
        .unwrap();
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes.is_empty());
            assert_eq!(this.boards.len(), 1);
            assert_eq!(this.texts.len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 2);
            assert_eq!(this.selection_ids(), BTreeSet::from([1, 2]));
        })
        .unwrap();
}

#[gpui::test]
fn marquee_filters_locked_hidden_and_deduplicates_board_children(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.add_artboard(rect(350., 90., 220., 210.), cx);
            this.shapes.push(Shape::new(
                5,
                Some(4),
                ShapeKind::Rectangle,
                rect(20., 20., 50., 50.),
            ));
            this.next_id = 6;
            this.shapes[0].layer.locked = true;
            this.shapes[1].layer.hidden = true;
            this.select(None, cx);
        })
        .unwrap();
    draw(&mut visual);
    pointer_drag(&mut visual, point(80., 70.), point(600., 370.), false);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([3, 4]))
        })
        .unwrap();
    let original = window
        .update(&mut visual.cx, |this, _, _| this.shapes[2].rect)
        .unwrap();
    visual.simulate_keystrokes("shift-right");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect.x, 360.);
            assert_eq!(this.shapes[2].rect, original);
            assert_eq!(this.texts[0].rect.x, 110.);
        })
        .unwrap();
}

#[gpui::test]
fn copies_preserve_rich_text_and_children_and_clipboard_is_not_stale(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.add_artboard(rect(350., 90., 220., 300.), cx);
            this.texts[0].board = Some(4);
            this.texts[0].rect = rect(10., 15., 180., 80.);
            this.texts[0].layer.hidden = true;
            this.shapes[0].board = Some(4);
            this.shapes[0].rect = rect(25., 120., 60., 50.);
            this.shapes[0].layer.locked = true;
            this.texts[0]
                .editor
                .update(cx, |e, cx| e.apply_style(StyleChange::Size(37.), cx));
            this.set_selection(BTreeSet::from([4, 1]), cx);
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-c");
    visual.simulate_keystrokes("ctrl-v");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards.len(), 2);
            assert_eq!(this.shapes.len(), 3);
            assert_eq!(this.texts.len(), 2);
            let board = this.boards.last().unwrap();
            let t = this.texts.last().unwrap();
            let s = this.shapes.last().unwrap();
            assert_eq!(t.board, Some(board.id));
            assert_eq!(s.board, Some(board.id));
            assert!(t.layer.hidden && s.layer.locked);
            assert_eq!(board.rect.x, 370.);
            assert_eq!(s.rect.x, 25.);
        })
        .unwrap();
    window
        .update(&mut visual.cx, |this, _, cx| {
            let e = this.texts.last().unwrap().editor.read(cx);
            assert_eq!(e.content, "多选文字");
            assert_eq!(e.effective_style().size, 37.);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards.len(), 1);
            assert_eq!(this.shapes.len(), 2);
            assert_eq!(this.texts.len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    visual
        .update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("外部文字".into())));
    visual.simulate_keystrokes("ctrl-v");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards.len(), 2)
        })
        .unwrap();
}

#[gpui::test]
fn mixed_properties_apply_only_one_field_and_scrub_as_one_undo(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.set_selection(BTreeSet::from([1, 2, 3]), cx);
        })
        .unwrap();
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.field_value(3, cx), Some(String::new()));
            assert_eq!(this.field_value(5, cx), Some(String::new()));
        })
        .unwrap();
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("120");
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes.iter().all(|s| s.rect.width == 120.));
            assert_eq!(this.texts[0].rect.width, 120.);
            assert_eq!(this.shapes[0].rect.height, 50.);
        })
        .unwrap();
    let before = window
        .update(&mut visual.cx, |this, _, _| {
            this.history.borrow().undo_len()
        })
        .unwrap();
    let start = visual.debug_bounds("property-drag-1").unwrap().center();
    let end = start + point(px(24.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    for dx in [12., 24.] {
        visual.simulate_mouse_move(
            start + point(px(dx), px(0.)),
            MouseButton::Left,
            Default::default(),
        );
        draw(&mut visual);
    }
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 124.);
            assert_eq!(this.shapes[1].rect.x, 244.);
            assert_eq!(this.texts[0].rect.x, 124.);
            assert_eq!(this.history.borrow().undo_len(), before + 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 100.);
            assert_eq!(this.texts[0].rect.x, 100.);
        })
        .unwrap();
    click(&mut visual, "property-5");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("FF0000");
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(this.shapes.iter().all(|s| s.color == rgb(0xff0000)));
            assert_eq!(
                this.texts[0].editor.read(cx).effective_style().color,
                rgb(0xff0000)
            );
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[0].color, rgb(0xd9d9d9));
            assert_eq!(
                this.texts[0].editor.read(cx).effective_style().color,
                rgb(0x20232b)
            );
        })
        .unwrap();
}
