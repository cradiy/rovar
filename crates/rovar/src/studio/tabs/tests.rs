use super::*;
use gpui::{MouseButton, TestAppContext, VisualTestContext, point, size};

#[gpui::test]
fn hovering_tabs_shows_document_thumbnail_after_delay_and_dismisses_on_leave_or_drag(
    cx: &mut TestAppContext,
) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| {
            window.activate_window();
            studio.new_document(window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    handle
        .update(cx, |studio, window, cx| {
            let tab = &studio.tabs[0];
            let mut document = crate::document::Document::decode(&tab.last_saved).unwrap();
            document.pages[0].shapes.push(crate::shape::Shape::new(
                1,
                None,
                crate::shape::ShapeKind::Rectangle,
                crate::artboard::Rect {
                    x: 0.,
                    y: 0.,
                    width: 100.,
                    height: 80.,
                },
            ));
            document.pages[0].next_id = 2;
            tab.editor.as_ref().unwrap().update(cx, |editor, cx| {
                editor
                    .load_document(
                        crate::document::Loaded {
                            needs_upgrade: false,
                            json: serde_json::to_vec(&document).unwrap(),
                            assets: Default::default(),
                        },
                        window,
                        cx,
                    )
                    .unwrap();
            });
            studio.save_tab(1, window, cx);
            studio.new_document(window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    handle
        .update(cx, |studio, _, _| {
            let preview = studio.tabs[0].file.preview.as_ref().unwrap();
            assert!(directory.path().join("previews").join(preview).exists());
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear());
    let first = visual.debug_bounds("document-tab-1").unwrap().center();
    visual.simulate_mouse_move(first, None, Default::default());
    visual.cx.run_until_parked();
    visual
        .cx
        .executor()
        .advance_clock(Duration::from_millis(419));
    visual.cx.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("tab-preview").is_none());
    visual.cx.executor().advance_clock(Duration::from_millis(1));
    visual.cx.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    let card = visual.debug_bounds("tab-preview").unwrap();
    assert!(card.top() > px(BAR_HEIGHT));
    assert!(visual.debug_bounds("tab-preview-image").is_some());
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.active, Some(2));
            assert_eq!(studio.strip.hover_card, Some(1));
        })
        .unwrap();

    let outside = point(px(700.), px(350.));
    visual.simulate_mouse_move(outside, None, Default::default());
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("tab-preview").is_none());
    visual.simulate_mouse_move(first, None, Default::default());
    visual.cx.run_until_parked();
    visual.simulate_mouse_move(outside, None, Default::default());
    visual
        .cx
        .executor()
        .advance_clock(Duration::from_millis(500));
    visual.cx.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("tab-preview").is_none());

    visual.simulate_mouse_move(first, None, Default::default());
    visual.cx.run_until_parked();
    visual
        .cx
        .executor()
        .advance_clock(Duration::from_millis(420));
    visual.cx.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("tab-preview").is_some());
    visual.simulate_mouse_down(first, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        first + point(px(12.), px(0.)),
        Some(MouseButton::Left),
        Default::default(),
    );
    visual.cx.run_until_parked();
    visual
        .cx
        .executor()
        .advance_clock(Duration::from_millis(500));
    visual.cx.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("tab-preview").is_none());
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert!(studio.strip.drag.is_some());
            assert!(studio.strip.hovered.is_none());
        })
        .unwrap();
    visual.update(|window, cx| {
        cx.stop_active_drag(window);
    });
}

