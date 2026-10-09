use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext};

fn fixture(cx: &mut TestAppContext) -> gpui::WindowHandle<Workspace> {
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            this.add_artboard(
                Rect {
                    x: 100.,
                    y: 100.,
                    width: 600.,
                    height: 400.,
                },
                cx,
            );
            for id in 2..=6 {
                this.shapes.push(Shape::new(
                    id,
                    Some(1),
                    ShapeKind::Rectangle,
                    Rect {
                        x: 0.,
                        y: 0.,
                        width: 100.,
                        height: 40.,
                    },
                ));
            }
            this.next_id = 7;
            this.enable_auto_layout(window, cx);
        })
        .unwrap();
    handle
}

fn input(visual: &mut VisualTestContext, index: usize, value: &str) {
    draw(visual);
    let bounds = visual
        .debug_bounds(
            [
                "grid-number-0",
                "grid-number-1",
                "grid-number-2",
                "grid-number-3",
            ][index],
        )
        .unwrap();
    visual.simulate_click(
        point(bounds.right() - px(30.), bounds.center().y),
        gpui::Modifiers::default(),
    );
    draw(visual);
    visual.simulate_keystrokes("secondary-a");
    visual.simulate_input(value);
    visual.simulate_keystrokes("enter");
    draw(visual);
}

#[gpui::test]
fn grid_scrubbing_updates_guides_commits_once_and_cancels(cx: &mut TestAppContext) {
    let handle = fixture(cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1400.)));
    click(&mut visual, "layout-grid");
    draw(&mut visual);
    let start = visual.debug_bounds("grid-drag-0").unwrap().center();
    let depth = handle
        .update(&mut visual.cx, |this, _, _| {
            this.history.borrow().undo_len()
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    let end = start + point(px(16.), px(0.));
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.hierarchy.layouts[&1].columns, 4);
            assert_eq!(this.auto_layout.grid.tracks.borrow()[&1].columns.len(), 4);
            assert_eq!(this.history.borrow().undo_len(), depth);
        })
        .unwrap();
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.history.borrow().undo_len(), depth + 1)
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        end,
        MouseButton::Left,
        gpui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.hierarchy.layouts[&1].columns, 22)
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.hierarchy.layouts[&1].columns, 2);
            assert_eq!(this.auto_layout.grid.tracks.borrow()[&1].columns.len(), 2);
            assert!(this.history.borrow().can_redo());
            assert_eq!(this.grid_guide_target(), Some(1));
        })
        .unwrap();
    click(&mut visual, "grid-guides-toggle");
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.grid_guide_target(), None);
            assert_eq!(this.history.borrow().undo_len(), depth);
        })
        .unwrap();
    click(&mut visual, "grid-guides-toggle");
    handle
        .update(&mut visual.cx, |this, _, cx| {
            this.select_shape(2, cx);
            assert_eq!(this.grid_guide_target(), Some(1));
            this.boards[0].layer.hidden = true;
            assert_eq!(this.grid_guide_target(), None);
        })
        .unwrap();
}

#[gpui::test]
fn grid_controls_resize_columns_and_spans_with_undo(cx: &mut TestAppContext) {
    let handle = fixture(cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1400.)));
    click(&mut visual, "layout-grid");
    input(&mut visual, 0, "3");
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.hierarchy.layouts[&1].columns, 3);
            assert_eq!(this.shapes[0].rect.y, this.shapes[2].rect.y);
            assert!(this.shapes[3].rect.y > this.shapes[0].rect.y);
        })
        .unwrap();
    input(&mut visual, 0, "0");
    input(&mut visual, 0, "2.5");
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.hierarchy.layouts[&1].columns, 3)
        })
        .unwrap();
    click(&mut visual, "layout-fixed");
    input(&mut visual, 1, "80");
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.hierarchy.layouts[&1].column_width, Some(80.));
            assert_eq!(this.shapes[1].rect.x - this.shapes[0].rect.x, 92.);
        })
        .unwrap();
    click(&mut visual, "layout-equal-columns");
    handle
        .update(&mut visual.cx, |this, _, cx| this.select_shape(2, cx))
        .unwrap();
    draw(&mut visual);
    input(&mut visual, 2, "2");
    input(&mut visual, 3, "2");
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.hierarchy.sizing[&2].column_span, 2);
            assert_eq!(this.hierarchy.sizing[&2].row_span, 2);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.hierarchy.sizing[&2].row_span, 1);
            this.undo_redo(true, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.hierarchy.sizing[&2].row_span, 2);
        })
        .unwrap();
}

