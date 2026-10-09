use super::*;
use crate::editor::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn point_gradient_controls_drag_undo_and_cancel_on_rotated_zoomed_shapes(cx: &mut TestAppContext) {
    for (tool, angle, zoom) in [("add-artboard", 0., 1.), ("add-rectangle", 90., 2.)] {
        let window = open(cx);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        create(&mut visual, tool);
        click(&mut visual, "property-drag-5");
        click(&mut visual, "fill-points");
        click(&mut visual, "gradient-point-select-3");
        window
            .update(&mut visual.cx, |w, _, cx| {
                assert_eq!(w.inspector.active_stop, 3);
                let slot = w.paint_surface(cx) * PROPERTY_COUNT + 21;
                w.edit_field(slot, &InputEvent::Submit("125".into()), cx);
                let (FillMode::Points(g), _) = w.fill_state(cx).unwrap() else {
                    panic!()
                };
                assert_eq!(g.points[3].radius, 1.25);
            })
            .unwrap();
        click(&mut visual, "color-close");
        let (original, undo, rect) = window
            .update(&mut visual.cx, |w, _, cx| {
                let id = w.selected_shape.or(w.selected).unwrap();
                if let Some(s) = w.shapes.iter_mut().find(|s| Some(s.id) == w.selected_shape) {
                    s.layer.rotation = angle;
                }
                w.view.zoom = zoom;
                w.view.pan = point(100., 100.);
                cx.notify();
                (
                    w.fill_state(cx).unwrap().0,
                    w.history.borrow().undo_len(),
                    w.world_rect(id).unwrap(),
                )
            })
            .unwrap();
        draw(&mut visual);
        let start = visual
            .debug_bounds("gradient-point-handle-0")
            .unwrap()
            .center();
        let delta = crate::scene::rotation::around(
            point(rect.width * 0.1 * zoom, 0.),
            point(0., 0.),
            angle,
        )
        .map(px);
        let end = start + delta;
        visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
        visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
        visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |w, _, cx| {
                let (FillMode::Points(g), _) = w.fill_state(cx).unwrap() else {
                    panic!()
                };
                assert!((g.points[0].position.x - 0.25).abs() < 0.002);
                assert!((g.points[0].position.y - 0.2).abs() < 0.002);
                assert_eq!(w.history.borrow().undo_len(), undo + 1);
            })
            .unwrap();
        visual.simulate_keystrokes("secondary-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |w, _, cx| {
                assert_eq!(w.fill_state(cx).unwrap().0, original)
            })
            .unwrap();
        let start = visual
            .debug_bounds("gradient-point-handle-0")
            .unwrap()
            .center();
        visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
        visual.simulate_mouse_move(start + delta, MouseButton::Left, Default::default());
        visual.simulate_keystrokes("escape");
        visual.simulate_mouse_up(start + delta, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |w, _, cx| {
                assert_eq!(w.fill_state(cx).unwrap().0, original)
            })
            .unwrap();
    }
}

