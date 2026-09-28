use super::*;
use gpui::{TestAppContext, VisualTestContext, WindowHandle};

pub(super) fn open(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    cx.update(uic::init);
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        // Canvas gesture fixtures use the full editor area. Sidebar tests expand it explicitly.
        let mut workspace = Workspace::new(window, cx);
        workspace.sidebar.collapsed = true;
        workspace.snapping.enabled = false; // Geometry fixtures exercise unsnapped coordinates.
        workspace
    });
    window
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    window
}

pub(super) fn draw(visual: &mut VisualTestContext) {
    visual.cx.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    visual.cx.run_until_parked();
}

#[gpui::test]
fn canvas_tools_allow_pointer_anchored_wheel_zoom_and_pan(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let pointer = point(px(500.), px(350.));
    for tool in ["tool-hand", "add-rectangle", "tool-move"] {
        click(&mut visual, tool);
        let (zoom, anchor) = window
            .update(&mut visual.cx, |this, _, _| {
                let local = pointer - this.bounds.get().origin;
                (
                    this.view.zoom,
                    this.view
                        .world(point(f32::from(local.x), f32::from(local.y))),
                )
            })
            .unwrap();
        visual.simulate_event(gpui::ScrollWheelEvent {
            position: pointer,
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(100.))),
            modifiers: gpui::Modifiers {
                control: true,
                ..Default::default()
            },
            ..Default::default()
        });
        draw(&mut visual);
        let pan = window
            .update(&mut visual.cx, |this, _, _| {
                assert!(
                    (this.view.zoom - zoom * 0.4_f32.exp()).abs() < 0.001,
                    "{tool}"
                );
                let screen = this.view.screen(anchor);
                let local = pointer - this.bounds.get().origin;
                assert!((screen.x - f32::from(local.x)).abs() < 0.01);
                assert!((screen.y - f32::from(local.y)).abs() < 0.01);
                this.view.pan
            })
            .unwrap();
        visual.simulate_event(gpui::ScrollWheelEvent {
            position: pointer,
            delta: gpui::ScrollDelta::Pixels(point(px(15.), px(-30.))),
            ..Default::default()
        });
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.view.pan, pan + point(15., -30.), "{tool}");
                assert!((this.view.zoom - zoom * 0.4_f32.exp()).abs() < 0.001);
            })
            .unwrap();
    }
}

#[gpui::test]
fn high_zoom_keeps_pointer_anchor_and_grid_allows_object_selection(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-rectangle");
    let pointer = point(px(500.), px(350.));
    let (id, original, anchor) = window
        .update(&mut visual.cx, |this, _, _| {
            let shape = this.selected_shape().unwrap().clone();
            let local = pointer - this.bounds.get().origin;
            let local = point(f32::from(local.x), f32::from(local.y));
            this.view.zoom = 4.;
            this.view.pan = local - crate::rotation::center(shape.rect) * this.view.zoom;
            (shape.id, shape.rect, this.view.world(local))
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: pointer,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(1200.))),
        modifiers: gpui::Modifiers {
            control: true,
            ..Default::default()
        },
        ..Default::default()
    });
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.view.zoom, 256.);
            let screen = this.view.screen(anchor);
            let local = pointer - this.bounds.get().origin;
            assert!((screen.x - f32::from(local.x)).abs() < 0.05);
            assert!((screen.y - f32::from(local.y)).abs() < 0.05);
            this.select(None, cx);
            assert!(this.selected_shape.is_none());
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_mouse_down(pointer, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(pointer, MouseButton::Left, Default::default());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(id));
            assert_eq!(this.selected_shape().unwrap().rect, original);
        })
        .unwrap();
    click(&mut visual, "zoom-reset");
    window
        .update(&mut visual.cx, |this, _, _| assert_eq!(this.view.zoom, 1.))
        .unwrap();
}

