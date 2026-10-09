use super::*;
use crate::{
    document::{Document, Loaded, Page},
    scene::shape::{Shape, ShapeKind},
};
use gpui::TestAppContext;

fn style(name: &str, color: u32) -> ColorStyle {
    ColorStyle {
        name: name.into(),
        color: rgb(color),
        gradient: None,
    }
}

fn gradient_style(name: &str, color: u32) -> ColorStyle {
    let mut style = style(name, color);
    let mut gradient = crate::scene::artboard::LinearGradient::default();
    gradient.kind = gpui::GradientKind::Radial;
    gradient.angle = 37.;
    gradient.stop_mut(0).unwrap().color = rgb(color);
    gradient.stop_mut(1).unwrap().color = gpui::rgba(0x8855ff40);
    gradient.add_stop();
    style.gradient = Some(gradient);
    style
}

#[gpui::test]
fn gradient_midpoint_in_style_dialog_stays_in_draft_until_saved(cx: &mut TestAppContext) {
    use crate::editor::tests::{click, draw, open};
    let window = open(cx);
    window
        .update(cx, |w, window, cx| {
            w.open_color_dialog(Scope::Document, None, false, window, cx);
            w.set_style_gradient(true, cx);
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = visual.debug_bounds("style-midpoint-0").unwrap().center();
    let end = start + point(px(30.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |w, _, _| {
            let g = w.colors.dialog.as_ref().unwrap().gradient.as_ref().unwrap();
            assert!(g.stop(0).unwrap().midpoint > 0.5);
            // Clicking the midpoint must not bubble to the stop-insertion track.
            assert_eq!(g.stops().len(), 2);
            assert!(w.colors.palette.is_empty());
        })
        .unwrap();
    let start = visual.debug_bounds("style-midpoint-0").unwrap().center();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        start - point(px(60.), px(0.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(start, MouseButton::Left, Default::default());
    draw(&mut visual);
    let expected = window
        .update(&mut visual.cx, |w, _, _| {
            w.colors.dialog.as_ref().unwrap().gradient.clone().unwrap()
        })
        .unwrap();
    assert!(expected.stop(0).unwrap().midpoint > 0.5);
    click(&mut visual, "save-color-style");
    window
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.colors.palette.values().next().unwrap().gradient.as_ref(),
                Some(&expected)
            );
        })
        .unwrap();
}

#[gpui::test]
fn cross_document_paste_preserves_conflicting_colors_and_undo_removes_import(
    cx: &mut TestAppContext,
) {
    cx.update(uic::init);
    let id = uuid::Uuid::new_v4().to_string();
    let source = cx.open_window(size(px(1000.), px(800.)), Workspace::new);
    let target = cx.open_window(size(px(1000.), px(800.)), Workspace::new);
    for window in [source, target] {
        window
            .update(cx, |w, window, cx| {
                w.load_document(
                    Loaded {
                        json: serde_json::to_vec(&document(&id)).unwrap(),
                        assets: Default::default(),
                        needs_upgrade: false,
                    },
                    window,
                    cx,
                )
                .unwrap();
                w.select_shape(1, cx);
            })
            .unwrap();
    }
    source
        .update(cx, |w, window, cx| {
            w.set_document_color(
                id.clone(),
                Some(gradient_style("Brand", 0xaabbcc)),
                window,
                cx,
            );
            w.copy_selection(cx);
        })
        .unwrap();
    target
        .update(cx, |w, window, cx| {
            w.set_document_color(id.clone(), Some(style("Other brand", 0xff0000)), window, cx);
            w.paste_selection(window, cx);
            assert_eq!(w.shapes.len(), 2);
            assert_eq!(w.shapes[0].color, rgb(0xff0000));
            assert_eq!(w.shapes[1].color, rgb(0xaabbcc));
            assert_eq!(w.shapes[1].fill_mode, FillMode::Linear);
            assert_eq!(
                w.shapes[1].gradient,
                gradient_style("Brand", 0xaabbcc).gradient.unwrap()
            );
            let imported = w.shapes[1].color_style.clone().unwrap();
            assert_ne!(imported, id);
            assert_eq!(w.colors.palette.len(), 2);
            w.undo_redo(false, window, cx);
            assert_eq!(w.shapes.len(), 1);
            assert_eq!(w.colors.palette.len(), 1);
            assert_eq!(w.colors.palette[&id].color, rgb(0xff0000));
            w.undo_redo(true, window, cx);
            assert_eq!(w.shapes[1].color_style.as_deref(), Some(imported.as_str()));
        })
        .unwrap();
}

#[gpui::test]
fn color_dialog_saves_and_cancel_keeps_existing_style(cx: &mut TestAppContext) {
    let window = crate::editor::tests::open(cx);
    window
        .update(cx, |w, _, _| {
            w.sidebar.collapsed = false;
            w.sidebar.resources = true;
            w.assets.scope = Scope::Document;
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    crate::editor::tests::draw(&mut visual);
    let add = visual.debug_bounds("add-color-style").unwrap().center();
    visual.simulate_click(add, Default::default());
    crate::editor::tests::draw(&mut visual);
    assert!(visual.debug_bounds("color-style-dialog").is_some());
    window
        .update(&mut visual.cx, |w, _, cx| {
            w.colors
                .name
                .update(cx, |input, cx| input.set_value("Brand", cx));
            w.colors
                .value
                .update(cx, |input, cx| input.set_value("#12345680", cx));
        })
        .unwrap();
    crate::editor::tests::draw(&mut visual);
    let save = visual.debug_bounds("save-color-style").unwrap().center();
    visual.simulate_click(save, Default::default());
    crate::editor::tests::draw(&mut visual);
    assert!(visual.debug_bounds("color-style-dialog").is_none());
    window
        .update(&mut visual.cx, |w, window, cx| {
            let id = w.colors.palette.keys().next().unwrap().clone();
            assert_eq!(w.colors.palette[&id].color, gpui::rgba(0x12345680));
            w.open_color_dialog(Scope::Document, Some(id), false, window, cx);
            w.colors
                .value
                .update(cx, |input, cx| input.set_value("#ffffff", cx));
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    crate::editor::tests::draw(&mut visual);
    assert!(visual.debug_bounds("color-style-dialog").is_none());
    window
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.colors.palette.values().next().unwrap().color,
                gpui::rgba(0x12345680)
            );
        })
        .unwrap();
}

fn document(id: &str) -> Document {
    let mut doc = Document::single(Page::empty("One".into()));
    doc.colors.insert(id.into(), style("Brand", 0xaabbcc));
    for index in 0..2 {
        let mut page = Page::empty(format!("Page {index}"));
        let mut shape = Shape::new(
            1,
            None,
            ShapeKind::Rectangle,
            Rect {
                x: 10.,
                y: 10.,
                width: 50.,
                height: 50.,
            },
        );
        shape.color = rgb(0xaabbcc);
        shape.color_style = Some(id.into());
        page.shapes.push(shape);
        page.next_id = 2;
        if index == 0 {
            doc.pages[0] = page;
        } else {
            doc.pages.push(page);
        }
    }
    doc
}

#[gpui::test]
fn gradient_dialog_selects_stops_and_saves_angle_color_and_position(cx: &mut TestAppContext) {
    let window = crate::editor::tests::open(cx);
    window
        .update(cx, |w, window, cx| {
            w.open_color_dialog(Scope::Document, None, false, window, cx)
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    crate::editor::tests::draw(&mut visual);
    let gradient = visual.debug_bounds("gradient").unwrap().center();
    visual.simulate_click(gradient, Default::default());
    crate::editor::tests::draw(&mut visual);
    let add = visual.debug_bounds("add-style-stop").unwrap().center();
    visual.simulate_click(add, Default::default());
    crate::editor::tests::draw(&mut visual);
    window
        .update(&mut visual.cx, |w, _, cx| {
            let dialog = w.colors.dialog.as_ref().unwrap();
            assert_eq!(dialog.gradient.as_ref().unwrap().stops().len(), 3);
            assert_eq!(dialog.active_stop, 2);
            w.colors
                .angle
                .update(cx, |input, cx| input.set_value("135", cx));
            w.colors
                .position
                .update(cx, |input, cx| input.set_value("30", cx));
            w.colors
                .value
                .update(cx, |input, cx| input.set_value("#22AAFF80", cx));
        })
        .unwrap();
    crate::editor::tests::draw(&mut visual);
    let save = visual.debug_bounds("save-color-style").unwrap().center();
    let bounds = visual.debug_bounds("color-style-dialog").unwrap();
    assert!(bounds.contains(&save));
    visual.simulate_click(save, Default::default());
    crate::editor::tests::draw(&mut visual);
    assert!(visual.debug_bounds("color-style-dialog").is_none());
    window
        .update(&mut visual.cx, |w, _, _| {
            let style = w.colors.palette.values().next().unwrap();
            let gradient = style.gradient.as_ref().unwrap();
            assert_eq!(gradient.angle, 135.);
            assert_eq!(gradient.stop(2).unwrap().position, 0.3);
            assert_eq!(gradient.stop(2).unwrap().color, gpui::rgba(0x22aaff80));
        })
        .unwrap();
}

#[gpui::test]
fn gradient_numbers_scrub_clamp_and_cancel_without_editing_document(cx: &mut TestAppContext) {
    let window = crate::editor::tests::open(cx);
    window
        .update(cx, |w, window, cx| {
            w.open_color_dialog(Scope::Document, None, false, window, cx);
            w.set_style_gradient(true, cx);
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    crate::editor::tests::draw(&mut visual);
    for (selector, delta, shift, expected) in [
        ("style-angle-drag", 2., false, 90.),
        ("style-angle-drag", 10., false, 100.),
        ("style-position-drag", 20., true, 100.),
        ("style-position-drag", -200., false, 0.),
        ("style-angle-drag", 700., false, 360.),
    ] {
        let start = visual.debug_bounds(selector).unwrap().center();
        let end = start + point(px(delta), px(0.));
        let modifiers = gpui::Modifiers {
            shift,
            ..Default::default()
        };
        visual.simulate_mouse_down(start, MouseButton::Left, modifiers);
        visual.simulate_mouse_move(end, MouseButton::Left, modifiers);
        visual.simulate_mouse_up(end, MouseButton::Left, modifiers);
        crate::editor::tests::draw(&mut visual);
        window
            .update(&mut visual.cx, |w, _, _| {
                let dialog = w.colors.dialog.as_ref().unwrap();
                let gradient = dialog.gradient.as_ref().unwrap();
                let value = if selector == "style-angle-drag" {
                    gradient.angle
                } else {
                    gradient.stop(dialog.active_stop).unwrap().position * 100.
                };
                assert_eq!(value, expected);
                assert!(w.gesture.is_none());
                assert_eq!(w.history.borrow().undo_len(), 0);
                assert!(w.colors.palette.is_empty());
            })
            .unwrap();
    }
    let start = visual.debug_bounds("style-angle-drag").unwrap().center();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        start - point(px(40.), px(0.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_keystrokes("escape");
    crate::editor::tests::draw(&mut visual);
    window
        .update(&mut visual.cx, |w, _, cx| {
            assert!(w.gesture.is_none());
            assert_eq!(
                w.colors
                    .dialog
                    .as_ref()
                    .unwrap()
                    .gradient
                    .as_ref()
                    .unwrap()
                    .angle,
                360.
            );
            assert_eq!(w.colors.angle.read(cx).value().as_ref(), "360");
        })
        .unwrap();
    assert!(visual.debug_bounds("color-style-dialog").is_some());
}

#[gpui::test]
fn style_scrub_reverses_at_boundary_and_changes_precision_without_jumping(cx: &mut TestAppContext) {
    let window = crate::editor::tests::open(cx);
    window
        .update(cx, |w, window, cx| {
            w.open_color_dialog(Scope::Document, None, false, window, cx);
            w.set_style_gradient(true, cx);
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    crate::editor::tests::draw(&mut visual);
    let start = visual.debug_bounds("style-position-drag").unwrap().center();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    for (delta, fine, shift, expected) in [
        (300., false, false, 100.),
        (299., false, false, 99.),
        (298., true, false, 98.9),
        (298., false, true, 98.9),
        (297., false, true, 89.),
    ] {
        visual.simulate_mouse_move(
            start + point(px(delta), px(0.)),
            MouseButton::Left,
            gpui::Modifiers {
                alt: fine,
                shift,
                ..Default::default()
            },
        );
        crate::editor::tests::draw(&mut visual);
        window
            .update(&mut visual.cx, |w, _, _| {
                let dialog = w.colors.dialog.as_ref().unwrap();
                let position = dialog
                    .gradient
                    .as_ref()
                    .unwrap()
                    .stop(dialog.active_stop)
                    .unwrap()
                    .position
                    * 100.;
                assert!(
                    (position - expected).abs() < 0.01,
                    "{position} != {expected}"
                );
            })
            .unwrap();
    }
    visual.simulate_keystrokes("escape");
    crate::editor::tests::draw(&mut visual);
    window
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.colors
                    .dialog
                    .as_ref()
                    .unwrap()
                    .gradient
                    .as_ref()
                    .unwrap()
                    .stop(0)
                    .unwrap()
                    .position,
                0.
            );
        })
        .unwrap();
}

#[gpui::test]
fn gradient_track_inserts_at_pointer_and_preserves_stop_identity_when_crossing(
    cx: &mut TestAppContext,
) {
    let window = crate::editor::tests::open(cx);
    window
        .update(cx, |w, window, cx| {
            w.open_color_dialog(Scope::Document, None, false, window, cx);
            w.set_style_gradient(true, cx);
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    crate::editor::tests::draw(&mut visual);
    let track = visual.debug_bounds("style-gradient-ramp").unwrap();
    let at = |fraction| {
        point(
            track.left() + track.size.width * fraction,
            track.top() + px(8.),
        )
    };
    let original = window
        .update(&mut visual.cx, |w, _, _| {
            w.colors.dialog.as_ref().unwrap().gradient.clone()
        })
        .unwrap();
    visual.simulate_mouse_down(at(0.5), MouseButton::Left, Default::default());
    crate::editor::tests::draw(&mut visual);
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(at(0.5), MouseButton::Left, Default::default());
    crate::editor::tests::draw(&mut visual);
    window
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(w.colors.dialog.as_ref().unwrap().gradient, original);
        })
        .unwrap();
    visual.simulate_click(at(0.25), Default::default());
    crate::editor::tests::draw(&mut visual);
    visual.simulate_click(at(0.6), Default::default());
    crate::editor::tests::draw(&mut visual);
    window
        .update(&mut visual.cx, |w, _, _| {
            let gradient = w.colors.dialog.as_ref().unwrap().gradient.as_ref().unwrap();
            assert!((gradient.stop(2).unwrap().position - 0.25).abs() < 0.001);
            assert!((gradient.stop(2).unwrap().color.a - 0.75).abs() < 0.001);
            assert_eq!(gradient.stops().len(), 4);
        })
        .unwrap();
    let start = visual.debug_bounds("style-stop-2").unwrap().center();
    let end = start + point(track.size.width * 0.6, px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    crate::editor::tests::draw(&mut visual);
    window
        .update(&mut visual.cx, |w, _, _| {
            let dialog = w.colors.dialog.as_ref().unwrap();
            assert_eq!(dialog.active_stop, 2);
            let gradient = dialog.gradient.as_ref().unwrap();
            assert!((gradient.stop(2).unwrap().position - 0.85).abs() < 0.001);
            assert!((gradient.stop(3).unwrap().position - 0.6).abs() < 0.001);
        })
        .unwrap();
    for _ in 4..20 {
        let add = visual.debug_bounds("add-style-stop").unwrap().center();
        visual.simulate_click(add, Default::default());
        crate::editor::tests::draw(&mut visual);
    }
    window
        .update(&mut visual.cx, |w, _, _| {
            let dialog = w.colors.dialog.as_ref().unwrap();
            let gradient = dialog.gradient.as_ref().unwrap();
            assert_eq!(gradient.stops().len(), 20);
            assert_eq!(dialog.active_stop, 19);
            assert!((gradient.stop(2).unwrap().position - 0.85).abs() < 0.001);
        })
        .unwrap();
}

#[gpui::test]
fn style_modes_keep_drafts_and_enter_confirms_field_without_saving(cx: &mut TestAppContext) {
    let window = crate::editor::tests::open(cx);
    window
        .update(cx, |w, window, cx| {
            w.open_color_dialog(Scope::Document, None, false, window, cx)
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    crate::editor::tests::draw(&mut visual);
    let top = visual.debug_bounds("color-style-dialog").unwrap().top();
    window
        .update(&mut visual.cx, |w, _, cx| {
            w.colors
                .value
                .update(cx, |i, cx| i.set_value("#226688FF", cx));
        })
        .unwrap();
    crate::editor::tests::draw(&mut visual);
    let gradient = visual.debug_bounds("gradient").unwrap().center();
    visual.simulate_click(gradient, Default::default());
    crate::editor::tests::draw(&mut visual);
    window
        .update(&mut visual.cx, |w, _, cx| {
            w.change_style_stops(true, cx);
            w.set_style_number(true, 135., cx);
        })
        .unwrap();
    crate::editor::tests::draw(&mut visual);
    let solid = visual.debug_bounds("solid").unwrap().center();
    visual.simulate_click(solid, Default::default());
    crate::editor::tests::draw(&mut visual);
    assert_eq!(
        visual.debug_bounds("color-style-dialog").unwrap().top(),
        top
    );
    window
        .update(&mut visual.cx, |w, _, cx| {
            assert_eq!(w.colors.value.read(cx).value().as_ref(), "#226688FF")
        })
        .unwrap();
    visual.simulate_click(gradient, Default::default());
    crate::editor::tests::draw(&mut visual);
    window
        .update(&mut visual.cx, |w, window, cx| {
            let g = w.colors.dialog.as_ref().unwrap().gradient.as_ref().unwrap();
            assert_eq!(g.stops().len(), 3);
            assert_eq!(g.angle, 135.);
            w.colors.angle.focus_handle(cx).focus(window, cx);
        })
        .unwrap();
    crate::editor::tests::draw(&mut visual);
    visual.simulate_keystrokes("secondary-a");
    visual.simulate_input("-");
    visual.simulate_keystrokes("escape");
    crate::editor::tests::draw(&mut visual);
    window
        .update(&mut visual.cx, |w, _, cx| {
            assert_eq!(w.colors.angle.read(cx).value().as_ref(), "135")
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-a");
    visual.simulate_input("120");
    visual.simulate_keystrokes("enter");
    crate::editor::tests::draw(&mut visual);
    window
        .update(&mut visual.cx, |w, _, _| {
            assert_eq!(
                w.colors
                    .dialog
                    .as_ref()
                    .unwrap()
                    .gradient
                    .as_ref()
                    .unwrap()
                    .angle,
                120.
            );
            assert!(w.colors.palette.is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn document_colors_update_all_pages_delete_without_repainting_and_undo(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let id = uuid::Uuid::new_v4().to_string();
    let doc = document(&id);
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.load_document(
            Loaded {
                json: serde_json::to_vec(&doc).unwrap(),
                assets: Default::default(),
                needs_upgrade: false,
            },
            window,
            cx,
        )
        .unwrap();
        w.set_document_color(
            id.clone(),
            Some(gradient_style("Primary", 0xff4466)),
            window,
            cx,
        );
        let current = Document::decode(&w.snapshot_document(&doc.id, cx).unwrap().0).unwrap();
        assert!(
            current
                .pages
                .iter()
                .all(|p| p.shapes[0].color == rgb(0xff4466))
        );
        assert!(
            current
                .pages
                .iter()
                .all(|p| p.shapes[0].fill_mode == FillMode::Linear
                    && Some(&p.shapes[0].gradient) == current.colors[&id].gradient.as_ref())
        );
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("gradient.rovar");
        crate::document::save_as(
            &path,
            &serde_json::to_vec(&current).unwrap(),
            &[],
            cx.text_system(),
        )
        .unwrap();
        assert_eq!(
            Document::decode(&crate::document::load(&path).unwrap().json)
                .unwrap()
                .colors,
            current.colors
        );
        w.set_document_color(id.clone(), None, window, cx);
        let current = Document::decode(&w.snapshot_document(&doc.id, cx).unwrap().0).unwrap();
        assert!(
            current
                .pages
                .iter()
                .all(|p| p.shapes[0].color_style.is_none() && p.shapes[0].color == rgb(0xff4466))
        );
        w.undo_redo(false, window, cx);
        let restored = Document::decode(&w.snapshot_document(&doc.id, cx).unwrap().0).unwrap();
        assert_eq!(restored.colors[&id].name, "Primary");
        assert_eq!(
            restored.colors[&id].gradient,
            gradient_style("Primary", 0xff4466).gradient
        );
        assert!(
            restored
                .pages
                .iter()
                .all(|p| p.shapes[0].color_style.as_deref() == Some(&id))
        );
        w.undo_redo(false, window, cx);
        assert_eq!(w.shapes[0].color, rgb(0xaabbcc));
        w.select_shape(1, cx);
        w.edit_shape(|s| s.color = rgb(0xffffff));
        assert!(w.shapes[0].color_style.is_none());
        w.undo_redo(false, window, cx);
        assert_eq!(w.shapes[0].color_style.as_deref(), Some(id.as_str()));
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("design.rovar");
        let json = w.snapshot_document(&doc.id, cx).unwrap().0;
        crate::document::save_as(&path, &json, &[], cx.text_system()).unwrap();
        let loaded = crate::document::load(&path).unwrap();
        assert_eq!(Document::decode(&loaded.json).unwrap().colors, doc.colors);
    });
}

#[gpui::test]
fn library_colors_copy_values_and_persist_without_changing_applied_objects(
    cx: &mut TestAppContext,
) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let library = cx.update(|cx| crate::document::library::Library::open(root.path(), cx));
    cx.run_until_parked();
    let id = uuid::Uuid::new_v4().to_string();
    library.update(cx, |l, cx| {
        l.set_color(
            id.clone(),
            Some(gradient_style("Library brand", 0x336699)),
            cx,
        )
        .unwrap()
    });
    let doc = document(&uuid::Uuid::new_v4().to_string());
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    workspace.update_in(cx, |w, window, cx| {
        w.load_document(
            Loaded {
                json: serde_json::to_vec(&doc).unwrap(),
                assets: Default::default(),
                needs_upgrade: false,
            },
            window,
            cx,
        )
        .unwrap();
        w.attach_library(library.clone(), cx);
        w.select_shape(1, cx);
        w.apply_color_style(Scope::Local, &id, false, window, cx);
        assert_eq!(w.shapes[0].color, rgb(0x336699));
        assert!(w.shapes[0].color_style.is_none());
        assert_eq!(w.shapes[0].fill_mode, FillMode::Linear);
        assert_eq!(
            w.shapes[0].gradient,
            gradient_style("Library brand", 0x336699).gradient.unwrap()
        );
        library.update(cx, |l, cx| {
            l.set_color(id.clone(), Some(style("Updated", 0xff0000)), cx)
                .unwrap()
        });
        assert_eq!(w.shapes[0].color, rgb(0x336699));
        w.undo_redo(false, window, cx);
        assert!(w.shapes[0].color_style.is_some());
    });
    cx.run_until_parked();
    let saved: Palette =
        serde_json::from_slice(&std::fs::read(root.path().join("components/colors.json")).unwrap())
            .unwrap();
    assert_eq!(saved[&id].color, rgb(0xff0000));
}
