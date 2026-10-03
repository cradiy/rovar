use super::*;
use crate::editor::layout::tests::fixture;
use crate::editor::tests::{click, draw};
use crate::scene::bezier::{Node, Nodes};
use gpui::{TestAppContext, VisualTestContext};
use std::collections::BTreeSet;

#[gpui::test]
fn grouped_flip_reflects_geometry_handles_corners_and_paint_with_undo(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let before = window
        .update(cx, |this, _, cx| {
            this.shapes[0].corners = Some([1., 2., 3., 4.]);
            this.shapes[0].gradient.angle = 30.;
            this.shapes[0].stroke.gradient.angle = 120.;
            this.shapes[1].kind = ShapeKind::Bezier;
            this.shapes[1].nodes = Nodes(Arc::new(vec![
                Node {
                    smooth: false,
                    anchor: point(0.25, 0.5),
                    incoming: point(-0.5, 0.25),
                    outgoing: point(1.25, 0.75),
                },
                Node::corner(point(1., 1.)),
            ]));
            this.set_selection(BTreeSet::from([1, 2]), cx);
            this.group_selection(cx);
            this.shapes.clone()
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    visual.simulate_keystrokes("shift-h");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 150.);
            assert_eq!(this.shapes[1].rect.x, 0.);
            assert_eq!(this.shapes[0].corners, Some([2., 1., 4., 3.]));
            assert_eq!(this.shapes[0].gradient.angle, 330.);
            assert_eq!(this.shapes[0].stroke.gradient.angle, 240.);
            assert_eq!(
                this.shapes[1].nodes.0[0],
                Node {
                    smooth: false,
                    anchor: point(0.75, 0.5),
                    incoming: point(1.5, 0.25),
                    outgoing: point(-0.25, 0.75),
                }
            );
            assert_eq!(this.shapes[2], before[2]);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    window
        .update(&mut visual.cx, |this, _, _| assert_eq!(this.shapes, before))
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z shift-v");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.y, 70.);
            assert_eq!(this.shapes[1].rect.y, 0.);
            assert_eq!(this.shapes[0].corners, Some([3., 4., 1., 2.]));
            assert_eq!(this.shapes[0].gradient.angle, 210.);
            assert_eq!(this.shapes[1].nodes.0[0].incoming, point(1.5, 0.75));
        })
        .unwrap();
}

#[gpui::test]
fn cut_and_in_place_paste_preserve_group_parent_position_and_clipboard_history(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let (group, original) = window
        .update(cx, |this, _, cx| {
            this.set_selection(BTreeSet::from([1, 2]), cx);
            this.group_selection(cx);
            let group = *this.selection_ids().first().unwrap();
            this.shapes[0].layer.aspect_locked = true;
            this.set_selection(BTreeSet::from([1]), cx);
            (group, this.shapes[0].clone())
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-x");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.shapes.iter().any(|s| s.id == 1));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], original);
            assert_eq!(this.layer_parent(1), Some(group));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-v");
    window
        .update(&mut visual.cx, |this, _, _| {
            let id = *this.selection_ids().first().unwrap();
            assert_eq!(this.world_rect(id), Some(original.rect));
            assert_eq!(this.layer_parent(id), Some(group));
            assert!(this.layer_info(id).unwrap().0.aspect_locked);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z ctrl-shift-z ctrl-v");
    window
        .update(&mut visual.cx, |this, _, _| {
            let id = *this.selection_ids().first().unwrap();
            assert_eq!(this.world_rect(id).unwrap().x, original.rect.x + 20.);
        })
        .unwrap();
}

#[gpui::test]
fn in_place_paste_keeps_board_coordinates_and_independent_objects(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(
                Rect {
                    x: -100.,
                    y: -100.,
                    width: 600.,
                    height: 500.,
                },
                cx,
            );
            let board = this.selected.unwrap();
            // An independent object overlapping a board must stay independent when pasted in place.
            this.set_selection(BTreeSet::from([1]), cx);
            this.copy_selection(cx);
            this.paste_in_place(window, cx);
            let id = *this.selection_ids().first().unwrap();
            assert_eq!(this.object_rect(id), this.object_rect(1));
            assert_eq!(this.layer_parent(id), None);
            // Pasting a child keeps its original frame even when another frame overlaps it.
            this.shapes[0].board = Some(board);
            this.set_selection(BTreeSet::from([1]), cx);
            let world = this.world_rect(1);
            this.copy_selection(cx);
            this.add_artboard(
                Rect {
                    x: -100.,
                    y: -100.,
                    width: 600.,
                    height: 500.,
                },
                cx,
            );
            this.paste_in_place(window, cx);
            let id = *this.selection_ids().first().unwrap();
            assert_eq!(this.layer_parent(id), Some(board));
            assert_eq!(this.world_rect(id), world);
            // A copied frame and its children remain a self-contained coordinate system.
            this.set_selection(BTreeSet::from([board]), cx);
            this.copy_selection(cx);
            this.paste_in_place(window, cx);
            let copied_board = *this.selection_ids().first().unwrap();
            let child = this
                .shapes
                .iter()
                .find(|s| s.board == Some(copied_board))
                .unwrap();
            assert_eq!(this.world_rect(child.id), world);
            assert_eq!(this.layer_parent(child.id), Some(copied_board));
        })
        .unwrap();
}

#[gpui::test]
fn aspect_lock_updates_companion_field_and_resize_anchor_and_cancels_scrub(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, cx| {
            this.set_selection(BTreeSet::from([2]), cx)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "aspect-lock");
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("ctrl-a 1 6 0 enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[1].rect.width, 160.);
            assert_eq!(this.shapes[1].rect.height, 80.);
            assert_eq!(this.inspector.fields[4].read(cx).value().as_ref(), "80");
            let resized = this.resize_with_snapping(
                2,
                this.shapes[1].rect,
                Handle(1, 0),
                point(40., 99.),
                false,
            );
            assert_eq!(
                resized,
                Rect {
                    x: 110.,
                    y: 80.,
                    width: 200.,
                    height: 100.
                }
            );
        })
        .unwrap();
    let start = visual.debug_bounds("property-drag-3").unwrap().center();
    let end = start + point(px(80.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[1].rect.width, 240.);
            assert_eq!(this.shapes[1].rect.height, 120.);
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("ctrl-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[1].rect.width, 80.);
            assert_eq!(this.shapes[1].rect.height, 40.);
            assert!(this.shapes[1].layer.aspect_locked);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.shapes[1].layer.aspect_locked)
        })
        .unwrap();
}
