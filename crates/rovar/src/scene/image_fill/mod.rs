use crate::asset::{AssetContent, FileAsset};
use gpui::{Bounds, Div, Pixels, Window, div, point, prelude::*, px, size};
use std::sync::Arc;
mod placement;
pub use placement::Placement;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ImageFit {
    #[default]
    Cover,
    Contain,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageFill {
    #[serde(skip)]
    pub asset: Option<Arc<FileAsset>>,
    pub fit: ImageFit,
    pub opacity: f32,
    #[serde(default)]
    pub placement: Placement,
}
impl Default for ImageFill {
    fn default() -> Self {
        Self {
            asset: None,
            fit: ImageFit::Cover,
            opacity: 1.,
            placement: Placement::default(),
        }
    }
}
impl ImageFill {
    pub fn element(&self) -> Div {
        self.element_with_radii(Default::default())
    }

    pub fn element_with_radii(&self, radii: gpui::Corners<Pixels>) -> Div {
        let fill = self.clone();
        div().overflow_hidden().opacity(self.opacity).child(
            gpui::canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| fill.paint_with_radii(bounds, radii, window, cx),
            )
            .size_full(),
        )
    }

    pub fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &gpui::App) {
        self.paint_with_radii(bounds, Default::default(), window, cx);
    }

    fn paint_with_radii(
        &self,
        bounds: Bounds<Pixels>,
        radii: gpui::Corners<Pixels>,
        window: &mut Window,
        cx: &gpui::App,
    ) {
        let Some(asset) = &self.asset else {
            return;
        };
        let Some(content) = asset.content() else {
            return;
        };
        let rect = self.placement.rect(
            crate::scene::artboard::Rect {
                x: f32::from(bounds.origin.x),
                y: f32::from(bounds.origin.y),
                width: f32::from(bounds.size.width),
                height: f32::from(bounds.size.height),
            },
            [asset.width, asset.height],
            self.fit,
        );
        if let AssetContent::Svg(svg) = content {
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            if width <= 0. || height <= 0. {
                return;
            }
            let x = (rect.x - f32::from(bounds.origin.x)) / width;
            let y = (rect.y - f32::from(bounds.origin.y)) / height;
            let w = rect.width / width;
            let h = rect.height / height;
            let data = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\" viewBox=\"0 0 1 1\" preserveAspectRatio=\"none\"><image x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\" href=\"{}\"/></svg>",
                svg.uri
            );
            let key = format!("rovar-svg-{}-{x}-{y}-{w}-{h}", asset.hash);
            let _ = window.paint_color_svg_with_fit(
                bounds,
                key.into(),
                Some(data.as_bytes()),
                Default::default(),
                radii,
                None,
                None,
                None,
                gpui::ObjectFit::Fill,
                cx,
            );
            return;
        }
        let AssetContent::Image(image) = content else {
            return;
        };
        let _ = window.paint_image_with_clip(
            Bounds::new(
                point(px(rect.x), px(rect.y)),
                size(px(rect.width), px(rect.height)),
            ),
            bounds,
            radii,
            image.clone(),
            0,
            false,
        );
    }
}
