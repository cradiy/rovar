use super::*;
use crate::{
    document::{AssetUse, Text},
    scene::artboard::{Artboard, FillMode},
    scene::layer::Hierarchy,
    scene::shape::{Shape, ShapeKind, StrokeAlign},
    scene::text::{TextStyle, styles::StyledText},
};
use gpui::{GradientKind, rgb};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}
fn document(shapes: Vec<Shape>) -> Page {
    Page {
        name: "Page 1".into(),
        id: uuid::Uuid::new_v4().to_string(),
        next_id: 100,
        boards: vec![],
        shapes,
        texts: vec![],
        hierarchy: Hierarchy::default(),
        assets: vec![],
    }
}
fn job(doc: &Page, bounds: Rect) -> Job {
    Job {
        preset: Default::default(),
        original: None,
        name: "Export".into(),
        json: Arc::new(serde_json::to_vec(doc).unwrap()),
        assets: Arc::new(vec![]),
        order: doc
            .boards
            .iter()
            .map(|s| s.id)
            .chain(doc.shapes.iter().map(|s| s.id))
            .chain(doc.texts.iter().map(|s| s.id))
            .collect(),
        bounds,
        clips: BTreeMap::new(),
        text: BTreeMap::new(),
    }
}
fn png(job: &Job, scale: u32) -> image::RgbaImage {
    image::load_from_memory(&job.render(Format::Png, scale, &Default::default()).unwrap())
        .unwrap()
        .into_rgba8()
}

fn background_blur_scene() -> Page {
    let mut left = Shape::new(1, None, ShapeKind::Rectangle, rect(0., 0., 50., 100.));
    left.color = rgb(0xff0000);
    let mut right = Shape::new(2, None, ShapeKind::Rectangle, rect(50., 0., 50., 100.));
    right.color = rgb(0x0000ff);
    let mut glass = Shape::new(3, None, ShapeKind::Rectangle, rect(20., 20., 60., 60.));
    glass.fill_enabled = false;
    let mut doc = document(vec![left, right, glass]);
    doc.hierarchy.effects.insert(
        3,
        vec![crate::scene::effects::Effect::BackgroundBlur {
            enabled: true,
            radius: 12.,
        }],
    );
    doc
}

#[test]
fn background_blur_remains_active_with_layer_blending_and_opacity() {
    use crate::scene::blend::Mode;
    let mut doc = background_blur_scene();
    for mode in Mode::ALL {
        doc.shapes[2].layer.blend = mode;
        doc.shapes[2].layer.opacity = 0.5;
        let pixels = png(&job(&doc, rect(0., 0., 100., 100.)), 1);
        let blurred = pixels.get_pixel(49, 35).0;
        assert!(
            blurred[0] > 160 && blurred[0] < 230,
            "{mode:?}: {blurred:?}"
        );
        assert!(blurred[2] > 30 && blurred[2] < 100, "{mode:?}: {blurred:?}");
        assert_eq!(blurred[3], 255);
    }
    doc.shapes[2].layer.opacity = 0.;
    assert_eq!(
        png(&job(&doc, rect(0., 0., 100., 100.)), 1)
            .get_pixel(49, 35)
            .0,
        [255, 0, 0, 255]
    );
}

