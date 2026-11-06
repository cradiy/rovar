use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}
fn shape(w: &mut Workspace, board: usize, rect: Rect) -> usize {
    let id = w.next_id;
    w.next_id += 1;
    w.shapes
        .push(Shape::new(id, Some(board), ShapeKind::Rectangle, rect));
    id
}

#[gpui::test]
fn playback_input_blocks_siblings_bubbles_to_parents_and_cancels_drag_clicks(
    cx: &mut TestAppContext,
) {
    let handle = open(cx);
    handle
        .update(cx, |w, window, cx| {
            w.add_artboard(rect(0., 0., 200., 200.), cx);
            let root = w.boards[0].id;
            let button = shape(w, root, rect(20., 20., 80., 40.));
            let cover = shape(w, root, rect(20., 20., 80., 40.));
            let action = Action::SetProperty {
                target: button,
                property: crate::scene::presentation::property::Property::Visible(
                    crate::scene::presentation::property::Boolean::Set(false),
                ),
            };
            w.set_click(button, Some(action.clone()), window, cx);
            w.set_selection(BTreeSet::from([button, cover]), cx);
            w.group_selection(cx);
            let group = *w.selection_ids().first().unwrap();
            w.start_presentation(window, cx);
            let player = w.presentation.player.clone().unwrap();
            player.update(cx, |p, window_cx| {
                assert_eq!(p.playback_hit(point(30., 30.), Trigger::Click), None);
                p.hierarchy
                    .interactions
                    .entry(cover)
                    .or_default()
                    .pass_through = true;
                assert_eq!(
                    p.playback_hit(point(30., 30.), Trigger::Click),
                    Some((button, action.clone()))
                );
                p.hierarchy
                    .interactions
                    .get_mut(&cover)
                    .unwrap()
                    .pass_through = false;
                p.hierarchy.interactions.entry(group).or_default().click = Some(action.clone());
                assert_eq!(
                    p.playback_hit(point(30., 30.), Trigger::Click),
                    Some((group, action.clone()))
                );
                p.bounds.set(Bounds::new(
                    point(px(0.), px(0.)),
                    gpui::size(px(200.), px(200.)),
                ));
                p.view = Viewport {
                    zoom: 1.,
                    pan: point(0., 0.),
                };
                p.playback_press(point(px(30.), px(30.)), window_cx);
                p.playback_drag(point(px(50.), px(30.)), window, window_cx);
                p.playback_release(point(px(50.), px(30.)), window, window_cx);
                assert!(p.presentation.scroll.press.is_none());
                assert_eq!(p.presentation.playback.as_ref().unwrap().current, root);
                assert!(
                    !p.shapes
                        .iter()
                        .find(|s| s.id == button)
                        .unwrap()
                        .layer
                        .hidden
                );
                p.playback_press(point(px(30.), px(30.)), window_cx);
                p.playback_release(point(px(30.), px(30.)), window, window_cx);
                assert!(
                    p.shapes
                        .iter()
                        .find(|s| s.id == button)
                        .unwrap()
                        .layer
                        .hidden
                );
            });
        })
        .unwrap();
}

