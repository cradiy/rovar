use super::*;
use crate::{
    editor::tests::open,
    scene::presentation::property::{Boolean, Property},
};
use gpui::TestAppContext;

#[gpui::test]
fn revealed_containers_cover_sibling_hotspots_without_reordering_the_document(
    cx: &mut TestAppContext,
) {
    let handle = open(cx);
    handle
        .update(cx, |w, window, cx| {
            w.add_artboard(
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 240.,
                    height: 240.,
                },
                cx,
            );
            let frame = w.boards[0].id;
            let surface = w.next_id;
            let option = surface + 1;
            let trigger = surface + 2;
            w.next_id += 3;
            for (id, rect) in [
                (
                    surface,
                    Rect {
                        x: 20.,
                        y: 40.,
                        width: 140.,
                        height: 100.,
                    },
                ),
                (
                    option,
                    Rect {
                        x: 30.,
                        y: 70.,
                        width: 120.,
                        height: 30.,
                    },
                ),
            ] {
                w.shapes
                    .push(Shape::new(id, Some(frame), ShapeKind::Rectangle, rect));
            }
            w.set_click(option, Some(Action::Back), window, cx);
            w.set_selection(BTreeSet::from([surface, option]), cx);
            w.group_selection(cx);
            let popup = *w.selection_ids().first().unwrap();
            w.hierarchy.groups.get_mut(&popup).unwrap().layer.hidden = true;
            let mut hotspot = Shape::new(
                trigger,
                Some(frame),
                ShapeKind::Rectangle,
                Rect {
                    x: 20.,
                    y: 70.,
                    width: 140.,
                    height: 30.,
                },
            );
            hotspot.color = gpui::transparent_black().into();
            w.shapes.push(hotspot);
            let toggle = Action::SetProperty {
                target: popup,
                property: Property::Visible(Boolean::Toggle),
            };
            w.set_click(trigger, Some(toggle.clone()), window, cx);
            w.set_prototype_action(
                trigger,
                Trigger::Hover,
                Some(Action::SetProperty {
                    target: popup,
                    property: Property::Visible(Boolean::Set(true)),
                }),
                window,
                cx,
            );
            let authored = w.hierarchy.order.clone();
            w.start_presentation(window, cx);
            w.presentation.player.clone().unwrap().update(cx, |p, cx| {
                assert_eq!(
                    p.playback_hit(point(60., 85.), Trigger::Click),
                    Some((trigger, toggle.clone()))
                );
                p.playback_click(trigger, toggle.clone(), window, cx);
                assert_eq!(
                    p.playback_hit(point(60., 85.), Trigger::Click),
                    Some((option, Action::Back))
                );
                let order = p.canvas_layer_order();
                assert!(
                    order.iter().position(|id| *id == option)
                        > order.iter().position(|id| *id == trigger)
                );
                assert_eq!(p.hierarchy.order, authored);
                p.playback_click(trigger, toggle.clone(), window, cx);
                assert_eq!(
                    p.playback_hit(point(60., 85.), Trigger::Click),
                    Some((trigger, toggle.clone()))
                );
                p.bounds
                    .set(Bounds::new(point(px(0.), px(0.)), size(px(240.), px(240.))));
                p.view = Viewport {
                    zoom: 1.,
                    pan: point(0., 0.),
                };
                p.playback_pointer(point(px(60.), px(85.)), window, cx);
                assert_eq!(
                    p.playback_hit(point(60., 85.), Trigger::Click),
                    Some((option, Action::Back))
                );
                p.leave_playback_hover(window, cx);
                assert!(p.presentation.raised.is_empty());
                assert_eq!(
                    p.playback_hit(point(60., 85.), Trigger::Click),
                    Some((trigger, toggle))
                );
            });
            assert_eq!(w.hierarchy.order, authored);
            assert!(w.hierarchy.groups[&popup].layer.hidden);
        })
        .unwrap();
}
