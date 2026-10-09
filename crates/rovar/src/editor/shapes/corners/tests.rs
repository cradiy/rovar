use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle};

fn fixture(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 100.);
            this.view.zoom = 1.;
            this.shapes.push(Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                Rect {
                    x: 100.,
                    y: 100.,
                    width: 200.,
                    height: 120.,
                },
            ));
            this.next_id = 2;
            this.select_shape(1, cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    window
}

fn hover(
    window: WindowHandle<Workspace>,
    visual: &mut VisualTestContext,
    corner: usize,
) -> Point<Pixels> {
    let p = window
        .update(&mut visual.cx, |this, _, _| {
            this.bounds.get().origin + this.corner_point(&this.shapes[0], corner).map(px)
        })
        .unwrap();
    visual.simulate_mouse_move(p, None, Default::default());
    draw(visual);
    visual
        .debug_bounds("corner-radius-handle")
        .unwrap()
        .center()
}

#[gpui::test]
fn full_corner_preset_preserves_geometry_and_mode_and_undoes_once(cx: &mut TestAppContext) {
    for (kind, width, height, independent) in [
        (ShapeKind::Rectangle, 200., 200., false),
        (ShapeKind::Rectangle, 240., 120., true),
        (ShapeKind::Image, 120., 240., false),
    ] {
        let window = fixture(cx);
        let original = window
            .update(cx, |this, _, cx| {
                let shape = &mut this.shapes[0];
                shape.kind = kind;
                shape.rect.width = width;
                shape.rect.height = height;
                shape.radius = 10.;
                shape.independent_corners = independent;
                shape.corners = Some([8., 16., 24., 32.]);
                let original = shape.clone();
                this.sync_fields(cx);
                cx.notify();
                original
            })
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        draw(&mut visual);
        click(&mut visual, "corners-full");
        let rounded = window
            .update(&mut visual.cx, |this, _, _| {
                let shape = &this.shapes[0];
                assert_eq!(shape.rect, original.rect);
                assert_eq!(shape.kind, kind);
                assert_eq!(shape.independent_corners, independent);
                assert_eq!(shape.displayed_radii(), [width.min(height) / 2.; 4]);
                assert_eq!(this.history.borrow().undo_len(), 1);
                shape.clone()
            })
            .unwrap();
        click(&mut visual, "corners-full");
        visual.simulate_keystrokes("secondary-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.shapes[0], original);
            })
            .unwrap();
        visual.simulate_keystrokes("secondary-shift-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.shapes[0], rounded);
                assert_eq!(this.history.borrow().undo_len(), 1);
            })
            .unwrap();
    }
}

#[gpui::test]
fn drag_linked_radius_and_undo_are_one_edit_without_resizing(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    assert!(visual.debug_bounds("corner-radius-handle").is_none());
    let start = hover(window, &mut visual, 0);
    assert!(visual.debug_bounds("corner-radius-value").is_none());
    let end = start + point(px(20. * ARC_OFFSET), px(20. * ARC_OFFSET));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("corner-radius-value").is_some());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("corner-radius-value").is_none());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].displayed_radii(), [20.; 4]);
            assert!(!this.shapes[0].independent_corners);
            assert_eq!(this.shapes[0].rect.width, 200.);
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].radius, 0.)
        })
        .unwrap();
    let start = hover(window, &mut visual, 0);
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(start, MouseButton::Left, Default::default());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.history.borrow().can_redo())
        })
        .unwrap();
}

#[gpui::test]
fn alt_drag_changes_one_corner_and_cancel_and_undo_restore_mode_and_saved_values(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let original = window
        .update(cx, |this, _, _| {
            this.shapes[0].radius = 12.;
            this.shapes[0].corners = Some([1., 2., 3., 4.]);
            this.shapes[0].clone()
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = hover(window, &mut visual, 2);
    let end = start - point(px(20. * ARC_OFFSET), px(20. * ARC_OFFSET));
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    visual.simulate_mouse_down(start, MouseButton::Left, alt);
    visual.simulate_mouse_move(end, MouseButton::Left, alt);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes[0].independent_corners);
            assert_eq!(this.shapes[0].corners, Some([12., 12., 32., 12.]));
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], original);
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
    let start = hover(window, &mut visual, 2);
    let end = start - point(px(20. * ARC_OFFSET), px(20. * ARC_OFFSET));
    visual.simulate_mouse_down(start, MouseButton::Left, alt);
    visual.simulate_mouse_move(end, MouseButton::Left, alt);
    visual.simulate_mouse_up(end, MouseButton::Left, alt);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].corners, Some([12., 12., 32., 12.]));
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], original);
        })
        .unwrap();
}

#[gpui::test]
fn numeric_radius_validates_commits_and_cancels_without_changing_other_corners(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, _| {
            this.shapes[0].independent_corners = true;
            this.shapes[0].corners = Some([4., 8., 12., 16.]);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    hover(window, &mut visual, 0);
    click(&mut visual, "corner-radius-handle");
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.corner_editor
                .input
                .update(cx, |input, cx| input.set_value("NaN", cx));
        })
        .unwrap();
    visual.simulate_keystrokes("enter");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(this.corner_editor.invalid);
            assert_eq!(this.shapes[0].corners, Some([4., 8., 12., 16.]));
            this.corner_editor
                .input
                .update(cx, |input, cx| input.set_value("24.5", cx));
        })
        .unwrap();
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].corners, Some([24.5, 8., 12., 16.]));
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    hover(window, &mut visual, 0);
    click(&mut visual, "corner-radius-handle");
    visual.simulate_keystrokes("secondary-a 4 0 escape");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].corners, Some([24.5, 8., 12., 16.]));
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
}

#[gpui::test]
fn rotated_zoomed_rectangle_uses_local_drag_direction_and_hides_tiny_controls(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, _| {
            this.shapes[0].layer.rotation = 45.;
            this.view.zoom = 2.;
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = hover(window, &mut visual, 0);
    let delta = crate::scene::rotation::vector(point(24. * ARC_OFFSET, 24. * ARC_OFFSET), 45.);
    let end = start + delta.map(px);
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[0].radius, 12.);
            this.view.zoom = 0.1;
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    assert!(visual.debug_bounds("corner-radius-handle").is_none());
}
