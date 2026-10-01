use super::*;
use crate::{
    artboard::{Artboard, FillMode},
    document::{AssetUse, Text},
    layer::Hierarchy,
    shape::{Shape, ShapeKind, StrokeAlign},
    text::{TextStyle, styles::StyledText},
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
    shape.image_fill.fit = crate::image_fill::ImageFit::Contain;
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
        id: 1,
        board: None,
        rect: bounds,
        layer: Default::default(),
        content: "Hello".into(),
        styles: StyledText {
            default: style.clone(),
            runs: vec![crate::text::styles::StyleRun {
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
    let options = crate::component_export::render_options(true).unwrap();
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
        let placement = crate::image_fill::Placement {
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
    let outputs = write(
        vec![make(), make()],
        dir.path().into(),
        Format::Png,
        1,
        true,
    )
    .unwrap();
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
    assert!(
        write(
            vec![make(), invalid],
            dir.path().into(),
            Format::Png,
            1,
            true
        )
        .is_err()
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
    assert!(
        write(
            vec![make()],
            dir.path().join("image.svg"),
            Format::Png,
            1,
            false
        )
        .is_err()
    );
    let mut huge = make();
    huge.bounds.width = 20_000.;
    assert!(
        write(
            vec![huge],
            dir.path().join("huge.png"),
            Format::Png,
            1,
            false
        )
        .is_err()
    );
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
    write(vec![video()], path.clone(), Format::Svg, 4, false).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let image = job_for_shape(Shape::new(
        2,
        None,
        ShapeKind::Rectangle,
        rect(0., 0., 10., 20.),
    ));
    let paths = write(
        vec![video(), video(), image],
        folder.path().into(),
        Format::Png,
        2,
        true,
    )
    .unwrap();
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
    assert!(
        write(
            vec![video()],
            folder.path().join("wrong.png"),
            Format::Png,
            1,
            false
        )
        .is_err()
    );
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
    assert!(
        write(
            vec![video(), missing],
            folder.path().into(),
            Format::Png,
            1,
            true
        )
        .is_err()
    );
    assert!(!folder.path().join("clip (3).mp4").exists());
}
