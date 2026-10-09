use super::*;
use crate::editor::{
    components::tests::main_component,
    tests::{click, draw, open},
};
use gpui::{EntityInputHandler, TestAppContext, VisualTestContext};

#[gpui::test]
fn prototype_wire_to_main_variant_creates_state_change(cx: &mut TestAppContext) {
    use crate::scene::presentation::Action;
    let handle = open(cx);
    let (target, variant) = handle
        .update(cx, |w, window, cx| {
            main_component(w, window, cx);
            w.add_variant(window, cx);
            let (target, binding) = w.selected_component().unwrap();
            w.set_selection(BTreeSet::from([1]), cx);
            w.set_presentation_tab(true, window, cx);
            w.view = Viewport {
                pan: point(300., 100.),
                zoom: 0.7,
            };
            (target, binding.component)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(gpui::size(px(1280.), px(1000.)));
    draw(&mut visual);
    click(&mut visual, "prototype-hover");
    let start = visual.debug_bounds("prototype-port").unwrap().center();
    let end = handle
        .update(&mut visual.cx, |w, _, _| {
            let r = w.world_rect(target).unwrap();
            w.bounds.get().origin
                + w.view
                    .screen(point(r.x + r.width / 2., r.y + r.height / 2.))
                    .map(px)
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    click(&mut visual, "prototype-hover-exit-menu");
    click(&mut visual, "prototype-exit-option-1");
    handle
        .update(&mut visual.cx, |w, window, cx| {
            use crate::scene::presentation::HoverExit;
            assert_eq!(w.hierarchy.interactions[&1].hover_exit, HoverExit::Keep);
            let page = w.snapshot_page(cx).0;
            let decoded =
                crate::document::Page::decode(&serde_json::to_vec(&page).unwrap()).unwrap();
            assert_eq!(
                decoded.hierarchy.interactions[&1].hover_exit,
                HoverExit::Keep
            );
            w.undo_redo(false, window, cx);
            assert_eq!(w.hierarchy.interactions[&1].hover_exit, HoverExit::Restore);
        })
        .unwrap();
    handle
        .update(&mut visual.cx, |w, window, cx| {
            assert_eq!(
                w.hierarchy.interactions[&1].hover,
                Some(Action::ChangeVariant {
                    target: uuid::Uuid::parse_str(&variant).unwrap()
                })
            );
            assert_eq!(w.hierarchy.interactions[&1].click, None);
            assert!(!w.hierarchy.interactions.contains_key(&target));
            w.undo_redo(false, window, cx);
            assert!(!w.hierarchy.interactions.contains_key(&1));
        })
        .unwrap();
}

#[gpui::test]
fn prototype_variants_are_independent_and_playback_is_temporary(cx: &mut TestAppContext) {
    use crate::scene::presentation::{Action, HoverExit, Trigger};
    let handle = open(cx);
    let (first, second, third, instance, other, saved) = handle
        .update(cx, |w, window, cx| {
            w.add_artboard(
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 400.,
                    height: 300.,
                },
                cx,
            );
            let master = w.next_id;
            w.next_id += 1;
            w.shapes.push(Shape::new(
                master,
                None,
                ShapeKind::Rectangle,
                Rect {
                    x: -300.,
                    y: 0.,
                    width: 100.,
                    height: 60.,
                },
            ));
            w.set_selection(BTreeSet::from([master]), cx);
            w.create_component(window, cx);
            let first = w.selected_component().unwrap().1.component;
            w.add_variant(window, cx);
            let (second_master, binding) = w.selected_component().unwrap();
            let second = binding.component;
            w.add_variant(window, cx);
            let third = w.selected_component().unwrap().1.component;
            w.set_prototype_action(
                master,
                Trigger::Hover,
                Some(Action::ChangeVariant {
                    target: uuid::Uuid::parse_str(&second).unwrap(),
                }),
                window,
                cx,
            );
            for trigger in [Trigger::Click, Trigger::Hover] {
                w.set_prototype_action(
                    second_master,
                    trigger,
                    Some(Action::ChangeVariant {
                        target: uuid::Uuid::parse_str(&first).unwrap(),
                    }),
                    window,
                    cx,
                );
            }
            let mut instances = Vec::new();
            for x in [40., 220.] {
                w.insert_document_component(&first, false, Some(point(x, 40.)), window, cx);
                let root = w.selected_component().unwrap().0;
                w.shapes.iter_mut().find(|s| s.id == root).unwrap().board = Some(1);
                instances.push(root);
            }
            w.history.borrow_mut().mark_changed();
            w.sync_components(window, cx);
            w.hierarchy.start = Some(1);
            w.set_selection(BTreeSet::from([instances[0]]), cx);
            w.copy_selection(cx);
            let (json, _) = w
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            Document::decode(&json).unwrap();
            let saved = serde_json::to_value(w.snapshot_page(cx).0).unwrap();
            w.start_presentation(window, cx);
            (first, second, third, instances[0], instances[1], saved)
        })
        .unwrap();
    // A copied instance carries the destination variant and its component set.
    let destination = open(cx);
    destination
        .update(cx, |w, window, cx| {
            w.paste_in_place(window, cx);
            let root = *w.selection_ids().first().unwrap();
            assert!(w.components.definitions.contains_key(&second));
            assert_eq!(w.components.sets.len(), 1);
            let page = w.pages.active.clone();
            w.switch_variant(root, &page, &second, window, cx);
            assert_eq!(w.hierarchy.components[&root].component, second);
            w.snapshot_page(cx).0.validate().unwrap();
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    draw(&mut visual);
    let surface = visual.debug_bounds("playback-surface").unwrap();
    let scale = f32::from(surface.size.width) / 400.;
    let hotspot = surface.origin + point(px(90. * scale), px(70. * scale));
    let blank = surface.origin + point(px(180. * scale), px(200. * scale));
    let step = std::cell::Cell::new(0);
    let assert_variant = |visual: &mut VisualTestContext, expected: &str| {
        step.set(step.get() + 1);
        handle
            .update(&mut visual.cx, |w, _, cx| {
                let p = w.presentation.player.as_ref().unwrap().read(cx);
                assert_eq!(
                    p.hierarchy.components[&instance].component,
                    expected,
                    "playback step {}",
                    step.get()
                );
                assert_eq!(p.hierarchy.components[&other].component, first);
                assert_eq!(serde_json::to_value(w.snapshot_page(cx).0).unwrap(), saved);
            })
            .unwrap();
    };
    visual.simulate_mouse_move(hotspot, None, Default::default());
    draw(&mut visual);
    assert_variant(&mut visual, &second);
    visual.simulate_mouse_move(hotspot + point(px(1.), px(1.)), None, Default::default());
    draw(&mut visual);
    assert_variant(&mut visual, &second);
    visual.simulate_mouse_move(blank, None, Default::default());
    draw(&mut visual);
    assert_variant(&mut visual, &first);
    // The destination does not need a reverse hover action to restore the source.
    handle
        .update(&mut visual.cx, |w, _, cx| {
            w.presentation.player.as_ref().unwrap().update(cx, |p, _| {
                let definition = p.components.definitions.get_mut(&second).unwrap();
                definition
                    .page
                    .hierarchy
                    .interactions
                    .get_mut(&definition.root)
                    .unwrap()
                    .hover = None;
            });
        })
        .unwrap();
    visual.simulate_mouse_move(hotspot, None, Default::default());
    draw(&mut visual);
    assert_variant(&mut visual, &second);
    visual.simulate_mouse_move(
        surface.origin - point(px(5.), px(5.)),
        None,
        Default::default(),
    );
    draw(&mut visual);
    assert_variant(&mut visual, &first);
    visual.simulate_mouse_move(hotspot, None, Default::default());
    draw(&mut visual);
    assert_variant(&mut visual, &second);
    visual.simulate_click(hotspot, Default::default());
    draw(&mut visual);
    assert_variant(&mut visual, &first);
    visual.simulate_mouse_move(hotspot, None, Default::default());
    draw(&mut visual);
    assert_variant(&mut visual, &first);
    visual.simulate_mouse_move(blank, None, Default::default());
    draw(&mut visual);
    assert_variant(&mut visual, &first);
    visual.simulate_mouse_move(hotspot, None, Default::default());
    draw(&mut visual);
    assert_variant(&mut visual, &second);
    click(&mut visual, "playback-restart");
    assert_variant(&mut visual, &first);
    for (exit, expected) in [
        (HoverExit::Keep, &second),
        (
            HoverExit::ChangeVariant {
                target: uuid::Uuid::parse_str(&third).unwrap(),
            },
            &third,
        ),
    ] {
        handle
            .update(&mut visual.cx, |w, _, cx| {
                w.presentation.player.as_ref().unwrap().update(cx, |p, _| {
                    p.hierarchy
                        .interactions
                        .get_mut(&instance)
                        .unwrap()
                        .hover_exit = exit;
                });
            })
            .unwrap();
        visual.simulate_mouse_move(hotspot, None, Default::default());
        draw(&mut visual);
        assert_variant(&mut visual, &second);
        visual.simulate_mouse_move(blank, None, Default::default());
        draw(&mut visual);
        assert_variant(&mut visual, expected);
        click(&mut visual, "playback-restart");
        assert_variant(&mut visual, &first);
    }
    visual.simulate_keystrokes("escape");
    handle
        .update(&mut visual.cx, |w, _, cx| {
            assert!(w.presentation.player.is_none());
            assert_eq!(serde_json::to_value(w.snapshot_page(cx).0).unwrap(), saved);
        })
        .unwrap();
}

#[gpui::test]
fn variants_switch_preserves_overrides_position_and_undo(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            let first = main_component(this, window, cx);
            let depth = this.history.borrow().undo_len();
            this.add_variant(window, cx);
            assert_eq!(this.history.borrow().undo_len(), depth + 1);
            this.undo_redo(false, window, cx);
            assert!(this.components.sets.is_empty());
            assert_eq!(this.components.definitions.len(), 1);
            this.undo_redo(true, window, cx);
            let (master, binding) = this.selected_component().unwrap();
            let second = binding.component;
            assert_ne!(first, second);
            let shape = this.hierarchy.components[&master].nodes[&2];
            this.shapes
                .iter_mut()
                .find(|s| s.id == shape)
                .unwrap()
                .rect
                .width = 90.;
            this.shapes
                .iter_mut()
                .find(|s| s.id == shape)
                .unwrap()
                .color = rgb(0x00ff00);
            this.history.borrow_mut().mark_changed();
            this.components.revision = None;
            this.sync_components(window, cx);
            let master_page = this.pages.active.clone();
            let master_shape = shape;
            this.add_page(None, window, cx);
            this.insert_document_component(&first, false, Some(point(700., 300.)), window, cx);
            let (root, link) = this.selected_component().unwrap();
            let text = link.nodes[&3];
            let shape = link.nodes[&2];
            this.texts
                .iter()
                .find(|t| t.id == text)
                .unwrap()
                .editor
                .update(cx, |editor, cx| {
                    editor.replace_text_in_range(Some(0..6), "My button", window, cx);
                });
            this.shapes
                .iter_mut()
                .find(|s| s.id == shape)
                .unwrap()
                .color = rgb(0xff0000);
            this.history.borrow_mut().mark_changed();
            let before = this.snapshot_page(cx).0;
            let page = this.pages.active.clone();
            this.switch_variant(root, &page, &second, window, cx);
            let current = this.snapshot_page(cx).0;
            assert_eq!(current.hierarchy.components[&root].component, second);
            assert_eq!(
                current.texts.iter().find(|t| t.id == text).unwrap().content,
                "My button"
            );
            let current_shape = current.shapes.iter().find(|s| s.id == shape).unwrap();
            assert_eq!(current_shape.color, rgb(0xff0000));
            assert_eq!(current_shape.rect.width, 90.);
            assert_eq!(this.world_rect(root).unwrap().x, 700.);
            assert_eq!(this.world_rect(root).unwrap().y, 300.);
            current.validate().unwrap();
            this.undo_redo(false, window, cx);
            this.sync_components(window, cx);
            assert!(this.snapshot_page(cx).0 == before);
            this.undo_redo(true, window, cx);
            this.sync_components(window, cx);
            assert!(this.snapshot_page(cx).0 == current);
            this.switch_page(&master_page, window, cx);
            this.shapes
                .iter_mut()
                .find(|s| s.id == master_shape)
                .unwrap()
                .rect
                .width = 110.;
            this.history.borrow_mut().mark_changed();
            this.components.revision = None;
            this.sync_components(window, cx);
            this.switch_page(&page, window, cx);
            let current = this.snapshot_page(cx).0;
            assert_eq!(
                current
                    .shapes
                    .iter()
                    .find(|s| s.id == shape)
                    .unwrap()
                    .rect
                    .width,
                110.
            );
            assert_eq!(
                current.shapes.iter().find(|s| s.id == shape).unwrap().color,
                rgb(0xff0000)
            );
            assert_eq!(
                current.texts.iter().find(|t| t.id == text).unwrap().content,
                "My button"
            );
            let (json, _) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            Document::decode(&json).unwrap();
        })
        .unwrap();
}

#[gpui::test]
fn variants_ui_rename_navigation_and_instance_switch(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            main_component(this, window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(gpui::size(px(1280.), px(1400.)));
    click(&mut visual, "add-variant");
    click(&mut visual, "rename-variant");
    visual.simulate_keystrokes("secondary-a");
    visual.simulate_input("Hover");
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.components.rename.is_none());
            let set = this.components.sets.values().next().unwrap();
            assert!(set.variants.values().any(|name| name == "Hover"));
        })
        .unwrap();
    click(&mut visual, "rename-variant");
    visual.simulate_keystrokes("secondary-a");
    visual.simulate_input("Default");
    visual.simulate_keystrokes("enter");
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.components.rename.as_ref().unwrap().error);
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    click(&mut visual, "variant-selector");
    click(&mut visual, "variant-option-Default");
    handle
        .update(&mut visual.cx, |this, _, _| {
            let (root, binding) = this.selected_component().unwrap();
            assert!(binding.master);
            assert_eq!(root, 1);
        })
        .unwrap();
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert!(this.components.rename.is_none());
            let set = this.components.sets.values().next().unwrap();
            let first = set
                .variants
                .iter()
                .find(|(_, name)| *name == "Default")
                .unwrap()
                .0
                .clone();
            this.insert_document_component(&first, false, Some(point(600., 400.)), window, cx);
        })
        .unwrap();
    click(&mut visual, "variant-selector");
    click(&mut visual, "variant-option-Hover");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, _, _| {
            let (_, binding) = this.selected_component().unwrap();
            assert_eq!(
                this.variant_set(&binding.component).unwrap().1.variants[&binding.component],
                "Hover"
            );
        })
        .unwrap();
}

