use super::*;
use crate::editor::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

pub(super) fn input(visual: &mut VisualTestContext, field: usize, value: &str) {
    let bounds = visual
        .debug_bounds(
            [
                "shadow-input-0-0",
                "shadow-input-0-1",
                "shadow-input-0-2",
                "shadow-input-0-3",
                "shadow-input-0-4",
                "shadow-input-0-5",
            ][field],
        )
        .unwrap();
    visual.simulate_click(
        point(bounds.right() - px(20.), bounds.center().y),
        Default::default(),
    );
    draw(visual);
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input(value);
    visual.simulate_keystrokes("enter");
    draw(visual);
}

#[gpui::test]
fn shadow_controls_preserve_undo_storage_clipboard_and_component_updates(cx: &mut TestAppContext) {
    let handle = open(cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1600.)));
    create(&mut visual, "add-rectangle");
    click(&mut visual, "shadow-add");
    input(&mut visual, 0, "-20");
    input(&mut visual, 2, "16");
    input(&mut visual, 4, "FF0000");
    input(&mut visual, 5, "50");
    click(&mut visual, "shadow-kind-0");
    click(&mut visual, "effect-inner-shadow");
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(
                this.hierarchy.effects[&1][0].shadow().unwrap().kind,
                ShadowKind::Inner
            );
            assert_eq!(this.hierarchy.effects[&1][0].shadow().unwrap().blur, 16.);
            this.undo_redo(false, window, cx);
            assert_eq!(
                this.hierarchy.effects[&1][0].shadow().unwrap().kind,
                ShadowKind::Drop
            );
            this.undo_redo(true, window, cx);
            assert_eq!(
                this.hierarchy.effects[&1][0].shadow().unwrap().kind,
                ShadowKind::Inner
            );
        })
        .unwrap();
    draw(&mut visual);
    click(&mut visual, "shadow-add");
    click(&mut visual, "shadow-toggle-0");
    click(&mut visual, "shadow-remove-1");
    handle
        .update(&mut visual.cx, |this, window, cx| {
            this.focus.focus(window, cx)
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, window, cx| {
            let shadows = this.hierarchy.effects[&1].clone();
            assert_eq!(shadows.len(), 2);
            assert_eq!(shadows[0].shadow().unwrap().kind, ShadowKind::Inner);
            assert_eq!(shadows[0].shadow().unwrap().x, -20.);
            assert_eq!(shadows[0].shadow().unwrap().blur, 16.);
            assert_eq!(
                shadows[0].shadow().unwrap().color,
                gpui::Rgba {
                    r: 1.,
                    g: 0.,
                    b: 0.,
                    a: 0.5
                }
            );
            assert!(!shadows[0].enabled());
            let (json, sources) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("shadows.rovar");
            crate::document::save_as(&path, &json, &sources, &cx.text_system().clone()).unwrap();
            let saved =
                crate::document::Document::decode(&crate::document::load(&path).unwrap().json)
                    .unwrap();
            assert_eq!(saved.pages[0].hierarchy.effects[&1], shadows);
            this.duplicate_selection(window, cx);
            let copy = this.selected_shape.unwrap();
            assert_eq!(this.hierarchy.effects[&copy], shadows);
            this.delete_selected(cx);
            assert!(!this.hierarchy.effects.contains_key(&copy));
            this.select_shape(1, cx);
            this.create_component(window, cx);
            this.sync_components(window, cx);
            let component = this.components.definitions.keys().next().unwrap().clone();
            this.insert_document_component(&component, false, Some(point(700., 200.)), window, cx);
            let instance = *this.selection_ids().first().unwrap();
            assert_eq!(this.hierarchy.effects[&instance], shadows);
            this.insert_document_component(&component, false, Some(point(900., 200.)), window, cx);
            let clean = *this.selection_ids().first().unwrap();
            this.hierarchy.effects.get_mut(&instance).unwrap()[0]
                .shadow_mut()
                .unwrap()
                .x = 80.;
            this.hierarchy.effects.get_mut(&1).unwrap()[1]
                .shadow_mut()
                .unwrap()
                .blur = 42.;
            this.history.borrow_mut().mark_changed();
            this.sync_components(window, cx);
            assert_eq!(
                this.hierarchy.effects[&instance][0].shadow().unwrap().x,
                80.
            );
            assert_eq!(
                this.hierarchy.effects[&instance][1].shadow().unwrap().blur,
                shadows[1].shadow().unwrap().blur
            );
            assert_eq!(
                this.hierarchy.effects[&clean][1].shadow().unwrap().blur,
                42.
            );
        })
        .unwrap();
}