pub(super) fn click(visual: &mut VisualTestContext, selector: &'static str) {
    draw(visual);
    // Paint controls now live in a swatch popover. Open it through the real trigger.
    if visual.debug_bounds(selector).is_none()
        && (matches!(
            selector,
            "fill-linear"
                | "fill-solid"
                | "property-7"
                | "property-8"
                | "property-12"
                | "property-13"
        ) || selector.starts_with("gradient-"))
    {
        let stroke = visual.update(|window, cx| {
            window
                .root::<Workspace>()
                .unwrap()
                .unwrap()
                .read(cx)
                .selected_shape
                .is_some()
                && window
                    .root::<Workspace>()
                    .unwrap()
                    .unwrap()
                    .read(cx)
                    .stroke_editing
        });
        let swatch = if stroke {
            "property-drag-16"
        } else {
            "property-drag-5"
        };
        if visual.debug_bounds(swatch).is_none() {
            click(visual, if stroke { "shape-stroke" } else { "shape-fill" });
        }
        click(visual, swatch);
    }
    let mut bounds = visual.debug_bounds(selector);
    if bounds.is_none() && matches!(selector, "property-5" | "property-6") {
        bounds = visual.debug_bounds(if selector == "property-5" {
            "property-16"
        } else {
            "property-17"
        });
    }
    if bounds.is_none() {
        let group = match selector {
            "add-rectangle" | "add-ellipse" | "draw-line" | "draw-arrow" | "add-polygon"
            | "add-star" | "import-media" => Some("shapes-menu"),
            "draw-bezier" | "draw-pen" => Some("pen-menu"),
            "tool-move" | "tool-hand" => Some("navigation-menu"),
            _ => None,
        };
        if let Some(group) = group {
            click(visual, group);
            bounds = visual.debug_bounds(match selector {
                "add-rectangle" => "option-add-rectangle",
                "add-ellipse" => "option-add-ellipse",
                "draw-line" => "option-draw-line",
                "draw-arrow" => "option-draw-arrow",
                "add-polygon" => "option-add-polygon",
                "add-star" => "option-add-star",
                "import-media" => "option-import-media",
                "draw-bezier" => "option-draw-bezier",
                "draw-pen" => "option-draw-pen",
                "tool-move" => "option-tool-move",
                "tool-hand" => "option-tool-hand",
                _ => unreachable!(),
            });
        }
    }
    let bounds = bounds.expect(selector);
    if let Some(panel) = visual.debug_bounds("color-panel")
        && !panel.contains(&bounds.center())
    {
        visual.simulate_keystrokes("escape");
        draw(visual);
    }
    visual.simulate_click(bounds.center(), Default::default());
    draw(visual);
}

// Existing inspector/history tests use this helper to draw a predictable fixture.
// Tool activation remains a separate click; creation goes through real pointer events.
pub(super) fn create(visual: &mut VisualTestContext, selector: &'static str) {
    click(visual, selector);
    let (start, end) = visual.update(|window, cx| {
        window
            .root::<Workspace>()
            .unwrap()
            .unwrap()
            .update(cx, |this, cx| {
                let rect = if selector == "add-artboard" {
                    let x = this
                        .boards
                        .iter()
                        .map(|b| b.rect.x + b.rect.width + 80.)
                        .reduce(f32::max)
                        .unwrap_or(0.);
                    Rect {
                        x,
                        y: 0.,
                        width: 640.,
                        height: 480.,
                    }
                } else {
                    let board = this.selected_board();
                    let area = board.map_or(
                        Rect {
                            x: 0.,
                            y: 0.,
                            width: 640.,
                            height: 480.,
                        },
                        |b| b.rect,
                    );
                    let (width, height, count) = if selector == "add-text" {
                        (
                            320_f32.min(area.width),
                            120_f32.min(area.height),
                            this.texts
                                .iter()
                                .filter(|t| t.board == board.map(|b| b.id))
                                .count(),
                        )
                    } else {
                        (
                            200_f32.min(area.width),
                            160_f32.min(area.height),
                            this.shapes
                                .iter()
                                .filter(|s| s.board == board.map(|b| b.id))
                                .count(),
                        )
                    };
                    let offset = (count % 8) as f32 * 16.;
                    Rect {
                        x: area.x + (area.width - width) / 2. + offset,
                        y: area.y + (area.height - height) / 2. + offset,
                        width,
                        height,
                    }
                };
                if selector == "add-artboard" {
                    let size = this.bounds.get().size;
                    let width = f32::from(size.width);
                    let height = f32::from(size.height);
                    this.view.zoom = ((width - 96.) / rect.width)
                        .min((height - 120.) / rect.height)
                        .clamp(0.1, 1.);
                    this.view.pan = point(
                        width / 2. - (rect.x + rect.width / 2.) * this.view.zoom,
                        height / 2. - (rect.y + rect.height / 2.) * this.view.zoom,
                    );
                    cx.notify();
                }
                let screen = |p| {
                    let p = this.view.screen(p);
                    this.bounds.get().origin + point(px(p.x), px(p.y))
                };
                (
                    screen(point(rect.x, rect.y)),
                    screen(point(rect.x + rect.width, rect.y + rect.height)),
                )
            })
    });
    draw(visual);
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(visual);
}

