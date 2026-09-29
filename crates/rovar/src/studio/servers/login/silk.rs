use gpui::{
    Bounds, IntoElement, PathBuilder, Pixels, Window, canvas, linear_color_stop, linear_gradient,
    point, prelude::*, px, rgb, rgba,
};
use std::{cell::RefCell, rc::Rc};
use web_time::Instant;

/// Transient presentation state. Never participates in document or account storage.
pub(in crate::studio::servers) struct Motion {
    last: Instant,
    phase: f32,
    pointer: [f32; 2],
    height: Option<f32>,
    target_height: f32,
    mode: &'static str,
    changed: Instant,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            last: Instant::now(),
            phase: 0.,
            pointer: [0.; 2],
            height: None,
            target_height: 0.,
            mode: "login",
            changed: Instant::now(),
        }
    }
}

impl Motion {
    pub fn content_opacity(&mut self, mode: &'static str) -> f32 {
        if self.mode != mode {
            self.mode = mode;
            self.changed = Instant::now();
        }
        #[cfg(target_family = "wasm")]
        if crate::web::login_motion_allowed() {
            return 0.6 + 0.4 * (self.changed.elapsed().as_secs_f32() / 0.24).min(1.);
        }
        1.
    }
    pub fn height(&self) -> Option<f32> {
        self.height
    }
    pub fn sheen(&self) -> f32 {
        self.pointer[0] * 12. + self.phase.sin() * 4.
    }

    fn advance(&mut self, bounds: Bounds<Pixels>, window: &Window) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32().min(0.05);
        self.last = now;
        #[cfg(target_family = "wasm")]
        let animate = crate::web::login_motion_allowed();
        #[cfg(not(target_family = "wasm"))]
        let animate = false;
        if animate {
            self.phase = (self.phase + dt * 0.13) % std::f32::consts::TAU;
            let mouse = window.mouse_position() - bounds.origin;
            let target = if bounds.contains(&window.mouse_position()) {
                [
                    (f32::from(mouse.x) / f32::from(bounds.size.width).max(1.) - 0.5) * 2.,
                    (f32::from(mouse.y) / f32::from(bounds.size.height).max(1.) - 0.5) * 2.,
                ]
            } else {
                [0.; 2]
            };
            let ease = 1. - (-dt * 3.).exp();
            for (value, target) in self.pointer.iter_mut().zip(target) {
                *value += (target - *value) * ease;
            }
        } else {
            self.pointer = [0.; 2];
        }
        if let Some(height) = &mut self.height {
            *height +=
                (self.target_height - *height) * if animate { 1. - (-dt * 16.).exp() } else { 1. };
            if (*height - self.target_height).abs() < 0.2 {
                *height = self.target_height;
            }
        }
        animate
    }
}

pub(super) fn measure(motion: Rc<RefCell<Motion>>) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let mut motion = motion.borrow_mut();
            if (motion.target_height - f32::from(bounds.size.height)).abs() > 0.2 {
                window.request_animation_frame();
            }
            motion.target_height = f32::from(bounds.size.height);
            if motion.height.is_none() {
                motion.height = Some(motion.target_height);
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

pub(super) fn background(motion: Rc<RefCell<Motion>>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let mut motion = motion.borrow_mut();
            let animate = motion.advance(bounds, window);
            let w = f32::from(bounds.size.width);
            let h = f32::from(bounds.size.height);
            let phase = motion.phase;
            let shift = point(px(motion.pointer[0] * 12.), px(motion.pointer[1] * 8.));
            // Separate gauzy folds keep the light distributed across the viewport.
            // Each surface has its own curvature and phase, with no shared focal point.
            for (layer, (level, offset, strength)) in
                [(0.19, 0.2, 0.8), (0.53, 2.1, 0.55), (0.86, 4.3, 0.7)]
                    .into_iter()
                    .enumerate()
            {
                let drift = (phase + offset).sin();
                let surface = |t: f32, u: f32| {
                    let wave = t * std::f32::consts::TAU * 0.72 + offset;
                    let x = w * (-0.16 + 1.32 * t);
                    let y = h * (level + 0.115 * wave.sin() + 0.028 * (phase + wave).sin());
                    let width = w.min(1450.) * (0.075 + 0.025 * (wave + drift * 0.4).cos());
                    let fold = 0.72 + 0.28 * (wave + drift * 0.2).cos();
                    bounds.origin
                        + shift
                        + point(
                            px(x + u * width * 0.22 * (wave + 0.4).sin()),
                            px(y + u * width * fold),
                        )
                };
                const STRIPS: usize = 36;
                const STEPS: usize = 32;
                for strip in 0..STRIPS {
                    let u = strip as f32 / STRIPS as f32 * 2. - 1.;
                    let v = (strip + 1) as f32 / STRIPS as f32 * 2. - 1.;
                    let mut path = PathBuilder::fill();
                    path.move_to(surface(0., u));
                    for step in 1..=STEPS {
                        path.line_to(surface(step as f32 / STEPS as f32, u));
                    }
                    for step in (0..=STEPS).rev() {
                        path.line_to(surface(step as f32 / STEPS as f32, v));
                    }
                    path.close();
                    let middle = (u + v) * 0.5;
                    let body = (1. - middle * middle).max(0.).powf(1.4);
                    let crest = (-((middle - 0.16) / 0.32).powi(2)).exp();
                    let light = (body * 0.10 + crest * 0.25) * strength;
                    let channel = |base: f32, tint: f32| (base + tint * light) as u32;
                    let color =
                        channel(17., 119.) << 16 | channel(18., 103.) << 8 | channel(23., 160.);
                    if let Ok(path) = path.build() {
                        window.paint_path(
                            path,
                            linear_gradient(
                                110. + layer as f32 * 25.,
                                linear_color_stop(rgb(color), 0.),
                                linear_color_stop(rgba((color << 8) | 0x55), 1.),
                            ),
                        );
                    }
                }
            }
            if animate {
                window.request_animation_frame();
            }
        },
    )
    .absolute()
    .size_full()
}
