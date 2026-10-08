use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext};

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
                decoded.hierarchy.interactions[&button].click.unwrap(),
                Action::Navigate { target: second }
            );
            w.set_selection(BTreeSet::from([second]), cx);
            w.delete_selected(cx);
            assert_eq!(w.hierarchy.interactions[&button].click, None);
            assert_eq!(w.hierarchy.interactions[&button].hover, Some(Action::Back));
            w.snapshot_page(cx).0.validate().unwrap();
            w.replay_history(false, window, cx);
            assert_eq!(
                w.hierarchy.interactions[&button].click.unwrap(),
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
                w.hierarchy.interactions[&copy].click.unwrap(),
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
    visual.simulate_keystrokes("delete ctrl-z");
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
            let action = w.hierarchy.interactions[&copied_button.id].click.unwrap();
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
                page.hierarchy.interactions[&button].click.unwrap(),
                Action::Navigate { target: second }
            );
            assert_eq!(page.shapes[0].color, rgb(0x00ff00));
        })
        .unwrap();
}
