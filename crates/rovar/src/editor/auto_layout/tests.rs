use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext};
use std::collections::BTreeSet;

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[gpui::test]
fn moving_layout_group_between_boards_preserves_child_constraints(cx: &mut TestAppContext) {
    use crate::scene::{auto_layout::Constraint, layer::LayerGroup};
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 100., 300., 200.), cx);
            this.add_artboard(rect(600., 100., 300., 200.), cx);
            this.hierarchy.groups.insert(
                3,
                LayerGroup {
                    boolean: None,
                    mask: None,
                    uid: uuid::Uuid::new_v4(),
                    name: "Card".into(),
                    board: Some(1),
                    layer: Default::default(),
                },
            );
            this.hierarchy
                .layouts
                .insert(3, Container::new(rect(20., 20., 100., 80.)));
            this.shapes.push(Shape::new(
                4,
                Some(1),
                ShapeKind::Rectangle,
                rect(80., 30., 20., 20.),
            ));
            this.hierarchy.parents.insert(4, 3);
            this.hierarchy.sizing.insert(
                4,
                Sizing {
                    absolute: true,
                    ..Default::default()
                },
            );
            this.next_id = 5;
            this.set_selection(BTreeSet::from([4]), cx);
            this.choose_constraint(4, 0, Constraint::End, cx);
            let pins = this.hierarchy.sizing[&4].constraints;
            this.set_selection(BTreeSet::from([3]), cx);
            this.batch_before = this.before_geometry();
            this.move_selection(point(500., 0.));
            let expected = this.world_rect(4).unwrap();
            this.finish_selection_move(cx);
            assert_eq!(this.hierarchy.groups[&3].board, Some(2));
            this.reflow_layout(cx);
            assert_eq!(this.world_rect(4), Some(expected));
            assert_eq!(this.hierarchy.sizing[&4].constraints, pins);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.world_rect(4).unwrap(), rect(180., 130., 20., 20.));
            this.undo_redo(true, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.world_rect(4), Some(expected));
        })
        .unwrap();
}

#[gpui::test]
fn cancelling_numeric_scrub_restores_constraint_geometry(cx: &mut TestAppContext) {
    use crate::scene::auto_layout::Constraint;
    let window = open(cx);
    window
        .update(cx, |this, _, cx| {
            this.add_artboard(rect(100., 100., 300., 200.), cx);
            this.shapes.push(Shape::new(
                2,
                Some(1),
                ShapeKind::Rectangle,
                rect(200., 100., 50., 30.),
            ));
            this.next_id = 3;
            this.set_selection(BTreeSet::from([2]), cx);
            this.choose_constraint(2, 0, Constraint::End, cx);
            let pins = this.hierarchy.sizing[&2].constraints;
            let original = this.shapes[0].rect;
            let depth = this.history.borrow().undo_len();
            this.history.borrow_mut().begin_preview();
            this.edit_shape(|shape| shape.rect.width = 80.);
            this.finish_property_scrub(false, cx);
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect, original);
            assert_eq!(this.hierarchy.sizing[&2].constraints, pins);
            assert_eq!(this.history.borrow().undo_len(), depth);
        })
        .unwrap();
}

#[gpui::test]
fn constrained_text_scrub_commits_and_undoes_geometry(cx: &mut TestAppContext) {
    use crate::scene::auto_layout::Constraint;
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 100., 300., 200.), cx);
            let text = this.make_text(2, Some(1), rect(100., 50., 100., 30.), window, cx);
            this.texts.push(text);
            this.next_id = 3;
            this.set_selection(BTreeSet::from([2]), cx);
            this.choose_constraint(2, 0, Constraint::End, cx);
            let original = this.texts[0].rect;
            let x = this.field_value(1, cx).unwrap().parse().unwrap();
            this.history.borrow_mut().begin_preview();
            this.scrub_property(1, x, 20., false, cx);
            this.finish_property_scrub(true, cx);
            this.reflow_layout(cx);
            assert_eq!(this.texts[0].rect.x, 120.);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.texts[0].rect, original);
            this.undo_redo(true, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.texts[0].rect.x, 120.);
            this.history.borrow_mut().begin_preview();
            this.scrub_property(3, 100., 30., false, cx);
            this.finish_property_scrub(false, cx);
            assert_eq!(this.texts[0].rect, rect(120., 50., 100., 30.));
        })
        .unwrap();
}

