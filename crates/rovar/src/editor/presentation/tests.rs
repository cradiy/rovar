use super::*;
use crate::editor::tests::{click, draw, open};
use crate::scene::presentation::{OverlayPosition, OverlaySettings};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn property_paint_reuses_inspector_and_preserves_canvas_color(cx: &mut TestAppContext) {
    use crate::scene::presentation::property::Property;
    let handle = open(cx);
    let (button, original, undo) = handle
        .update(cx, |w, window, cx| {
            let (_, _, button) = setup(w, window, cx);
            w.set_click(
                button,
                Some(Action::SetProperty {
                    target: button,
                    property: Property::Fill("#FF000080".into()),
                }),
                window,
                cx,
            );
            w.set_selection(BTreeSet::from([button]), cx);
            w.set_presentation_tab(true, window, cx);
            (
                button,
                w.shapes.iter().find(|s| s.id == button).unwrap().color,
                w.history.borrow().undo_len(),
            )
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    click(&mut visual, "property-drag-5");
    draw(&mut visual);
    assert!(visual.debug_bounds("color-panel").is_some());
    handle
        .update(&mut visual.cx, |w, _, cx| {
            assert!((w.inspector.picker.read(cx).value().a - 128. / 255.).abs() < 0.001);
            for event in [
                ColorPickerEvent::Preview(gpui::rgba(0x00ff0080)),
                ColorPickerEvent::Preview(gpui::rgba(0x0000ff40)),
                ColorPickerEvent::Commit(gpui::rgba(0x0000ff40)),
            ] {
                w.inspector.picker.update(cx, |_, cx| cx.emit(event));
            }
        })
        .unwrap();
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.hierarchy.interactions[&button].get(Trigger::Click),
                Some(Action::SetProperty {
                    target: button,
                    property: Property::Fill("#0000FF80".into()),
                })
            );
            assert_eq!(
                w.shapes.iter().find(|s| s.id == button).unwrap().color,
                original
            );
            assert_eq!(w.history.borrow().undo_len(), undo + 1);
        })
        .unwrap();
    visual.simulate_keystrokes("escape secondary-z");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.hierarchy.interactions[&button].get(Trigger::Click),
                Some(Action::SetProperty {
                    target: button,
                    property: Property::Fill("#FF000080".into()),
                })
            );
        })
        .unwrap();
    click(&mut visual, "property-drag-5");
    click(&mut visual, "fill-linear");
    click(&mut visual, "gradient-add");
    click(&mut visual, "gradient-kind");
    click(&mut visual, "gradient-angular");
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let paint = w.action_paint().unwrap();
            assert_eq!(paint.mode, FillMode::Linear);
            assert_eq!(paint.gradient.kind, gpui::GradientKind::Angular);
            assert_eq!(paint.gradient.stops().len(), 3);
            let property = w.inspector_action().unwrap().2.clone();
            let mut playback = w.snapshot_page(cx).0;
            property.apply(&mut playback, button).unwrap();
            assert_eq!(
                playback
                    .shapes
                    .iter()
                    .find(|s| s.id == button)
                    .unwrap()
                    .fill_mode,
                FillMode::Linear
            );
            assert_eq!(
                w.shapes.iter().find(|s| s.id == button).unwrap().color,
                original
            );
            assert_eq!(
                w.shapes.iter().find(|s| s.id == button).unwrap().fill_mode,
                FillMode::Solid
            );
        })
        .unwrap();
    click(&mut visual, "fill-points");
    click(&mut visual, "property-19");
    visual.simulate_keystrokes("secondary-a 4 2 enter");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, _| {
            let FillMode::Points(g) = w.action_paint().unwrap().mode else {
                panic!("Expected point paint")
            };
            assert_eq!(g.points[w.inspector.active_stop].position.x, 0.42);
        })
        .unwrap();
}

#[gpui::test]
fn action_image_crop_and_archive_keep_assets_without_editing_the_target(cx: &mut TestAppContext) {
    use crate::scene::presentation::{paint::Paint, property::Property};
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("paint.png");
    image::RgbaImage::from_pixel(20, 10, image::Rgba([42, 70, 90, 255]))
        .save(&source)
        .unwrap();
    let asset = crate::asset::FileAsset::load(source.clone()).unwrap();
    std::fs::remove_file(&source).unwrap();
    let handle = open(cx);
    handle
        .update(cx, |w, window, cx| {
            let (_, target, button) = setup(w, window, cx);
            w.set_click(
                button,
                Some(Action::SetProperty {
                    target,
                    property: Property::Fill(Paint::new(
                        rgb(0xffffff),
                        FillMode::Image,
                        Default::default(),
                        crate::scene::image_fill::ImageFill {
                            asset: Some(asset.clone()),
                            ..Default::default()
                        },
                    )),
                }),
                window,
                cx,
            );
            w.set_selection(BTreeSet::from([button]), cx);
            w.set_presentation_tab(true, window, cx);
            let before = w.boards.iter().find(|b| b.id == target).unwrap().clone();
            let undo = w.history.borrow().undo_len();
            assert!(w.start_image_crop(target, window, cx));
            w.zoom_image_crop(2., None, cx);
            w.finish_image_crop(false, window, cx);
            assert_eq!(w.action_paint().unwrap().image.placement.zoom, 1.);
            assert_eq!(w.history.borrow().undo_len(), undo);
            assert!(w.start_image_crop(target, window, cx));
            w.zoom_image_crop(2., None, cx);
            w.finish_image_crop(true, window, cx);
            assert_eq!(w.action_paint().unwrap().image.placement.zoom, 2.);
            assert_eq!(w.history.borrow().undo_len(), undo + 1);
            assert_eq!(w.boards.iter().find(|b| b.id == target).unwrap(), &before);
            let (json, sources) = w
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            assert_eq!(sources.len(), 1);
            let path = directory.path().join("action.rovar");
            crate::document::save_as(&path, &json, &sources, cx.text_system()).unwrap();
            let decoded = crate::document::load(&path)
                .unwrap()
                .into_document()
                .unwrap();
            let mut page = decoded.pages[0].clone();
            let Action::SetProperty { property, .. } =
                page.hierarchy.interactions[&button].click.clone().unwrap()
            else {
                panic!("Missing action")
            };
            assert_eq!(
                property.paint().unwrap().image.asset.as_ref().unwrap().hash,
                asset.hash
            );
            property.apply(&mut page, target).unwrap();
            let board = page.boards.iter().find(|b| b.id == target).unwrap();
            assert_eq!(board.image_fill.placement.zoom, 2.);
            assert_eq!(board.image_fill.asset.as_ref().unwrap().hash, asset.hash);
        })
        .unwrap();
}