#[gpui::test]
fn dragging_reorders_before_release_locks_to_rail_and_cancel_restores_order(
    cx: &mut TestAppContext,
) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| {
            for _ in 0..3 {
                studio.new_document(window, cx);
            }
        })
        .unwrap();
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear());
    let first = visual.debug_bounds("document-tab-1").unwrap().center();
    visual.simulate_mouse_down(first, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        first + point(px(10.), px(0.)),
        Some(MouseButton::Left),
        Default::default(),
    );
    visual.simulate_mouse_move(
        point(px(670.), px(38.)),
        Some(MouseButton::Left),
        Default::default(),
    );
    let payload = handle
        .update(&mut visual.cx, |studio, _, cx| {
            assert_eq!(
                studio.tabs.iter().map(|tab| tab.token).collect::<Vec<_>>(),
                [2, 3, 1]
            );
            let drag = studio.strip.drag.clone().unwrap();
            let preview = drag.preview.borrow().clone().unwrap();
            let preview = preview.read(cx);
            assert!(!preview.detached);
            assert!((38. - preview.cursor_offset_y + preview.y_offset - TAB_TOP).abs() < 0.01);
            let slot = studio.strip.slots.get(&2).unwrap();
            assert_eq!(slot.target, 0.);
            assert!(slot.current > slot.target);
            drag
        })
        .unwrap();
    visual.update(|window, cx| {
        assert!(cx.stop_active_drag(window));
    });
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(
                studio.tabs.iter().map(|tab| tab.token).collect::<Vec<_>>(),
                [1, 2, 3]
            );
            assert_eq!(studio.active, Some(3));
            assert!(studio.strip.drag.is_none());
            assert_eq!(
                payload.transaction.borrow().owner,
                Some((studio.window_id, 1))
            );
        })
        .unwrap();
}

#[gpui::test]
fn releasing_reordered_tab_in_titlebar_keeps_the_existing_window(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| {
            for _ in 0..3 {
                studio.new_document(window, cx);
            }
        })
        .unwrap();
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    for target in [point(px(670.), px(30.)), point(px(1245.), px(30.))] {
        visual.update(|window, cx| window.draw(cx).clear());
        let first = visual.debug_bounds("document-tab-1").unwrap().center();
        visual.simulate_mouse_down(first, MouseButton::Left, Default::default());
        visual.simulate_mouse_move(
            first + point(px(10.), px(0.)),
            Some(MouseButton::Left),
            Default::default(),
        );
        visual.simulate_mouse_move(target, Some(MouseButton::Left), Default::default());
        visual.simulate_mouse_up(target, MouseButton::Left, Default::default());
        visual.cx.run_until_parked();
        handle
            .update(&mut visual.cx, |studio, _, cx| {
                assert_eq!(cx.windows().len(), 1, "Reordering must not create a window");
                assert_eq!(
                    studio.tabs.iter().map(|tab| tab.token).collect::<Vec<_>>(),
                    [2, 3, 1]
                );
                assert_eq!(studio.active, Some(1));
                assert!(studio.strip.drag.is_none());
            })
            .unwrap();
    }
}