#[test]
fn background_blur_exports_only_preceding_artwork_and_preserves_vector_foreground() {
    let mut doc = background_blur_scene();
    let mut front = Shape::new(4, None, ShapeKind::Rectangle, rect(48., 45., 4., 10.));
    front.color = rgb(0x00ff00);
    doc.shapes.push(front);
    let job = job(&doc, rect(0., 0., 100., 100.));
    for scale in [1, 2, 4] {
        let pixels = png(&job, scale);
        assert_eq!(pixels.get_pixel(49 * scale, 10 * scale).0, [255, 0, 0, 255]);
        let blurred = pixels.get_pixel(49 * scale, 35 * scale).0;
        assert!(blurred[0] > 80 && blurred[0] < 180, "{blurred:?}");
        assert_eq!(blurred[1], 0, "Foreground must not enter the backdrop");
        assert!(blurred[2] > 80 && blurred[2] < 180);
        assert_eq!(blurred[3], 255);
        assert_eq!(pixels.get_pixel(49 * scale, 50 * scale).0, [0, 255, 0, 255]);
        assert_eq!(pixels.get_pixel(47 * scale, 50 * scale).0[1], 0);
    }
    let svg = job.render(Format::Svg, 1, &Default::default()).unwrap();
    let source = std::str::from_utf8(&svg).unwrap();
    assert!(source.contains("data:image/png;base64,"));
    assert!(
        source.contains("<path"),
        "Foreground remains vector artwork"
    );
    let rendered =
        crate::render::raster::render(source, [100, 100], 1., false, false, &Default::default())
            .unwrap();
    let svg_pixels = image::load_from_memory(&rendered).unwrap().into_rgba8();
    let pixels = png(&job, 1);
    for (x, y) in [(49, 10), (49, 35), (49, 50), (80, 50)] {
        assert_eq!(pixels.get_pixel(x, y), svg_pixels.get_pixel(x, y));
    }
}

#[test]
fn background_blur_respects_rotated_rounded_and_elliptical_contours_and_board_clips() {
    for kind in [ShapeKind::Rectangle, ShapeKind::Ellipse] {
        let mut doc = background_blur_scene();
        doc.shapes[2].kind = kind;
        doc.shapes[2].layer.rotation = 45.;
        doc.shapes[2].rect = rect(30., 10., 40., 80.);
        let pixels = png(&job(&doc, rect(0., 0., 100., 100.)), 1);
        assert_eq!(pixels.get_pixel(20, 20).0, [255, 0, 0, 255]);
        let blurred = pixels.get_pixel(49, 40).0;
        assert!(blurred[0] < 180 && blurred[2] > 80, "{kind:?}: {blurred:?}");
    }
    let mut rounded = background_blur_scene();
    rounded.shapes[2].rect = rect(40., 20., 40., 60.);
    rounded.shapes[2].radius = 20.;
    let pixels = png(&job(&rounded, rect(0., 0., 100., 100.)), 1);
    assert_eq!(pixels.get_pixel(49, 21).0, [255, 0, 0, 255]);
    assert!(pixels.get_pixel(49, 50).0[2] > 80);
    let doc = background_blur_scene();
    let mut job = job(&doc, rect(0., 0., 100., 100.));
    job.clips.insert(3, rect(0., 40., 100., 60.));
    let pixels = png(&job, 1);
    assert_eq!(pixels.get_pixel(49, 30).0, [255, 0, 0, 255]);
    assert!(pixels.get_pixel(49, 50).0[2] > 80);
}

#[test]
fn background_blur_replaces_transparent_backdrop_without_doubling_alpha() {
    let mut doc = background_blur_scene();
    doc.shapes[0].color = gpui::rgba(0xff000080);
    doc.shapes[1].fill_enabled = false;
    let pixels = png(&job(&doc, rect(0., 0., 100., 100.)), 1);
    assert_eq!(pixels.get_pixel(49, 10).0[3], 128);
    let edge = pixels.get_pixel(49, 50).0;
    assert!(edge[3] > 55 && edge[3] < 80, "{edge:?}");
    assert_eq!(&edge[..3], &[255, 0, 0]);
    let mut second = doc.shapes[2].clone();
    second.id = 4;
    second.uid = uuid::Uuid::new_v4();
    doc.shapes.push(second);
    doc.hierarchy
        .effects
        .insert(4, doc.hierarchy.effects[&3].clone());
    let pixels = png(&job(&doc, rect(0., 0., 100., 100.)), 1);
    let edge = pixels.get_pixel(49, 50).0;
    assert!(edge[3] > 55 && edge[3] < 80, "Stacked: {edge:?}");
    assert_eq!(&edge[..3], &[255, 0, 0]);
}