#[gpui::test]
fn property_action_drag_previews_commits_once_and_cancels_without_losing_redo(
    cx: &mut TestAppContext,
) {
    use crate::scene::presentation::property::Property;
    let handle = open(cx);
    let (button, undo) = handle
        .update(cx, |w, window, cx| {
            let (_, _, button) = setup(w, window, cx);
            w.set_click(
                button,
                Some(Action::SetProperty {
                    target: button,
                    property: Property::Opacity(0.5),
                }),
                window,
                cx,
            );
            w.set_selection(BTreeSet::from([button]), cx);
            w.set_presentation_tab(true, window, cx);
            (button, w.history.borrow().undo_len())
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    let point = visual.debug_bounds("property-drag-18").unwrap().center();
    let drag = |visual: &mut VisualTestContext, delta: f32, shift: bool| {
        visual.simulate_mouse_move(
            point + gpui::point(px(delta), px(0.)),
            MouseButton::Left,
            gpui::Modifiers {
                shift,
                ..Default::default()
            },
        );
        draw(visual);
    };
    visual.simulate_mouse_down(point, MouseButton::Left, Default::default());
    drag(&mut visual, 10., false);
    drag(&mut visual, 25., false);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.prototype_property_number(button, Trigger::Click),
                Some(75.)
            );
            assert_eq!(w.history.borrow().undo_len(), undo);
            assert_eq!(
                w.shapes
                    .iter()
                    .find(|n| n.id == button)
                    .unwrap()
                    .layer
                    .opacity,
                1.
            );
        })
        .unwrap();
    visual.simulate_mouse_up(
        point + gpui::point(px(25.), px(0.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(w.history.borrow().undo_len(), undo + 1)
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    visual.simulate_mouse_down(point, MouseButton::Left, Default::default());
    drag(&mut visual, 10., true);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.prototype_property_number(button, Trigger::Click),
                Some(100.)
            )
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(point, MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.prototype_property_number(button, Trigger::Click),
                Some(50.)
            )
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-shift-z");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.prototype_property_number(button, Trigger::Click),
                Some(75.)
            )
        })
        .unwrap();
    visual.simulate_mouse_down(point, MouseButton::Left, Default::default());
    drag(&mut visual, -10., false);
    drag(&mut visual, 0., false);
    visual.simulate_mouse_up(point, MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(w.history.borrow().undo_len(), undo + 1)
        })
        .unwrap();
    click(&mut visual, "property-18");
    visual.simulate_keystrokes("secondary-a 4 2 enter");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.prototype_property_number(button, Trigger::Click),
                Some(42.)
            )
        })
        .unwrap();
}

#[gpui::test]
fn property_actions_author_hidden_targets_and_remap_on_duplicate(cx: &mut TestAppContext) {
    use crate::scene::presentation::property::{Boolean, Property};
    let handle = open(cx);
    let (frame, button, target) = handle
        .update(cx, |w, window, cx| {
            let (frame, _, button) = setup(w, window, cx);
            let target = w.next_id;
            w.next_id += 1;
            let mut shape = Shape::new(
                target,
                Some(frame),
                ShapeKind::Rectangle,
                Rect {
                    x: 20.,
                    y: 100.,
                    width: 80.,
                    height: 40.,
                },
            );
            shape.layer.hidden = true;
            w.shapes.push(shape);
            w.set_selection(BTreeSet::from([button]), cx);
            w.set_presentation_tab(true, window, cx);
            (frame, button, target)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "prototype-click-menu");
    click(&mut visual, "prototype-option-1-7");
    assert!(visual.debug_bounds("prototype-property-menu-0").is_some());
    let target_index = handle
        .update(&mut visual.cx, |w, _, cx| {
            w.prototype_property_targets(cx)
                .iter()
                .position(|(id, _)| *id == target)
                .unwrap()
        })
        .unwrap();
    click(&mut visual, "prototype-target-menu");
    let target_selector: &'static str = format!("prototype-option-4-{target_index}").leak();
    click(&mut visual, target_selector);
    click(&mut visual, "prototype-property-menu-1");
    click(&mut visual, "prototype-property-option-1-2");
    handle
        .update(&mut visual.cx, |w, window, cx| {
            assert_eq!(
                w.hierarchy.interactions[&button].click,
                Some(Action::SetProperty {
                    target,
                    property: Property::Visible(Boolean::Toggle)
                })
            );
            assert!(
                w.shapes
                    .iter()
                    .find(|n| n.id == target)
                    .unwrap()
                    .layer
                    .hidden,
                "Authoring must not execute actions"
            );
            let page = w.snapshot_page(cx).0;
            let decoded =
                crate::document::Page::decode(&serde_json::to_vec(&page).unwrap()).unwrap();
            assert_eq!(
                decoded.hierarchy.interactions[&button],
                w.hierarchy.interactions[&button]
            );
            w.set_selection(BTreeSet::from([frame]), cx);
            w.duplicate_selection(window, cx);
            let copy = *w.selection_ids().first().unwrap();
            let copy_button = w
                .shapes
                .iter()
                .find(|n| n.board == Some(copy) && !n.layer.hidden)
                .unwrap()
                .id;
            let copy_target = w
                .shapes
                .iter()
                .find(|n| n.board == Some(copy) && n.layer.hidden)
                .unwrap()
                .id;
            assert_eq!(
                w.hierarchy.interactions[&copy_button].click,
                Some(Action::SetProperty {
                    target: copy_target,
                    property: Property::Visible(Boolean::Toggle)
                })
            );
            w.shapes
                .iter_mut()
                .find(|n| n.id == target)
                .unwrap()
                .layer
                .hidden = false;
            w.set_selection(BTreeSet::from([target]), cx);
            w.delete_selected(cx);
            assert!(!w.hierarchy.interactions.contains_key(&button));
            w.replay_history(false, window, cx);
            assert_eq!(
                w.hierarchy.interactions[&button].click,
                Some(Action::SetProperty {
                    target,
                    property: Property::Visible(Boolean::Toggle)
                })
            );
        })
        .unwrap();
}

#[gpui::test]
fn property_playback_restores_self_hiding_hover_and_resets_text_and_layout(
    cx: &mut TestAppContext,
) {
    use crate::scene::{
        auto_layout,
        presentation::property::{Boolean, Property},
    };
    let handle = open(cx);
    let (frame, button, text, saved, undo) = handle
        .update(cx, |w, window, cx| {
            let (frame, _, button) = setup(w, window, cx);
            let text = w.next_id;
            w.next_id += 1;
            let item = w.make_text(
                text,
                Some(frame),
                Rect {
                    x: 20.,
                    y: 100.,
                    width: 200.,
                    height: 40.,
                },
                window,
                cx,
            );
            w.texts.push(item);
            let mut layout = auto_layout::Container::new(w.boards[0].rect);
            layout.gap = 12.;
            w.hierarchy.layouts.insert(frame, layout);
            w.reflow_layout(cx);
            w.set_prototype_action(
                button,
                Trigger::Hover,
                Some(Action::SetProperty {
                    target: button,
                    property: Property::Visible(Boolean::Toggle),
                }),
                window,
                cx,
            );
            w.set_click(
                button,
                Some(Action::SetProperty {
                    target: text,
                    property: Property::Content("Playing now".into()),
                }),
                window,
                cx,
            );
            w.set_selection(BTreeSet::from([frame]), cx);
            w.create_component(window, cx);
            let saved = serde_json::to_value(w.snapshot_page(cx).0).unwrap();
            let undo = w.history.borrow().undo_len();
            w.start_presentation(window, cx);
            (frame, button, text, saved, undo)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    draw(&mut visual);
    let (hotspot, outside) = handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            let rect = p.world_rect(button).unwrap();
            let (view, bounds) = p.playback_coordinates();
            (
                bounds.origin
                    + view
                        .screen(point(rect.x + rect.width / 2., rect.y + rect.height / 2.))
                        .map(px),
                bounds.origin - point(px(5.), px(5.)),
            )
        })
        .unwrap();
    visual.simulate_mouse_move(hotspot, None, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            assert!(
                p.shapes
                    .iter()
                    .find(|n| n.id == button)
                    .unwrap()
                    .layer
                    .hidden
            );
            assert!(p.presentation.playback.as_ref().unwrap().hover.is_some());
        })
        .unwrap();
    visual.simulate_mouse_move(hotspot + point(px(1.), px(1.)), None, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, cx| {
            assert!(
                w.presentation
                    .player
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .shapes
                    .iter()
                    .find(|n| n.id == button)
                    .unwrap()
                    .layer
                    .hidden
            )
        })
        .unwrap();
    visual.simulate_mouse_move(outside, None, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, window, cx| {
            let player = w.presentation.player.as_ref().unwrap().clone();
            player.update(cx, |p, cx| {
                assert!(
                    !p.shapes
                        .iter()
                        .find(|n| n.id == button)
                        .unwrap()
                        .layer
                        .hidden
                );
                let action = p.hierarchy.interactions[&button]
                    .get(Trigger::Click)
                    .unwrap();
                p.playback_click(button, action, window, cx);
                p.activate_prototype(
                    button,
                    Action::SetProperty {
                        target: frame,
                        property: Property::Gap(40.),
                    },
                    window,
                    cx,
                );
                p.activate_prototype(
                    button,
                    Action::SetProperty {
                        target: text,
                        property: Property::Width(250.),
                    },
                    window,
                    cx,
                );
                p.activate_prototype(
                    button,
                    Action::SetProperty {
                        target: text,
                        property: Property::FontSize(32.),
                    },
                    window,
                    cx,
                );
                let page = p.snapshot_page(cx).0;
                page.validate().unwrap();
                let node = page.texts.iter().find(|n| n.id == text).unwrap();
                assert_eq!(node.content, "Playing now");
                assert_eq!(node.styles.default.size, 32.);
                assert_eq!(page.hierarchy.layouts[&frame].gap, 40.);
                assert_eq!(page.hierarchy.sizing[&text].width, auto_layout::Mode::Fixed);
                p.activate_prototype(
                    button,
                    Action::SetProperty {
                        target: button,
                        property: Property::Rotation(405.),
                    },
                    window,
                    cx,
                );
                assert_eq!(p.object_rotation(button), 45.);
                for (value, expected) in [
                    (Boolean::Set(false), false),
                    (Boolean::Toggle, true),
                    (Boolean::Toggle, false),
                    (Boolean::Set(true), true),
                ] {
                    for (target, property) in [
                        (text, Property::Visible(value)),
                        (button, Property::FillEnabled(value)),
                        (button, Property::StrokeEnabled(value)),
                        (frame, Property::LayoutWrap(value)),
                    ] {
                        let action = Action::SetProperty { target, property };
                        let action =
                            serde_json::from_value(serde_json::to_value(action).unwrap()).unwrap();
                        p.activate_prototype(button, action, window, cx);
                    }
                    let page = p.snapshot_page(cx).0;
                    let shape = page.shapes.iter().find(|n| n.id == button).unwrap();
                    let text = page.texts.iter().find(|n| n.id == text).unwrap();
                    assert_eq!(!text.layer.hidden, expected);
                    assert_eq!(shape.fill_enabled, expected);
                    assert_eq!(shape.stroke.enabled, expected);
                    assert_eq!(page.hierarchy.layouts[&frame].wrap, expected);
                }
            });
            assert_eq!(serde_json::to_value(w.snapshot_page(cx).0).unwrap(), saved);
            assert_eq!(w.history.borrow().undo_len(), undo);
        })
        .unwrap();
    click(&mut visual, "playback-restart");
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            let page = p.snapshot_page(cx).0;
            assert_eq!(page.hierarchy.layouts[&frame].gap, 12.);
            assert!(
                page.texts
                    .iter()
                    .find(|n| n.id == text)
                    .unwrap()
                    .content
                    .is_empty()
            );
            assert_eq!(serde_json::to_value(w.snapshot_page(cx).0).unwrap(), saved);
        })
        .unwrap();
}