#[gpui::test]
fn size_limit_edits_restore_geometry_on_undo_and_reject_invalid_ranges(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 100., 300., 200.), cx);
            this.sync_layout_inputs(cx);
            this.auto_layout.limits.inputs[1].update(cx, |input, cx| input.set_value("200", cx));
            this.apply_size_limit(1, cx);
            assert_eq!(this.boards[0].rect.width, 200.);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.boards[0].rect.width, 300.);
            assert!(
                this.auto_layout.limits.inputs[1]
                    .read(cx)
                    .value()
                    .is_empty()
            );
            this.undo_redo(true, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.boards[0].rect.width, 200.);
            let depth = this.history.borrow().undo_len();
            for invalid in ["201", "NaN", "-1", "invalid"] {
                this.auto_layout.limits.inputs[0]
                    .update(cx, |input, cx| input.set_value(invalid, cx));
                this.apply_size_limit(0, cx);
                assert!(this.hierarchy.sizing[&1].limits.min_width.is_none());
                assert_eq!(this.history.borrow().undo_len(), depth);
            }
            this.auto_layout.limits.inputs[0].update(cx, |input, cx| input.set_value("150", cx));
            this.apply_size_limit(0, cx);
            assert_eq!(this.hierarchy.sizing[&1].limits.min_width, Some(150.));
            let jobs = this.component_export_jobs(window, cx).unwrap();
            let exported = crate::document::Page::decode(&jobs[0].json).unwrap();
            assert_eq!(exported.boards[0].rect.width, 200.);
            this.auto_layout.limits.inputs[1].update(cx, |input, cx| input.set_value("", cx));
            this.apply_size_limit(1, cx);
            assert!(this.hierarchy.sizing[&1].limits.max_width.is_none());
        })
        .unwrap();
}

#[gpui::test]
fn size_limit_blur_cannot_edit_another_page_with_the_same_object_id(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(0., 0., 300., 200.), cx);
            this.sync_layout_inputs(cx);
            this.auto_layout.limits.inputs[1].update(cx, |input, cx| input.set_value("100", cx));
            this.add_page(None, window, cx);
            this.add_artboard(rect(0., 0., 400., 200.), cx);
            this.apply_size_limit(1, cx);
            assert_eq!(this.boards[0].rect.width, 400.);
            assert!(
                this.hierarchy
                    .sizing
                    .get(&1)
                    .is_none_or(|s| s.limits.is_empty())
            );
        })
        .unwrap();
}

#[gpui::test]
fn constraints_follow_frame_resize_and_manual_move_with_undo(cx: &mut TestAppContext) {
    use crate::scene::auto_layout::Constraint;
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 100., 300., 200.), cx);
            this.shapes.push(Shape::new(
                2,
                Some(1),
                ShapeKind::Rectangle,
                rect(200., 150., 50., 30.),
            ));
            this.next_id = 3;
            this.set_selection(BTreeSet::from([2]), cx);
            this.choose_constraint(2, 0, Constraint::End, cx);
            this.choose_constraint(2, 1, Constraint::End, cx);
            this.set_selection(BTreeSet::from([1]), cx);
            this.edit_board(None, |b| {
                b.rect.width = 500.;
                b.rect.height = 300.;
            });
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect, rect(400., 250., 50., 30.));
            let jobs = this.component_export_jobs(window, cx).unwrap();
            let exported = crate::document::Page::decode(&jobs[0].json).unwrap();
            assert_eq!(exported.shapes[0].rect, this.shapes[0].rect);
            assert!(jobs[0].svg().is_ok());
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect, rect(200., 150., 50., 30.));
            this.undo_redo(true, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect, rect(400., 250., 50., 30.));
            this.set_selection(BTreeSet::from([2]), cx);
            this.edit_shape(|shape| shape.rect.x -= 20.);
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect.x, 380.);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect.x, 400.);
            this.undo_redo(true, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect.x, 380.);
            let (page, _) = this.snapshot_page(cx);
            let saved = crate::document::Page::decode(&serde_json::to_vec(&page).unwrap()).unwrap();
            assert_eq!(saved.shapes[0].rect, rect(380., 250., 50., 30.));
            this.duplicate_selection(window, cx);
            let copied = *this.selection_ids().first().unwrap();
            let position = this.world_rect(copied).unwrap();
            this.reflow_layout(cx);
            assert_eq!(this.world_rect(copied).unwrap(), position);
        })
        .unwrap();
}

