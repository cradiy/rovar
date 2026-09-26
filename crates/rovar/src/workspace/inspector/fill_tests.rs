use super::*;
use crate::workspace::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

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
                    this.picker.read(cx).value(),
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
                        (this.fields[index].read(cx).value().parse::<f32>().unwrap()
                            - value * 100.)
                            .abs()
                            < 0.01
                    );
                    let surface = if gradient { 2 } else { 1 };
                    assert!(
                        (this.fields[index + PROPERTY_COUNT * surface]
                            .read(cx)
                            .value()
                            .parse::<f32>()
                            .unwrap()
                            - value * 100.)
                            .abs()
                            < 0.01
                    );
                    let color = this.picker.read(cx).value();
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
        visual.simulate_keystrokes("ctrl-a");
        click(&mut visual, "property-drag-5");
        click(&mut visual, selector);
        visual.simulate_keystrokes("ctrl-a left");
        window
            .update(&mut visual.cx, |this, window, cx| {
                let sidebar = this.fields[index].update(cx, |input, cx| {
                    input.selected_text_range(false, window, cx).unwrap().range
                });
                let solid = this.fields[index + PROPERTY_COUNT].update(cx, |input, cx| {
                    input.selected_text_range(false, window, cx).unwrap().range
                });
                assert!(!sidebar.is_empty());
                assert_eq!(solid, 0..0);
                assert!(!this.fields[index].focus_handle(cx).is_focused(window));
                assert!(
                    this.fields[index + PROPERTY_COUNT]
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
                        .fields
                        .iter()
                        .any(|input| input.focus_handle(cx).is_focused(window))
                );
            })
            .unwrap();
        click(&mut visual, selector);
        visual.simulate_keystrokes("ctrl-a");
        window
            .update(&mut visual.cx, |this, window, cx| {
                assert!(
                    this.fields[index + PROPERTY_COUNT * 2]
                        .focus_handle(cx)
                        .is_focused(window)
                );
                let solid = this.fields[index + PROPERTY_COUNT].update(cx, |input, cx| {
                    input.selected_text_range(false, window, cx).unwrap().range
                });
                assert_eq!(solid, 0..0);
            })
            .unwrap();
        click(&mut visual, "fill-solid");
        click(&mut visual, selector);
        visual.simulate_keystrokes("ctrl-a");
        visual.simulate_input(value);
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, window, cx| {
                assert_eq!(this.fields[index].read(cx).value().as_ref(), value);
                assert!(!this.fields[index].focus_handle(cx).is_focused(window));
                assert!(
                    this.fields[index + PROPERTY_COUNT]
                        .focus_handle(cx)
                        .is_focused(window)
                );
            })
            .unwrap();
        click(&mut visual, "color-close");
        window
            .update(&mut visual.cx, |this, window, cx| {
                assert!(
                    !this.fields[index + PROPERTY_COUNT]
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
            assert!((this.picker.read(cx).hsva().h - 0.25).abs() < 0.01);
        })
        .unwrap();
    click(&mut visual, "gradient-add");
    click(&mut visual, "property-8");
    visual.simulate_keystrokes("ctrl-a 3 0 enter");
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
        visual.simulate_keystrokes("ctrl-a 4 0 enter");
        let before = window
            .update(&mut visual.cx, |this, _, _| {
                let fill = this.current_image_fill().unwrap();
                assert_eq!(fill.fit, crate::image_fill::ImageFit::Contain);
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
                this.media_error = None;
            })
            .unwrap();
        click(&mut visual, "color-close");
    }
    std::fs::remove_file(path).unwrap();
    draw(&mut visual);
}