#[gpui::test]
fn video_actions_author_roundtrip_and_remap_with_their_targets(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (frame, button, video) = handle
        .update(cx, |w, window, cx| {
            let (frame, _, button) = setup(w, window, cx);
            let video = w.next_id;
            w.next_id += 1;
            let mut shape = Shape::new(
                video,
                Some(frame),
                ShapeKind::Video,
                Rect {
                    x: 20.,
                    y: 100.,
                    width: 160.,
                    height: 90.,
                },
            );
            shape.asset = Some(std::sync::Arc::new(crate::asset::FileAsset {
                path: "video.mp4".into(),
                source: crate::asset::Source::file(
                    tempfile::NamedTempFile::new().unwrap().into_temp_path(),
                ),
                hash: "0".repeat(64),
                width: 160,
                height: 90,
                content: std::sync::OnceLock::from(Err("No decoder in authoring test".into())),
                video: true,
            }));
            w.shapes.push(shape);
            w.set_selection(BTreeSet::from([button]), cx);
            w.set_presentation_tab(true, window, cx);
            (frame, button, video)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "prototype-click-menu");
    click(&mut visual, "prototype-option-1-6");
    assert!(visual.debug_bounds("prototype-target-menu").is_some());
    assert!(visual.debug_bounds("prototype-instant").is_none());
    handle
        .update(&mut visual.cx, |w, window, cx| {
            assert_eq!(
                w.hierarchy.interactions[&button].click,
                Some(Action::Media {
                    target: video,
                    command: MediaCommand::Play
                })
            );
            w.set_prototype_action(
                button,
                Trigger::Click,
                Some(Action::Media {
                    target: frame,
                    command: MediaCommand::Pause,
                }),
                window,
                cx,
            );
            assert_eq!(
                w.hierarchy.interactions[&button].click,
                Some(Action::Media {
                    target: video,
                    command: MediaCommand::Play
                })
            );
            let page = w.snapshot_page(cx).0;
            page.validate().unwrap();
            let document = crate::document::Document::single(page);
            let decoded =
                crate::document::Document::decode(&serde_json::to_vec(&document).unwrap()).unwrap();
            assert_eq!(
                decoded.pages[0].hierarchy.interactions[&button].click,
                w.hierarchy.interactions[&button].click
            );
            w.set_selection(BTreeSet::from([frame]), cx);
            w.duplicate_selection(window, cx);
            let copied_video = w
                .shapes
                .iter()
                .find(|s| s.id != video && s.kind == ShapeKind::Video)
                .unwrap()
                .id;
            let copied_button = w
                .shapes
                .iter()
                .find(|s| s.id != button && s.kind == ShapeKind::Rectangle)
                .unwrap()
                .id;
            assert_eq!(
                w.hierarchy.interactions[&copied_button].click,
                Some(Action::Media {
                    target: copied_video,
                    command: MediaCommand::Play
                })
            );
            let mut page = w.snapshot_page(cx).0;
            page.validate().unwrap();
            page.shapes.retain(|shape| shape.id != copied_video);
            let ids = crate::scene::components::ids(&page);
            page.hierarchy.retain_interactions(&ids);
            assert!(!page.hierarchy.interactions.contains_key(&copied_button));
            assert!(page.hierarchy.interactions.contains_key(&button));
        })
        .unwrap();
}

#[gpui::test]
fn overlay_authoring_roundtrips_and_duplicate_targets_remain_internal(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (first, second, button) = handle
        .update(cx, |w, window, cx| {
            let ids = setup(w, window, cx);
            w.set_selection(BTreeSet::from([ids.2]), cx);
            w.set_presentation_tab(true, window, cx);
            ids
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "prototype-click-menu");
    click(&mut visual, "prototype-option-1-3");
    click(&mut visual, "prototype-overlay-bottom");
    click(&mut visual, "prototype-overlay-backdrop");
    click(&mut visual, "prototype-target-menu");
    assert!(visual.debug_bounds("prototype-option-4-0").is_none());
    click(&mut visual, "prototype-option-4-2");
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.hierarchy.interactions[&button].click,
                Some(Action::OpenOverlay {
                    target: first,
                    settings: OverlaySettings {
                        position: OverlayPosition::Bottom,
                        backdrop: false,
                        dismiss_outside: true
                    },
                })
            );
        })
        .unwrap();
    click(&mut visual, "prototype-target-menu");
    click(&mut visual, "prototype-option-4-3");
    click(&mut visual, "prototype-overlay-outside");
    handle
        .update(&mut visual.cx, |w, window, cx| {
            let settings = OverlaySettings {
                position: OverlayPosition::Bottom,
                backdrop: false,
                dismiss_outside: false,
            };
            assert_eq!(
                w.hierarchy.interactions[&button].click,
                Some(Action::OpenOverlay {
                    target: second,
                    settings
                })
            );
            w.undo_redo(false, window, cx);
            assert!(matches!(
                w.hierarchy.interactions[&button].click,
                Some(Action::OpenOverlay {
                    settings: OverlaySettings {
                        dismiss_outside: true,
                        ..
                    },
                    ..
                })
            ));
            w.undo_redo(true, window, cx);
            let page = w.snapshot_page(cx).0;
            let decoded =
                crate::document::Page::decode(&serde_json::to_vec(&page).unwrap()).unwrap();
            assert_eq!(decoded.hierarchy.interactions, page.hierarchy.interactions);
            w.set_selection(BTreeSet::from([first, second]), cx);
            w.duplicate_selection(window, cx);
            let copied_button = w.shapes.iter().find(|s| s.id != button).unwrap();
            let Some(Action::OpenOverlay {
                target,
                settings: copied,
            }) = w.hierarchy.interactions[&copied_button.id].click
            else {
                panic!("Expected overlay")
            };
            assert_ne!(target, second);
            assert!(w.selection_ids().contains(&target));
            assert_eq!(copied, settings);
            w.snapshot_page(cx).0.validate().unwrap();
        })
        .unwrap();
}