#[gpui::test]
fn wrap_switch_and_line_gap_are_undoable_and_fix_hug_main_axis(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 100., 130., 200.), cx);
            for id in [2, 3] {
                this.shapes.push(Shape::new(
                    id,
                    Some(1),
                    ShapeKind::Rectangle,
                    rect(0., 0., 60., 30.),
                ));
            }
            this.next_id = 4;
            this.enable_auto_layout(window, cx);
            this.choose_sizing(1, 1, Mode::Hug, cx);
            this.toggle_layout_wrap(cx);
            assert!(this.hierarchy.layouts[&1].wrap);
            assert_eq!(this.shapes[1].rect.y, 54.);
            this.sync_layout_inputs(cx);
            this.auto_layout.inputs[5].update(cx, |input, cx| input.set_value("20", cx));
            this.apply_layout_number(5, cx);
            assert_eq!(this.shapes[1].rect.y, 62.);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.shapes[1].rect.y, 54.);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert!(!this.hierarchy.layouts[&1].wrap);
            assert_eq!(this.shapes[1].rect.y, 12.);
            this.choose_sizing(1, 0, Mode::Hug, cx);
            this.toggle_layout_wrap(cx);
            assert_eq!(this.hierarchy.sizing[&1].width, Mode::Fixed);
            assert!(!this.sizing_enabled(1, 0, Mode::Hug));
        })
        .unwrap();
}

#[gpui::test]
fn wrapped_drag_order_uses_the_destination_row(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 100., 160., 200.), cx);
            for id in 2..6 {
                this.shapes.push(Shape::new(
                    id,
                    Some(1),
                    ShapeKind::Rectangle,
                    rect(0., 0., 60., 30.),
                ));
            }
            this.next_id = 6;
            this.enable_auto_layout(window, cx);
            this.toggle_layout_wrap(cx);
            this.shapes[0].rect = rect(110., 58., 60., 30.);
            let before = this.reorder_layout_item(2).unwrap();
            this.history.borrow_mut().record(vec![before], None);
            this.reflow_layout(cx);
            assert_eq!(this.ordered_children(Some(1)), vec![3, 4, 5, 2]);
            assert_eq!(this.shapes[0].rect, rect(84., 54., 60., 30.));
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.ordered_children(Some(1)), vec![2, 3, 4, 5]);
            assert_eq!(this.shapes[0].rect, rect(12., 12., 60., 30.));
        })
        .unwrap();
}

#[gpui::test]
fn frame_resize_previews_constraints_and_escape_restores_children(cx: &mut TestAppContext) {
    use crate::scene::auto_layout::Constraint;
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 100., 300., 200.), cx);
            this.shapes.push(Shape::new(
                2,
                Some(1),
                ShapeKind::Rectangle,
                rect(200., 150., 50., 30.),
            ));
            this.next_id = 3;
            this.set_selection(BTreeSet::from([2]), cx);
            this.choose_constraint(2, 0, Constraint::End, cx);
            this.set_selection(BTreeSet::from([1]), cx);
            let original = this.boards[0].rect;
            this.begin(
                GestureKind::Resize {
                    id: 1,
                    original,
                    handle: Handle(1, 1),
                },
                point(px(0.), px(0.)),
                MouseButton::Left,
                window,
                cx,
            );
            this.boards[0].rect.width = 500.;
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect.x, 400.);
            this.boards[0].rect.width = 450.;
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect.x, 350.);
            this.cancel_gesture(window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect.x, 200.);
        })
        .unwrap();
}

#[gpui::test]
fn enabling_flow_releases_old_pins_and_undo_restores_them(cx: &mut TestAppContext) {
    use crate::scene::auto_layout::Constraint;
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 100., 300., 200.), cx);
            this.shapes.push(Shape::new(
                2,
                Some(1),
                ShapeKind::Rectangle,
                rect(200., 150., 50., 30.),
            ));
            this.next_id = 3;
            this.set_selection(BTreeSet::from([2]), cx);
            this.choose_constraint(2, 0, Constraint::End, cx);
            this.set_selection(BTreeSet::from([1]), cx);
            this.enable_auto_layout(window, cx);
            assert_eq!(this.shapes[0].rect, rect(12., 12., 50., 30.));
            this.enable_auto_layout(window, cx);
            assert_eq!(this.shapes[0].rect, rect(12., 12., 50., 30.));
            this.undo_redo(false, window, cx);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.shapes[0].rect, rect(200., 150., 50., 30.));
            assert_eq!(
                this.hierarchy.sizing[&2].constraints.unwrap().horizontal,
                Constraint::End
            );
        })
        .unwrap();
}

