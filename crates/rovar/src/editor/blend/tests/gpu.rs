use super::{Mode, expected};
use gpui::{
    Bounds, ContentMask, DevicePixels, EffectQuad, EffectUniforms, Quad, ScaledPixels, Scene,
    point, size,
};

fn bounds(x: f32, width: f32) -> Bounds<ScaledPixels> {
    Bounds::new(
        point(ScaledPixels(x), ScaledPixels(0.)),
        size(ScaledPixels(width), ScaledPixels(16.)),
    )
}

fn quad(bounds: Bounds<ScaledPixels>, color: gpui::Rgba) -> Quad {
    Quad {
        bounds,
        content_mask: ContentMask { bounds },
        background: color.into(),
        ..Default::default()
    }
}

#[test]
#[ignore = "requires a working WGPU adapter; run explicitly for blend changes"]
fn gpu_blend_matches_reference_pixels_with_transparent_inputs() {
    let mut renderer =
        gpui_wgpu::WgpuOffscreenRenderer::new(size(DevicePixels(32), DevicePixels(16))).unwrap();
    for mode in Mode::ALL {
        for (ba, sa, opacity) in [(1., 1., 1.), (0.4, 0.6, 0.5), (0., 1., 0.7), (1., 0., 1.)] {
            let mut b = gpui::rgb(0x4080c0);
            let mut s = gpui::rgb(0xc06040);
            b.a = ba;
            s.a = sa;
            let mut scene = Scene::default();
            let region = bounds(0., 32.);
            let composite = EffectQuad {
                order: 0,
                bounds: region,
                effect_bounds: region,
                content_mask: ContentMask { bounds: region },
                shader: super::super::paint::shader(),
                uniforms: EffectUniforms::default()
                    .with_slot(0, [mode as u32 as f32, opacity, 0., 0.]),
                opacity: 1.,
                transformation: Default::default(),
                time: 0.,
                corner_radii: Default::default(),
                image_tile: None,
                second_image_tile: None,
                third_image_tile: None,
                fourth_image_tile: None,
            };
            let mut background = Scene::default();
            background.insert_primitive(quad(bounds(0., 16.), b));
            background.finish();
            let mut foreground = Scene::default();
            foreground.insert_primitive(quad(bounds(8., 16.), s));
            foreground.finish();
            scene.insert_primitive(gpui::Primitive::SubtreeLayer(gpui::SubtreeLayer {
                composite,
                scene: std::rc::Rc::new(background),
                second_scene: Some(std::rc::Rc::new(foreground)),
                scene3d: None,
                intermediate_effects: Default::default(),
            }));
            scene.finish();
            let actual = renderer.render_rgba(&scene).unwrap();
            let a = sa * opacity;
            let alpha = a + ba * (1. - a);
            let channel = |b: f32, s: f32| {
                if alpha == 0. {
                    0.
                } else {
                    (a * (1. - ba) * s + a * ba * expected(mode, b, s) + (1. - a) * ba * b) / alpha
                }
            };
            let reference_color = gpui::Rgba {
                r: channel(b.r, s.r),
                g: channel(b.g, s.g),
                b: channel(b.b, s.b),
                a: alpha,
            };
            let mut reference = Scene::default();
            reference.insert_primitive(quad(bounds(0., 8.), b));
            reference.insert_primitive(quad(bounds(8., 8.), reference_color));
            s.a = a;
            reference.insert_primitive(quad(bounds(16., 8.), s));
            reference.finish();
            let expected = renderer.render_rgba(&reference).unwrap();
            for x in [4, 12, 20, 28] {
                let offset = (8 * 32 + x) * 4;
                for c in 0..4 {
                    assert!(
                        actual[offset + c].abs_diff(expected[offset + c]) <= 3,
                        "{mode:?}, alpha {ba}/{sa}/{opacity}, x={x}: actual {:?}, expected {:?}",
                        &actual[offset..offset + 4],
                        &expected[offset..offset + 4]
                    );
                }
            }
        }
    }
}
