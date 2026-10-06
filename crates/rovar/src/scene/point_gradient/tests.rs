use super::*;

#[test]
fn transparent_sources_do_not_add_dark_fringes_and_small_radii_remain_finite() {
    let mut gradient = PointGradient::default();
    for p in &mut gradient.points {
        p.color = rgb(0xff0000);
        p.radius = 0.01;
    }
    for p in &mut gradient.points[1..] {
        p.color = gpui::rgba(0);
    }
    let c = gradient.sample(point(0.15, 0.2), 200., 100.);
    assert_eq!(c, rgb(0xff0000));
    let c = gradient.sample(point(0.5, 0.5), 200., 100.);
    assert!([c.r, c.g, c.b, c.a].into_iter().all(f32::is_finite));
    gradient.points[0].color.a = 0.;
    assert_eq!(gradient.sample(point(0.5, 0.5), 200., 100.), gpui::rgba(0));
}

#[cfg(not(target_family = "wasm"))]
#[test]
#[ignore = "requires a WGPU adapter; run explicitly for point gradient changes"]
fn gpu_point_gradient_matches_export_reference_across_zoom_and_alpha() {
    use gpui::{Bounds, ContentMask, DevicePixels, EffectQuad, Quad, ScaledPixels, Scene, size};
    for scale in [1, 2, 4] {
        let (width, height) = (80 * scale, 40 * scale);
        let mut renderer =
            gpui_wgpu::WgpuOffscreenRenderer::new(size(DevicePixels(width), DevicePixels(height)))
                .unwrap();
        let bounds = Bounds::new(
            point(ScaledPixels(0.), ScaledPixels(0.)),
            size(ScaledPixels(width as f32), ScaledPixels(height as f32)),
        );
        let mut gradient = PointGradient::default();
        gradient.points[0].radius = 0.2;
        gradient.points[1].color.a = 0.;
        gradient.points[2].color.a = 0.3;
        let mut scene = Scene::default();
        scene.insert_primitive(EffectQuad {
            order: 0,
            bounds,
            effect_bounds: bounds,
            content_mask: ContentMask { bounds },
            shader: gpui_effects::point_gradient_shader(),
            uniforms: gpui_effects::point_gradient_uniforms(gradient.gpu_points()),
            opacity: 1.,
            transformation: Default::default(),
            time: 0.,
            corner_radii: Default::default(),
            image_tile: None,
            second_image_tile: None,
            third_image_tile: None,
            fourth_image_tile: None,
        });
        scene.finish();
        let actual = renderer.render_rgba(&scene).unwrap();
        for (x, y) in [(10, 10), (30, 20), (60, 30)] {
            let (x, y) = (x * scale, y * scale);
            let c = gradient.sample(
                point(
                    (x as f32 + 0.5) / width as f32,
                    (y as f32 + 0.5) / height as f32,
                ),
                80.,
                40.,
            );
            let mut reference = Scene::default();
            reference.insert_primitive(Quad {
                bounds,
                content_mask: ContentMask { bounds },
                background: c.into(),
                ..Default::default()
            });
            reference.finish();
            let expected = renderer.render_rgba(&reference).unwrap();
            let offset = ((y * width + x) * 4) as usize;
            for channel in 0..4 {
                assert!(
                    actual[offset + channel].abs_diff(expected[offset + channel]) <= 3,
                    "scale {scale} at {x}/{y}: {:?} vs {:?}",
                    &actual[offset..offset + 4],
                    &expected[offset..offset + 4]
                );
            }
        }
    }
}
