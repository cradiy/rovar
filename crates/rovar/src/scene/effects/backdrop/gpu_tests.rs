use super::{Filter, Region};
use crate::scene::artboard::Rect;
use gpui::{
    Bounds, ContentMask, DevicePixels, EffectQuad, Quad, ScaledPixels, Scene, point, px, rgb, size,
};
use gpui_wgpu::{WgpuContext, WgpuExternalRendererConfig, WgpuRenderer, wgpu};

fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<ScaledPixels> {
    Bounds::new(
        point(ScaledPixels(x), ScaledPixels(y)),
        size(ScaledPixels(width), ScaledPixels(height)),
    )
}

fn filtered(source: Scene, filter: Filter) -> Scene {
    let viewport = bounds(0., 0., 512., 512.);
    let pass = filter.pass(
        point(px(0.), px(0.)),
        Bounds::new(point(px(0.), px(0.)), size(px(512.), px(512.))),
        1.,
    );
    let composite = EffectQuad {
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
    };
    let mut scene = Scene::default();
    scene.insert_primitive(gpui::Primitive::SubtreeLayer(gpui::SubtreeLayer {
        composite,
        scene: std::rc::Rc::new(source),
        second_scene: None,
        scene3d: None,
        intermediate_effects: vec![pass].into(),
    }));
    scene.finish();
    scene
}

// A premultiplied target matches native windows and exposes transparent black
// being accidentally turned opaque by a backdrop inside an isolated layer.
#[test]
#[ignore = "requires a working WGPU adapter; run explicitly for backdrop changes"]
fn gpu_background_blur_preserves_alpha_and_world_radius_across_zoom() {
    let context = WgpuContext::new_headless().unwrap();
    let mut renderer = WgpuRenderer::new_external(
        &context,
        WgpuExternalRendererConfig {
            size: size(DevicePixels(512), DevicePixels(512)),
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            target_usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        },
    )
    .unwrap();
    let extent = wgpu::Extent3d {
        width: 512,
        height: 512,
        depth_or_array_layers: 1,
    };
    let target = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("backdrop regression"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let readback = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 2048 * 512,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut render = |scene: &Scene| {
        assert!(renderer.draw_external(scene, &target, &view, wgpu::Color::TRANSPARENT));
        let mut encoder = context.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(2048),
                    rows_per_image: Some(512),
                },
            },
            extent,
        );
        context.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            tx.send(result).unwrap();
        });
        context
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
        rx.recv().unwrap().unwrap();
        let bytes = readback.get_mapped_range(..).unwrap().to_vec();
        readback.unmap();
        bytes
    };
    for radius in [12., 80., 256.] {
        let mut reference: Option<Vec<u8>> = None;
        for zoom in [1., 2., 4.] {
            let filter = Filter {
                region: Region {
                    rect: Rect {
                        x: 8. * zoom,
                        y: 8. * zoom,
                        width: 112. * zoom,
                        height: 112. * zoom,
                    },
                    rotation: 0.,
                    corners: [8. * zoom; 4],
                    ellipse: false,
                },
                radius: radius * zoom,
                opacity: 1.,
                clip: None,
            };
            let mut empty = Scene::default();
            empty.finish();
            assert!(
                render(&filtered(empty, filter)).iter().all(|v| *v == 0),
                "empty backdrop became opaque"
            );
            let mut source = Scene::default();
            let region = bounds(48. * zoom, 48. * zoom, 32. * zoom, 32. * zoom);
            source.insert_primitive(Quad {
                bounds: region,
                content_mask: ContentMask { bounds: region },
                background: rgb(0xffffff).into(),
                ..Default::default()
            });
            source.finish();
            let backdrop = std::rc::Rc::new(filtered(source, filter));
            let pixels = render(&backdrop);
            let profile: Vec<u8> = (36..=92)
                .map(|x| {
                    let offset = ((64. * zoom) as usize * 512 + (x as f32 * zoom) as usize) * 4;
                    pixels[offset + 3]
                })
                .collect();
            if radius == 12. {
                assert!(
                    profile[12] > 80 && profile[12] < 200,
                    "sharp source leaked through: {profile:?}"
                );
                assert!(profile[4] > 0, "blur did not spread beyond source");
            }
            if radius == 80. {
                assert!(
                    profile[28] > 15 && profile[28] < 35,
                    "incorrect Gaussian radius: {profile:?}"
                );
            }
            if let Some(reference) = &reference {
                let error = profile
                    .iter()
                    .zip(reference)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(
                    error <= 8,
                    "radius {radius}, zoom {zoom}: alpha drift {error}: {profile:?} versus {reference:?}"
                );
            } else {
                reference = Some(profile);
            }
            assert_eq!(pixels[(4 * 512 + 4) * 4 + 3], 0, "paint escaped outline");
            // Exercise the actual nested capture used by a translucent layer
            // with a blend mode above a background blur.
            for mode in crate::scene::blend::Mode::ALL {
                let mut foreground = Scene::default();
                let mut color = rgb(0xd9d9d9);
                color.a = 0.19;
                let viewport = bounds(0., 0., 512., 512.);
                foreground.insert_primitive(Quad {
                    bounds: viewport,
                    content_mask: ContentMask { bounds: viewport },
                    background: color.into(),
                    ..Default::default()
                });
                foreground.finish();
                let mut composite = backdrop.subtree_layers[0].composite.clone();
                composite.shader = gpui::EffectShader::wgsl_two_images(include_str!(
                    "../../../editor/blend/blend.wgsl"
                ));
                composite.uniforms =
                    gpui::EffectUniforms::default().with_slot(0, [mode as u32 as f32, 1., 0., 0.]);
                let mut scene = Scene::default();
                scene.insert_primitive(gpui::Primitive::SubtreeLayer(gpui::SubtreeLayer {
                    composite,
                    scene: backdrop.clone(),
                    second_scene: Some(std::rc::Rc::new(foreground)),
                    scene3d: None,
                    intermediate_effects: Default::default(),
                }));
                scene.finish();
                let blended = render(&scene);
                for x in [16., 48., 64., 80., 112.] {
                    let offset = ((64. * zoom) as usize * 512 + (x * zoom) as usize) * 4;
                    let expected_alpha = 0.19 * 255. + f32::from(pixels[offset + 3]) * 0.81;
                    assert!(
                        (f32::from(blended[offset + 3]) - expected_alpha).abs() <= 3.,
                        "{mode:?}: backdrop alpha changed in nested capture"
                    );
                    assert!(
                        blended[offset] > 50,
                        "{mode:?}: gray over white/transparent became black"
                    );
                }
            }
        }
    }
}