#[gpui::test]
fn variants_storage_delta_and_independent_property_merge(cx: &mut TestAppContext) {
    use crate::document;
    use rovar_format::delta::Snapshot;
    let handle = open(cx);
    let mut document = handle
        .update(cx, |this, window, cx| {
            main_component(this, window, cx);
            this.add_variant(window, cx);
            let (json, _) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            Document::decode(&json).unwrap()
        })
        .unwrap();
    let system = cx.update(|cx| cx.text_system().clone());
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("variants.rovar");
    let original = serde_json::to_vec(&document).unwrap();
    document::save_as(&path, &original, &[], &system).unwrap();
    let before = document::load(&path).unwrap();
    assert_eq!(
        Document::decode(&before.json).unwrap().component_sets,
        document.component_sets
    );
    let limit = rovar_api::MAX_METADATA_BYTES;
    let base = Snapshot::from_bytes(&std::fs::read(&path).unwrap(), limit).unwrap();
    document.component_sets.values_mut().next().unwrap().name = "Buttons".into();
    let json = serde_json::to_vec(&document).unwrap();
    document::save(&path, &json, &[], &before.json, &system).unwrap();
    let next = Snapshot::from_bytes(&std::fs::read(&path).unwrap(), limit).unwrap();
    let bytes = base
        .apply(&base.difference(&next).unwrap(), limit)
        .unwrap()
        .to_bytes(limit)
        .unwrap();
    let synced = temp.path().join("synced.rovar");
    std::fs::write(&synced, bytes).unwrap();
    assert_eq!(
        Document::decode(&document::load(&synced).unwrap().json)
            .unwrap()
            .component_sets,
        document.component_sets
    );
    let mut remote = Document::decode(&original).unwrap();
    remote.pages[0].name = "Renamed page".into();
    let merged =
        document::merge::merge(&original, &json, &serde_json::to_vec(&remote).unwrap()).unwrap();
    let merged = Document::decode(&merged).unwrap();
    assert_eq!(merged.component_sets, document.component_sets);
    assert_eq!(merged.pages[0].name, "Renamed page");
    let set = document.component_sets.values_mut().next().unwrap();
    let duplicate = set.variants.values().next().unwrap().clone();
    *set.variants.values_mut().next_back().unwrap() = duplicate;
    assert!(document.validate().is_err());
}