#[gpui::test]
fn gradient_track_inserts_drags_and_cancels_as_one_edit(cx: &mut TestAppContext) {
    for (tool, stroke) in [
        ("add-artboard", false),
        ("add-rectangle", false),
        ("add-rectangle", true),
        ("add-text", false),
    ] {
        let window = open(cx);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        create(&mut visual, tool);
        if tool == "add-text" {
            visual.simulate_input("Gradient");
        }
        if stroke {
            click(&mut visual, "stroke-visibility");
        }
        click(
            &mut visual,
            if stroke {
                "property-drag-16"
            } else {
                "property-drag-5"
            },
        );
        click(&mut visual, "fill-linear");
        let (original, undo) = window
            .update(&mut visual.cx, |w, _, cx| {
                (w.fill_state(cx).unwrap().1, w.history.borrow().undo_len())
            })
            .unwrap();
        let ramp = visual.debug_bounds("fill-gradient-ramp").unwrap();
        let at = |position| point(ramp.left() + ramp.size.width * position, ramp.center().y);
        visual.simulate_mouse_down(at(0.25), MouseButton::Left, Default::default());
        visual.simulate_mouse_move(at(0.8), MouseButton::Left, Default::default());
        visual.simulate_mouse_up(at(0.8), MouseButton::Left, Default::default());
        draw(&mut visual);
        let added = window
            .update(&mut visual.cx, |w, _, cx| {
                let g = w.fill_state(cx).unwrap().1;
                assert_eq!(w.inspector.active_stop, 2);
                assert_eq!(g.stops().len(), 3);
                assert!((g.stop(2).unwrap().position - 0.8).abs() < 0.001);
                assert_eq!(w.history.borrow().undo_len(), undo + 1);
                g
            })
            .unwrap();
        // Clicking off-center selects the handle without moving it or adding history.
        let handle = visual.debug_bounds("fill-stop-2").unwrap().center() + point(px(4.), px(0.));
        visual.simulate_click(handle, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |w, _, cx| {
                assert_eq!(w.fill_state(cx).unwrap().1, added);
                assert_eq!(w.history.borrow().undo_len(), undo + 1);
            })
            .unwrap();
        // Crossing another stop keeps identity, and Escape restores the entire preview.
        let start = visual.debug_bounds("fill-stop-1").unwrap().center();
        let end = start - point(ramp.size.width * 0.9, px(0.));
        visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
        visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |w, _, cx| {
                let g = w.fill_state(cx).unwrap().1;
                assert_eq!(w.inspector.active_stop, 1);
                assert_eq!(g.stops()[1].id, 1);
                assert!((g.stop(1).unwrap().position - 0.1).abs() < 0.001);
            })
            .unwrap();
        visual.simulate_keystrokes("escape");
        visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        let ramp = visual.debug_bounds("fill-gradient-ramp").unwrap();
        visual.simulate_mouse_down(ramp.center(), MouseButton::Left, Default::default());
        draw(&mut visual);
        visual.simulate_keystrokes("escape");
        visual.simulate_mouse_up(ramp.center(), MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |w, window, cx| {
                assert_eq!(w.fill_state(cx).unwrap().1, added);
                assert_eq!(w.history.borrow().undo_len(), undo + 1);
                w.replay_history(false, window, cx);
                assert_eq!(w.fill_state(cx).unwrap().1, original);
                w.replay_history(true, window, cx);
                assert_eq!(w.fill_state(cx).unwrap().1, added);
            })
            .unwrap();
    }
}

#[gpui::test]
fn gradient_list_grows_until_viewport_limit_without_displacing_picker(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    click(&mut visual, "property-drag-5");
    click(&mut visual, "fill-linear");
    for _ in 2..5 {
        click(&mut visual, "gradient-add");
    }
    let initial = visual.debug_bounds("color-panel").unwrap().size.height;
    click(&mut visual, "gradient-add");
    assert!(visual.debug_bounds("color-panel").unwrap().size.height > initial + px(30.));
    for _ in 6..20 {
        click(&mut visual, "gradient-add");
    }
    for height in [800., 640., 1100.] {
        visual.simulate_resize(size(px(1280.), px(height)));
        draw(&mut visual);
        let panel = visual.debug_bounds("color-panel").unwrap();
        let list = visual.debug_bounds("gradient-stop-list").unwrap();
        let picker = visual.debug_bounds("color-picker-sv").unwrap();
        assert!(panel.top() >= px(0.) && panel.bottom() <= px(height));
        assert!(list.size.height <= px(374.));
        assert!(picker.top() >= list.bottom());
        assert!(picker.bottom() <= panel.bottom());
        assert_eq!(picker.size.height, px(152.));
    }
}