#[gpui::test]
fn selection_properties_and_gradient_keep_other_characters_unchanged(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let window = cx.open_window(size(px(1280.), px(1200.)), |window, cx| {
        Workspace::new(window, cx)
    });
    window
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "toggle-layers");
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("abcd");
    visual.simulate_keystrokes("ctrl-home shift-right shift-right");
    draw(&mut visual);
    click(&mut visual, "property-7");
    visual.simulate_keystrokes("ctrl-a 4 8");
    draw(&mut visual);
    click(&mut visual, "fill-linear");
    click(&mut visual, "gradient-add");
    click(&mut visual, "gradient-add");
    click(&mut visual, "property-5");
    visual.simulate_keystrokes("ctrl-a f f 0 0 0 0");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            let text = this.texts[0].editor.read(cx);
            assert!(text.has_style_selection());
            assert_eq!(text.style_range(), 0..2);
            assert_eq!(text.effective_style().size, 48.);
            assert_eq!(text.effective_style().fill_mode, FillMode::Linear);
            assert_eq!(text.effective_style().gradient.stops().len(), 4);
            assert_eq!(
                text.effective_style().editable_color(this.active_stop),
                rgb(0xff0000)
            );
            assert_eq!(this.boards[0].fill_mode, FillMode::Solid);
            let focus = text.focus.clone();
            focus.focus(window, cx);
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-a");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            let text = this.texts[0].editor.read(cx);
            assert!(text.mixed(TextProperty::Size));
            assert!(text.mixed(TextProperty::FillMode));
            assert_eq!(this.fields[7].read(cx).value().as_ref(), "");
            assert_eq!(text.content, "abcd");
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-end shift-left shift-left");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            let text = this.texts[0].editor.read(cx);
            assert_eq!(text.style_range(), 2..4);
            assert_eq!(text.effective_style().size, 24.);
            assert_eq!(text.effective_style().fill_mode, FillMode::Solid);
            assert_eq!(this.fields[7].read(cx).value().as_ref(), "24");
        })
        .unwrap();
    // Escape switches the target back to the entire box without flattening other properties.
    visual.simulate_keystrokes("escape");
    click(&mut visual, "property-10");
    visual.simulate_keystrokes("ctrl-a 7 0 0");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            let text = this.texts[0].editor.read(cx);
            assert!(!text.mixed(TextProperty::Weight));
            assert!(text.mixed(TextProperty::Size));
            assert_eq!(text.effective_style().weight, 700.);
            assert_eq!(this.texts[0].rect.width, 320.);
        })
        .unwrap();
}

#[gpui::test]
fn text_creation_typing_properties_and_escape_preserve_the_artboard(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "add-text");
    window
        .update(&mut visual.cx, |this, _, _| assert!(this.texts.is_empty()))
        .unwrap();
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_keystrokes("h e l l o space w o r l d enter a backspace");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.texts[0].editor.read(cx).content, "hello world\n");
            assert!(this.texts[0].editor.read(cx).focus.is_focused(window));
            assert!(!this.space_down);
            assert_eq!(this.boards.len(), 1);
            assert_eq!(this.texts[0].board, Some(1));
            assert_eq!(this.texts[0].rect.height, 120.);
        })
        .unwrap();
    click(&mut visual, "property-4");
    visual.simulate_keystrokes("ctrl-a 1");
    click(&mut visual, "property-7");
    visual.simulate_keystrokes("ctrl-a 4 8");
    click(&mut visual, "align-center");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].rect.height, 1.);
            assert_eq!(this.texts[0].editor.read(cx).effective_style().size, 48.);
            assert_eq!(
                this.texts[0].editor.read(cx).effective_style().align,
                gpui::TextAlign::Center
            );
            assert_eq!(this.texts[0].editor.read(cx).content, "hello world\n");
            assert_eq!(this.boards[0].rect.height, 480.);
        })
        .unwrap();
    click(&mut visual, "property-4");
    visual.simulate_keystrokes("ctrl-a 1 2 0");
    draw(&mut visual);
    // Re-enter through the actual double-click event, then continue at the retained caret.
    let position = visual.debug_bounds("text-box-2").unwrap().center();
    visual.simulate_event(gpui::MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        modifiers: Default::default(),
        first_mouse: false,
    });
    draw(&mut visual);
    visual.simulate_keystrokes("b escape");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.texts[0].editor.read(cx).content, "hello world\nb");
            assert!(this.focus.is_focused(window));
            assert_eq!(this.selected_text, Some(2));
        })
        .unwrap();
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.texts.is_empty());
            assert_eq!(this.boards.len(), 1);
        })
        .unwrap();
}

