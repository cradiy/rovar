use super::*;
use crate::editor::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn contour_glow_controls_history_storage_and_component_instances(cx: &mut TestAppContext) {
    let handle = open(cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1600.)));
    create(&mut visual, "add-rectangle");
    click(&mut visual, "shadow-add");
    click(&mut visual, "shadow-kind-0");
    click(&mut visual, "effect-contour-glow");
    for (field, value) in [
        (0, "175"),
        (1, "2.5"),
        (2, "24"),
        (3, "25"),
        (4, "AA22FF"),
        (5, "60"),
    ] {
        super::tests::input(&mut visual, field, value);
    }
    let expected = Effect::Glow(Glow {
        enabled: true,
        radius: 24.,
        edge_width: 2.5,
        intensity: 1.75,
        threshold: 0.25,
        color: gpui::Rgba {
            a: 0.6,
            ..rgb(0xaa22ff)
        },
    });
    let depth = handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(w.hierarchy.effects[&1], vec![expected.clone()]);
            w.history.borrow().undo_len()
        })
        .unwrap();
    let start = visual.debug_bounds("shadow-drag-0-2").unwrap().center();
    let end = start + point(px(12.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(w.hierarchy.effects[&1], vec![expected.clone()]);
            assert_eq!(w.history.borrow().undo_len(), depth);
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, window, cx| {
            assert_eq!(w.effect_number(0, 2), Some(36.));
            assert_eq!(w.history.borrow().undo_len(), depth + 1);
            w.undo_redo(false, window, cx);
            assert_eq!(w.hierarchy.effects[&1], vec![expected.clone()]);
            let (json, sources) = w
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("glow.rovar");
            crate::document::save_as(&path, &json, &sources, &cx.text_system().clone()).unwrap();
            let saved = crate::document::load(&path)
                .unwrap()
                .into_document()
                .unwrap();
            assert_eq!(saved.pages[0].hierarchy.effects[&1], vec![expected.clone()]);
            w.create_component(window, cx);
            w.sync_components(window, cx);
            let component = w.components.definitions.keys().next().unwrap().clone();
            w.insert_document_component(&component, false, Some(point(700., 200.)), window, cx);
            let instance = *w.selection_ids().first().unwrap();
            assert_eq!(w.hierarchy.effects[&instance], vec![expected.clone()]);
        })
        .unwrap();
    draw(&mut visual);
    click(&mut visual, "shadow-toggle-0");
    handle
        .update(&mut visual.cx, |w, _, _| {
            let id = *w.selection_ids().first().unwrap();
            assert!(!w.hierarchy.effects[&id][0].enabled());
            assert_eq!(w.effect_padding(id), 0.);
        })
        .unwrap();
    click(&mut visual, "shadow-remove-0");
    handle
        .update(&mut visual.cx, |w, window, cx| {
            let id = *w.selection_ids().first().unwrap();
            assert!(!w.hierarchy.effects.contains_key(&id));
            w.undo_redo(false, window, cx);
            assert!(matches!(w.hierarchy.effects[&id][0], Effect::Glow(_)));
        })
        .unwrap();
}