#[gpui::test]
fn overlay_blocks_background_and_maps_positioned_hits_without_navigation(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (first, second, button) = handle
        .update(cx, |w, window, cx| {
            let ids = setup(w, window, cx);
            w.boards[1].rect.width = 100.;
            w.boards[1].rect.height = 80.;
            w.set_click(ids.1, Some(Action::CloseOverlay), window, cx);
            w.start_presentation(window, cx);
            ids
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    draw(&mut visual);
    let surface = visual.debug_bounds("playback-surface").unwrap();
    let scale = f32::from(surface.size.width) / 320.;
    let background_button = surface.origin + point(px(50. * scale), px(40. * scale));
    for position in [
        OverlayPosition::Center,
        OverlayPosition::Bottom,
        OverlayPosition::Right,
    ] {
        handle
            .update(&mut visual.cx, |w, _, cx| {
                w.presentation.player.as_ref().unwrap().update(cx, |p, cx| {
                    p.playback_action(
                        Action::OpenOverlay {
                            target: second,
                            settings: OverlaySettings {
                                position,
                                backdrop: false,
                                dismiss_outside: false,
                            },
                        },
                        cx,
                    );
                });
            })
            .unwrap();
        draw(&mut visual);
        let bounds = visual.debug_bounds("playback-overlay").unwrap();
        assert!((f32::from(bounds.size.width) - 100. * scale).abs() < 1.);
        match position {
            OverlayPosition::Center => {
                assert!((bounds.center() - surface.center()).magnitude() < 1.)
            }
            OverlayPosition::Bottom => {
                assert!((f32::from(bounds.bottom() - surface.bottom())).abs() < 1.)
            }
            OverlayPosition::Right => {
                assert!((f32::from(bounds.right() - surface.right())).abs() < 1.)
            }
        }
        visual.simulate_click(background_button, Default::default());
        draw(&mut visual);
        handle
            .update(&mut visual.cx, |w, _, cx| {
                let p = w.presentation.player.as_ref().unwrap().read(cx);
                let playback = p.presentation.playback.as_ref().unwrap();
                assert_eq!(playback.current, first);
                assert!(playback.history.is_empty());
                assert!(playback.overlay.is_some());
                let world = p.playback_world(bounds.center());
                assert_eq!(
                    p.playback_hit(world, Trigger::Click),
                    Some((second, Action::CloseOverlay))
                );
                assert_eq!(
                    p.hierarchy.interactions[&button].click,
                    Some(Action::Navigate { target: second })
                );
            })
            .unwrap();
        visual.simulate_click(bounds.center(), Default::default());
        draw(&mut visual);
        assert!(visual.debug_bounds("playback-overlay").is_none());
    }
    // Both the underlying page and the surrounding stage dismiss without click-through.
    for outside in [background_button, surface.origin - point(px(8.), px(8.))] {
        handle
            .update(&mut visual.cx, |w, _, cx| {
                w.presentation.player.as_ref().unwrap().update(cx, |p, cx| {
                    p.playback_action(
                        Action::OpenOverlay {
                            target: second,
                            settings: Default::default(),
                        },
                        cx,
                    );
                });
            })
            .unwrap();
        draw(&mut visual);
        visual.simulate_click(outside, Default::default());
        draw(&mut visual);
        handle
            .update(&mut visual.cx, |w, _, cx| {
                let p = w.presentation.player.as_ref().unwrap().read(cx);
                assert_eq!(p.presentation.playback.as_ref().unwrap().current, first);
                assert!(p.presentation.playback.as_ref().unwrap().overlay.is_none());
            })
            .unwrap();
    }
}

#[gpui::test]
fn overlay_hover_restore_and_escape_preserve_the_base_page(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (first, second, button) = handle
        .update(cx, |w, window, cx| {
            let ids = setup(w, window, cx);
            w.boards[1].rect.width = 100.;
            w.boards[1].rect.height = 80.;
            w.set_prototype_action(
                ids.2,
                Trigger::Hover,
                Some(Action::OpenOverlay {
                    target: ids.1,
                    settings: Default::default(),
                }),
                window,
                cx,
            );
            w.set_prototype_transition(
                ids.2,
                Trigger::Hover,
                Transition::Dissolve { duration_ms: 200 },
                window,
                cx,
            );
            w.set_prototype_action(
                ids.1,
                Trigger::Hover,
                Some(Action::CloseOverlay),
                window,
                cx,
            );
            w.start_presentation(window, cx);
            ids
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    draw(&mut visual);
    let surface = visual.debug_bounds("playback-surface").unwrap();
    let scale = f32::from(surface.size.width) / 320.;
    visual.simulate_mouse_move(
        surface.origin + point(px(50. * scale), px(40. * scale)),
        None,
        Default::default(),
    );
    draw(&mut visual);
    visual
        .cx
        .executor()
        .advance_clock(std::time::Duration::from_millis(100));
    draw(&mut visual);
    assert!(visual.debug_bounds("playback-overlay").is_some());
    visual.simulate_mouse_move(
        surface.origin + point(px(250. * scale), px(200. * scale)),
        None,
        Default::default(),
    );
    draw(&mut visual);
    visual
        .cx
        .executor()
        .advance_clock(std::time::Duration::from_millis(300));
    draw(&mut visual);
    assert!(visual.debug_bounds("playback-overlay").is_none());
    handle
        .update(&mut visual.cx, |w, _, cx| {
            w.presentation.player.as_ref().unwrap().update(cx, |p, cx| {
                assert_eq!(p.presentation.playback.as_ref().unwrap().current, first);
                assert!(p.presentation.playback.as_ref().unwrap().history.is_empty());
                assert!(p.hierarchy.interactions[&button].hover.is_some());
                p.playback_action(
                    Action::OpenOverlay {
                        target: second,
                        settings: Default::default(),
                    },
                    cx,
                );
            });
        })
        .unwrap();
    draw(&mut visual);
    // Hover-close also respects the configured restore behavior on pointer exit.
    visual.simulate_mouse_move(surface.center(), None, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            assert!(p.presentation.playback.as_ref().unwrap().overlay.is_none());
        })
        .unwrap();
    visual.simulate_mouse_move(
        surface.origin + point(px(250. * scale), px(200. * scale)),
        None,
        Default::default(),
    );
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            assert!(p.presentation.playback.as_ref().unwrap().overlay.is_some());
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            assert!(p.presentation.playback.as_ref().unwrap().overlay.is_none());
            assert_eq!(p.presentation.playback.as_ref().unwrap().current, first);
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("playback").is_none());
}

#[gpui::test]
fn transition_controls_persist_per_trigger_with_undo_and_validation(cx: &mut TestAppContext) {
    let handle = open(cx);
    let button = handle
        .update(cx, |w, window, cx| {
            let (_, second, button) = setup(w, window, cx);
            w.set_prototype_action(
                button,
                Trigger::Hover,
                Some(Action::Navigate { target: second }),
                window,
                cx,
            );
            w.set_selection(BTreeSet::from([button]), cx);
            w.set_presentation_tab(true, window, cx);
            button
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    click(&mut visual, "prototype-dissolve");
    click(&mut visual, "prototype-duration");
    click(&mut visual, "prototype-duration-500");
    click(&mut visual, "prototype-hover");
    assert!(visual.debug_bounds("prototype-duration").is_none());
    click(&mut visual, "prototype-dissolve");
    handle
        .update(&mut visual.cx, |w, window, cx| {
            let settings = w.hierarchy.interactions[&button].clone();
            assert_eq!(
                settings.click_transition,
                Transition::Dissolve { duration_ms: 500 }
            );
            assert_eq!(
                settings.hover_transition,
                Transition::Dissolve { duration_ms: 200 }
            );
            w.undo_redo(false, window, cx);
            assert_eq!(
                w.hierarchy.interactions[&button].hover_transition,
                Transition::Instant
            );
            assert_eq!(
                w.hierarchy.interactions[&button].click_transition,
                settings.click_transition
            );
            w.undo_redo(true, window, cx);
            let page = w.snapshot_page(cx).0;
            let mut decoded =
                crate::document::Page::decode(&serde_json::to_vec(&page).unwrap()).unwrap();
            assert_eq!(decoded.hierarchy.interactions[&button], settings);
            decoded
                .hierarchy
                .interactions
                .get_mut(&button)
                .unwrap()
                .click_transition = Transition::Dissolve { duration_ms: 0 };
            assert!(decoded.validate().is_err());
            w.set_prototype_action(button, Trigger::Click, None, window, cx);
            assert_eq!(
                w.hierarchy.interactions[&button].click_transition,
                Transition::Instant
            );
            assert_eq!(
                w.hierarchy.interactions[&button].hover_transition,
                settings.hover_transition
            );
        })
        .unwrap();
}

#[gpui::test]
fn dissolve_reverses_hover_and_restart_clears_outgoing_frames(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (first, second, saved) = handle
        .update(cx, |w, window, cx| {
            let (first, second, button) = setup(w, window, cx);
            w.boards[1].rect.width = 160.;
            w.boards[1].rect.height = 100.;
            w.set_prototype_action(
                button,
                Trigger::Hover,
                Some(Action::Navigate { target: second }),
                window,
                cx,
            );
            w.set_prototype_transition(
                button,
                Trigger::Hover,
                Transition::Dissolve { duration_ms: 500 },
                window,
                cx,
            );
            let saved = serde_json::to_value(w.snapshot_page(cx).0).unwrap();
            w.start_presentation(window, cx);
            (first, second, saved)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    draw(&mut visual);
    let surface = visual.debug_bounds("playback-surface").unwrap();
    let scale = f32::from(surface.size.width) / 320.;
    let hotspot = surface.origin + point(px(50. * scale), px(40. * scale));
    let outside = surface.origin + point(px(250. * scale), px(200. * scale));
    let from_selector: &'static str = format!("playback-frame-{first}").leak();
    let to_selector: &'static str = format!("playback-frame-{second}").leak();
    visual.simulate_mouse_move(hotspot, None, Default::default());
    draw(&mut visual);
    visual
        .cx
        .executor()
        .advance_clock(std::time::Duration::from_millis(200));
    draw(&mut visual);
    let from = visual.debug_bounds(from_selector).unwrap();
    let to = visual.debug_bounds(to_selector).unwrap();
    assert!((from.size.width / from.size.height - 320. / 480.).abs() < 0.001);
    assert!((to.size.width / to.size.height - 160. / 100.).abs() < 0.001);
    visual.simulate_mouse_move(outside, None, Default::default());
    draw(&mut visual);
    visual
        .cx
        .executor()
        .advance_clock(std::time::Duration::from_millis(600));
    draw(&mut visual);
    assert!(visual.debug_bounds(from_selector).is_some());
    assert!(visual.debug_bounds(to_selector).is_none());
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            let playback = p.presentation.playback.as_ref().unwrap();
            assert_eq!(playback.current, first);
            assert!(playback.history.is_empty());
            assert!(playback.hover.is_none());
            assert_eq!(serde_json::to_value(w.snapshot_page(cx).0).unwrap(), saved);
        })
        .unwrap();
    visual.simulate_mouse_move(hotspot, None, Default::default());
    draw(&mut visual);
    visual
        .cx
        .executor()
        .advance_clock(std::time::Duration::from_millis(100));
    draw(&mut visual);
    click(&mut visual, "playback-restart");
    assert!(visual.debug_bounds(to_selector).is_none());
    assert!(visual.debug_bounds(from_selector).is_some());
}

#[gpui::test]
fn rapid_dissolve_clicks_finish_at_latest_target_without_changing_history(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (first, second, third) = handle
        .update(cx, |w, window, cx| {
            let (first, second, button) = setup(w, window, cx);
            w.add_artboard(
                Rect {
                    x: 1000.,
                    y: 0.,
                    width: 240.,
                    height: 360.,
                },
                cx,
            );
            let third = w.boards.last().unwrap().id;
            w.set_click(second, Some(Action::Navigate { target: third }), window, cx);
            for source in [button, second] {
                w.set_prototype_transition(
                    source,
                    Trigger::Click,
                    Transition::Dissolve { duration_ms: 500 },
                    window,
                    cx,
                );
            }
            w.start_presentation(window, cx);
            (first, second, third)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    draw(&mut visual);
    let surface = visual.debug_bounds("playback-surface").unwrap();
    let scale = f32::from(surface.size.width) / 320.;
    visual.simulate_click(
        surface.origin + point(px(50. * scale), px(40. * scale)),
        Default::default(),
    );
    draw(&mut visual);
    visual
        .cx
        .executor()
        .advance_clock(std::time::Duration::from_millis(100));
    draw(&mut visual);
    let destination = visual.debug_bounds("playback-surface").unwrap().center();
    visual.simulate_click(destination, Default::default());
    draw(&mut visual);
    for _ in 0..2 {
        visual
            .cx
            .executor()
            .advance_clock(std::time::Duration::from_millis(600));
        draw(&mut visual);
    }
    for (id, visible) in [(first, false), (second, false), (third, true)] {
        let selector: &'static str = format!("playback-frame-{id}").leak();
        assert_eq!(visual.debug_bounds(selector).is_some(), visible);
    }
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            let playback = p.presentation.playback.as_ref().unwrap();
            assert_eq!(playback.current, third);
            assert_eq!(playback.history, vec![first, second]);
        })
        .unwrap();
}

fn setup(
    w: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> (usize, usize, usize) {
    w.add_artboard(
        Rect {
            x: 0.,
            y: 0.,
            width: 320.,
            height: 480.,
        },
        cx,
    );
    let first = w.boards.last().unwrap().id;
    w.add_artboard(
        Rect {
            x: 500.,
            y: 0.,
            width: 320.,
            height: 480.,
        },
        cx,
    );
    let second = w.boards.last().unwrap().id;
    let button = w.next_id;
    w.next_id += 1;
    w.shapes.push(Shape::new(
        button,
        Some(first),
        ShapeKind::Rectangle,
        Rect {
            x: 20.,
            y: 20.,
            width: 100.,
            height: 40.,
        },
    ));
    w.set_click(
        button,
        Some(Action::Navigate { target: second }),
        window,
        cx,
    );
    w.set_start(Some(first), window, cx);
    (first, second, button)
}

#[gpui::test]
fn playback_toolbar_fits_long_names_and_close_returns_to_editor(cx: &mut TestAppContext) {
    let handle = open(cx);
    let saved = handle
        .update(cx, |w, window, cx| {
            let (_, _, button) = setup(w, window, cx);
            w.boards[0].name = "A long frame title ".repeat(30);
            w.set_selection(BTreeSet::from([button]), cx);
            let saved = (w.view, w.selection_ids(), w.document_revision());
            w.start_presentation(window, cx);
            saved
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    for (width, height) in [(1280., 800.), (320., 400.), (520., 380.)] {
        visual.simulate_resize(size(px(width), px(height)));
        draw(&mut visual);
        draw(&mut visual);
        let bar = visual.debug_bounds("playback-toolbar").unwrap();
        let back = visual.debug_bounds("playback-back").unwrap();
        let restart = visual.debug_bounds("playback-restart").unwrap();
        let name = visual.debug_bounds("playback-frame-name").unwrap();
        let close = visual.debug_bounds("presentation-close").unwrap();
        let surface = visual.debug_bounds("playback-surface").unwrap();
        let expected_zoom = ((width - 32.) / 320_f32).min((height - 52. - 32.) / 480_f32);
        assert!((f32::from(surface.size.height) - 480. * expected_zoom).abs() < 0.1);
        assert!(
            (f32::from(surface.center().x) - width / 2.).abs() <= 0.5,
            "frame: {surface:?}, window width: {width}"
        );
        assert!(bar.size.height <= px(52.));
        assert!(back.left() >= bar.left() && back.right() <= restart.left());
        assert!(restart.right() <= name.left() && name.right() <= close.left());
        assert!(close.right() <= bar.right());
        assert_eq!(back.top(), close.top());
        assert!(surface.top() >= bar.bottom() && surface.bottom() <= px(height));
    }
    click(&mut visual, "playback-back");
    handle
        .update(&mut visual.cx, |w, _, cx| {
            assert!(
                w.presentation
                    .player
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .presentation
                    .playback
                    .as_ref()
                    .unwrap()
                    .history
                    .is_empty()
            );
        })
        .unwrap();
    click(&mut visual, "presentation-close");
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert!(w.presentation.player.is_none());
            assert_eq!((w.view, w.selection_ids(), w.document_revision()), saved);
        })
        .unwrap();
}

#[gpui::test]
fn navigation_persists_and_delete_undo_restores_references(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |w, window, cx| {
            let (first, second, button) = setup(w, window, cx);
            w.set_prototype_action(button, Trigger::Hover, Some(Action::Back), window, cx);
            let page = w.snapshot_page(cx).0;
            let decoded =
                crate::document::Page::decode(&serde_json::to_vec(&page).unwrap()).unwrap();
            assert_eq!(decoded.hierarchy.start, Some(first));
            assert_eq!(
                decoded.hierarchy.interactions[&button]
                    .click
                    .clone()
                    .unwrap(),
                Action::Navigate { target: second }
            );
            w.set_selection(BTreeSet::from([second]), cx);
            w.delete_selected(cx);
            assert_eq!(w.hierarchy.interactions[&button].click, None);
            assert_eq!(w.hierarchy.interactions[&button].hover, Some(Action::Back));
            w.snapshot_page(cx).0.validate().unwrap();
            w.replay_history(false, window, cx);
            assert_eq!(
                w.hierarchy.interactions[&button].click.clone().unwrap(),
                Action::Navigate { target: second }
            );
            w.set_selection(BTreeSet::from([first]), cx);
            w.delete_selected(cx);
            assert_eq!(w.hierarchy.start, None);
            w.replay_history(false, window, cx);
            assert_eq!(w.hierarchy.start, Some(first));
            w.set_selection(BTreeSet::from([button]), cx);
            w.duplicate_selection(window, cx);
            let copy = *w.selection_ids().first().unwrap();
            assert_eq!(
                w.hierarchy.interactions[&copy].click.clone().unwrap(),
                Action::Navigate { target: second }
            );
            w.snapshot_page(cx).0.validate().unwrap();
        })
        .unwrap();
}

#[gpui::test]
fn prototype_wire_drag_commit_cancel_and_undo(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (second, button) = handle
        .update(cx, |w, window, cx| {
            let (_, second, button) = setup(w, window, cx);
            w.set_click(button, None, window, cx);
            w.set_selection(BTreeSet::from([button]), cx);
            w.set_presentation_tab(true, window, cx);
            w.view = Viewport {
                pan: point(350., 100.),
                zoom: 0.7,
            };
            (second, button)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1000.)));
    draw(&mut visual);
    assert!(visual.debug_bounds("prototype-disconnect").is_none());
    let target = handle
        .update(&mut visual.cx, |w, _, _| {
            w.bounds.get().origin + w.view.screen(point(550., 100.)).map(px)
        })
        .unwrap();
    for trigger in [Trigger::Click, Trigger::Hover] {
        if trigger == Trigger::Hover {
            click(&mut visual, "prototype-hover");
        }
        let start = visual.debug_bounds("prototype-port").unwrap().center();
        visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
        visual.simulate_mouse_move(target, Some(MouseButton::Left), Default::default());
        draw(&mut visual);
        assert!(visual.debug_bounds("prototype-target").is_some());
        visual.simulate_mouse_up(target, MouseButton::Left, Default::default());
        draw(&mut visual);
        handle
            .update(&mut visual.cx, |w, _, _| {
                assert_eq!(
                    w.hierarchy.interactions[&button].get(trigger),
                    Some(Action::Navigate { target: second })
                );
            })
            .unwrap();
    }
    let click_label: &'static str = format!("prototype-wire-{button}-prototype-click").leak();
    let hover_label: &'static str = format!("prototype-wire-{button}-prototype-hover").leak();
    assert!(visual.debug_bounds(click_label).is_none());
    assert!(visual.debug_bounds(hover_label).is_none());
    let disconnect = visual.debug_bounds("prototype-disconnect").unwrap();
    let menu = visual.debug_bounds("prototype-click-menu").unwrap();
    let panel = visual.debug_bounds("properties-panel").unwrap();
    assert!(menu.right() <= disconnect.left());
    assert!(disconnect.right() <= panel.right());
    click(&mut visual, "prototype-disconnect");
    handle
        .update(&mut visual.cx, |w, window, cx| {
            assert_eq!(w.hierarchy.interactions[&button].hover, None);
            assert_eq!(
                w.hierarchy.interactions[&button].click,
                Some(Action::Navigate { target: second })
            );
            w.undo_redo(false, window, cx);
            assert_eq!(
                w.hierarchy.interactions[&button].hover,
                Some(Action::Navigate { target: second })
            );
        })
        .unwrap();
    draw(&mut visual);
    let depth = handle
        .update(&mut visual.cx, |w, _, _| w.history.borrow().undo_len())
        .unwrap();
    for cancel in [false, true] {
        let start = visual.debug_bounds("prototype-port").unwrap().center();
        visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
        let end = if cancel {
            target
        } else {
            point(px(800.), px(600.))
        };
        visual.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
        if cancel {
            visual.simulate_keystrokes("escape");
        }
        visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        handle
            .update(&mut visual.cx, |w, _, _| {
                assert!(w.presentation.connection.is_none());
                assert!(w.gesture.is_none());
                assert_eq!(w.history.borrow().undo_len(), depth);
            })
            .unwrap();
    }
    handle
        .update(&mut visual.cx, |w, window, cx| {
            w.undo_redo(false, window, cx);
            assert_eq!(w.hierarchy.interactions[&button].hover, None);
            assert_eq!(
                w.hierarchy.interactions[&button].click,
                Some(Action::Navigate { target: second })
            );
            w.undo_redo(true, window, cx);
            assert_eq!(
                w.hierarchy.interactions[&button].hover,
                Some(Action::Navigate { target: second })
            );
            w.set_presentation_tab(false, window, cx);
        })
        .unwrap();
    draw(&mut visual);
    assert!(visual.debug_bounds("prototype-port").is_none());
}

#[gpui::test]
fn hover_navigation_and_unavailable_variant_targets(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (first, second, button) = handle
        .update(cx, |w, window, cx| {
            let (first, second, button) = setup(w, window, cx);
            let target = w.boards.iter_mut().find(|b| b.id == second).unwrap();
            target.rect.width = 160.;
            target.rect.height = 100.;
            let revision = w.document_revision();
            for trigger in [Trigger::Click, Trigger::Hover] {
                w.set_prototype_action(
                    button,
                    trigger,
                    Some(Action::ChangeVariant {
                        target: uuid::Uuid::new_v4(),
                    }),
                    window,
                    cx,
                );
            }
            assert_eq!(w.document_revision(), revision);
            w.set_prototype_action(
                button,
                Trigger::Hover,
                Some(Action::Navigate { target: second }),
                window,
                cx,
            );
            w.set_selection(BTreeSet::from([button]), cx);
            w.set_presentation_tab(true, window, cx);
            (first, second, button)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    click(&mut visual, "prototype-hover");
    click(&mut visual, "prototype-hover-exit-menu");
    click(&mut visual, "prototype-exit-option-1");
    handle
        .update(&mut visual.cx, |w, window, cx| {
            assert_eq!(
                w.hierarchy.interactions[&button].hover_exit,
                HoverExit::Keep
            );
            w.undo_redo(false, window, cx);
            assert_eq!(
                w.hierarchy.interactions[&button].hover_exit,
                HoverExit::Restore
            );
        })
        .unwrap();
    click(&mut visual, "present");
    draw(&mut visual);
    draw(&mut visual);
    let surface = visual.debug_bounds("playback-surface").unwrap();
    let scale = f32::from(surface.size.width) / 320.;
    let hotspot = surface.origin + point(px(50. * scale), px(40. * scale));
    let outside = surface.origin + point(px(250. * scale), px(200. * scale));
    let assert_frame = |visual: &mut VisualTestContext, current, history: Vec<usize>| {
        handle
            .update(&mut visual.cx, |w, _, cx| {
                let p = w.presentation.player.as_ref().unwrap().read(cx);
                assert_eq!(p.presentation.playback.as_ref().unwrap().current, current);
                assert_eq!(p.presentation.playback.as_ref().unwrap().history, history);
            })
            .unwrap();
    };
    for _ in 0..2 {
        visual.simulate_mouse_move(hotspot, None, Default::default());
        draw(&mut visual);
        assert_frame(&mut visual, second, vec![first]);
        // Different frame positions and sizes must not move the original hover region.
        visual.simulate_mouse_move(hotspot + point(px(1.), px(1.)), None, Default::default());
        draw(&mut visual);
        assert_frame(&mut visual, second, vec![first]);
        visual.simulate_mouse_move(outside, None, Default::default());
        draw(&mut visual);
        assert_frame(&mut visual, first, vec![]);
    }
    handle
        .update(&mut visual.cx, |w, _, cx| {
            w.presentation.player.as_ref().unwrap().update(cx, |p, _| {
                p.hierarchy
                    .interactions
                    .get_mut(&button)
                    .unwrap()
                    .hover_exit = HoverExit::Keep;
            });
        })
        .unwrap();
    visual.simulate_mouse_move(hotspot, None, Default::default());
    draw(&mut visual);
    visual.simulate_mouse_move(outside, None, Default::default());
    draw(&mut visual);
    assert_frame(&mut visual, second, vec![first]);
    click(&mut visual, "playback-back");
    assert_frame(&mut visual, first, vec![]);
    // Resizing ends the old hover region using the configured leave behavior.
    for exit in [HoverExit::Restore, HoverExit::Keep] {
        handle
            .update(&mut visual.cx, |w, _, cx| {
                w.presentation.player.as_ref().unwrap().update(cx, |p, _| {
                    p.hierarchy
                        .interactions
                        .get_mut(&button)
                        .unwrap()
                        .hover_exit = exit;
                });
            })
            .unwrap();
        let surface = visual.debug_bounds("playback-surface").unwrap();
        let scale = f32::from(surface.size.width) / 320.;
        let hotspot = surface.origin + point(px(50. * scale), px(40. * scale));
        visual.simulate_mouse_move(hotspot, None, Default::default());
        draw(&mut visual);
        assert_frame(&mut visual, second, vec![first]);
        let width = if exit == HoverExit::Restore {
            520.
        } else {
            720.
        };
        visual.simulate_resize(size(px(width), px(380.)));
        draw(&mut visual);
        draw(&mut visual);
        if exit == HoverExit::Restore {
            assert_frame(&mut visual, first, vec![]);
        } else {
            assert_frame(&mut visual, second, vec![first]);
        }
        handle
            .update(&mut visual.cx, |w, _, cx| {
                let p = w.presentation.player.as_ref().unwrap().read(cx);
                assert!(p.presentation.playback.as_ref().unwrap().hover.is_none());
            })
            .unwrap();
    }
}

#[gpui::test]
fn presentation_click_back_restart_and_exit_preserve_editor(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (first, second, button, saved_view, saved_selection, revision, saved_page) = handle
        .update(cx, |w, window, cx| {
            let (first, second, button) = setup(w, window, cx);
            w.shapes[0].layer.locked = true;
            let label = w.make_text(
                w.next_id,
                Some(first),
                Rect {
                    x: 20.,
                    y: 20.,
                    width: 100.,
                    height: 40.,
                },
                window,
                cx,
            );
            w.next_id += 1;
            w.texts.push(label);
            w.set_selection(BTreeSet::from([second]), cx);
            w.view = Viewport {
                pan: point(50., 80.),
                zoom: 2.,
            };
            let values = (
                first,
                second,
                button,
                w.view,
                w.selection_ids(),
                w.document_revision(),
                serde_json::to_value(w.snapshot_page(cx).0).unwrap(),
            );
            w.start_presentation(window, cx);
            values
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    draw(&mut visual);
    assert!(visual.debug_bounds("properties-panel").is_none());
    visual.simulate_resize(size(px(520.), px(380.)));
    draw(&mut visual);
    draw(&mut visual);
    let surface = visual.debug_bounds("playback-surface").unwrap();
    assert!(surface.size.height <= px(380.));
    assert!((surface.size.width / surface.size.height - 320. / 480.).abs() < 0.001);
    let scale = f32::from(surface.size.width) / 320.;
    let hotspot = surface.origin + point(px(50. * scale), px(40. * scale));
    let blank = surface.origin + point(px(200. * scale), px(200. * scale));
    // Real pointer motion precedes clicks and must never use paint-only APIs.
    for position in [
        hotspot,
        blank,
        hotspot,
        surface.origin - point(px(5.), px(5.)),
        hotspot,
    ] {
        visual.simulate_mouse_move(position, None, Default::default());
        draw(&mut visual);
    }
    visual.simulate_click(hotspot, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let player = w.presentation.player.as_ref().unwrap();
            assert_eq!(
                player
                    .read(cx)
                    .presentation
                    .playback
                    .as_ref()
                    .unwrap()
                    .current,
                second
            );
            assert_eq!(w.document_revision(), revision);
            assert_eq!(w.view, saved_view);
            assert_eq!(w.selection_ids(), saved_selection);
            assert_eq!(
                serde_json::to_value(w.snapshot_page(cx).0).unwrap(),
                saved_page
            );
        })
        .unwrap();
    click(&mut visual, "playback-back");
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let player = w.presentation.player.clone().unwrap();
            player.update(cx, |p, cx| {
                assert_eq!(p.presentation.playback.as_ref().unwrap().current, first);
                assert_eq!(
                    p.playback_hit(point(50., 40.), Trigger::Click)
                        .map(|(_, action)| action),
                    Some(Action::Navigate { target: second })
                );
                p.shapes
                    .iter_mut()
                    .find(|s| s.id == button)
                    .unwrap()
                    .layer
                    .hidden = true;
                assert_eq!(p.playback_hit(point(50., 40.), Trigger::Click), None);
                p.playback_action(Action::Navigate { target: second }, cx);
            });
        })
        .unwrap();
    click(&mut visual, "playback-restart");
    handle
        .update(&mut visual.cx, |w, _, cx| {
            let p = w.presentation.player.as_ref().unwrap().read(cx);
            assert_eq!(p.presentation.playback.as_ref().unwrap().current, first);
            assert!(p.presentation.playback.as_ref().unwrap().history.is_empty());
        })
        .unwrap();
    visual.simulate_keystrokes("delete secondary-z");
    click(&mut visual, "presentation-close");
    handle
        .update(&mut visual.cx, |w, _, cx| {
            assert!(w.presentation.player.is_none());
            assert_eq!(w.view, saved_view);
            assert_eq!(w.selection_ids(), saved_selection);
            assert_eq!(w.document_revision(), revision);
            assert_eq!(
                serde_json::to_value(w.snapshot_page(cx).0).unwrap(),
                saved_page
            );
        })
        .unwrap();
}

#[gpui::test]
fn prototype_controls_configure_and_remove_clicks(cx: &mut TestAppContext) {
    let handle = open(cx);
    let button = handle
        .update(cx, |w, window, cx| {
            let (_, _, button) = setup(w, window, cx);
            w.set_selection(BTreeSet::from([button]), cx);
            w.panels.set(panels::Side::Right, 256.);
            button
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "inspector-prototype-tab");
    let panel = visual.debug_bounds("properties-panel").unwrap();
    let tab = visual.debug_bounds("inspector-prototype-tab").unwrap();
    let play = visual.debug_bounds("present").unwrap();
    let zoom = visual.debug_bounds("inspector-zoom").unwrap();
    assert!(
        tab.right() <= play.left(),
        "tab={tab:?}, play={play:?}, zoom={zoom:?}, panel={panel:?}"
    );
    assert!(play.bottom() <= zoom.top());
    assert!(zoom.right() < panel.right());
    click(&mut visual, "prototype-click-menu");
    click(&mut visual, "prototype-option-1-1");
    assert!(visual.debug_bounds("prototype-target-menu").is_none());
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(w.hierarchy.interactions[&button].click, Some(Action::Back))
        })
        .unwrap();
    click(&mut visual, "prototype-click-menu");
    click(&mut visual, "prototype-option-1-0");
    handle
        .update(&mut visual.cx, |w, window, cx| {
            assert!(!w.hierarchy.interactions.contains_key(&button));
            w.replay_history(false, window, cx);
            assert_eq!(w.hierarchy.interactions[&button].click, Some(Action::Back));
        })
        .unwrap();
    click(&mut visual, "present");
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("prototype-controls").is_some());
    assert!(visual.debug_bounds("playback").is_none());
}

#[gpui::test]
fn copied_flows_remap_internal_targets_and_drop_cross_page_links(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |w, window, cx| {
            let (first, second, button) = setup(w, window, cx);
            w.set_selection(BTreeSet::from([first, second]), cx);
            w.duplicate_selection(window, cx);
            let copied_button = w.shapes.iter().find(|s| s.id != button).unwrap();
            let action = w.hierarchy.interactions[&copied_button.id]
                .click
                .clone()
                .unwrap();
            let Action::Navigate { target } = action else {
                panic!("Expected navigation")
            };
            assert_ne!(target, second);
            assert!(w.selection_ids().contains(&target));
            assert_eq!(w.hierarchy.start, Some(first));
            w.set_selection(BTreeSet::from([button]), cx);
            w.copy_selection(cx);
            w.add_page(None, window, cx);
            w.paste_in_place(window, cx);
            assert!(w.hierarchy.interactions.is_empty());
            w.snapshot_page(cx).0.validate().unwrap();
        })
        .unwrap();
}

#[gpui::test]
fn navigation_merge_aligns_handles_before_merging(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |w, window, cx| {
            let (first, second, button) = setup(w, window, cx);
            let mut base = crate::document::Document::single(w.snapshot_page(cx).0);
            base.pages[0].hierarchy.start = None;
            base.pages[0].hierarchy.interactions.clear();
            let mut local = base.clone();
            local.pages[0].shapes[0].color = rgb(0x00ff00);
            let mut remote = base.clone();
            let page = &mut remote.pages[0];
            for board in &mut page.boards {
                board.id += 100;
            }
            for shape in &mut page.shapes {
                shape.id += 100;
                shape.board = shape.board.map(|id| id + 100);
            }
            for id in &mut page.hierarchy.order {
                *id += 100;
            }
            page.next_id += 100;
            page.hierarchy.start = Some(first + 100);
            page.hierarchy.interactions.insert(
                button + 100,
                crate::scene::presentation::Interaction {
                    click: Some(Action::Navigate {
                        target: second + 100,
                    }),
                    ..Default::default()
                },
            );
            let merged = crate::document::merge::merge(
                &serde_json::to_vec(&base).unwrap(),
                &serde_json::to_vec(&local).unwrap(),
                &serde_json::to_vec(&remote).unwrap(),
            )
            .unwrap();
            let document = crate::document::Document::decode(&merged).unwrap();
            let page = &document.pages[0];
            assert_eq!(page.hierarchy.start, Some(first));
            assert_eq!(
                page.hierarchy.interactions[&button].click.clone().unwrap(),
                Action::Navigate { target: second }
            );
            assert_eq!(page.shapes[0].color, rgb(0x00ff00));
        })
        .unwrap();
}