#[test]
fn background_blur_keeps_antialiased_outline_opaque_and_disabled_effect_is_inert() {
    let mut doc = background_blur_scene();
    doc.shapes[0].rect.width = 100.;
    doc.shapes[1].fill_enabled = false;
    doc.shapes[2].rect = rect(30.5, 30.5, 39., 39.);
    doc.shapes[2].radius = 10.;
    doc.shapes[2].layer.rotation = 30.;
    let bounds = rect(0., 0., 100., 100.);
    let pixels = png(&job(&doc, bounds), 1);
    for y in 20..80 {
        for x in 20..80 {
            let p = pixels.get_pixel(x, y).0;
            assert!(p[3] >= 254, "Transparent seam at {x},{y}: {p:?}");
        }
    }
    doc.hierarchy.effects.get_mut(&3).unwrap()[0].toggle();
    let export = job(&doc, bounds);
    assert!(!export.svg().unwrap().contains("background-replace"));
    assert_eq!(png(&export, 1).get_pixel(30, 30).0, [255, 0, 0, 255]);
}

#[test]
fn transparent_png_scales_rotated_content_and_svg_stays_vector() {
    let mut shape = Shape::new(1, None, ShapeKind::Rectangle, rect(100., 100., 80., 40.));
    shape.color = rgb(0xff0000);
    shape.layer.rotation = 90.;
    let job = job(&document(vec![shape]), rect(120., 80., 40., 80.));
    let image = png(&job, 2);
    assert_eq!(image.dimensions(), (80, 160));
    assert_eq!(image.get_pixel(40, 80).0, [255, 0, 0, 255]);
    let svg = String::from_utf8(job.render(Format::Svg, 1, &Default::default()).unwrap()).unwrap();
    assert!(svg.contains("<path"));
    assert!(!svg.contains("<image"));
    let mut ellipse = Shape::new(1, None, ShapeKind::Ellipse, rect(0., 0., 40., 40.));
    ellipse.color = rgb(0xff0000);
    let image = png(&job_for_shape(ellipse), 1);
    assert_eq!(image.get_pixel(0, 0).0[3], 0);
    assert_eq!(image.get_pixel(20, 20).0, [255, 0, 0, 255]);
}
fn job_for_shape(shape: Shape) -> Job {
    let bounds = shape.rect;
    job(&document(vec![shape]), bounds)
}

#[test]
fn shape_stroke_rings_preserve_inside_center_and_outside_alignment() {
    for (align, bounds, edge, interior) in [
        (
            StrokeAlign::Inside,
            rect(0., 0., 40., 40.),
            (2, 20),
            (12, 20),
        ),
        (
            StrokeAlign::Center,
            rect(-5., -5., 50., 50.),
            (2, 25),
            (12, 25),
        ),
        (
            StrokeAlign::Outside,
            rect(-10., -10., 60., 60.),
            (2, 30),
            (12, 30),
        ),
    ] {
        let mut shape = Shape::new(1, None, ShapeKind::Rectangle, rect(0., 0., 40., 40.));
        shape.fill_enabled = false;
        shape.stroke.enabled = true;
        shape.stroke.align = align;
        shape.stroke.width = 10.;
        shape.stroke.color = rgb(0x0000ff);
        let image = png(&job(&document(vec![shape]), bounds), 1);
        assert_eq!(image.get_pixel(edge.0, edge.1).0, [0, 0, 255, 255]);
        assert_eq!(image.get_pixel(interior.0, interior.1).0[3], 0);
    }
}