#[gpui::test]
fn angular_seam_drag_keeps_colors_and_can_be_undone(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    click(&mut visual, "property-drag-5");
    click(&mut visual, "fill-linear");
    window
        .update(&mut visual.cx, |w, _, cx| {
            w.mutate_gradient(|g| g.kind = gpui::GradientKind::Angular, cx);
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    let (before, undo) = window
        .update(&mut visual.cx, |w, _, cx| {
            (w.fill_state(cx).unwrap().1, w.history.borrow().undo_len())
        })
        .unwrap();
    let start = visual.debug_bounds("fill-seam-width").unwrap().center();
    let end = start + point(px(15.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |w, window, cx| {
            let after = w.fill_state(cx).unwrap().1;
            assert!((after.seam_width - 0.27).abs() < 0.0001);
            assert_eq!(after.stops(), before.stops());
            assert_eq!(w.history.borrow().undo_len(), undo + 1);
            w.replay_history(false, window, cx);
            assert_eq!(w.fill_state(cx).unwrap().1, before);
        })
        .unwrap();
}

#[gpui::test]
fn gradient_midpoint_drag_is_one_edit_and_escape_restores_the_preview(cx: &mut TestAppContext) {
    for (tool, stroke) in [
        ("add-artboard", false),
        ("add-rectangle", false),
        ("add-rectangle", true),
        ("add-text", false),
    ] {
        let window = open(cx);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        create(&mut visual, tool);
        if tool == "add-text" {
            visual.simulate_input("Gradient");
        }
        if stroke {
            click(&mut visual, "stroke-visibility");
        }
        click(
            &mut visual,
            if stroke {
                "property-drag-16"
            } else {
                "property-drag-5"
            },
        );
        click(&mut visual, "fill-linear");
        let before = window
            .update(&mut visual.cx, |this, _, _| {
                this.history.borrow().undo_len()
            })
            .unwrap();
        let start = visual.debug_bounds("fill-midpoint-0").unwrap().center();
        let end = start - point(px(45.), px(0.));
        visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
        for dx in [15., 30., 45.] {
            visual.simulate_mouse_move(
                start - point(px(dx), px(0.)),
                MouseButton::Left,
                Default::default(),
            );
            draw(&mut visual);
        }
        visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, window, cx| {
                let after = this.fill_state(cx).unwrap().1;
                assert!(
                    after.stop(0).unwrap().midpoint < 0.5,
                    "{tool}, stroke={stroke}"
                );
                assert_eq!(this.history.borrow().undo_len(), before + 1);
                this.replay_history(false, window, cx);
                assert_eq!(
                    this.fill_state(cx).unwrap().1.stop(0).unwrap().midpoint,
                    0.5
                );
                this.replay_history(true, window, cx);
                assert_eq!(this.fill_state(cx).unwrap().1, after);
            })
            .unwrap();
        draw(&mut visual);
        // Re-open after history playback restores editor focus.
        click(
            &mut visual,
            if stroke {
                "property-drag-16"
            } else {
                "property-drag-5"
            },
        );
        let start = visual.debug_bounds("fill-midpoint-0").unwrap().center();
        let original = window
            .update(&mut visual.cx, |this, _, cx| this.fill_state(cx).unwrap().1)
            .unwrap();
        visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
        visual.simulate_mouse_move(
            start + point(px(90.), px(0.)),
            MouseButton::Left,
            Default::default(),
        );
        draw(&mut visual);
        visual.simulate_keystrokes("escape");
        visual.simulate_mouse_up(start, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert_eq!(this.fill_state(cx).unwrap().1, original);
                assert_eq!(this.history.borrow().undo_len(), before + 1);
            })
            .unwrap();
    }
}

#[gpui::test]
fn alpha_slider_updates_values_and_paint_live_without_changing_rgb_and_undoes_once(
    cx: &mut TestAppContext,
) {
    for (tool, gradient, stroke) in [
        ("add-artboard", false, false),
        ("add-rectangle", false, false),
        ("add-rectangle", true, false),
        ("add-rectangle", true, true),
        ("add-text", true, false),
    ] {
        let window = open(cx);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        create(&mut visual, tool);
        if tool == "add-text" {
            visual.simulate_input("Alpha");
        }
        if stroke {
            click(&mut visual, "stroke-visibility");
        }
        click(
            &mut visual,
            if stroke {
                "property-drag-16"
            } else {
                "property-drag-5"
            },
        );
        if gradient {
            click(&mut visual, "fill-linear");
        }
        let index = if stroke { 17 } else { 6 };
        let before = window
            .update(&mut visual.cx, |this, _, cx| {
                this.history.borrow_mut().break_group();
                (
                    this.inspector.picker.read(cx).value(),
                    this.history.borrow().undo_len(),
                )
            })
            .unwrap();
        let track = visual.debug_bounds("color-picker-alpha").unwrap();
        // The slider's interaction area sits inside its one-pixel border.
        let at = |fraction| {
            point(
                track.left() + px(1.) + (track.size.width - px(2.)) * fraction,
                track.center().y,
            )
        };
        visual.simulate_mouse_down(at(0.75), MouseButton::Left, Default::default());
        for value in [0.5, 0.25] {
            visual.simulate_mouse_move(at(value), MouseButton::Left, Default::default());
            draw(&mut visual);
            window
                .update(&mut visual.cx, |this, _, cx| {
                    assert!(
                        (this.field_value(index, cx).unwrap().parse::<f32>().unwrap()
                            - value * 100.)
                            .abs()
                            < 0.01,
                        "{tool}, gradient={gradient}, stroke={stroke}: expected {}, got {:?}",
                        value * 100.,
                        this.field_value(index, cx)
                    );
                    assert!(
                        (this.inspector.fields[index]
                            .read(cx)
                            .value()
                            .parse::<f32>()
                            .unwrap()
                            - value * 100.)
                            .abs()
                            < 0.01
                    );
                    let surface = if gradient { 2 } else { 1 };
                    assert!(
                        (this.inspector.fields[index + PROPERTY_COUNT * surface]
                            .read(cx)
                            .value()
                            .parse::<f32>()
                            .unwrap()
                            - value * 100.)
                            .abs()
                            < 0.01
                    );
                    let color = this.inspector.picker.read(cx).value();
                    assert!((color.a - value).abs() < 0.001);
                    assert!((color.r - before.0.r).abs() < 0.001);
                    assert!((color.g - before.0.g).abs() < 0.001);
                    assert!((color.b - before.0.b).abs() < 0.001);
                })
                .unwrap();
        }
        visual.simulate_mouse_up(at(0.25), MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, window, cx| {
                assert_eq!(this.history.borrow().undo_len(), before.1 + 1);
                this.replay_history(false, window, cx);
                assert!(
                    (this.field_value(index, cx).unwrap().parse::<f32>().unwrap()
                        - before.0.a * 100.)
                        .abs()
                        < 0.01
                );
                this.replay_history(true, window, cx);
                assert!(
                    (this.field_value(index, cx).unwrap().parse::<f32>().unwrap() - 25.).abs()
                        < 0.01
                );
            })
            .unwrap();
    }
}

#[gpui::test]
fn paint_inputs_keep_focus_and_selection_separate_between_surfaces(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-rectangle");
    for (index, selector, value) in [(5, "property-5", "00FF00"), (6, "property-6", "25")] {
        window
            .update(&mut visual.cx, |this, _, cx| {
                let shape = &mut this.shapes[0];
                shape.gradient.stop_mut(0).unwrap().color = shape.color;
                this.sync_fields(cx);
            })
            .unwrap();
        click(&mut visual, selector);
        visual.simulate_keystrokes("secondary-a");
        click(&mut visual, "property-drag-5");
        click(&mut visual, selector);
        visual.simulate_keystrokes("secondary-a left");
        window
            .update(&mut visual.cx, |this, window, cx| {
                let sidebar = this.inspector.fields[index].update(cx, |input, cx| {
                    input.selected_text_range(false, window, cx).unwrap().range
                });
                let solid = this.inspector.fields[index + PROPERTY_COUNT]
                    .update(cx, |input, cx| {
                        input.selected_text_range(false, window, cx).unwrap().range
                    });
                assert!(!sidebar.is_empty());
                assert_eq!(solid, 0..0);
                assert!(
                    !this.inspector.fields[index]
                        .focus_handle(cx)
                        .is_focused(window)
                );
                assert!(
                    this.inspector.fields[index + PROPERTY_COUNT]
                        .focus_handle(cx)
                        .is_focused(window)
                );
            })
            .unwrap();
        click(&mut visual, "fill-linear");
        window
            .update(&mut visual.cx, |this, window, cx| {
                assert!(
                    !this
                        .inspector
                        .fields
                        .iter()
                        .any(|input| input.focus_handle(cx).is_focused(window))
                );
            })
            .unwrap();
        click(&mut visual, selector);
        visual.simulate_keystrokes("secondary-a");
        window
            .update(&mut visual.cx, |this, window, cx| {
                assert!(
                    this.inspector.fields[index + PROPERTY_COUNT * 2]
                        .focus_handle(cx)
                        .is_focused(window)
                );
                let solid = this.inspector.fields[index + PROPERTY_COUNT]
                    .update(cx, |input, cx| {
                        input.selected_text_range(false, window, cx).unwrap().range
                    });
                assert_eq!(solid, 0..0);
            })
            .unwrap();
        click(&mut visual, "fill-solid");
        click(&mut visual, selector);
        visual.simulate_keystrokes("secondary-a");
        visual.simulate_input(value);
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, window, cx| {
                assert_eq!(
                    this.inspector.fields[index].read(cx).value().as_ref(),
                    value
                );
                assert!(
                    !this.inspector.fields[index]
                        .focus_handle(cx)
                        .is_focused(window)
                );
                assert!(
                    this.inspector.fields[index + PROPERTY_COUNT]
                        .focus_handle(cx)
                        .is_focused(window)
                );
            })
            .unwrap();
        click(&mut visual, "color-close");
        window
            .update(&mut visual.cx, |this, window, cx| {
                assert!(
                    !this.inspector.fields[index + PROPERTY_COUNT]
                        .focus_handle(cx)
                        .is_focused(window)
                );
            })
            .unwrap();
    }
}

