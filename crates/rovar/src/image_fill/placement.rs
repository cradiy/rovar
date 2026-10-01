use super::ImageFit;
use crate::artboard::Rect;
use gpui::Point;

/// Image magnification relative to its fit, with offsets in frame dimensions.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Placement {
    pub zoom: f32,
    pub offset: [f32; 2],
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            zoom: 1.,
            offset: [0.; 2],
        }
    }
}

impl Placement {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1. ..=16.).contains(&self.zoom) && self.offset.iter().all(|v| v.is_finite()),
            "Invalid image placement"
        );
        Ok(())
    }

    pub fn rect(self, frame: Rect, source: [u32; 2], fit: ImageFit) -> Rect {
        let [w, h] = source.map(|v| v.max(1) as f32);
        let (sx, sy) = (frame.width / w, frame.height / h);
        let scale = match fit {
            ImageFit::Cover => sx.max(sy),
            ImageFit::Contain => sx.min(sy),
        } * self.zoom;
        let (width, height) = (w * scale, h * scale);
        let dx = (self.offset[0] * frame.width).clamp(
            -(width - frame.width).abs() / 2.,
            (width - frame.width).abs() / 2.,
        );
        let dy = (self.offset[1] * frame.height).clamp(
            -(height - frame.height).abs() / 2.,
            (height - frame.height).abs() / 2.,
        );
        Rect {
            x: frame.x + (frame.width - width) / 2. + dx,
            y: frame.y + (frame.height - height) / 2. + dy,
            width,
            height,
        }
    }

    fn constrain(&mut self, frame: Rect, source: [u32; 2], fit: ImageFit) {
        let rect = self.rect(frame, source, fit);
        let delta = crate::rotation::center(rect) - crate::rotation::center(frame);
        self.offset = [delta.x / frame.width, delta.y / frame.height];
    }

    pub fn pan(&mut self, delta: Point<f32>, frame: Rect, source: [u32; 2], fit: ImageFit) {
        self.offset[0] += delta.x / frame.width;
        self.offset[1] += delta.y / frame.height;
        self.constrain(frame, source, fit);
    }

    pub fn zoom_at(
        &mut self,
        zoom: f32,
        anchor: Point<f32>,
        frame: Rect,
        source: [u32; 2],
        fit: ImageFit,
    ) {
        let before = self.rect(frame, source, fit);
        let zoom = zoom.clamp(1., 16.);
        let ratio = zoom / self.zoom;
        self.zoom = zoom;
        let center = anchor + (crate::rotation::center(before) - anchor) * ratio;
        let delta = center - crate::rotation::center(frame);
        self.offset = [delta.x / frame.width, delta.y / frame.height];
        self.constrain(frame, source, fit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::point;

    #[test]
    fn crop_stays_covered_and_zoom_preserves_the_image_point_under_the_pointer() {
        let frame = Rect {
            x: 10.,
            y: 20.,
            width: 100.,
            height: 100.,
        };
        let mut p = Placement::default();
        let anchor = point(35., 60.);
        let before = p.rect(frame, [400, 200], ImageFit::Cover);
        p.zoom_at(2., anchor, frame, [400, 200], ImageFit::Cover);
        let after = p.rect(frame, [400, 200], ImageFit::Cover);
        assert_eq!(
            (anchor.x - before.x) / before.width,
            (anchor.x - after.x) / after.width
        );
        assert_eq!(
            (anchor.y - before.y) / before.height,
            (anchor.y - after.y) / after.height
        );
        p.pan(point(10000., -10000.), frame, [400, 200], ImageFit::Cover);
        let rect = p.rect(frame, [400, 200], ImageFit::Cover);
        assert_eq!(rect.x, frame.x);
        assert_eq!(rect.y + rect.height, frame.y + frame.height);
        p.zoom_at(0.1, anchor, frame, [400, 200], ImageFit::Cover);
        assert_eq!(p.zoom, 1.);
        assert_eq!(
            p.rect(frame, [400, 200], ImageFit::Cover).height,
            frame.height
        );
    }
}