#[gpui::test]
fn variants_combine_independent_masters_and_switch_structure(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (first, second, roots) = handle
        .update(cx, |this, window, cx| {
            let first = main_component(this, window, cx);
            this.insert_document_component(&first, false, Some(point(400., 100.)), window, cx);
            let (root, _) = this.selected_component().unwrap();
            this.detach_component(root, cx);
            this.create_component(window, cx);
            let second = this.hierarchy.components[&root].component.clone();
            let removed = this.hierarchy.components[&root]
                .nodes
                .keys()
                .find(|id| this.shapes.iter().any(|s| s.id == **id))
                .copied()
                .unwrap();
            this.shapes.retain(|s| s.id != removed);
            let id = this.next_id;
            this.next_id += 1;
            let mut shape = Shape::new(
                id,
                Some(root),
                ShapeKind::Ellipse,
                Rect {
                    x: 60.,
                    y: 20.,
                    width: 25.,
                    height: 25.,
                },
            );
            shape.name = "Badge".into();
            this.shapes.push(shape);
            this.history.borrow_mut().mark_changed();
            this.components.revision = None;
            this.sync_components(window, cx);
            let roots = BTreeSet::from([1, root]);
            this.set_selection(roots.clone(), cx);
            (first, second, roots)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(gpui::size(px(1280.), px(1400.)));
    click(&mut visual, "combine-variants");
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.components.sets.len(), 1);
            this.undo_redo(false, window, cx);
            assert!(this.components.sets.is_empty());
            assert_eq!(this.selection_ids(), roots);
            this.undo_redo(true, window, cx);
            assert_eq!(this.components.sets.len(), 1);
            this.insert_document_component(&first, false, Some(point(800., 200.)), window, cx);
            let (root, link) = this.selected_component().unwrap();
            let removed_shape = link.nodes[&2];
            let text = link.nodes[&3];
            this.texts
                .iter()
                .find(|t| t.id == text)
                .unwrap()
                .editor
                .update(cx, |editor, cx| {
                    editor.replace_text_in_range(Some(0..6), "Custom", window, cx);
                });
            let page = this.pages.active.clone();
            this.switch_variant(root, "stale-page", &second, window, cx);
            assert_eq!(this.hierarchy.components[&root].component, first);
            this.boards
                .iter_mut()
                .find(|b| b.id == root)
                .unwrap()
                .layer
                .locked = true;
            this.switch_variant(root, &page, &second, window, cx);
            assert_eq!(this.hierarchy.components[&root].component, first);
            this.boards
                .iter_mut()
                .find(|b| b.id == root)
                .unwrap()
                .layer
                .locked = false;
            this.switch_variant(root, &page, &second, window, cx);
            let result = this.snapshot_page(cx).0;
            result.validate().unwrap();
            assert!(!result.shapes.iter().any(|s| s.id == removed_shape));
            assert!(
                result
                    .shapes
                    .iter()
                    .any(|s| s.board == Some(root) && s.name == "Badge")
            );
            assert_eq!(
                result.texts.iter().find(|t| t.id == text).unwrap().content,
                "Custom"
            );
            this.switch_variant(root, &page, &first, window, cx);
            let result = this.snapshot_page(cx).0;
            result.validate().unwrap();
            assert!(
                !result
                    .shapes
                    .iter()
                    .any(|s| s.board == Some(root) && s.name == "Badge")
            );
            assert_eq!(
                result.texts.iter().find(|t| t.id == text).unwrap().content,
                "Custom"
            );
            let (json, _) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            Document::decode(&json).unwrap();
        })
        .unwrap();
}
