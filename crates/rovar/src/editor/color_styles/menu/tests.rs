use super::*;
use crate::editor::tests::{click, draw, open};
use crate::scene::paint_layers::Fill;
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn inline_save_preserves_layer_and_popup_position_and_undoes_atomically(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |w, window, cx| {
            let mut shape = Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                Rect {
                    x: 10.,
                    y: 10.,
                    width: 60.,
                    height: 60.,
                },
            );
            shape.color = rgb(0x123456);
            let mut fill = Fill::from_shape(&shape);
            fill.fill_mode = FillMode::Linear;
            fill.opacity = 0.55;
            fill.gradient.stop_mut(0).unwrap().color = gpui::rgba(0x9977bb80);
            shape.fill_layers.push(fill);
            w.shapes.push(shape);
            w.next_id = 2;
            w.select_shape(1, cx);
            w.select_paint_layer(false, 1, window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "fill-color-style");
    let before = visual.debug_bounds("color-style-panel").unwrap();
    click(&mut visual, "save-fill-style");
    visual.simulate_input("Brand / Iris");
    draw(&mut visual);
    assert_eq!(
        visual.debug_bounds("color-style-panel").unwrap().top(),
        before.top()
    );
    assert!(visual.debug_bounds("inline-style-form").is_some());
    assert!(visual.debug_bounds("color-style-dialog").is_none());
    let panel = visual.debug_bounds("color-style-panel").unwrap();
    for control in [
        "color-style-heading",
        "style-inline-name",
        "style-save-location",
        "confirm-inline-style",
    ] {
        let bounds = visual.debug_bounds(control).unwrap();
        assert!(
            panel.contains(&bounds.center()),
            "{control} must be visible"
        );
    }
    click(&mut visual, "style-save-location");
    click(&mut visual, "assets-document");
    window
        .update(&mut visual.cx, |w, _, cx| {
            assert!(w.colors.menu[0].read(cx).is_open())
        })
        .unwrap();
    click(&mut visual, "cancel-inline-style");
    assert_eq!(
        visual.debug_bounds("color-style-panel").unwrap().top(),
        before.top()
    );
    window
        .update(&mut visual.cx, |w, _, _| {
            assert!(w.colors.palette.is_empty());
            assert!(w.shapes[0].fill_layers[0].color_style.is_none());
        })
        .unwrap();
    click(&mut visual, "save-fill-style");
    visual.simulate_input("Brand / Iris");
    click(&mut visual, "confirm-inline-style");
    window
        .update(&mut visual.cx, |w, window, cx| {
            assert!(w.colors.menu[0].read(cx).is_open());
            assert!(w.colors.browser.draft.is_none());
            let fill = &w.shapes[0].fill_layers[0];
            let id = fill.color_style.as_ref().unwrap();
            assert_eq!(w.colors.palette[id].name, "Brand / Iris");
            assert_eq!(w.colors.palette[id].gradient.as_ref(), Some(&fill.gradient));
            assert_eq!(fill.opacity, 0.55);
            assert_eq!(w.shapes[0].color, rgb(0x123456));
            assert!(w.shapes[0].color_style.is_none());
            w.undo_redo(false, window, cx);
            assert!(w.colors.palette.is_empty());
            assert!(w.shapes[0].fill_layers[0].color_style.is_none());
            w.undo_redo(true, window, cx);
            assert_eq!(w.colors.palette.len(), 1);
            assert!(w.shapes[0].fill_layers[0].color_style.is_some());
        })
        .unwrap();
}

#[gpui::test]
fn style_search_library_save_and_changed_target_keep_document_values(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open(cx);
    let library = cx.update(|cx| crate::document::library::Library::open(root.path(), cx));
    cx.run_until_parked();
    window
        .update(cx, |w, _, cx| {
            w.attach_library(library.clone(), cx);
            for id in 1..=2 {
                let mut shape = Shape::new(
                    id,
                    None,
                    ShapeKind::Rectangle,
                    Rect {
                        x: id as f32 * 100.,
                        y: 10.,
                        width: 60.,
                        height: 60.,
                    },
                );
                shape.color = rgb(0x8877aa);
                w.shapes.push(shape);
            }
            w.next_id = 3;
            w.select_shape(1, cx);
            for (id, name) in [("iris", "Iris"), ("sage", "Sage")] {
                w.colors.palette.insert(
                    id.into(),
                    ColorStyle {
                        name: name.into(),
                        color: rgb(0xaabbcc),
                        gradient: None,
                    },
                );
            }
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "fill-color-style");
    click(&mut visual, "style-search");
    visual.simulate_input("iris");
    draw(&mut visual);
    assert!(visual.debug_bounds("pick-color-iris").is_some());
    assert!(visual.debug_bounds("pick-color-sage").is_none());
    click(&mut visual, "save-fill-style");
    visual.simulate_input("Library / Iris");
    click(&mut visual, "style-save-location");
    click(&mut visual, "assets-local");
    click(&mut visual, "confirm-inline-style");
    window
        .update(&mut visual.cx, |w, _, cx| {
            assert_eq!(library.read(cx).colors.len(), 1);
            assert_eq!(
                library.read(cx).colors.values().next().unwrap().color,
                rgb(0x8877aa)
            );
            assert_eq!(w.colors.palette.len(), 2);
            assert!(
                w.shapes
                    .iter()
                    .all(|s| s.color_style.is_none() && s.color == rgb(0x8877aa))
            );
        })
        .unwrap();
    click(&mut visual, "save-fill-style");
    visual.simulate_input("Wrong target");
    window
        .update(&mut visual.cx, |w, window, cx| {
            w.select_shape(2, cx);
            w.save_inline_color_style(window, cx);
            assert!(w.colors.browser.error.is_some());
            assert_eq!(library.read(cx).colors.len(), 1);
            assert!(w.shapes[1].color_style.is_none());
        })
        .unwrap();
}
