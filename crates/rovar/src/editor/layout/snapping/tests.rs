use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle};

#[gpui::test]
fn equal_gaps_snap_while_dragging_release_with_alt_and_undo_as_one_edit(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, cx| {
            this.shapes.clear();
            for (id, x, width) in [(1, 0., 60.), (2, 80., 80.), (3, 240., 40.)] {
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
            this.select_shape(3, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = visual.debug_bounds("shape-3").unwrap().center();
    let end = start - point(px(57.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[2].rect.x, 180.);
            assert_eq!(
                this.snapping
                    .gaps
                    .iter()
                    .map(|g| g.end - g.start)
                    .collect::<Vec<_>>(),
                vec![20., 20.]
            );
        })
        .unwrap();
    modifiers(
        &mut visual,
        Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[2].rect.x, 183.);
            assert!(this.snapping.gaps.is_empty());
        })
        .unwrap();
    modifiers(&mut visual, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[2].rect.x, 180.);
            assert!(this.snapping.gaps.is_empty());
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[2].rect.x, 240.)
        })
        .unwrap();
}

#[gpui::test]
fn spacing_between_neighbors_respects_zoom_and_parent_scope(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, cx| {
            this.shapes.clear();
            for (id, x) in [(1, 0.), (2, 200.), (3, 400.)] {
                this.shapes.push(Shape::new(
                    id,
                    None,
                    ShapeKind::Rectangle,
                    Rect {
                        x,
                        y: 100.,
                        width: 40.,
                        height: 40.,
                    },
                ));
            }
            this.view.zoom = 2.;
            this.select_shape(3, cx);
            this.begin_snapping(GestureKind::Shape {
                id: 3,
                original: this.shapes[2].rect,
                handle: None,
            });
            assert_eq!(this.snap_delta(point(-298., 0.)).x, -300.); // Two equal 60-unit gaps.
            assert_eq!(this.snapping.gaps.len(), 2);
            assert_eq!(this.snap_delta(point(-296., 0.)).x, -296.); // Eight screen pixels is outside tolerance.
            assert!(this.snapping.gaps.is_empty());
            // A different parent cannot supply a spacing neighbor, even if nearby.
            this.next_id = 4;
            this.add_artboard(
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 600.,
                    height: 400.,
                },
                cx,
            );
            this.shapes[1].board = this.selected;
            this.select_shape(3, cx);
            this.begin_snapping(GestureKind::Shape {
                id: 3,
                original: this.shapes[2].rect,
                handle: None,
            });
            assert_eq!(this.snap_delta(point(-298., 0.)).x, -298.);
            assert!(this.snapping.gaps.is_empty());
        })
        .unwrap();
}