#[gpui::test]
fn text_resize_keeps_font_size_and_children_follow_board_without_clipping(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("第一行文字\n第二行文字");
    draw(&mut visual);
    let original = window
        .update(&mut visual.cx, |this, _, _| this.texts[0].rect)
        .unwrap();
    let start = visual.debug_bounds("text-handle-4").unwrap().center();
    let end = start + point(px(-45.), px(20.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(
                (this.texts[0].rect.width - original.width + 45. / this.view.zoom).abs() < 0.01
            );
            assert_eq!(this.texts[0].editor.read(cx).effective_style().size, 24.);
            assert_eq!(
                this.texts[0].editor.read(cx).content,
                "第一行文字\n第二行文字"
            );
            // Leave the board's boundary, but stay within the editor viewport.
            this.texts[0].rect.x = -20.;
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    let text_before = visual.debug_bounds("text-box-2").unwrap();
    let board_before = visual.debug_bounds("artboard-1").unwrap();
    assert!(text_before.left() < board_before.left());
    let grab = board_before.origin + point(px(10.), px(10.));
    visual.simulate_mouse_down(grab, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        grab + point(px(30.), px(25.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_up(
        grab + point(px(30.), px(25.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(&mut visual);
    let after = visual.debug_bounds("text-box-2").unwrap();
    assert!((f32::from(after.left() - text_before.left()) - 30.).abs() < 0.01);
    assert!((f32::from(after.top() - text_before.top()) - 25.).abs() < 0.01);
    // The part outside the board can still be selected and moved.
    let outside = point(after.left() + px(1.), after.top() + px(20.));
    visual.simulate_click(outside, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_text, Some(2))
        })
        .unwrap();
}

#[gpui::test]
fn text_selection_survives_zoom_and_multiple_text_boxes_keep_independent_content(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("first");
    visual.simulate_keystrokes("ctrl-a");
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.view.zoom_at(point(400., 300.), 0.75);
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-c");
    visual.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "first"));
    create(&mut visual, "add-text");
    visual.simulate_input("second");
    draw(&mut visual);
    let bounds = visual.debug_bounds("text-box-3").unwrap();
    // Drag beyond the box, ending over the inspector; release must stop selection.
    let start = bounds.origin + point(px(1.), px(8.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    draw(&mut visual);
    let end = point(px(1200.), bounds.bottom() + px(20.));
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-c");
    visual.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "second"));
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).content, "first");
            assert_eq!(this.texts[1].editor.read(cx).content, "second");
            assert!(this.gesture.is_none());
        })
        .unwrap();
}

#[gpui::test]
fn creation_and_property_editing_keep_boards_independent(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("ctrl-a 3 2 0");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.boards.len(), 1);
            assert_eq!(this.boards[0].rect.width, 320.);
            assert_eq!(this.fields[3].read(cx).value().as_ref(), "320");
        })
        .unwrap();
    click(&mut visual, "property-6");
    visual.simulate_keystrokes("ctrl-a 0");
    draw(&mut visual);
    create(&mut visual, "add-artboard");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.boards.len(), 2);
            assert_eq!(this.boards[0].color.a, 0.);
            assert_eq!(this.boards[0].rect.width, 320.);
            assert_eq!(this.boards[1].color.a, 1.);
            assert_eq!(this.boards[1].rect.width, 640.);
            assert_eq!(this.fields[6].read(cx).value().as_ref(), "100");
            assert_eq!(this.selected, Some(2));
        })
        .unwrap();
}

