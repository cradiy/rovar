use super::SvgAsset;
use gpui::{App, Pixels, RenderImage, Size, Window};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_EDGE: u32 = 2048;

#[derive(Default)]
pub(super) struct State {
    image: Option<Arc<crate::asset::ManagedImage>>,
    edge: u32,
    pending: bool,
    failed: Option<Instant>,
}

impl SvgAsset {
    pub(crate) fn image(
        &self,
        size: Size<Pixels>,
        window: &mut Window,
        cx: &App,
    ) -> Option<Arc<RenderImage>> {
        let edge = (f32::from(size.width).max(f32::from(size.height))
            * window.raster_scale_factor()
            * gpui::SMOOTH_SVG_SCALE_FACTOR)
            .ceil()
            .clamp(64., MAX_EDGE as f32) as u32;
        let edge = edge.next_power_of_two().min(MAX_EDGE);
        let mut state = self.raster.lock().ok()?;
        let image = state.image.as_ref().map(|image| image.render(cx));
        if state.edge >= edge
            || state.pending
            || state
                .failed
                .is_some_and(|time| time.elapsed() < Duration::from_secs(1))
        {
            return image;
        }
        state.pending = true;
        drop(state);
        let source = self.source.clone();
        let state = self.raster.clone();
        let renderer = cx.svg_renderer().clone();
        let scale = edge as f32 / self.size[0].max(self.size[1]) / gpui::SMOOTH_SVG_SCALE_FACTOR;
        let render = cx.background_executor().spawn(async move {
            renderer.render_single_frame_with_limits(
                &source,
                scale,
                gpui::ImageLoadLimits {
                    max_width: MAX_EDGE,
                    max_height: MAX_EDGE,
                    max_decoded_bytes: u64::from(MAX_EDGE) * u64::from(MAX_EDGE) * 4,
                    ..Default::default()
                },
            )
        });
        window
            .spawn(cx, async move |cx| {
                let result = render.await;
                // Publish even if the requesting window closed; shared assets must not remain pending.
                {
                    let Ok(mut state) = state.lock() else {
                        return;
                    };
                    state.pending = false;
                    match result {
                        Ok(image) => {
                            state.edge = edge;
                            state.failed = None;
                            state.image = Some(crate::asset::ManagedImage::new(image));
                        }
                        Err(error) => {
                            state.failed = Some(Instant::now());
                            eprintln!("Could not rasterize SVG: {error}");
                        }
                    }
                };
                let _ = cx.update(|window, cx| {
                    window.refresh();
                    cx.refresh_windows();
                });
            })
            .detach();
        // Resizing never discards the last completed raster. Geometry and clipping
        // are applied by the image painter independently of this resolution cache.
        image
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, px, size};

    #[gpui::test]
    fn zoom_keeps_the_previous_raster_until_the_larger_one_is_ready(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("icon.svg");
        std::fs::write(&path, "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='12'><rect width='24' height='12' fill='red'/></svg>").unwrap();
        let asset = crate::asset::FileAsset::load(path).unwrap();
        let cx = cx.add_empty_window();
        let image = |window: &mut Window, cx: &App, width| {
            let crate::asset::AssetContent::Svg(svg) = asset.content().unwrap() else {
                panic!("Expected SVG");
            };
            svg.image(size(px(width), px(width / 2.)), window, cx)
        };
        assert!(cx.update(|window, cx| image(window, cx, 24.)).is_none());
        cx.run_until_parked();
        let first = cx.update(|window, cx| image(window, cx, 24.)).unwrap();
        let during = cx.update(|window, cx| image(window, cx, 180.)).unwrap();
        assert!(Arc::ptr_eq(&first, &during));
        for width in [40., 240., 80., 160.] {
            let current = cx.update(|window, cx| image(window, cx, width)).unwrap();
            assert!(Arc::ptr_eq(&first, &current));
        }
        cx.run_until_parked();
        let larger = cx.update(|window, cx| image(window, cx, 180.)).unwrap();
        assert!(larger.size(0).width > first.size(0).width);
        assert_eq!(larger.size(0).width.0, larger.size(0).height.0 * 2);
        let smaller = cx.update(|window, cx| image(window, cx, 24.)).unwrap();
        assert!(Arc::ptr_eq(&larger, &smaller));
    }
}
