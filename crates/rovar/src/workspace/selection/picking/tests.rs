use super::*;
use crate::workspace::tests::{click, draw, open};
use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle};

fn fixture(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 100.);
            this.view.zoom = 1.;
            for id in 1..=3 {
                this.shapes.push(Shape::new(
                    id,
                    None,
                    ShapeKind::Rectangle,
                    Rect {
                        x: 100.,
                        y: 100.,
                        width: 120.,
                        height: 100.,
                    },
                ));
            }
            this.next_id = 4;
            this.set_selection(BTreeSet::from([1, 2]), cx);
            this.group_selection(cx);
            this.select(None, cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    window
}

fn right(visual: &mut VisualTestContext, p: Point<Pixels>) {
    visual.simulate_mouse_down(p, MouseButton::Right, Default::default());
    visual.simulate_mouse_up(p, MouseButton::Right, Default::default());
    draw(visual);
    click(visual, "context-select-layer");
}

#[gpui::test]
fn picking_respects_stacking_hidden_ancestors_rotation_and_zoom(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, _| {
            let p = this.bounds.get().origin + this.view.screen(point(160., 150.)).map(px);
            assert_eq!(this.layers_at(p), vec![3, 2, 1, 4]);
            this.hierarchy.groups.get_mut(&4).unwrap().layer.hidden = true;
            assert_eq!(this.layers_at(p), vec![3]);
            this.hierarchy.groups.get_mut(&4).unwrap().layer.hidden = false;
            this.shapes[2].layer.rotation = 45.;
            this.view.zoom = 2.;
            this.view.pan = point(-40., 70.);
            let p = this.bounds.get().origin + this.view.screen(point(90., 80.)).map(px);
            assert!(!this.layers_at(p).contains(&3));
            let p = this.bounds.get().origin + this.view.screen(point(160., 150.)).map(px);
            assert_eq!(this.layers_at(p), vec![3, 2, 1, 4]);
        })
        .unwrap();
}

#[gpui::test]
fn menu_hover_only_previews_click_selects_and_dismissal_clears_preview(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let p = visual.debug_bounds("shape-3").unwrap().center();
    right(&mut visual, p);
    let row = visual.debug_bounds("pick-layer-1").unwrap().center();
    visual.simulate_mouse_move(row, None, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("layer-pick-preview").is_some());
    assert!(visual.debug_bounds("selection-name-1").is_some());
    assert!(visual.debug_bounds("selection-name-3").is_none());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.pick_hover, Some(1));
            assert_eq!(this.selection_ids(), BTreeSet::from([3]));
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    click(&mut visual, "pick-layer-1");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.selection_ids(), BTreeSet::from([1]));
            assert_eq!(this.pick_hover, None);
            assert!(!context_menu::is_open(cx));
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    right(&mut visual, p);
    let row = visual.debug_bounds("pick-layer-2").unwrap().center();
    visual.simulate_mouse_move(row, None, Default::default());
    draw(&mut visual);
    visual.simulate_keystrokes("escape escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("layer-pick-preview").is_none());
    assert!(visual.debug_bounds("selection-name-2").is_none());
    assert!(visual.debug_bounds("selection-name-3").is_some());
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.pick_hover, None);
            assert!(!context_menu::is_open(cx));
            assert_eq!(this.selection_ids(), BTreeSet::from([3]));
        })
        .unwrap();
}

#[gpui::test]
fn locked_candidates_cannot_be_selected_and_outside_click_clears_hover(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, _| this.shapes[0].layer.locked = true)
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let p = visual.debug_bounds("shape-3").unwrap().center();
    right(&mut visual, p);
    click(&mut visual, "pick-layer-1");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.selection_ids(), BTreeSet::from([3]));
            assert!(context_menu::is_open(cx));
        })
        .unwrap();
    let row = visual.debug_bounds("pick-layer-2").unwrap().center();
    visual.simulate_mouse_move(row, None, Default::default());
    draw(&mut visual);
    visual.simulate_click(point(px(900.), px(100.)), Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.pick_hover, None);
            assert!(!context_menu::is_open(cx));
            assert!(this.gesture.is_none());
        })
        .unwrap();
}

#[gpui::test]
fn command_click_selects_group_child_and_shift_toggles_without_editing(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, _| this.shapes[2].layer.hidden = true)
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let p = visual.debug_bounds("shape-2").unwrap().center();
    visual.simulate_click(p, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([4]))
        })
        .unwrap();
    visual.simulate_click(
        p,
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([2]));
            assert!(this.vector_edit.is_none());
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    visual.simulate_click(
        p,
        Modifiers {
            platform: true,
            shift: true,
            ..Default::default()
        },
    );
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.selection_ids().is_empty());
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
}