#[gpui::test]
fn nested_scroll_consumes_remainders_clips_hits_and_keeps_fixed_branches(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |w, window, cx| {
            w.add_artboard(rect(0., 0., 200., 200.), cx);
            let root = w.boards[0].id;
            let inner = w.next_id;
            w.next_id += 1;
            w.hierarchy.groups.insert(
                inner,
                crate::scene::layer::LayerGroup {
                    uid: uuid::Uuid::new_v4(),
                    name: "List".into(),
                    board: Some(root),
                    layer: Default::default(),
                    mask: None,
                    boolean: None,
                },
            );
            w.hierarchy.layouts.insert(
                inner,
                crate::scene::auto_layout::Container::new(rect(10., 80., 100., 60.)),
            );
            let item = shape(w, root, rect(20., 300., 20., 20.));
            let pinned = shape(w, root, rect(80., 90., 20., 15.));
            for id in [item, pinned] {
                w.hierarchy.parents.insert(id, inner);
                w.hierarchy.sizing.insert(
                    id,
                    crate::scene::auto_layout::Sizing {
                        absolute: true,
                        ..Default::default()
                    },
                );
            }
            let fixed = shape(w, root, rect(140., 5., 30., 20.));
            shape(w, root, rect(140., 500., 20., 20.));
            w.set_prototype_behavior(root, ScrollAxis::Vertical, false, false, window, cx);
            w.set_prototype_behavior(inner, ScrollAxis::Vertical, false, false, window, cx);
            for id in [pinned, fixed] {
                w.set_prototype_behavior(id, ScrollAxis::None, true, false, window, cx);
            }
            w.set_click(item, Some(Action::Back), window, cx);
            w.start_presentation(window, cx);
            w.presentation.player.clone().unwrap().update(cx, |p, cx| {
                assert!(p.scroll_by(&[inner, root], point(0., 260.)));
                assert_eq!(p.presentation.scroll.offsets[&inner], point(0., 180.));
                assert_eq!(p.presentation.scroll.offsets[&root], point(0., 80.));
                assert_eq!(p.playback_scroll_offset(item), point(0., 260.));
                assert_eq!(p.playback_scroll_offset(pinned), point(0., 80.));
                assert_eq!(p.playback_scroll_offset(fixed), point(0., 0.));
                assert_eq!(
                    p.playback_hit(point(25., 45.), Trigger::Click),
                    Some((item, Action::Back))
                );
                assert_eq!(p.playback_hit(point(25., 100.), Trigger::Click), None);
                assert!(!p.scroll_by(&[inner, root], point(100., 0.)));
                p.playback_scroll_to(item, window, cx);
                assert_eq!(p.presentation.scroll.offsets[&root].y, 120.);
                let before = p.presentation.scroll.offsets.clone();
                p.playback_scroll_to(fixed, window, cx);
                assert_eq!(p.presentation.scroll.offsets, before);
                p.hierarchy.interactions.get_mut(&inner).unwrap().fixed = true;
                p.refresh_playback_scroll();
                p.playback_scroll_to(item, window, cx);
                assert_eq!(p.presentation.scroll.offsets[&root].y, 120.);
            });
            assert_eq!(w.shapes.iter().find(|s| s.id == item).unwrap().rect.y, 300.);
        })
        .unwrap();
}

#[gpui::test]
fn wheel_and_scrollbar_drag_move_rendered_content_without_changing_the_document(
    cx: &mut TestAppContext,
) {
    let handle = open(cx);
    let (root, item, saved) = handle
        .update(cx, |w, window, cx| {
            w.add_artboard(rect(0., 0., 200., 200.), cx);
            let root = w.boards[0].id;
            let item = shape(w, root, rect(20., 100., 30., 30.));
            shape(w, root, rect(400., 500., 30., 30.));
            w.set_prototype_behavior(root, ScrollAxis::Both, false, false, window, cx);
            let saved = w.snapshot_page(cx).0;
            w.start_presentation(window, cx);
            (root, item, saved)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    let (before, zoom, origin) = handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            (
                p.presentation.scroll.nodes[&item].displayed(),
                p.view.zoom,
                p.bounds.get().origin,
            )
        })
        .unwrap();
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: origin + point(px(50. * zoom), px(50. * zoom)),
        delta: gpui::ScrollDelta::Pixels(point(px(-20. * zoom), px(-40. * zoom))),
        ..Default::default()
    });
    draw(&mut visual);
    let rendered = visual.debug_bounds(format!("shape-{item}").leak()).unwrap();
    assert!((f32::from(rendered.origin.y - origin.y) / zoom - (before.origin.y - 40.)).abs() < 1.);
    let bar_start = visual
        .debug_bounds(format!("playback-scrollbar-{root}-vertical").leak())
        .unwrap()
        .center();
    let bar = handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            assert_eq!(p.presentation.scroll.offsets[&root].y, 40.);
            assert_eq!(p.presentation.scroll.offsets[&root].x, 20.);
            p.playback_scrollbar(p.playback_world(bar_start)).unwrap()
        })
        .unwrap();
    visual.simulate_mouse_down(bar_start, gpui::MouseButton::Left, Default::default());
    let end = bar_start + point(px(0.), px(20.));
    visual.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            assert!(
                (p.presentation.scroll.offsets[&root].y - (40. + 20. / zoom * bar.scale)).abs()
                    < 0.01
            );
            assert!(w.snapshot_page(cx).0 == saved);
        })
        .unwrap();
    click(&mut visual, "playback-restart");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, cx| {
            assert_eq!(
                w.presentation
                    .player
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .presentation
                    .scroll
                    .offsets[&root],
                point(0., 0.)
            );
            assert!(w.snapshot_page(cx).0 == saved);
        })
        .unwrap();
}