fn fixture(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 80.);
            this.view.zoom = 1.;
            this.snapping.enabled = true;
            this.shapes.push(Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                Rect {
                    x: 100.,
                    y: 100.,
                    width: 100.,
                    height: 100.,
                },
            ));
            this.next_id = 2;
            this.focus.focus(window, cx);
        })
        .unwrap();
    window
}
fn screen(visual: &mut VisualTestContext, p: Point<f32>) -> Point<Pixels> {
    visual.update(|window, cx| {
        let root = window.root::<Workspace>().unwrap().unwrap();
        let this = root.read(cx);
        let p = this.view.screen(p);
        this.bounds.get().origin + point(px(p.x), px(p.y))
    })
}
fn modifiers(visual: &mut VisualTestContext, modifiers: Modifiers) {
    visual.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::ModifiersChanged(gpui::ModifiersChangedEvent {
                modifiers,
                ..Default::default()
            }),
            cx,
        );
    });
    draw(visual);
}
#[gpui::test]
fn reverse_creation_snaps_corners_and_click_does_not_create_after_alt_changes(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    for tool in ["add-rectangle", "add-ellipse", "add-artboard", "add-text"] {
        click(&mut visual, tool);
        let start = screen(&mut visual, point(203., 203.));
        let end = screen(&mut visual, point(98., 103.));
        visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
        visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
        visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, window, cx| {
                let id = *this.selection_ids().first().unwrap();
                assert_eq!(
                    this.world_rect(id),
                    Some(Rect {
                        x: 100.,
                        y: 100.,
                        width: 100.,
                        height: 100.
                    })
                );
                assert!(this.snapping.guides.is_empty());
                this.replay_history(false, window, cx);
            })
            .unwrap();
        draw(&mut visual);
    }
    click(&mut visual, "add-rectangle");
    let p = screen(&mut visual, point(103., 103.));
    visual.simulate_mouse_down(
        p,
        MouseButton::Left,
        Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    modifiers(&mut visual, Default::default());
    visual.simulate_mouse_up(p, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 1)
        })
        .unwrap();
    let start = screen(&mut visual, point(20., 20.));
    let end = screen(&mut visual, point(198., 170.));
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    visual.simulate_mouse_down(start, MouseButton::Left, shift);
    visual.simulate_mouse_move(end, MouseButton::Left, shift);
    visual.simulate_mouse_up(end, MouseButton::Left, shift);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let rect = this.shapes.last().unwrap().rect;
            assert_eq!(
                rect,
                Rect {
                    x: 20.,
                    y: 20.,
                    width: 180.,
                    height: 180.
                }
            );
        })
        .unwrap();
}
#[gpui::test]
fn resizing_preserves_opposite_corner_aspect_and_reacts_to_stationary_modifiers(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.shapes[0].rect.height = 50.;
            this.shapes.push(Shape::new(
                2,
                None,
                ShapeKind::Rectangle,
                Rect {
                    x: 300.,
                    y: 300.,
                    width: 100.,
                    height: 100.,
                },
            ));
            this.next_id = 3;
            this.select_shape(1, cx);
        })
        .unwrap();
    draw(&mut visual);
    let start = visual.debug_bounds("shape-handle-4").unwrap().center();
    let end = start + point(px(98.), px(25.));
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    visual.simulate_mouse_down(start, MouseButton::Left, shift);
    visual.simulate_mouse_move(end, MouseButton::Left, shift);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(
                this.shapes[0].rect,
                Rect {
                    x: 100.,
                    y: 100.,
                    width: 200.,
                    height: 100.
                }
            );
            assert!(!this.snapping.guides.is_empty());
        })
        .unwrap();
    modifiers(
        &mut visual,
        Modifiers {
            alt: true,
            shift: true,
            ..Default::default()
        },
    );
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.width, 198.);
            assert_eq!(this.shapes[0].rect.height, 99.);
            assert!(this.snapping.guides.is_empty());
        })
        .unwrap();
    modifiers(&mut visual, Default::default());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.width, 200.);
            assert_eq!(this.shapes[0].rect.height, 75.);
        })
        .unwrap();
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.history.borrow().undo_len(), 1)
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.height, 50.)
        })
        .unwrap();
    let start = visual.debug_bounds("shape-handle-0").unwrap().center();
    let end = start + point(px(500.), px(500.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let r = this.shapes[0].rect;
            assert_eq!(r.width, 1.);
            assert_eq!(r.height, 1.);
            assert_eq!(r.x + r.width, 200.);
            assert_eq!(r.y + r.height, 150.);
            assert!(this.snapping.guides.is_empty());
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.width, 100.);
            assert!(this.history.borrow().can_redo());
        })
        .unwrap();
}
#[gpui::test]
fn text_resize_snaps_in_world_coordinates_and_frame_resize_excludes_its_children(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.add_artboard(
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 400.,
                    height: 400.,
                },
                cx,
            );
            let board = this.selected.unwrap();
            this.shapes[0].board = Some(board);
            let text = this.make_text(
                3,
                Some(board),
                Rect {
                    x: 20.,
                    y: 30.,
                    width: 50.,
                    height: 40.,
                },
                window,
                cx,
            );
            this.texts.push(text);
            this.next_id = 4;
            this.select_text(3, cx);
            this.view.zoom = 2.;
        })
        .unwrap();
    draw(&mut visual);
    let start = visual.debug_bounds("text-handle-3").unwrap().center();
    let end = start + point(px(57.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].rect.width, 80.);
            assert_eq!(this.texts[0].rect.height, 40.);
            this.select(Some(2), cx);
            this.begin_snapping(GestureKind::Resize {
                id: 2,
                original: this.boards[0].rect,
                handle: Handle(1, 1),
            });
            let rect = this.resize_with_snapping(
                2,
                this.boards[0].rect,
                Handle(1, 1),
                point(-201., -201.),
                false,
            );
            assert_eq!(rect.width, 199.);
            assert_eq!(rect.height, 199.);
            assert!(this.snapping.guides.is_empty());
        })
        .unwrap();
}
#[gpui::test]
fn line_endpoint_snapping_keeps_shift_angle_and_freehand_is_not_magnetized(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "draw-line");
    let start = screen(&mut visual, point(0., 0.));
    let end = screen(&mut visual, point(198., 196.));
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    visual.simulate_mouse_down(start, MouseButton::Left, shift);
    visual.simulate_mouse_move(end, MouseButton::Left, shift);
    visual.simulate_mouse_up(end, MouseButton::Left, shift);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let line = this.shapes.last().unwrap();
            let p = line.path_point(1);
            assert!((p.x - 200.).abs() < 0.001 && (p.y - 200.).abs() < 0.001);
        })
        .unwrap();
    let from = visual.debug_bounds("line-end-1").unwrap().center();
    let to = screen(&mut visual, point(99., 98.));
    visual.simulate_mouse_down(from, MouseButton::Left, shift);
    visual.simulate_mouse_move(to, MouseButton::Left, shift);
    visual.simulate_mouse_up(to, MouseButton::Left, shift);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let p = this.shapes.last().unwrap().path_point(1);
            assert!((p.x - 100.).abs() < 0.001 && (p.y - 100.).abs() < 0.001);
        })
        .unwrap();
    click(&mut visual, "draw-pen");
    let from = screen(&mut visual, point(103., 103.));
    let to = screen(&mut visual, point(197., 197.));
    visual.simulate_mouse_down(from, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(to, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(to, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let path = this.shapes.last().unwrap();
            assert_eq!(path.path_point(0), point(103., 103.));
            assert_eq!(path.path_point(1), point(197., 197.));
        })
        .unwrap();
}
