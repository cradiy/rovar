use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext, WindowHandle};

#[gpui::test]
fn vertical_gap_moves_layout_group_as_a_unit_and_undo_restores_its_frame(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 80.);
            this.view.zoom = 1.;
            for (id, y) in [(1, 0.), (2, 100.), (3, 170.)] {
                this.shapes.push(Shape::new(
                    id,
                    None,
                    ShapeKind::Rectangle,
                    Rect {
                        x: 100.,
                        y,
                        width: 80.,
                        height: 40.,
                    },
                ));
            }
            this.next_id = 4;
            this.set_selection(BTreeSet::from([2, 3]), cx);
            this.group_selection(cx);
            let group = *this.selection_ids().first().unwrap();
            this.enable_auto_layout(window, cx);
            this.reflow_layout(cx);
            this.set_selection(BTreeSet::from([1, group]), cx);
            let frame = this.world_rect(group).unwrap();
            let child = this.world_rect(2).unwrap();
            let plan = this.spacing_plan().unwrap();
            assert_eq!(plan.axis, 1);
            let before = this.before_geometry();
            this.apply_spacing(&plan, 90.);
            this.history.borrow_mut().record(before, None);
            this.reflow_layout(cx);
            assert_eq!(this.world_rect(1).unwrap().y, 0.);
            assert_eq!(this.world_rect(group).unwrap().y, 130.);
            assert_eq!(this.world_rect(2).unwrap().y, child.y + 130. - frame.y);
            this.replay_history(false, window, cx);
            assert_eq!(this.world_rect(group), Some(frame));
            assert_eq!(this.world_rect(2), Some(child));
            this.set_selection(BTreeSet::from([2, 3]), cx);
            assert!(this.spacing_plan().is_none());
        })
        .unwrap();
}

fn fixture(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 100.);
            this.view.zoom = 2.;
            for (id, x, width) in [(1, 0., 40.), (2, 60., 60.), (3, 150., 40.)] {
                this.shapes.push(Shape::new(
                    id,
                    None,
                    ShapeKind::Rectangle,
                    Rect {
                        x,
                        y: 100.,
                        width,
                        height: 60.,
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
fn dragging_gap_preserves_sizes_and_anchor_and_cancel_restores_unequal_gaps(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = visual
        .debug_bounds("selection-gap-handle-0")
        .unwrap()
        .center();
    let end = start + point(px(20.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(
                this.shapes.iter().map(|s| s.rect.x).collect::<Vec<_>>(),
                vec![0., 70., 160.]
            );
            assert_eq!(
                this.shapes.iter().map(|s| s.rect.width).collect::<Vec<_>>(),
                vec![40., 60., 40.]
            );
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(
                this.shapes.iter().map(|s| s.rect.x).collect::<Vec<_>>(),
                vec![0., 60., 150.]
            );
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.history.borrow().undo_len(), 1)
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[2].rect.x, 150.)
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(start, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.history.borrow().can_redo())
        })
        .unwrap();
}

#[gpui::test]
fn numeric_gap_commits_once_rejects_invalid_and_escape_keeps_geometry(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "selection-gap-value-0");
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[2].rect.x, 150.);
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
    click(&mut visual, "selection-gap-value-0");
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert!(this.spacing.input.focus_handle(cx).is_focused(window));
            this.spacing
                .input
                .update(cx, |input, cx| input.set_value("NaN", cx));
        })
        .unwrap();
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(this.spacing.invalid);
            assert_eq!(this.shapes[1].rect.x, 60.);
            this.spacing
                .input
                .update(cx, |input, cx| input.set_value("32.5", cx));
        })
        .unwrap();
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.spacing.editing());
            assert_eq!(this.shapes[1].rect.x, 72.5);
            assert_eq!(this.shapes[2].rect.x, 165.);
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    click(&mut visual, "selection-gap-value-1");
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.spacing
                .input
                .update(cx, |input, cx| input.set_value("100", cx))
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.spacing.editing());
            assert_eq!(this.shapes[2].rect.x, 165.);
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
}