#[test]
fn artboard_bounds_clip_children_without_editor_checkerboard() {
    let mut doc = document(vec![Shape::new(
        2,
        Some(1),
        ShapeKind::Rectangle,
        rect(-20., 10., 50., 30.),
    )]);
    doc.shapes[0].color = rgb(0xff0000);
    let bounds = rect(100., 200., 100., 80.);
    doc.boards.push(Artboard {
        uid: uuid::Uuid::new_v4(),
        color_style: None,
        id: 1,
        layer: Default::default(),
        name: "Board".into(),
        rect: bounds,
        color: gpui::rgba(0),
        fill_mode: FillMode::Solid,
        gradient: Default::default(),
        image_fill: Default::default(),
    });
    let mut job = job(&doc, bounds);
    job.clips.insert(2, bounds);
    let image = png(&job, 1);
    assert_eq!(image.dimensions(), (100, 80));
    assert_eq!(image.get_pixel(0, 15).0, [255, 0, 0, 255]);
    assert_eq!(image.get_pixel(30, 15).0[3], 0);
    assert_eq!(image.get_pixel(90, 70).0[3], 0);
    // The same clip must remain effective when a containing group's bounds extend beyond the board.
    job.bounds = rect(80., 190., 140., 100.);
    let image = png(&job, 1);
    assert_eq!(image.get_pixel(10, 25).0[3], 0);
    assert_eq!(image.get_pixel(21, 25).0, [255, 0, 0, 255]);
}

#[test]
fn point_gradient_export_roundtrips_color_alpha_and_contour_at_multiple_scales() {
    use crate::scene::point_gradient::PointGradient;
    let mut gradient = PointGradient::default();
    gradient.points[0].position = gpui::point(0.3, 0.2);
    gradient.points[0].radius = 0.25;
    gradient.points[1].color.a = 0.;
    gradient.points[2].color.a = 0.35;
    let mut shape = Shape::new(1, None, ShapeKind::Ellipse, rect(90., -20., 120., 80.));
    shape.fill_mode = FillMode::Points(gradient);
    let doc = document(vec![shape]);
    let restored: Page = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.shapes[0].fill_mode, FillMode::Points(gradient));
    let job = job(&restored, rect(90., -20., 120., 80.));
    let svg = String::from_utf8(job.render(Format::Svg, 1, &Default::default()).unwrap()).unwrap();
    assert!(svg.contains("data:image/png;base64,"));
    for scale in [1, 2, 4] {
        let image = png(&job, scale);
        assert_eq!(image.get_pixel(0, 0).0[3], 0);
        for (x, y) in [(30, 30), (60, 40), (90, 45)] {
            let (x, y) = (x * scale, y * scale);
            let expected = gradient.sample(
                gpui::point(
                    (x as f32 + 0.5) / (120 * scale) as f32,
                    (y as f32 + 0.5) / (80 * scale) as f32,
                ),
                120.,
                80.,
            );
            let actual = image.get_pixel(x, y).0;
            for (a, b) in actual
                .into_iter()
                .zip([expected.r, expected.g, expected.b, expected.a])
            {
                assert!(
                    (a as f32 - b * 255.).abs() < 5.,
                    "scale {scale}: {actual:?} vs {expected:?}"
                );
            }
        }
    }
    let mut invalid = restored;
    if let FillMode::Points(g) = &mut invalid.shapes[0].fill_mode {
        g.points[0].radius = 0.;
    }
    assert!(invalid.validate().is_err());
}

