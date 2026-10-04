use crate::{i18n::t, ui::MUTED};
use gpui::{
    Animation, AnimationExt, IntoElement, Transformation, color_svg, div, point, prelude::*, px,
    radians, size,
};
use serde::Deserialize;
use std::{f32::consts::TAU, sync::OnceLock, time::Duration};

#[derive(Deserialize)]
struct Motion {
    intro_ms: u64,
    rotation_ms: u64,
    mark_width: f32,
    pieces: Vec<Piece>,
}

#[derive(Deserialize)]
struct Piece {
    name: String,
    pivot: [f32; 2],
    offset: [f32; 2],
    turn: f32,
    wave: f32,
}

impl Motion {
    fn progress(&self, stage: usize, phase: f32) -> (f32, f32) {
        let intro_turn = TAU * self.intro_ms as f32 / self.rotation_ms as f32;
        if stage == 0 {
            let spread = phase.powi(3) * (10. - 15. * phase + 6. * phase * phase);
            // Integrate a smooth angular-speed ramp, arriving at the loop's
            // constant speed without a stop or a change in acceleration.
            let angle = intro_turn * (phase.powi(3) - 0.5 * phase.powi(4));
            (spread, angle)
        } else {
            (1., intro_turn * 0.5 + TAU * phase)
        }
    }
}

impl Piece {
    fn transform(&self, spread: f32, orbit: f32) -> Transformation {
        let wave = (2. * orbit + self.wave).sin();
        let turn = orbit + (self.turn + 3. * wave).to_radians() * spread;
        let scale = 1. - spread * (0.045 - 0.015 * wave);
        let [px0, py0] = self.pivot;
        let x = px0 + self.offset[0] * spread;
        let y = py0 + self.offset[1] * spread;
        let (sin, cos) = orbit.sin_cos();
        let (local_sin, local_cos) = turn.sin_cos();
        // Orbit each ribbon's center, then turn its surface around that center.
        // The SVG bounds stay fixed so only the cached GPU sprite is transformed.
        Transformation::translate(point(
            px(cos * x - sin * y - scale * (local_cos * px0 - local_sin * py0)),
            px(sin * x + cos * y - scale * (local_sin * px0 + local_cos * py0)),
        ))
        .with_rotation(radians(turn))
        .with_scaling(size(scale, scale))
    }
}

pub(super) fn view(token: usize) -> impl IntoElement {
    static MOTION: OnceLock<Motion> = OnceLock::new();
    let motion = MOTION.get_or_init(|| {
        serde_json::from_str(include_str!("../../../../assets/loading/motion.json"))
            .expect("embedded loading choreography must be valid")
    });
    div()
        .id("document-loading")
        .flex()
        .flex_col()
        .items_center()
        .gap(px(14.))
        .child(div().relative().size(px(128.)).with_animations(
            ("document-loading-mark", token),
            vec![
                Animation::new(Duration::from_millis(motion.intro_ms)),
                Animation::new(Duration::from_millis(motion.rotation_ms)).repeat(),
            ],
            move |mark, stage, phase| {
                let (spread, orbit) = motion.progress(stage, phase);
                mark.children(motion.pieces.iter().map(|piece| {
                    color_svg()
                        .path(format!("loading/{}.svg", piece.name))
                        .absolute()
                        .left(px((128. - motion.mark_width) / 2.))
                        .top(px((128. - motion.mark_width * 710. / 740.) / 2.))
                        .w(px(motion.mark_width))
                        .h(px(motion.mark_width * 710. / 740.))
                        .with_transformation(piece.transform(spread, orbit))
                }))
            },
        ))
        .child(
            div()
                .text_size(px(12.))
                .text_color(MUTED.color())
                .child(t("loading")),
        )
}
