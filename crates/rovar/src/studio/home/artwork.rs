use super::*;
use gpui::{Bounds, PathBuilder, canvas, point, quad, size};

pub(super) fn start_art(primary: bool) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let p = |x, y| bounds.origin + point(px(x), px(y));
            if primary {
                let mut ribbon = PathBuilder::fill();
                ribbon.move_to(p(75., 15.));
                ribbon.cubic_bezier_to(p(205., 55.), p(142., -36.), p(157., 105.));
                ribbon.cubic_bezier_to(p(168., 148.), p(266., -4.), p(274., 117.));
                ribbon.cubic_bezier_to(p(75., 15.), p(72., 177.), p(135., 55.));
                ribbon.close();
                if let Ok(path) = ribbon.build() {
                    window.paint_path(
                        path,
                        linear_gradient(
                            140.,
                            linear_color_stop(rgba(0xd3c1ff32), 0.),
                            linear_color_stop(rgba(0xb89aff08), 1.),
                        ),
                    );
                }
                let mut curve = PathBuilder::stroke(px(1.5));
                curve.move_to(p(30., 77.));
                curve.cubic_bezier_to(p(209., 29.), p(98., -30.), p(161., 143.));
                if let Ok(path) = curve.build() {
                    window.paint_path(path, rgba(0xd8c5ffbb));
                }
                let mut guides = PathBuilder::stroke(px(1.));
                guides.move_to(p(76., 3.));
                guides.line_to(p(30., 77.));
                guides.move_to(p(173., 100.));
                guides.line_to(p(209., 29.));
                if let Ok(path) = guides.build() {
                    window.paint_path(path, rgba(0xc5aaff55));
                }
                for (x, y) in [(30., 77.), (209., 29.)] {
                    window.paint_quad(quad(
                        Bounds::new(p(x - 3., y - 3.), size(px(6.), px(6.))),
                        px(1.),
                        rgb(0xd7c7ff),
                        px(1.),
                        gpui::Hsla::from(rgb(0xf0e7ff)),
                        gpui::BorderStyle::Solid,
                    ));
                }
                for (x, y) in [(76., 3.), (173., 100.)] {
                    window.paint_quad(quad(
                        Bounds::new(p(x - 2.5, y - 2.5), size(px(5.), px(5.))),
                        px(3.),
                        rgb(0x574571),
                        px(1.),
                        gpui::Hsla::from(rgba(0xd8c5ff99)),
                        gpui::BorderStyle::Solid,
                    ));
                }
            } else {
                for (x, y, w, h) in [
                    (107., 26., 78., 84.),
                    (91., 39., 78., 84.),
                    (75., 53., 78., 84.),
                ] {
                    window.paint_quad(quad(
                        Bounds::new(p(x, y), size(px(w), px(h))),
                        px(8.),
                        linear_gradient(
                            140.,
                            linear_color_stop(rgb(0x383741), 0.),
                            linear_color_stop(rgb(0x2a2a34), 1.),
                        ),
                        px(1.),
                        gpui::Hsla::from(rgba(0xd4c9ef24)),
                        gpui::BorderStyle::Solid,
                    ));
                }
                let mut mark = PathBuilder::stroke(px(1.));
                mark.move_to(p(102., 87.));
                mark.line_to(p(126., 87.));
                mark.move_to(p(114., 75.));
                mark.line_to(p(114., 99.));
                if let Ok(path) = mark.build() {
                    window.paint_path(path, rgba(0xd0c6e88a));
                }
            }
        },
    )
    .size_full()
}

pub(super) fn preview_grid() -> impl IntoElement {
    canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            for x in (12..width as usize).step_by(20) {
                for y in (12..height as usize).step_by(20) {
                    window.paint_quad(gpui::fill(
                        Bounds::new(
                            bounds.origin + point(px(x as f32), px(y as f32)),
                            size(px(1.), px(1.)),
                        ),
                        rgba(0xd1c4f51c),
                    ));
                }
            }
        },
    )
    .absolute()
    .size_full()
}

pub(super) fn blank_canvas() -> impl IntoElement {
    div()
        .w(px(124.))
        .h(px(88.))
        .relative()
        .bg(rgb(0x2b2934))
        .border_1()
        .border_color(rgba(0xc7b7ec66))
        .shadow(vec![
            gpui::BoxShadow::new(px(0.), px(8.), rgba(0x00000030).into()).blur_radius(px(20.)),
        ])
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgba(0xc9b8ef80))
        .child(icon(LucideIcons::Frame, 22.))
        .children(
            [(false, false), (true, false), (false, true), (true, true)].map(|(right, bottom)| {
                div()
                    .absolute()
                    .size(px(5.))
                    .bg(rgb(0xc7b7ec))
                    .when(right, |el| el.right(px(-3.)))
                    .when(!right, |el| el.left(px(-3.)))
                    .when(bottom, |el| el.bottom(px(-3.)))
                    .when(!bottom, |el| el.top(px(-3.)))
            }),
        )
}