#[gpui::test]
fn resized_handle_uses_world_delta_and_escape_restores_original(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    let (original, zoom) = window
        .update(&mut visual.cx, |this, _, cx| {
            this.view.zoom_at(point(400., 350.), this.view.zoom * 0.5);
            cx.notify();
            (this.boards[0].rect, this.view.zoom)
        })
        .unwrap();
    draw(&mut visual);
    let start = visual.debug_bounds("handle-4").unwrap().center();
    let end = start + point(px(40.), px(-20.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    draw(&mut visual);
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!((this.boards[0].rect.width - original.width - 40. / zoom).abs() < 0.01);
            assert!((this.boards[0].rect.height - original.height + 20. / zoom).abs() < 0.01);
            assert_eq!(this.boards[0].rect.x, original.x);
            assert_eq!(this.boards[0].rect.y, original.y);
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect, original);
            assert!(this.gesture.is_none());
        })
        .unwrap();
}

#[gpui::test]
fn artboard_edges_resize_away_from_midpoints_with_only_four_visible_corners(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    let original = Rect {
        x: 0.,
        y: 0.,
        width: 300.,
        height: 200.,
    };
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.boards[0].rect = original;
            this.view.zoom = 1.5;
            this.view.pan = point(100., 80.);
            this.sync_fields(cx);
        })
        .unwrap();
    draw(&mut visual);
    for (index, selector) in [
        "board-corner-0",
        "board-corner-1",
        "board-corner-2",
        "board-corner-3",
        "board-corner-4",
        "board-corner-5",
        "board-corner-6",
        "board-corner-7",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(visual.debug_bounds(selector).is_some(), index % 2 == 0);
    }
    for (selector, delta, expected) in [
        (
            "handle-1",
            point(0., -30.),
            Rect {
                y: -20.,
                height: 220.,
                ..original
            },
        ),
        (
            "handle-3",
            point(30., 0.),
            Rect {
                width: 320.,
                ..original
            },
        ),
        (
            "handle-5",
            point(0., 30.),
            Rect {
                height: 220.,
                ..original
            },
        ),
        (
            "handle-7",
            point(-30., 0.),
            Rect {
                x: -20.,
                width: 320.,
                ..original
            },
        ),
    ] {
        draw(&mut visual);
        let edge = visual.debug_bounds(selector).unwrap();
        let start = if delta.x == 0. {
            point(edge.left() + edge.size.width * 0.25, edge.center().y)
        } else {
            point(edge.center().x, edge.top() + edge.size.height * 0.25)
        };
        let end = start + point(px(delta.x), px(delta.y));
        visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
        visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
        visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, window, cx| {
                assert_eq!(this.boards[0].rect, expected);
                this.replay_history(false, window, cx);
                assert_eq!(this.boards[0].rect, original);
            })
            .unwrap();
    }
}

#[gpui::test]
fn drag_releases_over_sidebar_and_updates_inspector(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    let start = visual.debug_bounds("artboard-1").unwrap().center();
    let (original, zoom) = window
        .update(&mut visual.cx, |this, _, _| {
            (this.boards[0].rect, this.view.zoom)
        })
        .unwrap();
    let end = point(px(1120.), start.y + px(30.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    draw(&mut visual);
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert!(this.gesture.is_none());
            assert!(window.captured_hitbox().is_none());
            assert!(
                (this.boards[0].rect.x - original.x - f32::from(end.x - start.x) / zoom).abs()
                    < 0.01
            );
            assert_eq!(this.boards[0].rect.width, original.width);
            let shown = this.fields[1].read(cx).value().parse::<f32>().unwrap();
            assert!((shown - this.boards[0].rect.x).abs() < 0.01);
        })
        .unwrap();
}

#[gpui::test]
fn invalid_input_does_not_resize_and_normalizes_on_blur(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("ctrl-a 0");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect.width, 640.);
            assert!(this.invalid[3]);
        })
        .unwrap();
    click(&mut visual, "property-4");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.fields[3].read(cx).value().as_ref(), "640");
            assert!(!this.invalid[3]);
        })
        .unwrap();
}

#[gpui::test]
fn inspector_name_does_not_expand_to_fill_unused_panel_height(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    let name = visual.debug_bounds("property-0").unwrap();
    let x = visual.debug_bounds("property-1").unwrap();
    // The layout heading and alignment toolbar now sit between the name and position.
    assert!(
        x.origin.y - name.bottom() < px(100.),
        "name: {name:?}, x: {x:?}"
    );
    assert_eq!(name.size.height, px(30.));
}