#[test]
fn contour_glow_exports_preserve_holes_source_color_and_layer_opacity() {
    use crate::scene::effects::{Effect, Glow};
    let mut shape = Shape::new(1, None, ShapeKind::Rectangle, rect(20., 20., 24., 24.));
    shape.fill_enabled = false;
    shape.stroke.enabled = true;
    shape.stroke.width = 4.;
    shape.stroke.color = rgb(0xff0000);
    shape.layer.opacity = 0.5;
    let mut doc = document(vec![shape]);
    doc.hierarchy.effects.insert(
        1,
        vec![Effect::Glow(Glow {
            radius: 8.,
            color: rgb(0x00ff00),
            ..Default::default()
        })],
    );
    let job = job(&doc, rect(0., 0., 64., 64.));
    let svg = String::from_utf8(job.render(Format::Svg, 1, &Default::default()).unwrap()).unwrap();
    assert!(svg.contains("data:image/png;base64,"));
    for scale in [1, 2, 4] {
        let pixels = png(&job, scale);
        let pixel = |x, y| pixels.get_pixel(x * scale, y * scale).0;
        assert_eq!(pixel(32, 32)[3], 0, "hole must remain transparent");
        assert_eq!(pixel(0, 0)[3], 0);
        let source = pixel(21, 32);
        assert!(
            source[0] >= 250 && source[1] == 0 && source[3].abs_diff(128) <= 1,
            "{source:?}"
        );
        for x in [18, 26] {
            let glow = pixel(x, 32);
            assert!(
                glow[1] >= 250 && glow[0] == 0 && glow[3] > 0 && glow[3] < 128,
                "scale {scale}: {glow:?}"
            );
        }
    }
}

#[test]
fn all_gradient_kinds_match_expected_colors_and_have_no_sector_seams() {
    for kind in [
        GradientKind::Linear,
        GradientKind::Radial,
        GradientKind::Angular,
        GradientKind::Diamond,
    ] {
        let mut shape = Shape::new(1, None, ShapeKind::Rectangle, rect(0., 0., 100., 100.));
        shape.fill_mode = FillMode::Linear;
        shape.gradient.kind = kind;
        shape.gradient.stop_mut(0).unwrap().color = rgb(0xff0000);
        shape.gradient.stop_mut(1).unwrap().color = rgb(0x0000ff);
        let image = png(&job_for_shape(shape), 1);
        assert!(image.pixels().all(|p| p.0[3] == 255), "{kind:?} has seams");
        let expected = match kind {
            GradientKind::Linear => [126, 0, 129],
            GradientKind::Radial => [251, 0, 4],
            GradientKind::Angular => [224, 0, 31],
            GradientKind::Diamond => [250, 0, 5],
        };
        let actual = image.get_pixel(50, 50).0;
        for i in 0..3 {
            assert!(
                (actual[i] as i32 - expected[i]).abs() < 5,
                "{kind:?}: {actual:?}"
            );
        }
    }
}

#[test]
fn gradient_midpoint_survives_serialization_and_controls_exported_color() {
    for midpoint in [0.01, 0.2, 0.5, 0.8, 0.99] {
        let mut shape = Shape::new(1, None, ShapeKind::Rectangle, rect(0., 0., 100., 100.));
        shape.fill_mode = FillMode::Linear;
        shape.gradient.stop_mut(0).unwrap().color = rgb(0xff0000);
        shape.gradient.stop_mut(1).unwrap().color = rgb(0x0000ff);
        shape.gradient.set_midpoint(0, midpoint);
        let saved = serde_json::to_vec(&shape).unwrap();
        let restored: Shape = serde_json::from_slice(&saved).unwrap();
        assert_eq!(restored.gradient, shape.gradient);
        let gradient = restored.gradient.clone();
        let image = png(&job_for_shape(restored), 1);
        for x in [1, 10, 20, 40, 60, 80, 98] {
            let expected = gradient.sample((x as f32 + 0.5) / 100.);
            let actual = image.get_pixel(x, 50).0;
            for (channel, expected) in [expected.r, expected.g, expected.b].into_iter().enumerate()
            {
                assert!(
                    (actual[channel] as f32 - expected * 255.).abs() < 4.,
                    "midpoint={midpoint}, x={x}, actual={actual:?}, expected={expected}"
                );
            }
        }
    }
}