#[gpui::test]
fn shadow_exports_preserve_silhouettes_stack_order_and_expanded_bounds(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            let mut shape = Shape::new(
                1,
                None,
                ShapeKind::Ellipse,
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 40.,
                    height: 40.,
                },
            );
            shape.color = rgb(0xffffff);
            this.shapes.push(shape);
            this.next_id = 2;
            this.select_shape(1, cx);
            this.hierarchy.effects.insert(
                1,
                vec![
                    Effect::Shadow(Shadow {
                        x: 60.,
                        y: 0.,
                        blur: 0.,
                        spread: 0.,
                        color: rgb(0xff0000),
                        ..Default::default()
                    }),
                    Effect::Shadow(Shadow {
                        x: 60.,
                        y: 0.,
                        blur: 0.,
                        spread: 0.,
                        color: rgb(0x0000ff),
                        ..Default::default()
                    }),
                    Effect::Shadow(Shadow {
                        kind: ShadowKind::Inner,
                        x: 8.,
                        y: 0.,
                        blur: 0.,
                        spread: 0.,
                        color: rgb(0x00ff00),
                        ..Default::default()
                    }),
                ],
            );
            let job = this.component_export_jobs(window, cx).unwrap().remove(0);
            assert!(job.bounds.x + job.bounds.width >= 100.);
            let options = crate::render::raster::Options::default();
            let bytes = job
                .render(crate::document::export::Format::Png, 1, &options)
                .unwrap();
            let png = image::load_from_memory(&bytes).unwrap().into_rgba8();
            let at = |x: f32, y: f32| {
                png.get_pixel((x - job.bounds.x) as u32, (y - job.bounds.y) as u32)
                    .0
            };
            assert_eq!(at(80., 20.), [255, 0, 0, 255], "First shadow is on top");
            assert_eq!(
                at(2., 20.),
                [0, 255, 0, 255],
                "Inner shadows paint above the original artwork"
            );
            assert_eq!(
                at(61., 1.)[3],
                0,
                "Ellipse shadow must not fill its bounding-box corners"
            );
            let svg = job
                .render(crate::document::export::Format::Svg, 1, &options)
                .unwrap();
            let tree = resvg::usvg::Tree::from_data(&svg, &options).unwrap();
            let mut raster = resvg::tiny_skia::Pixmap::new(png.width(), png.height()).unwrap();
            resvg::render(&tree, Default::default(), &mut raster.as_mut());
            assert_eq!(raster.pixel(81, 21).unwrap().red(), 255);
            this.hierarchy.effects.get_mut(&1).unwrap()[0]
                .shadow_mut()
                .unwrap()
                .blur = 8.;
            this.hierarchy.effects.get_mut(&1).unwrap()[0]
                .shadow_mut()
                .unwrap()
                .spread = 4.;
            this.hierarchy.effects.get_mut(&1).unwrap()[1].toggle();
            let blurred = this.component_export_jobs(window, cx).unwrap().remove(0);
            let png = image::load_from_memory(
                &blurred
                    .render(crate::document::export::Format::Png, 1, &options)
                    .unwrap(),
            )
            .unwrap()
            .into_rgba8();
            let alpha = png
                .get_pixel(
                    (104. - blurred.bounds.x) as u32,
                    (20. - blurred.bounds.y) as u32,
                )
                .0[3];
            assert!(
                alpha > 0 && alpha < 255,
                "Blur must produce a soft edge beyond the spread contour"
            );
            this.hierarchy.effects.get_mut(&1).unwrap()[0].toggle();
            let job = this.component_export_jobs(window, cx).unwrap().remove(0);
            assert_eq!(job.bounds.width, 40.);
        })
        .unwrap();
}

