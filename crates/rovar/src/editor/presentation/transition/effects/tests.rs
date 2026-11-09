use super::*;
use gpui::{ContentMask, DevicePixels, EffectQuad, Quad, ScaledPixels, Scene, size};
use gpui_wgpu::{WgpuContext, WgpuExternalRendererConfig, WgpuRenderer, wgpu};

#[test]
#[ignore = "requires WGPU; run explicitly for prototype transition shaders"]
fn gpu_transition_endpoints_directions_and_transparency() {
    let bounds = Bounds::new(
        point(ScaledPixels(0.), ScaledPixels(0.)),
        size(ScaledPixels(128.), ScaledPixels(64.)),
    );
    let source = |color| {
        let mut scene = Scene::default();
        scene.insert_primitive(Quad {
            bounds,
            content_mask: ContentMask { bounds },
            background: color,
            ..Default::default()
        });
        let marker = Bounds::new(
            point(ScaledPixels(8.), ScaledPixels(6.)),
            size(ScaledPixels(20.), ScaledPixels(12.)),
        );
        scene.insert_primitive(Quad {
            bounds: marker,
            content_mask: ContentMask { bounds },
            background: gpui::rgba(0x00ff0040).into(),
            ..Default::default()
        });
        scene.finish();
        Rc::new(scene)
    };
    let first = source(gpui::rgba(0xff000080).into());
    let second = source(gpui::rgba(0x0000ff80).into());
    let context = WgpuContext::new_headless().unwrap();
    let mut renderer = WgpuRenderer::new_external(
        &context,
        WgpuExternalRendererConfig {
            size: size(DevicePixels(128), DevicePixels(64)),
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            target_usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        },
    )
    .unwrap();
    let extent = wgpu::Extent3d {
        width: 128,
        height: 64,
        depth_or_array_layers: 1,
    };
    let target = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("prototype transitions"),
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
        size: 512 * 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut render = |effect, direction, progress| {
        let mut scene = Scene::default();
        scene.insert_primitive(gpui::Primitive::SubtreeLayer(gpui::SubtreeLayer {
            composite: EffectQuad {
                order: 0,
                bounds,
                effect_bounds: bounds,
                content_mask: ContentMask { bounds },
                shader: shader(),
                uniforms: uniforms(effect, direction, progress),
                opacity: 1.,
                transformation: Default::default(),
                time: 0.,
                corner_radii: Default::default(),
                image_tile: None,
                second_image_tile: None,
                third_image_tile: None,
                fourth_image_tile: None,
            },
            scene: first.clone(),
            second_scene: Some(second.clone()),
            scene3d: None,
            intermediate_effects: Vec::new().into(),
        }));
        scene.finish();
        assert!(renderer.draw_external(&scene, &target, &view, wgpu::Color::TRANSPARENT));
        let mut encoder = context.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(512),
                    rows_per_image: Some(64),
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
    let before = render(TransitionEffect::Slide, WipeDirection::Right, 0.);
    let after = render(TransitionEffect::Slide, WipeDirection::Right, 1.);
    assert_ne!(before, after);
    for effect in [
        TransitionEffect::Slide,
        TransitionEffect::Push,
        TransitionEffect::ZoomIn,
        TransitionEffect::ZoomOut,
        TransitionEffect::Rotate,
        TransitionEffect::Flip,
        TransitionEffect::Iris,
        TransitionEffect::Blinds,
        TransitionEffect::Pixelate,
    ] {
        for direction in [
            WipeDirection::Left,
            WipeDirection::Right,
            WipeDirection::Up,
            WipeDirection::Down,
        ] {
            assert_eq!(
                render(effect, direction, 0.),
                before,
                "{effect:?} {direction:?}"
            );
            assert_eq!(
                render(effect, direction, 1.),
                after,
                "{effect:?} {direction:?}"
            );
            let middle = render(effect, direction, 0.3);
            assert_ne!(middle, before, "{effect:?} {direction:?}");
            assert_ne!(middle, after, "{effect:?} {direction:?}");
            assert!(
                middle
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] > 0 && p[3] < 255),
                "{effect:?} must preserve transparency"
            );
        }
        if effect.directional() {
            assert_ne!(
                render(effect, WipeDirection::Right, 0.3),
                render(effect, WipeDirection::Left, 0.3),
                "{effect:?} direction must change the image"
            );
        }
    }
}
