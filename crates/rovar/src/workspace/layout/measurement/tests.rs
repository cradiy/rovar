use super::*;
use crate::workspace::tests::{draw, open};
use gpui::{Modifiers, TestAppContext, VisualTestContext};

#[test]
fn nested_bounds_measure_padding_and_diagonal_bounds_measure_gaps() {
    let outer = Rect {
        x: 100.,
        y: 200.,
        width: 300.,
        height: 200.,
    };
    let inner = Rect {
        x: 124.,
        y: 216.,
        width: 100.,
        height: 40.,
    };
    let values: Vec<_> = distances(inner, outer)
        .iter()
        .map(|d| (d.axis, d.end - d.start))
        .collect();
    assert_eq!(values, vec![(0, 24.), (0, 176.), (1, 16.), (1, 144.)]);
    let other = Rect {
        x: 250.,
        y: 290.,
        ..inner
    };
    let values: Vec<_> = distances(inner, other)
        .iter()
        .map(|d| (d.axis, d.end - d.start))
        .collect();
    assert_eq!(values, vec![(0, 26.), (1, 34.)]);
    assert!(distances(inner, inner).is_empty());
}

#[gpui::test]
fn alt_hover_measures_without_editing_and_overlay_does_not_block_selection(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 100.);
            this.view.zoom = 1.;
            for (id, x) in [(1, 0.), (2, 160.)] {
                this.shapes.push(Shape::new(
                    id,
                    None,
                    ShapeKind::Rectangle,
                    Rect {
                        x,
                        y: 100.,
                        width: 100.,
                        height: 100.,
                    },
                ));
            }
            this.next_id = 3;
            this.select_shape(1, cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let pointer = visual.debug_bounds("shape-2").unwrap().center();
    visual.simulate_event(gpui::MouseMoveEvent {
        position: pointer,
        ..Default::default()
    });
    draw(&mut visual);
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    visual.simulate_event(gpui::ModifiersChangedEvent {
        modifiers: alt,
        ..Default::default()
    });
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.measure_target, Some(2));
            assert_eq!(this.selection_ids(), BTreeSet::from([1]));
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
    visual.simulate_event(gpui::ModifiersChangedEvent::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.measure_target, None)
        })
        .unwrap();
    visual.simulate_event(gpui::ModifiersChangedEvent {
        modifiers: alt,
        ..Default::default()
    });
    draw(&mut visual);
    visual.simulate_mouse_down(pointer, MouseButton::Left, alt);
    visual.simulate_mouse_up(pointer, MouseButton::Left, alt);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids(), BTreeSet::from([2]));
            assert_eq!(this.measure_target, None);
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
}

#[gpui::test]
fn measurement_uses_world_coordinates_and_excludes_hidden_layers_and_panels(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(200., 50.);
            this.view.zoom = 2.;
            this.add_artboard(
                Rect {
                    x: 30.,
                    y: 40.,
                    width: 300.,
                    height: 250.,
                },
                cx,
            );
            let board = this.selected.unwrap();
            this.shapes.push(Shape::new(
                2,
                Some(board),
                ShapeKind::Rectangle,
                Rect {
                    x: 20.,
                    y: 30.,
                    width: 50.,
                    height: 60.,
                },
            ));
            for (id, hidden) in [(3, false), (4, true)] {
                let mut shape = Shape::new(
                    id,
                    Some(board),
                    ShapeKind::Rectangle,
                    Rect {
                        x: 140.,
                        y: 30.,
                        width: 50.,
                        height: 60.,
                    },
                );
                shape.layer.hidden = hidden;
                shape.layer.locked = true;
                this.shapes.push(shape);
            }
            this.next_id = 5;
            this.select_shape(2, cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    for (world, expected) in [(point(180., 80.), Some(3)), (point(100., 180.), Some(1))] {
        window
            .update(&mut visual.cx, |this, window, cx| {
                let pointer = this.bounds.get().origin + this.view.screen(world).map(px);
                this.update_measurement(pointer, true, window, cx);
                assert_eq!(this.measure_target, expected);
                let values = distances(
                    this.world_bounds(2).unwrap(),
                    this.world_bounds(expected.unwrap()).unwrap(),
                );
                if expected == Some(3) {
                    assert_eq!(values[0].end - values[0].start, 70.);
                }
            })
            .unwrap();
        draw(&mut visual);
    }
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.update_measurement(
                this.bounds.get().origin + point(px(10.), px(200.)),
                true,
                window,
                cx,
            );
            assert_eq!(this.measure_target, None);
        })
        .unwrap();
}