#[gpui::test]
fn inner_shadow_exports_clip_to_the_contour_and_keep_signed_spread(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            let rect = Rect {
                x: 0.,
                y: 0.,
                width: 40.,
                height: 40.,
            };
            let mut shape = Shape::new(1, None, ShapeKind::Ellipse, rect);
            shape.color = rgb(0xffffff);
            this.shapes.push(shape);
            this.next_id = 2;
            this.select_shape(1, cx);
            let inner = Shadow {
                kind: ShadowKind::Inner,
                x: 8.,
                y: 0.,
                blur: 0.,
                spread: 0.,
                color: rgb(0xff0000),
                ..Default::default()
            };
            this.hierarchy.effects.insert(
                1,
                vec![
                    Effect::Shadow(inner.clone()),
                    Effect::Shadow(Shadow {
                        color: rgb(0x0000ff),
                        ..inner
                    }),
                ],
            );
            let options = crate::render::raster::Options::default();
            let render = |this: &Workspace, window: &mut Window, cx: &mut Context<Workspace>| {
                let job = this.component_export_jobs(window, cx).unwrap().remove(0);
                assert_eq!(job.bounds, rect, "Inner shadows must not enlarge exports");
                let bytes = job
                    .render(crate::document::export::Format::Png, 1, &options)
                    .unwrap();
                let png = image::load_from_memory(&bytes).unwrap().into_rgba8();
                let svg = job
                    .render(crate::document::export::Format::Svg, 1, &options)
                    .unwrap();
                let tree = resvg::usvg::Tree::from_data(&svg, &options).unwrap();
                let mut raster = resvg::tiny_skia::Pixmap::new(40, 40).unwrap();
                resvg::render(&tree, Default::default(), &mut raster.as_mut());
                for (x, y) in [(2, 20), (20, 20), (1, 1), (6, 20), (10, 20), (5, 20)] {
                    let svg = raster.pixel(x, y).unwrap().demultiply();
                    let svg = [svg.red(), svg.green(), svg.blue(), svg.alpha()];
                    let png = png.get_pixel(x, y).0;
                    assert!(
                        png.into_iter().zip(svg).all(|(a, b)| a.abs_diff(b) <= 1),
                        "PNG/SVG mismatch at ({x}, {y}): {png:?} vs {svg:?}"
                    );
                }
                png
            };
            let png = render(this, window, cx);
            assert_eq!(
                png.get_pixel(2, 20).0,
                [255, 0, 0, 255],
                "First inner shadow is on top"
            );
            assert_eq!(png.get_pixel(20, 20).0, [255, 255, 255, 255]);
            assert_eq!(
                png.get_pixel(1, 1).0[3],
                0,
                "Do not fill bounding-box corners"
            );
            this.hierarchy.effects.get_mut(&1).unwrap().truncate(1);
            this.shapes[0].fill_enabled = false;
            this.shapes[0].stroke.enabled = true;
            this.shapes[0].stroke.width = 8.;
            this.shapes[0].stroke.color = rgb(0xffffff);
            let png = render(this, window, cx);
            assert_eq!(
                png.get_pixel(20, 20).0[3],
                0,
                "The inner shadow must leave holes transparent"
            );
            assert_eq!(png.get_pixel(2, 20).0, [255, 0, 0, 255]);
            this.shapes[0].fill_enabled = true;
            this.shapes[0].stroke.enabled = false;
            this.shapes[0].kind = ShapeKind::Rectangle;
            let shadow = this.hierarchy.effects.get_mut(&1).unwrap()[0]
                .shadow_mut()
                .unwrap();
            shadow.x = 12.;
            shadow.spread = -4.;
            let png = render(this, window, cx);
            assert_eq!(png.get_pixel(6, 20).0, [255, 0, 0, 255]);
            assert_eq!(
                png.get_pixel(10, 20).0,
                [255, 255, 255, 255],
                "Negative spread contracts the inner shadow before offset"
            );
            let shadow = this.hierarchy.effects.get_mut(&1).unwrap()[0]
                .shadow_mut()
                .unwrap();
            shadow.x = 0.;
            shadow.spread = 5.;
            shadow.blur = 4.;
            let png = render(this, window, cx);
            let edge = png.get_pixel(5, 20).0;
            assert_eq!(edge[0], 255);
            assert!(edge[1] > 0 && edge[1] < 255, "Blur softens the inner edge");
            this.hierarchy.effects.get_mut(&1).unwrap()[0].toggle();
            let png = render(this, window, cx);
            assert_eq!(png.get_pixel(2, 20).0, [255, 255, 255, 255]);
        })
        .unwrap();
}

#[gpui::test]
fn shadow_scrubbing_is_one_undo_step_and_escape_restores_values(cx: &mut TestAppContext) {
    let handle = open(cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1600.)));
    create(&mut visual, "add-rectangle");
    click(&mut visual, "shadow-add");
    let depth = handle
        .update(&mut visual.cx, |this, _, _| {
            this.history.borrow().undo_len()
        })
        .unwrap();
    let start = visual.debug_bounds("shadow-drag-0-2").unwrap().center();
    let end = start + point(px(20.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.hierarchy.effects[&1][0].shadow().unwrap().blur, 28.);
            assert_eq!(this.history.borrow().undo_len(), depth);
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.hierarchy.effects[&1][0].shadow().unwrap().blur, 8.);
            assert_eq!(this.history.borrow().undo_len(), depth);
        })
        .unwrap();
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.history.borrow().undo_len(), depth + 1)
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.hierarchy.effects[&1][0].shadow().unwrap().blur, 8.)
        })
        .unwrap();
}