#[gpui::test]
fn gradient_geometry_reverse_and_stop_edits_preserve_modes_and_history(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-rectangle");
    click(&mut visual, "fill-linear");
    let hue = visual.debug_bounds("color-picker-hue").unwrap();
    assert!(hue.size.width > hue.size.height);
    let position = point(hue.left() + hue.size.width * 0.25, hue.center().y);
    visual.simulate_mouse_down(position, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(position, MouseButton::Left, Default::default());
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!((this.inspector.picker.read(cx).hsva().h - 0.25).abs() < 0.01);
        })
        .unwrap();
    click(&mut visual, "gradient-add");
    click(&mut visual, "property-8");
    visual.simulate_keystrokes("secondary-a 3 0 enter");
    for (id, kind) in [
        ("gradient-radial", gpui::GradientKind::Radial),
        ("gradient-angular", gpui::GradientKind::Angular),
        ("gradient-diamond", gpui::GradientKind::Diamond),
    ] {
        let panel = visual.debug_bounds("color-panel").unwrap();
        let picker = visual.debug_bounds("color-picker-sv").unwrap();
        click(&mut visual, "gradient-kind");
        assert_eq!(visual.debug_bounds("color-panel").unwrap(), panel);
        assert_eq!(visual.debug_bounds("color-picker-sv").unwrap(), picker);
        assert!(visual.debug_bounds("gradient-kind-menu").is_some());
        click(&mut visual, id);
        assert!(visual.debug_bounds("gradient-kind-menu").is_none());
        window
            .update(&mut visual.cx, |this, _, _| {
                let shape = this.selected_shape().unwrap();
                assert_eq!(shape.gradient.kind, kind);
                assert_eq!(shape.gradient.stops().len(), 3);
                assert_eq!(shape.gradient.stops()[1].position, 0.3);
            })
            .unwrap();
    }
    click(&mut visual, "gradient-kind");
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("gradient-kind-menu").is_none());
    assert!(visual.debug_bounds("color-panel").is_some());
    click(&mut visual, "gradient-reverse");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(
                this.selected_shape().unwrap().gradient.stops()[1].position,
                0.7
            );
        })
        .unwrap();
    click(&mut visual, "fill-solid");
    click(&mut visual, "fill-linear");
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(
                this.selected_shape().unwrap().gradient.kind,
                gpui::GradientKind::Diamond
            );
            this.replay_history(false, window, cx);
            assert_eq!(this.selected_shape().unwrap().fill_mode, FillMode::Solid);
        })
        .unwrap();
}