#[gpui::test]
fn native_hover_reserves_slot_until_drop_and_uses_snap_hysteresis(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let source = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    source
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    cx.run_until_parked();
    let (drag, session, path) = source
        .update(cx, |studio, window, cx| {
            let drag = studio.drag_payload(1, window, cx);
            *drag.preview.borrow_mut() = Some(cx.new(|_| DragPreview {
                title: drag.title.clone(),
                thumbnail: None,
                width: drag.width,
                detached: true,
                hidden: false,
                cursor_offset_y: 12.,
                y_offset: 0.,
            }));
            studio.begin_tab_drag(drag.clone(), window, cx);
            drag.transaction.borrow_mut().native = true;
            let path = studio.tabs[0].file.path.clone();
            assert!(studio.detach_owned_tab(&drag, window, cx));
            studio.persist_session();
            assert!(studio.session.borrow().open.contains(&path));
            (drag, studio.session.clone(), path)
        })
        .unwrap();
    let target = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        let mut studio = Studio::new(directory.path().into(), window, cx);
        studio.tabs.clear();
        studio.session = session;
        studio.new_document(window, cx);
        studio
    });
    cx.run_until_parked();
    target
        .update(cx, |studio, window, cx| {
            studio.move_tab_drag(drag.clone(), point(px(130.), px(44.)), window, cx);
            assert!(studio.strip.snap_index.is_none());
            studio.move_tab_drag(drag.clone(), point(px(130.), px(42.)), window, cx);
            assert!(studio.strip.snap_index.is_some());
            assert_eq!(studio.tabs.len(), 1);
            assert!(drag.transaction.borrow().owner.is_none());
            assert!(drag.preview.borrow().as_ref().unwrap().read(cx).hidden);
            studio.move_tab_drag(drag.clone(), point(px(130.), px(48.)), window, cx);
            assert!(studio.strip.snap_index.is_some());
            studio.move_tab_drag(drag.clone(), point(px(130.), px(50.)), window, cx);
            assert!(studio.strip.snap_index.is_none());
            assert!(!drag.preview.borrow().as_ref().unwrap().read(cx).hidden);
            studio.move_tab_drag(drag.clone(), point(px(130.), px(20.)), window, cx);
            let index = studio.strip.snap_index.unwrap();
            studio.dropped_on_strip(&drag, window, cx);
            assert_eq!(studio.tabs.len(), 2);
            assert_eq!(studio.tabs[index].file.path, path);
            assert_eq!(studio.active, Some(studio.tabs[index].token));
            assert!(drag.transaction.borrow().detached.is_none());
        })
        .unwrap();
    cx.update(|cx| {
        Studio::finish_tab_drag(
            drag,
            gpui::DragEnd::Dropped {
                target_window: target.window_id(),
                position: point(px(130.), px(20.)),
            },
            cx,
        )
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(32));
    cx.run_until_parked();
    assert!(source.update(cx, |_, _, _| ()).is_err());
    target
        .update(cx, |studio, _, _| {
            assert!(studio.strip.drag.is_none());
            assert_eq!(
                studio
                    .session
                    .borrow()
                    .open
                    .iter()
                    .filter(|entry| **entry == path)
                    .count(),
                1
            );
        })
        .unwrap();
}

#[gpui::test]
fn cancelling_native_drag_detaches_instead_of_rolling_back(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let source = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    source
        .update(cx, |studio, window, cx| {
            studio.new_document(window, cx);
            studio.new_document(window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    let drag = source
        .update(cx, |studio, window, cx| {
            let drag = studio.drag_payload(1, window, cx);
            studio.begin_tab_drag(drag.clone(), window, cx);
            drag.transaction.borrow_mut().native = true;
            assert!(studio.detach_owned_tab(&drag, window, cx));
            drag
        })
        .unwrap();
    cx.update(|cx| Studio::finish_tab_drag(drag.clone(), gpui::DragEnd::Cancelled, cx));
    cx.run_until_parked();
    let owner = drag.transaction.borrow().owner.unwrap().0;
    assert_ne!(owner, source.window_id().as_u64());
    assert!(drag.transaction.borrow().detached.is_none());
    source
        .update(cx, |studio, _, _| {
            assert_eq!(studio.tabs.len(), 1);
            assert_eq!(studio.tabs[0].token, 2);
            assert!(studio.strip.drag.is_none());
        })
        .unwrap();
}

#[gpui::test]
fn dropping_on_content_opens_a_window_and_cleans_up_drag_state(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| {
            studio.new_document(window, cx);
            studio.new_document(window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear());
    let first = visual.debug_bounds("document-tab-1").unwrap().center();
    visual.simulate_mouse_down(first, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        first + point(px(10.), px(0.)),
        Some(MouseButton::Left),
        Default::default(),
    );
    let drag = handle
        .update(&mut visual.cx, |studio, _, _| {
            studio.strip.drag.clone().unwrap()
        })
        .unwrap();
    visual.simulate_mouse_move(
        point(px(660.), px(400.)),
        Some(MouseButton::Left),
        Default::default(),
    );
    visual.simulate_mouse_up(
        point(px(660.), px(400.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.cx.run_until_parked();
    let owner = drag.transaction.borrow().owner.unwrap().0;
    assert_ne!(owner, handle.window_id().as_u64());
    assert!(drag.transaction.borrow().detached.is_none());
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.tabs.len(), 1);
            assert_eq!(studio.tabs[0].token, 2);
            assert!(studio.strip.drag.is_none());
        })
        .unwrap();
}