#[test]
fn twenty_stop_gradient_survives_storage_gpui_conversion_and_export() {
    let mut shape = Shape::new(1, None, ShapeKind::Rectangle, rect(0., 0., 380., 100.));
    shape.fill_mode = FillMode::Linear;
    for index in 1..19 {
        shape.gradient.add_stop_at(index as f32 / 19.).unwrap();
    }
    let ids: Vec<_> = shape.gradient.stops().iter().map(|stop| stop.id).collect();
    for (index, id) in ids.into_iter().enumerate() {
        shape.gradient.stop_mut(id).unwrap().color = if index % 2 == 0 {
            rgb(0xff0000)
        } else {
            rgb(0x0000ff)
        };
        if index < 19 {
            assert!(shape.gradient.set_midpoint(id, 0.25));
        }
    }
    let page = document(vec![shape]);
    let restored: Page = serde_json::from_slice(&serde_json::to_vec(&page).unwrap()).unwrap();
    restored.validate().unwrap();
    let gradient = &restored.shapes[0].gradient;
    assert_eq!(gradient, &page.shapes[0].gradient);
    let background = gradient.background();
    assert_eq!(background.gradient_stops().len(), 20);
    for (index, stop) in gradient.stops().iter().enumerate() {
        assert_eq!(
            background.gradient_stops()[index],
            gpui::linear_color_stop(stop.color, stop.position)
        );
        if index < 19 {
            assert_eq!(background.gradient_midpoint_at(index), Some(0.25));
        }
    }
    let image = png(&job(&restored, restored.shapes[0].rect), 1);
    for x in (5..380).step_by(10) {
        let expected = gradient.sample((x as f32 + 0.5) / 380.);
        let actual = image.get_pixel(x, 50).0;
        for (channel, expected) in [expected.r, expected.g, expected.b].into_iter().enumerate() {
            assert!(
                (actual[channel] as f32 - expected * 255.).abs() < 4.,
                "x={x}, actual={actual:?}, expected={expected}"
            );
        }
    }
}

#[test]
fn angular_seam_export_blends_across_wrap_and_preserves_the_palette() {
    for angle in [90., 270.] {
        let mut shape = Shape::new(1, None, ShapeKind::Rectangle, rect(0., 0., 256., 256.));
        shape.fill_mode = FillMode::Linear;
        shape.gradient.kind = GradientKind::Angular;
        shape.gradient.angle = angle;
        shape.gradient.stop_mut(0).unwrap().color = rgb(0xdd0099);
        shape.gradient.stop_mut(1).unwrap().color = rgb(0x220088);
        let stops = shape.gradient.stops().to_vec();
        let smooth = png(&job_for_shape(shape.clone()), 1);
        shape.gradient.seam_width = 0.;
        let hard = png(&job_for_shape(shape.clone()), 1);
        let x = if angle == 90. { 230 } else { 25 };
        let difference = |image: &image::RgbaImage| {
            let a = image.get_pixel(x, 127).0;
            let b = image.get_pixel(x, 128).0;
            (a[0] as i16 - b[0] as i16).abs()
        };
        assert!(difference(&hard) > 100);
        assert!(difference(&smooth) < 12);
        assert_eq!(shape.gradient.stops(), stops);
    }
}

#[test]
fn image_export_embeds_full_pixels_and_preserves_fit_opacity_and_clipping() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.png");
    image::RgbaImage::from_pixel(40, 20, image::Rgba([23, 87, 145, 255]))
        .save(&source)
        .unwrap();
    let asset = crate::media::MediaAsset::load_image(&source).unwrap();
    std::fs::remove_file(&source).unwrap();
    let mut shape = Shape::new(1, None, ShapeKind::Ellipse, rect(0., 0., 40., 40.));
    shape.fill_mode = FillMode::Image;
    shape.image_fill.fit = crate::scene::image_fill::ImageFit::Contain;
    shape.image_fill.opacity = 0.5;
    let mut doc = document(vec![shape]);
    doc.assets.push(AssetUse {
        object: 1,
        fill: true,
        hash: asset.hash.clone(),
    });
    let mut job = job(&doc, rect(0., 0., 40., 40.));
    job.assets = Arc::new(vec![AssetSource {
        hash: asset.hash.clone(),
        name: asset.name(),
        path: asset.source.clone(),
        size: [asset.width, asset.height],
    }]);
    let pixels = png(&job, 1);
    let center = pixels.get_pixel(20, 20).0;
    assert!((center[0] as i32 - 23).abs() <= 1);
    assert!((center[2] as i32 - 145).abs() <= 2, "{center:?}");
    assert!((center[3] as i32 - 128).abs() <= 1);
    assert_eq!(pixels.get_pixel(20, 2).0[3], 0);
    assert_eq!(pixels.get_pixel(0, 10).0[3], 0);
    let svg = String::from_utf8(job.render(Format::Svg, 1, &Default::default()).unwrap()).unwrap();
    assert!(svg.contains("data:image/png;base64,"));
    assert!(!svg.contains("source.png"));
}