#[gpui::test]
fn dragging_out_of_layout_detaches_at_drop_position_and_undo_restores_fill(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let original = window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 100., 300., 200.), cx);
            for id in [2, 3] {
                this.shapes.push(Shape::new(
                    id,
                    Some(1),
                    ShapeKind::Rectangle,
                    rect(0., 0., 50., 30.),
                ));
            }
            this.next_id = 4;
            this.enable_auto_layout(window, cx);
            this.set_selection(BTreeSet::from([2]), cx);
            this.choose_sizing(2, 0, Mode::Fill, cx);
            this.world_rect(2).unwrap()
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = visual.debug_bounds("shape-2").unwrap().center();
    let end = start + point(px(450.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.layer_parent(2), None);
            assert_eq!(
                this.world_rect(2).unwrap(),
                Rect {
                    x: original.x + 450.,
                    ..original
                }
            );
            assert!(!this.hierarchy.sizing.contains_key(&2));
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.layer_parent(2), Some(1));
            assert_eq!(this.world_rect(2), Some(original));
            assert_eq!(this.hierarchy.sizing[&2].width, Mode::Fill);
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.layer_parent(2), Some(1));
            assert_eq!(this.world_rect(2), Some(original));
            assert!(this.history.borrow().can_redo());
            this.set_selection(BTreeSet::from([2, 3]), cx);
            this.batch_before = this.before_geometry();
            this.move_selection(point(500., 0.));
            this.finish_selection_move(cx);
            this.reflow_layout(cx);
            assert!(this.shapes.iter().all(|s| s.board.is_none()));
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert!(this.shapes.iter().all(|s| s.board == Some(1)));
        })
        .unwrap();
}

#[gpui::test]
fn nested_layout_group_can_detach_with_its_children(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 100., 300., 200.), cx);
            this.shapes.push(Shape::new(
                2,
                Some(1),
                ShapeKind::Rectangle,
                rect(20., 20., 50., 30.),
            ));
            this.shapes.push(Shape::new(
                3,
                Some(1),
                ShapeKind::Rectangle,
                rect(90., 20., 30., 30.),
            ));
            this.next_id = 4;
            this.set_selection(BTreeSet::from([2, 3]), cx);
            this.enable_auto_layout(window, cx);
            let group = this.layout_target().unwrap();
            this.select(Some(1), cx);
            this.enable_auto_layout(window, cx);
            this.set_selection(BTreeSet::from([group]), cx);
            let original = this.world_rect(2).unwrap();
            this.batch_before = this.before_geometry();
            this.move_selection(point(500., 0.));
            this.finish_selection_move(cx);
            this.reflow_layout(cx);
            assert_eq!(this.layer_parent(group), None);
            assert_eq!(this.shapes[0].board, None);
            assert_eq!(
                this.world_rect(2),
                Some(Rect {
                    x: original.x + 500.,
                    ..original
                })
            );
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.layer_parent(group), Some(1));
            assert_eq!(this.layer_parent(2), Some(group));
            assert_eq!(this.world_rect(2), Some(original));
        })
        .unwrap();
}

#[gpui::test]
fn ordinary_objects_cannot_enable_layout_but_frames_keep_the_context_action(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            for (id, kind) in [(1, ShapeKind::Rectangle), (2, ShapeKind::Image)] {
                this.shapes
                    .push(Shape::new(id, None, kind, rect(100., 80., 60., 30.)));
            }
            this.next_id = 3;
            this.add_text(None, rect(100., 150., 80., 24.), window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    for id in [1, 2, 3] {
        window
            .update(&mut visual.cx, |this, window, cx| {
                this.set_selection(BTreeSet::from([id]), cx);
                let depth = this.history.borrow().undo_len();
                this.enable_auto_layout(window, cx);
                assert!(!this.can_auto_layout());
                assert!(this.hierarchy.groups.is_empty());
                assert!(this.hierarchy.layouts.is_empty());
                assert_eq!(this.history.borrow().undo_len(), depth);
                this.open_context_menu(Some(id), true, point(px(400.), px(200.)), window, cx);
            })
            .unwrap();
        draw(&mut visual);
        assert!(visual.debug_bounds("context-auto-layout").is_none());
        visual.simulate_keystrokes("escape");
        draw(&mut visual);
        assert!(visual.debug_bounds("toggle-auto-layout").is_none());
    }
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.add_artboard(rect(0., 0., 300., 200.), cx);
            this.open_context_menu(this.selected, true, point(px(400.), px(200.)), window, cx);
        })
        .unwrap();
    click(&mut visual, "context-auto-layout");
    window
        .update(&mut visual.cx, |this, _, _| assert!(this.has_auto_layout()))
        .unwrap();
}