#[gpui::test]
fn image_fill_import_fit_opacity_and_history_keep_source_dimensions_and_reject_invalid_files(
    cx: &mut TestAppContext,
) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("generated.png");
    image::RgbaImage::from_pixel(40, 20, image::Rgba([40, 180, 220, 255]))
        .save(&path)
        .unwrap();
    let invalid = temp.path().join("invalid.png");
    std::fs::write(&invalid, b"invalid image").unwrap();
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    for tool in ["add-artboard", "add-ellipse"] {
        create(&mut visual, tool);
        click(&mut visual, "property-drag-5");
        click(&mut visual, "fill-image");
        let rect = window
            .update(&mut visual.cx, |this, _, _| {
                this.selected_shape()
                    .map(|s| s.rect)
                    .unwrap_or_else(|| this.selected_board().unwrap().rect)
            })
            .unwrap();
        click(&mut visual, "image-fill-import");
        visual
            .cx
            .simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        draw(&mut visual);
        click(&mut visual, "image-fill-contain");
        click(&mut visual, "property-6");
        visual.simulate_keystrokes("secondary-a 4 0 enter");
        let before = window
            .update(&mut visual.cx, |this, _, _| {
                let fill = this.current_image_fill().unwrap();
                assert_eq!(fill.fit, crate::scene::image_fill::ImageFit::Contain);
                assert_eq!(fill.opacity, 0.4);
                let asset = fill.asset.as_ref().unwrap();
                assert_eq!((asset.width, asset.height), (40, 20));
                assert_eq!(
                    this.selected_shape()
                        .map(|s| s.rect)
                        .unwrap_or_else(|| this.selected_board().unwrap().rect),
                    rect
                );
                fill.clone()
            })
            .unwrap();
        click(&mut visual, "image-fill-import");
        visual
            .cx
            .simulate_path_prompt_response(|_| Some(vec![invalid.clone()]));
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.current_image_fill(), Some(&before))
            })
            .unwrap();
        click(&mut visual, "fill-solid");
        click(&mut visual, "fill-image");
        window
            .update(&mut visual.cx, |this, window, cx| {
                assert_eq!(this.current_image_fill(), Some(&before));
                this.replay_history(false, window, cx);
                this.replay_history(true, window, cx);
                assert_eq!(this.current_image_fill(), Some(&before));
                this.media.error = None;
            })
            .unwrap();
        click(&mut visual, "color-close");
    }
    std::fs::remove_file(path).unwrap();
    draw(&mut visual);
}
