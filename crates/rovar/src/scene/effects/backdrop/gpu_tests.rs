use gpui::{
    BackdropBlur, BackdropShader, Bounds, ContentMask, DevicePixels, EffectUniforms, Quad,
    ScaledPixels, Scene, point, rgb, size,
};
use gpui_wgpu::{WgpuContext, WgpuExternalRendererConfig, WgpuRenderer, wgpu};

fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<ScaledPixels> {
    Bounds::new(
        point(ScaledPixels(x), ScaledPixels(y)),
        size(ScaledPixels(width), ScaledPixels(height)),
    )
}

// This uses the native-window alpha mode and a transparent window surround.
// An opaque offscreen clear hides the regression by making every sample opaque.
#[test]
#[ignore = "requires a working WGPU adapter; run explicitly for backdrop changes"]
fn gpu_background_blur_replaces_detail_even_with_transparent_window_margins() {
    let context = WgpuContext::new_headless().unwrap();
    let mut renderer = WgpuRenderer::new_external(
        &context,
        WgpuExternalRendererConfig {
            size: size(DevicePixels(200), DevicePixels(200)),
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            target_usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        },
    )
    .unwrap();
    let extent = wgpu::Extent3d {
        width: 200,
        height: 200,
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
        size: 1024 * 200,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    for radius in [0., 256., 2048.] {
        let viewport = bounds(0., 0., 200., 200.);
        let canvas = bounds(50., 50., 100., 100.);
        let mut scene = Scene::default();
        for x in 0..5 {
            scene.insert_primitive(Quad {
                bounds: bounds(50. + x as f32 * 20., 50., 20., 100.),
                content_mask: ContentMask { bounds: canvas },
                background: rgb(if x % 2 == 0 { 0xffffff } else { 0 }).into(),
                ..Default::default()
            });
        }
        if radius > 0. {
            scene.insert_primitive(BackdropBlur {
                order: 0,
                bounds: canvas,
                content_mask: ContentMask { bounds: viewport },
                corner_radii: Default::default(),
                blur_radius: ScaledPixels(radius),
                opacity: 1.,
                shader: Some(BackdropShader::wgsl(include_str!(
                    "../background_blur.wgsl"
                ))),
                uniforms: EffectUniforms::default()
                    .with_slot(0, [100., 100., 1., 0.])
                    .with_slot(2, [0., radius, 0., 0.])
                    .with_slot(3, [50., 50., 150., 150.]),
                time: 0.,
                pointer: point(0., 0.),
                pointer_active: false,
            });
        }
        scene.finish();
        assert!(renderer.draw_external(&scene, &target, &view, wgpu::Color::TRANSPARENT));
        let mut encoder = context.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(1024),
                    rows_per_image: Some(200),
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
        let bytes = readback.get_mapped_range(..).unwrap();
        let values: Vec<_> = (80..120).map(|x| bytes[100 * 1024 + x * 4]).collect();
        let contrast = values.iter().max().unwrap() - values.iter().min().unwrap();
        if radius == 0. {
            assert_eq!(contrast, 255);
        } else {
            assert!(
                contrast < 40,
                "radius {radius} retained sharp detail: {values:?}"
            );
            assert!((80..120).all(|x| bytes[100 * 1024 + x * 4 + 3] == 255));
        }
        assert_eq!(
            bytes[20 * 1024 + 20 * 4 + 3],
            0,
            "Do not paint outside the outline"
        );
        drop(bytes);
        readback.unmap();
    }
}