#[gpui::test]
fn spacing_scrub_updates_layout_and_commits_once_or_cancels(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(0., 0., 300., 180.), cx);
            for id in [2, 3] {
                this.shapes.push(Shape::new(
                    id,
                    Some(1),
                    ShapeKind::Rectangle,
                    rect(0., 0., 50., 30.),
                ));
            }
            this.next_id = 4;
            this.enable_auto_layout(window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1100.)));
    for (index, selector) in [
        "layout-drag-0",
        "layout-drag-1",
        "layout-drag-2",
        "layout-drag-3",
        "layout-drag-4",
    ]
    .into_iter()
    .enumerate()
    {
        draw(&mut visual);
        let start = visual.debug_bounds(selector).unwrap().center();
        let depth = window
            .update(&mut visual.cx, |this, _, _| {
                this.history.borrow().undo_len()
            })
            .unwrap();
        visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
        let end = start + point(px(20.), px(0.));
        visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.layout_number_value(index), Some(32.));
                assert_eq!(this.history.borrow().undo_len(), depth);
                if index == 0 {
                    assert_eq!(this.shapes[1].rect.x, 94.);
                }
                if index == 4 {
                    assert_eq!(this.shapes[0].rect.x, 32.);
                }
            })
            .unwrap();
        visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.history.borrow().undo_len(), depth + 1)
            })
            .unwrap();
        visual.simulate_keystrokes("secondary-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.layout_number_value(index), Some(12.))
            })
            .unwrap();

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
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.layout_number_value(index), Some(212.))
            })
            .unwrap();
        visual.simulate_keystrokes("escape");
        visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.layout_number_value(index), Some(12.));
                assert!(this.history.borrow().can_redo());
                assert_eq!(this.shapes[1].rect.x, 74.);
            })
            .unwrap();
    }
}

#[gpui::test]
fn create_edit_move_and_undo_layout_from_the_inspector(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, _, cx| {
            this.shapes.push(Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                rect(40., 50., 60., 30.),
            ));
            this.shapes.push(Shape::new(
                2,
                None,
                ShapeKind::Rectangle,
                rect(180., 90., 80., 40.),
            ));
            this.next_id = 3;
            this.set_selection(BTreeSet::from([1, 2]), cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "toggle-auto-layout");
    window
        .update(&mut visual.cx, |this, window, cx| {
            let id = this.layout_target().unwrap();
            assert!(this.hierarchy.layouts.contains_key(&id));
            assert_eq!(this.shapes[0].rect.x, 52.);
            assert_eq!(this.shapes[1].rect.x, 124.);
            assert_eq!(this.group_bounds(id).unwrap().width, 176.);
            this.undo_redo(false, window, cx);
            assert!(this.hierarchy.layouts.is_empty());
            assert_eq!(this.shapes[1].rect.x, 180.);
            this.undo_redo(true, window, cx);
            this.reflow_layout(cx);
            this.sync_layout_inputs(cx);
            this.auto_layout.inputs[0].update(cx, |input, cx| input.set_value("24", cx));
            this.apply_layout_number(0, cx);
            assert_eq!(this.shapes[1].rect.x, 136.);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.shapes[1].rect.x, 124.);
            assert_eq!(this.auto_layout.inputs[0].read(cx).value(), "12");
            this.batch_before = this.before_geometry();
            this.move_selection(point(20., 10.));
            this.finish_selection_move(cx);
            this.reflow_layout(cx);
            assert_eq!(this.group_bounds(id).unwrap().x, 60.);
            assert_eq!(this.shapes[0].rect.x, 72.);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.group_bounds(id).unwrap().x, 40.);
            assert_eq!(this.shapes[0].rect.x, 52.);
            assert!(this.edit_multi_field(Property::Width, "300", cx));
            this.reflow_layout(cx);
            assert_eq!(this.group_bounds(id).unwrap().width, 300.);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.group_bounds(id).unwrap().width, 176.);
        })
        .unwrap();
    draw(&mut visual);
}

