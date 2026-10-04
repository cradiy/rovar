use super::*;
use crate::editor::tests::{click, create, draw, open};
use gpui::EntityInputHandler;
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn layer_blur_controls_preserve_history_storage_and_component_overrides(cx: &mut TestAppContext) {
    blur_controls(cx, "effect-layer-blur", |radius| Effect::LayerBlur {
        enabled: true,
        radius,
    });
}

#[gpui::test]
fn background_blur_controls_preserve_history_storage_and_component_overrides(
    cx: &mut TestAppContext,
) {
    blur_controls(cx, "effect-background-blur", |radius| {
        Effect::BackgroundBlur {
            enabled: true,
            radius,
        }
    });
}

fn blur_controls(cx: &mut TestAppContext, kind: &'static str, effect: fn(f32) -> Effect) {
    let handle = open(cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1600.)));
    create(&mut visual, "add-rectangle");
    click(&mut visual, "shadow-add");
    click(&mut visual, "shadow-kind-0");
    click(&mut visual, kind);
    assert!(visual.debug_bounds("shadow-input-0-2").is_some());
    assert!(visual.debug_bounds("shadow-input-0-0").is_none());
    assert!(visual.debug_bounds("shadow-color-0").is_none());
    super::tests::input(&mut visual, 2, "20");
    let start = visual.debug_bounds("shadow-drag-0-2").unwrap().center();
    let end = start + point(px(12.), px(0.));
    let depth = handle
        .update(&mut visual.cx, |this, _, _| {
            this.history.borrow().undo_len()
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.effect_number(0, 2), Some(32.));
            assert_eq!(this.history.borrow().undo_len(), depth);
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.effect_number(0, 2), Some(20.));
            assert_eq!(this.history.borrow().undo_len(), depth);
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.history.borrow().undo_len(), depth + 1);
            this.undo_redo(false, window, cx);
            assert_eq!(this.effect_number(0, 2), Some(20.));
            this.undo_redo(true, window, cx);
            assert_eq!(this.effect_number(0, 2), Some(32.));
        })
        .unwrap();
    draw(&mut visual);
    click(&mut visual, "shadow-toggle-0");
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.hierarchy.effects[&1][0].enabled());
            assert_eq!(this.effect_padding(1), 0.);
        })
        .unwrap();
    click(&mut visual, "shadow-toggle-0");
    super::tests::input(&mut visual, 2, "-10");
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.effect_number(0, 2), Some(0.))
        })
        .unwrap();
    super::tests::input(&mut visual, 2, "999");
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.effect_number(0, 2), Some(MAX_RADIUS))
        })
        .unwrap();
    super::tests::input(&mut visual, 2, "12");
    handle
        .update(&mut visual.cx, |this, window, cx| {
            let effects = this.hierarchy.effects[&1].clone();
            let (json, sources) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("blur.rovar");
            crate::document::save_as(&path, &json, &sources, &cx.text_system().clone()).unwrap();
            let saved = crate::document::load(&path)
                .unwrap()
                .into_document()
                .unwrap();
            assert_eq!(saved.pages[0].hierarchy.effects[&1], effects);
            this.duplicate_selection(window, cx);
            let copy = this.selected_shape.unwrap();
            assert_eq!(this.hierarchy.effects[&copy], effects);
            this.delete_selected(cx);
            assert!(!this.hierarchy.effects.contains_key(&copy));
            this.select_shape(1, cx);
            this.create_component(window, cx);
            this.sync_components(window, cx);
            let component = this.components.definitions.keys().next().unwrap().clone();
            this.insert_document_component(&component, false, Some(point(700., 200.)), window, cx);
            let instance = *this.selection_ids().first().unwrap();
            assert_eq!(this.hierarchy.effects[&instance], effects);
            this.insert_document_component(&component, false, Some(point(900., 200.)), window, cx);
            let clean = *this.selection_ids().first().unwrap();
            this.hierarchy.effects.get_mut(&instance).unwrap()[0] = effect(24.);
            this.hierarchy.effects.get_mut(&1).unwrap()[0] = effect(40.);
            this.history.borrow_mut().mark_changed();
            this.sync_components(window, cx);
            assert_eq!(this.hierarchy.effects[&instance][0], effect(24.));
            assert_eq!(this.hierarchy.effects[&clean][0], effect(40.));
            this.select_shape(1, cx);
        })
        .unwrap();
    draw(&mut visual);
    click(&mut visual, "shadow-remove-0");
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert!(!this.hierarchy.effects.contains_key(&1));
            this.undo_redo(false, window, cx);
            assert_eq!(this.hierarchy.effects[&1][0], effect(40.));
        })
        .unwrap();
}