#[gpui::test]
fn gradient_editor_targets_selected_stop_and_preserves_each_fill_mode(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1100.)));
    create(&mut visual, "add-artboard");
    click(&mut visual, "property-6");
    visual.simulate_keystrokes("ctrl-a 5 0");
    draw(&mut visual);
    click(&mut visual, "fill-linear");
    click(&mut visual, "gradient-stop-1");
    click(&mut visual, "property-5");
    visual.simulate_keystrokes("ctrl-a F F 0 0 0 0");
    draw(&mut visual);
    click(&mut visual, "property-6");
    visual.simulate_keystrokes("ctrl-a 2 5");
    draw(&mut visual);
    click(&mut visual, "property-7");
    visual.simulate_keystrokes("ctrl-a 4 5");
    draw(&mut visual);
    click(&mut visual, "property-8");
    visual.simulate_keystrokes("ctrl-a 7 0");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let board = &this.boards[0];
            assert_eq!(board.fill_mode, FillMode::Linear);
            assert_eq!(board.color.a, 0.5);
            let end = board.gradient.stop(1).unwrap();
            assert_eq!(end.color.r, 1.);
            assert_eq!(end.color.g, 0.);
            assert_eq!(end.color.a, 0.25);
            assert_eq!(end.position, 0.7);
            assert_eq!(board.gradient.angle, 45.);
        })
        .unwrap();
    click(&mut visual, "gradient-add");
    click(&mut visual, "property-8");
    visual.simulate_keystrokes("ctrl-a 9 0");
    draw(&mut visual);
    let saved = window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.active_stop, 2);
            assert_eq!(this.boards[0].gradient.stops().last().unwrap().id, 2);
            assert_eq!(this.fields[8].read(cx).value().as_ref(), "90");
            this.boards[0].gradient.clone()
        })
        .unwrap();
    click(&mut visual, "fill-solid");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.fields[6].read(cx).value().as_ref(), "50");
            assert_eq!(this.fields[5].read(cx).value().as_ref(), "FFFFFF");
        })
        .unwrap();
    click(&mut visual, "fill-linear");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].gradient, saved);
        })
        .unwrap();
    click(&mut visual, "gradient-remove");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].gradient.stops().len(), 2);
            assert!(this.boards[0].gradient.stop(this.active_stop).is_some());
        })
        .unwrap();
    create(&mut visual, "add-artboard");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[1].fill_mode, FillMode::Solid);
            assert_eq!(this.boards[1].color.a, 1.);
            assert_eq!(this.boards[0].gradient.angle, 45.);
        })
        .unwrap();
}

#[gpui::test]
fn vertical_alignment_targets_selection_and_supports_mixed_state_and_undo(cx: &mut TestAppContext) {
    use crate::text::VerticalAlign;
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1200.)));
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("AB");
    visual.simulate_keystrokes("ctrl-home shift-right");
    click(&mut visual, "property-7");
    visual.simulate_keystrokes("ctrl-a 7 2");
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.texts[0]
                .editor
                .read(cx)
                .focus
                .clone()
                .focus(window, cx);
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-end shift-left");
    click(&mut visual, "vertical-top");
    window
        .update(&mut visual.cx, |this, _, cx| {
            let editor = this.texts[0].editor.read(cx);
            assert_eq!(editor.style_range(), 1..2);
            assert_eq!(editor.effective_style().vertical_align, VerticalAlign::Top);
            assert_eq!(editor.effective_style().size, 24.);
        })
        .unwrap();
    click(&mut visual, "vertical-bottom");
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(
                this.texts[0]
                    .editor
                    .read(cx)
                    .effective_style()
                    .vertical_align,
                VerticalAlign::Top
            );
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z ctrl-a");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            let editor = this.texts[0].editor.read(cx);
            assert!(editor.mixed(TextProperty::VerticalAlign));
            assert!(editor.mixed(TextProperty::Size));
        })
        .unwrap();
    // Whole-box alignment changes only this property, preserving the mixed sizes.
    visual.simulate_keystrokes("escape");
    click(&mut visual, "vertical-center");
    window
        .update(&mut visual.cx, |this, _, cx| {
            let editor = this.texts[0].editor.read(cx);
            assert!(!editor.mixed(TextProperty::VerticalAlign));
            assert!(editor.mixed(TextProperty::Size));
            assert_eq!(
                editor.effective_style().vertical_align,
                VerticalAlign::Center
            );
            assert_eq!(editor.content, "AB");
        })
        .unwrap();
}