#[gpui::test]
fn reorder_copy_across_pages_and_persist_nested_layout(cx: &mut TestAppContext) {
    let window = open(cx);
    let directory = tempfile::tempdir().unwrap();
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(100., 200., 300., 180.), cx);
            this.shapes.push(Shape::new(
                2,
                Some(1),
                ShapeKind::Rectangle,
                rect(20., 20., 50., 30.),
            ));
            this.shapes.push(Shape::new(
                3,
                Some(1),
                ShapeKind::Rectangle,
                rect(100., 20., 70., 40.),
            ));
            this.next_id = 4;
            this.set_selection(BTreeSet::from([2, 3]), cx);
            this.enable_auto_layout(window, cx);
            let group = this.layout_target().unwrap();
            this.select(Some(1), cx);
            this.enable_auto_layout(window, cx);
            this.set_selection(BTreeSet::from([group]), cx);
            this.choose_sizing(group, 0, Mode::Fill, cx);
            assert_eq!(this.group_bounds(group).unwrap().width, 276.);
            let component = this.component_snapshot(cx).unwrap();
            let saved: crate::document::Document = serde_json::from_slice(&component.json).unwrap();
            assert_eq!(saved.pages[0].hierarchy.layouts[&group].frame.x, 0.);
            assert_eq!(saved.pages[0].hierarchy.sizing[&group].width, Mode::Fill);
            this.set_selection(BTreeSet::from([2]), cx);
            this.batch_before = this.before_geometry();
            this.move_selection(point(160., 0.));
            this.finish_selection_move(cx);
            this.reflow_layout(cx);
            assert_eq!(this.ordered_children(Some(group)), vec![3, 2]);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.ordered_children(Some(group)), vec![2, 3]);
            this.set_selection(BTreeSet::from([1]), cx);
            this.copy_selection(cx);
            this.add_page(None, window, cx);
            this.paste_in_place(window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.hierarchy.layouts.len(), 2);
            let (json, sources) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            let path = directory.path().join("layout.rovar");
            crate::document::save_as(&path, &json, &sources, cx.text_system()).unwrap();
            let document = crate::document::load(&path)
                .unwrap()
                .into_document()
                .unwrap();
            assert_eq!(document.pages[1].hierarchy.layouts.len(), 2);
            assert_eq!(document.pages[1].shapes[0].rect, this.shapes[0].rect);
        })
        .unwrap();
}

#[gpui::test]
fn layout_controls_preserve_page_scope_and_acyclic_sizing(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(0., 0., 300., 180.), cx);
            this.shapes.push(Shape::new(
                2,
                Some(1),
                ShapeKind::Rectangle,
                rect(10., 10., 50., 30.),
            ));
            this.next_id = 3;
            this.select(Some(1), cx);
            this.enable_auto_layout(window, cx);
            this.set_selection(BTreeSet::from([2]), cx);
            this.choose_sizing(2, 0, Mode::Fill, cx);
            assert!(!this.sizing_enabled(1, 0, Mode::Hug));
            this.toggle_layout_absolute(2, cx);
            this.choose_sizing(1, 0, Mode::Hug, cx);
            this.toggle_layout_absolute(2, cx);
            this.reflow_layout(cx);
            assert_eq!(this.hierarchy.sizing[&2].width, Mode::Fixed);
            this.undo_redo(false, window, cx);
            assert!(this.hierarchy.sizing[&2].absolute);
            assert_eq!(this.hierarchy.sizing[&2].width, Mode::Fill);

            this.select(Some(1), cx);
            this.sync_layout_inputs(cx);
            this.auto_layout.inputs[0].update(cx, |input, cx| input.set_value("99", cx));
            let old_page = this.pages.active.clone();
            this.add_page(None, window, cx);
            this.add_artboard(rect(0., 0., 200., 120.), cx);
            this.select(Some(1), cx);
            this.enable_auto_layout(window, cx);
            // A blur queued on the old page must not apply to an equal ID on this page.
            this.apply_layout_number(0, cx);
            assert_eq!(this.hierarchy.layouts[&1].gap, 12.);
            assert_eq!(this.auto_layout.target, Some((old_page, 1)));
            this.sync_layout_inputs(cx);
            assert_eq!(
                this.auto_layout.target,
                Some((this.pages.active.clone(), 1))
            );
        })
        .unwrap();
}