#[test]
fn svg_text_is_outlined_and_png_contains_the_glyphs() {
    let style = TextStyle {
        family: "DejaVu Sans".into(),
        size: 24.,
        color: rgb(0xff0000),
        ..Default::default()
    };
    let bounds = rect(0., 0., 200., 50.);
    let mut doc = document(vec![]);
    doc.texts.push(Text {
        uid: uuid::Uuid::new_v4(),
        id: 1,
        board: None,
        rect: bounds,
        layer: Default::default(),
        content: "Hello".into(),
        styles: StyledText {
            default: style.clone(),
            runs: vec![crate::scene::text::styles::StyleRun {
                range: 0..5,
                style: style.clone(),
            }],
        },
    });
    let mut job = job(&doc, bounds);
    job.text.insert(
        1,
        vec![TextFragment {
            text: "Hello".into(),
            x: 5.,
            baseline: 30.,
            style,
        }],
    );
    let options = crate::render::export::render_options(true).unwrap();
    let svg = String::from_utf8(job.render(Format::Svg, 1, &options).unwrap()).unwrap();
    assert!(!svg.contains("<text"));
    assert!(svg.contains("<path"));
    let pixels = image::load_from_memory(&job.render(Format::Png, 1, &options).unwrap())
        .unwrap()
        .into_rgba8();
    assert!(pixels.pixels().filter(|p| p.0[3] > 100).count() > 150);
}

#[test]
fn cropped_media_and_image_fills_export_the_same_selected_source_region() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("two-colors.png");
    image::RgbaImage::from_fn(200, 100, |x, _| {
        if x < 100 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 255, 255])
        }
    })
    .save(&path)
    .unwrap();
    let asset = crate::media::MediaAsset::load_image(&path).unwrap();
    for media in [false, true] {
        let mut shape = Shape::new(
            1,
            None,
            if media {
                ShapeKind::Image
            } else {
                ShapeKind::Ellipse
            },
            rect(0., 0., 100., 100.),
        );
        let placement = crate::scene::image_fill::Placement {
            zoom: 2.,
            offset: [0.5, 0.],
        };
        if media {
            shape.media_placement = placement;
            shape.radius = 12.;
        } else {
            shape.fill_mode = FillMode::Image;
            shape.image_fill.placement = placement;
        }
        let mut doc = document(vec![shape]);
        doc.assets.push(AssetUse {
            object: 1,
            fill: !media,
            hash: asset.hash.clone(),
        });
        let mut export = job(&doc, rect(0., 0., 100., 100.));
        export.assets = Arc::new(vec![AssetSource {
            hash: asset.hash.clone(),
            name: asset.name(),
            path: asset.source.clone(),
            size: [200, 100],
        }]);
        let pixels = png(&export, 1);
        assert_eq!(pixels.get_pixel(50, 50).0, [255, 0, 0, 255]);
        assert_eq!(pixels.get_pixel(0, 0).0[3], 0);
        let svg = export.render(Format::Svg, 1, &Default::default()).unwrap();
        let tree = resvg::usvg::Tree::from_data(&svg, &Default::default()).unwrap();
        let mut raster = resvg::tiny_skia::Pixmap::new(100, 100).unwrap();
        resvg::render(&tree, Default::default(), &mut raster.as_mut());
        assert_eq!(
            &raster.data()[(50 * 100 + 50) * 4..(50 * 100 + 50) * 4 + 4],
            &[255, 0, 0, 255]
        );
    }
}