#[gpui::test]
fn layer_blur_export_preserves_color_alpha_bounds_and_shadow_composition(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            let rect = Rect {
                x: 0.,
                y: 0.,
                width: 40.,
                height: 40.,
            };
            let mut shape = Shape::new(1, None, ShapeKind::Rectangle, rect);
            shape.color = gpui::rgba(0xff000080);
            this.shapes.push(shape);
            this.next_id = 2;
            this.select_shape(1, cx);
            this.hierarchy.effects.insert(
                1,
                vec![Effect::LayerBlur {
                    enabled: true,
                    radius: 8.,
                }],
            );
            let render = |this: &Workspace, window: &mut Window, cx: &mut Context<Workspace>| {
                let job = this.component_export_jobs(window, cx).unwrap().remove(0);
                let options = crate::render::raster::Options::default();
                let png = image::load_from_memory(
                    &job.render(crate::document::export::Format::Png, 1, &options)
                        .unwrap(),
                )
                .unwrap()
                .into_rgba8();
                let svg = job
                    .render(crate::document::export::Format::Svg, 1, &options)
                    .unwrap();
                let tree = resvg::usvg::Tree::from_data(&svg, &options).unwrap();
                let mut raster = resvg::tiny_skia::Pixmap::new(png.width(), png.height()).unwrap();
                resvg::render(&tree, Default::default(), &mut raster.as_mut());
                for (x, y) in [(-4., 20.), (0., 20.), (20., 20.), (44., 20.)] {
                    let (x, y) = ((x - job.bounds.x) as u32, (y - job.bounds.y) as u32);
                    let p = png.get_pixel(x, y).0;
                    let s = raster.pixel(x, y).unwrap().demultiply();
                    assert!(
                        p.into_iter()
                            .zip([s.red(), s.green(), s.blue(), s.alpha()])
                            .all(|(a, b)| a.abs_diff(b) <= 1)
                    );
                }
                (job.bounds, png)
            };
            let (bounds, png) = render(this, window, cx);
            assert_eq!(
                bounds,
                Rect {
                    x: -12.,
                    y: -12.,
                    width: 64.,
                    height: 64.
                }
            );
            let outside = png.get_pixel(8, 32).0;
            assert!(outside[3] > 0 && outside[3] < 128);
            assert!(
                outside[0] >= 250 && outside[1] == 0 && outside[2] == 0,
                "Transparent edges must retain their color"
            );
            assert_eq!(png.get_pixel(32, 32).0, [255, 0, 0, 128]);
            this.hierarchy
                .effects
                .get_mut(&1)
                .unwrap()
                .push(Effect::Shadow(Shadow {
                    x: 60.,
                    y: 0.,
                    blur: 0.,
                    color: rgb(0x0000ff),
                    ..Default::default()
                }));
            let (bounds, png) = render(this, window, cx);
            assert!(bounds.x + bounds.width > 100.);
            let shadow_edge = png
                .get_pixel((104. - bounds.x) as u32, (20. - bounds.y) as u32)
                .0;
            assert!(
                shadow_edge[2] > 240 && shadow_edge[3] > 0,
                "Layer blur includes the composed shadow"
            );
            this.hierarchy.effects.insert(
                1,
                vec![Effect::LayerBlur {
                    enabled: false,
                    radius: 8.,
                }],
            );
            let job = this.component_export_jobs(window, cx).unwrap().remove(0);
            assert_eq!(job.bounds, rect);
        })
        .unwrap();
}

#[gpui::test]
fn image_and_text_blur_exports_include_pixels_outside_the_original_layer(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("image.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 80, 255, 255]))
        .save(&path)
        .unwrap();
    let asset = crate::media::MediaAsset::load(path).unwrap();
    let options = crate::render::svg::render_options(true).unwrap();
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            let rect = Rect {
                x: 0.,
                y: 0.,
                width: 40.,
                height: 40.,
            };
            let mut image = Shape::new(1, None, ShapeKind::Image, rect);
            image.media = Some(asset);
            this.shapes.push(image);
            let text = this.make_text(2, None, rect, window, cx);
            text.editor.update(cx, |editor, cx| {
                editor.replace_text_in_range(None, "MMMM", window, cx)
            });
            this.texts.push(text);
            this.next_id = 3;
            for id in [1, 2] {
                this.hierarchy.effects.insert(
                    id,
                    vec![Effect::LayerBlur {
                        enabled: true,
                        radius: 12.,
                    }],
                );
                if id == 1 {
                    this.select_shape(id, cx);
                } else {
                    this.select_text(id, cx);
                }
                let job = this.component_export_jobs(window, cx).unwrap().remove(0);
                assert_eq!(
                    job.bounds,
                    Rect {
                        x: -18.,
                        y: -18.,
                        width: 76.,
                        height: 76.
                    }
                );
                let bytes = job
                    .render(crate::document::export::Format::Png, 1, &options)
                    .unwrap();
                let png = image::load_from_memory(&bytes).unwrap().into_rgba8();
                assert!(
                    png.enumerate_pixels().any(|(x, y, pixel)| {
                        (x < 18 || y < 18 || x >= 58 || y >= 58) && pixel.0[3] > 0
                    }),
                    "Layer {id} should blur beyond its source clip"
                );
                this.hierarchy.effects.get_mut(&id).unwrap()[0].toggle();
                assert_eq!(
                    this.component_export_jobs(window, cx)
                        .unwrap()
                        .remove(0)
                        .bounds,
                    rect
                );
            }
        })
        .unwrap();
}
