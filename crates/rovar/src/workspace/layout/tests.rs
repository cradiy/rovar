use super::*;
use crate::workspace::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext, WindowHandle};

pub(in crate::workspace) fn fixture(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 100.);
            this.view.zoom = 1.;
            for (id, x, y, w, h) in [
                (1, 0., 0., 40., 60.),
                (2, 110., 90., 80., 40.),
                (3, 300., 160., 60., 80.),
            ] {
                this.shapes.push(Shape::new(
                    id,
                    None,
                    ShapeKind::Rectangle,
                    Rect {
                        x,
                        y,
                        width: w,
                        height: h,
                    },
                ));
            }
            this.next_id = 4;
            this.set_selection(BTreeSet::from([1, 2, 3]), cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    window
}
#[gpui::test]
fn alignment_and_spacing_use_world_bounds_preserve_parents_and_undo(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "distribute-x");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 0.);
            assert_eq!(this.shapes[1].rect.x, 130.);
            assert_eq!(this.shapes[2].rect.x, 300.);
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    click(&mut visual, "align-bottom");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes.iter().all(|s| s.rect.y + s.rect.height == 240.));
            assert_eq!(this.history.borrow().undo_len(), 2);
        })
        .unwrap();
    click(&mut visual, "align-bottom");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.history.borrow().undo_len(), 2)
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    visual.simulate_keystrokes("ctrl-z");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[1].rect.x, 110.);
            assert_eq!(this.shapes[1].rect.y, 90.);
            this.add_artboard(
                Rect {
                    x: 500.,
                    y: 300.,
                    width: 400.,
                    height: 300.,
                },
                cx,
            );
            let board = this.selected.unwrap();
            this.shapes[0].board = Some(board);
            this.set_selection(BTreeSet::from([1]), cx);
        })
        .unwrap();
    draw(&mut visual);
    click(&mut visual, "align-center-x");
    click(&mut visual, "align-center-y");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[0].rect.x, 180.);
            assert_eq!(this.shapes[0].rect.y, 120.);
            assert_eq!(this.world_rect(1).unwrap().x, 680.);
            let board = this.shapes[0].board.unwrap();
            this.set_selection(BTreeSet::from([1, 2]), cx);
            this.arrange_selection(LayoutAction::Left, cx);
            assert_eq!(this.world_rect(1).unwrap().x, this.world_rect(2).unwrap().x);
            assert_eq!(this.shapes[0].board, Some(board));
        })
        .unwrap();
}
#[gpui::test]
fn group_alignment_moves_members_once_and_single_root_has_no_alignment_frame(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            this.set_selection(BTreeSet::from([1, 2]), cx);
            this.group_selection(cx);
            let group = *this.selection_ids().first().unwrap();
            assert!(!this.can_layout(LayoutAction::Left));
            this.set_selection(BTreeSet::from([group, 3]), cx);
            let before = this.world_rect(group).unwrap();
            this.arrange_selection(LayoutAction::Right, cx);
            assert_eq!(this.world_rect(group).unwrap().x + before.width, 360.);
            assert_eq!(this.shapes[1].rect.x - this.shapes[0].rect.x, 110.);
            this.replay_history(false, window, cx);
            assert_eq!(this.world_rect(group), Some(before));
        })
        .unwrap();
}
#[gpui::test]
fn snapping_respects_zoom_alt_hidden_targets_and_cancel_history(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.snapping.enabled = true;
            this.view.zoom = 2.;
            this.shapes[0].rect = Rect {
                x: 0.,
                y: 0.,
                width: 40.,
                height: 40.,
            };
            this.shapes[1].rect = Rect {
                x: 100.,
                y: 100.,
                width: 80.,
                height: 80.,
            };
            this.shapes[1].layer.locked = true; // Visible locked objects can be alignment references.
            this.shapes[2].rect = Rect {
                x: 98.,
                y: 200.,
                width: 60.,
                height: 60.,
            };
            this.shapes[2].layer.hidden = true;
            this.set_selection(BTreeSet::from([1]), cx);
        })
        .unwrap();
    draw(&mut visual);
    let start = visual.debug_bounds("shape-1").unwrap().center();
    let end = start + point(px(116.), px(0.)); // Right edge 98 is within 6 screen px of x=100.
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 60.)
        })
        .unwrap();
    visual.simulate_mouse_move(
        end,
        MouseButton::Left,
        gpui::Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 58.)
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 0.);
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 60.);
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 0.)
        })
        .unwrap();
}

#[gpui::test]
fn grouped_drag_snaps_as_one_bounds_and_never_targets_its_own_members(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.set_selection(BTreeSet::from([1, 2]), cx);
            this.group_selection(cx);
            this.snapping.enabled = true;
        })
        .unwrap();
    draw(&mut visual);
    let start = visual.debug_bounds("shape-1").unwrap().center();
    let end = start + point(px(108.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 110.);
            assert_eq!(this.shapes[1].rect.x, 220.);
            assert_eq!(this.shapes[2].rect.x, 300.);
        })
        .unwrap();
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.x, 0.);
            assert_eq!(this.shapes[1].rect.x, 110.);
            assert_eq!(this.selection_ids(), BTreeSet::from([4]));
        })
        .unwrap();
}