#[test]
fn batch_export_keeps_existing_files_numbers_duplicates_and_is_atomic_on_render_failure() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Export.png"), b"original").unwrap();
    let make = || {
        job_for_shape(Shape::new(
            1,
            None,
            ShapeKind::Rectangle,
            rect(0., 0., 10., 10.),
        ))
    };
    let outputs = write(vec![make(), make()], dir.path().into(), true).unwrap();
    assert_eq!(
        outputs,
        [
            dir.path().join("Export (1).png"),
            dir.path().join("Export (2).png")
        ]
    );
    assert_eq!(
        std::fs::read(dir.path().join("Export.png")).unwrap(),
        b"original"
    );
    let invalid = job_for_shape(Shape::new(
        1,
        None,
        ShapeKind::Video,
        rect(0., 0., 10., 10.),
    ));
    assert!(write(vec![make(), invalid], dir.path().into(), true).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
    assert!(write(vec![make()], dir.path().join("image.svg"), false).is_err());
    let mut huge = make();
    huge.bounds.width = 20_000.;
    assert!(write(vec![huge], dir.path().join("huge.png"), false).is_err());
    assert!(!dir.path().join("huge.png").exists());
}

#[test]
fn original_video_export_copies_cached_bytes_and_mixed_batch_keeps_each_extension() {
    let source = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    let bytes = b"\x00\x00\x00\x18ftypmp42\0\xff\x80video-and-audio-payload";
    std::fs::write(&source, bytes).unwrap();
    let source = AssetSource {
        hash: "a".repeat(64),
        name: "clip.MP4".into(),
        path: crate::media::Source::file(source),
        size: [1, 1],
    };
    let video = || {
        let mut job = job_for_shape(Shape::new(
            1,
            None,
            ShapeKind::Video,
            rect(0., 0., 180., 320.),
        ));
        job.name = "clip.MP4".into();
        job.original = Some(source.clone());
        job
    };
    let folder = tempfile::tempdir().unwrap();
    // No decoding, resizing, frame extraction, or re-encoding is involved.
    let path = folder.path().join("clip.mp4");
    let mut original = video();
    original.preset.format = Format::Svg;
    original.preset.scale = 4;
    write(vec![original], path.clone(), false).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut image = job_for_shape(Shape::new(
        2,
        None,
        ShapeKind::Rectangle,
        rect(0., 0., 10., 20.),
    ));
    image.preset.scale = 2;
    let paths = write(vec![video(), video(), image], folder.path().into(), true).unwrap();
    assert_eq!(
        paths,
        vec![
            folder.path().join("clip (1).mp4"),
            folder.path().join("clip (2).mp4"),
            folder.path().join("Export.png")
        ]
    );
    assert_eq!(std::fs::read(&paths[1]).unwrap(), bytes);
    assert_eq!(image::open(&paths[2]).unwrap().width(), 20);
    assert!(write(vec![video()], folder.path().join("wrong.png"), false).is_err());
    let mut missing = video();
    missing.original.as_mut().unwrap().path =
        crate::media::Source::file(tempfile::NamedTempFile::new().unwrap().into_temp_path());
    std::fs::remove_file(
        &*missing
            .original
            .as_ref()
            .unwrap()
            .path
            .cached_path()
            .unwrap(),
    )
    .unwrap();
    assert!(write(vec![video(), missing], folder.path().into(), true).is_err());
    assert!(!folder.path().join("clip (3).mp4").exists());
}