#[gpui::test]
fn grid_mode_switch_and_reordering_are_undoable(cx: &mut TestAppContext) {
    let handle = fixture(cx);
    handle
        .update(cx, |this, window, cx| {
            let original = this.snapshot_page(cx).0;
            this.edit_layout(|layout| layout.axis = Axis::Grid, cx);
            assert!(this.shapes[2].rect.y > this.shapes[0].rect.y);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert!(this.snapshot_page(cx).0.hierarchy == original.hierarchy);
            assert_eq!(this.shapes, original.shapes);
            this.undo_redo(true, window, cx);
            this.reflow_layout(cx);
            let moved = this.shapes[0].id;
            let last = this.shapes.last().unwrap().rect;
            this.shapes[0].rect.x = last.x + 120.;
            this.shapes[0].rect.y = last.y;
            let before = this.reorder_layout_item(moved).unwrap();
            this.history.borrow_mut().record(vec![before], None);
            this.reflow_layout(cx);
            assert_eq!(this.ordered_children(Some(1)).last(), Some(&moved));
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.ordered_children(Some(1)).first(), Some(&moved));
            assert!(this.shapes[0].rect.y < this.shapes[2].rect.y);
        })
        .unwrap();
}

#[gpui::test]
fn grid_pending_input_cannot_change_another_selection(cx: &mut TestAppContext) {
    let handle = fixture(cx);
    handle
        .update(cx, |this, _, cx| {
            this.edit_layout(|layout| layout.axis = Axis::Grid, cx);
            this.auto_layout.grid.inputs[0].update(cx, |input, cx| input.set_value("5", cx));
            this.add_artboard(
                Rect {
                    x: 800.,
                    y: 0.,
                    width: 400.,
                    height: 400.,
                },
                cx,
            );
            this.apply_grid_number(0, cx);
            assert_eq!(this.hierarchy.layouts[&1].columns, 2);
            assert_eq!(this.boards[1].rect.width, 400.);
        })
        .unwrap();
}

#[gpui::test]
fn grid_fixed_tracks_allow_hug_with_fill_children(cx: &mut TestAppContext) {
    let handle = fixture(cx);
    handle
        .update(cx, |this, _, cx| {
            this.edit_layout(
                |layout| {
                    layout.axis = Axis::Grid;
                    layout.column_width = Some(80.);
                },
                cx,
            );
            this.hierarchy.sizing.entry(2).or_default().width = Mode::Fill;
            assert!(this.sizing_enabled(1, 0, Mode::Hug));
            this.choose_sizing(1, 0, Mode::Hug, cx);
            assert!(this.sizing_enabled(2, 0, Mode::Fill));
            assert_eq!(this.boards[0].rect.width, 196.);
            assert_eq!(this.shapes[0].rect.width, 80.);
            this.toggle_layout_absolute(2, cx);
            this.toggle_layout_absolute(2, cx);
            assert_eq!(this.hierarchy.sizing[&2].width, Mode::Fill);
            this.edit_layout(|layout| layout.column_width = None, cx);
            assert_eq!(this.hierarchy.sizing[&1].width, Mode::Fixed);
            assert!(!this.sizing_enabled(1, 0, Mode::Hug));
        })
        .unwrap();
}
