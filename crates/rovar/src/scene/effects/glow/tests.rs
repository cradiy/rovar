use super::*;

#[test]
fn contour_glow_empty_input_threshold_and_holes() {
    let mut source = image::RgbaImage::new(64, 64);
    let mut glow = Glow {
        radius: 8.,
        ..Default::default()
    };
    let render = |source: &image::RgbaImage, glow: &Glow| {
        crate::render::svg::glow::rasterize(source, glow, 1.)
    };
    assert!(render(&source, &glow).pixels().all(|p| p.0[3] == 0));
    for y in 20..44 {
        for x in 20..44 {
            if !(24..40).contains(&x) || !(24..40).contains(&y) {
                source.put_pixel(x, y, image::Rgba([255, 0, 0, 100]));
            }
        }
    }
    assert!(render(&source, &glow).pixels().all(|p| p.0[3] == 0));
    glow.threshold = 0.2;
    let light = render(&source, &glow);
    assert!(light.get_pixel(18, 32).0[3] > 0);
    assert!(light.get_pixel(26, 32).0[3] > 0);
    assert_eq!(light.get_pixel(32, 32).0[3], 0);
    assert_eq!(light.get_pixel(0, 0).0[3], 0);
    glow.edge_width = 9.;
    assert!(glow.validate().is_err());
}

#[cfg(not(target_family = "wasm"))]
#[test]
#[ignore = "requires WGPU; run explicitly for contour glow changes"]
fn gpu_contour_glow_matches_export_at_multiple_scales() {
    use gpui::{
        Bounds, ContentMask, DevicePixels, EffectQuad, Quad, ScaledPixels, Scene, point, size,
    };
    for scale in [1, 2, 4] {
        let n = 64 * scale;
        let mut renderer =
            gpui_wgpu::WgpuOffscreenRenderer::new(size(DevicePixels(n), DevicePixels(n))).unwrap();
        let region = |x, y, w, h| {
            Bounds::new(
                point(ScaledPixels(x), ScaledPixels(y)),
                size(ScaledPixels(w), ScaledPixels(h)),
            )
        };
        let viewport = region(0., 0., n as f32, n as f32);
        let mut source = Scene::default();
        let mut pixels = image::RgbaImage::new(n as u32, n as u32);
        for (x, y, w, h) in [
            (20, 20, 24, 4),
            (20, 40, 24, 4),
            (20, 24, 4, 16),
            (40, 24, 4, 16),
        ] {
            let (x, y, w, h) = (x * scale, y * scale, w * scale, h * scale);
            let bounds = region(x as f32, y as f32, w as f32, h as f32);
            source.insert_primitive(Quad {
                bounds,
                content_mask: ContentMask { bounds },
                background: gpui::rgb(0xffffff).into(),
                ..Default::default()
            });
            for py in y..y + h {
                for px in x..x + w {
                    pixels.put_pixel(px as u32, py as u32, image::Rgba([255; 4]));
                }
            }
        }
        source.finish();
        let glow = Glow {
            radius: 8.,
            ..Default::default()
        };
        let cpu = crate::render::svg::glow::rasterize(&pixels, &glow, scale as f32);
        let mut scene = Scene::default();
        scene.insert_primitive(gpui::Primitive::SubtreeLayer(gpui::SubtreeLayer {
            composite: EffectQuad {
                order: 0,
                bounds: viewport,
                effect_bounds: viewport,
                content_mask: ContentMask { bounds: viewport },
                shader: gpui_effects::subtree_identity_shader(),
                uniforms: Default::default(),
                opacity: 1.,
                transformation: Default::default(),
                time: 0.,
                corner_radii: Default::default(),
                image_tile: None,
                second_image_tile: None,
                third_image_tile: None,
                fourth_image_tile: None,
            },
            scene: std::rc::Rc::new(source),
            second_scene: None,
            scene3d: None,
            intermediate_effects: vec![glow.pass(scale as f32)].into(),
        }));
        scene.finish();
        let actual = renderer.render_rgba(&scene).unwrap();
        for (x, y) in [(18, 32), (26, 32), (32, 32), (12, 12), (19, 19)] {
            let (x, y) = (x * scale, y * scale);
            let p = cpu.get_pixel(x as u32, y as u32).0;
            let color = gpui::Rgba {
                r: p[0] as f32 / 255.,
                g: p[1] as f32 / 255.,
                b: p[2] as f32 / 255.,
                a: p[3] as f32 / 255.,
            };
            let mut reference = Scene::default();
            reference.insert_primitive(Quad {
                bounds: viewport,
                content_mask: ContentMask { bounds: viewport },
                background: color.into(),
                ..Default::default()
            });
            reference.finish();
            let expected = renderer.render_rgba(&reference).unwrap();
            let offset = ((y * n + x) * 4) as usize;
            for c in 0..4 {
                assert!(
                    actual[offset + c].abs_diff(expected[offset + c]) <= 4,
                    "scale {scale} at {x}/{y}: {:?} vs {:?}",
                    &actual[offset..offset + 4],
                    &expected[offset..offset + 4]
                );
            }
        }
    }
}
