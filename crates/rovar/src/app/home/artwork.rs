use super::*;
use crate::ui::theme::Color;
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
                            linear_color_stop(Color::Accent.color().opacity(0.1961), 0.),
                            linear_color_stop(Color::Accent.color().opacity(0.0314), 1.),
                        ),
                    );
                }
                let mut curve = PathBuilder::stroke(px(1.5));
                curve.move_to(p(30., 77.));
                curve.cubic_bezier_to(p(209., 29.), p(98., -30.), p(161., 143.));
                if let Ok(path) = curve.build() {
                    window.paint_path(path, Color::Accent.color().opacity(0.7333));
                }
                let mut guides = PathBuilder::stroke(px(1.));
                guides.move_to(p(76., 3.));
                guides.line_to(p(30., 77.));
                guides.move_to(p(173., 100.));
                guides.line_to(p(209., 29.));
                if let Ok(path) = guides.build() {
                    window.paint_path(path, Color::Accent.color().opacity(0.3333));
                }
                for (x, y) in [(30., 77.), (209., 29.)] {
                    window.paint_quad(quad(
                        Bounds::new(p(x - 3., y - 3.), size(px(6.), px(6.))),
                        px(1.),
                        Color::Accent.color(),
                        px(1.),
                        gpui::Hsla::from(Color::Accent.color()),
                        gpui::BorderStyle::Solid,
                    ));
                }
                for (x, y) in [(76., 3.), (173., 100.)] {
                    window.paint_quad(quad(
                        Bounds::new(p(x - 2.5, y - 2.5), size(px(5.), px(5.))),
                        px(3.),
                        Color::Selected.color(),
                        px(1.),
                        gpui::Hsla::from(Color::Accent.color().opacity(0.6000)),
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
                            linear_color_stop(Color::Surface.color(), 0.),
                            linear_color_stop(Color::Surface.color(), 1.),
                        ),
                        px(1.),
                        gpui::Hsla::from(Color::Accent.color().opacity(0.1412)),
                        gpui::BorderStyle::Solid,
                    ));
                }
                let mut mark = PathBuilder::stroke(px(1.));
                mark.move_to(p(102., 87.));
                mark.line_to(p(126., 87.));
                mark.move_to(p(114., 75.));
                mark.line_to(p(114., 99.));
                if let Ok(path) = mark.build() {
                    window.paint_path(path, Color::Accent.color().opacity(0.5412));
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
                        Color::Accent.color().opacity(0.1098),
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
        .bg(Color::Surface.color())
        .border_1()
        .border_color(Color::Accent.color().opacity(0.4000))
        .shadow(vec![
            gpui::BoxShadow::new(px(0.), px(8.), Color::Shadow.color().into()).blur_radius(px(20.)),
        ])
        .flex()
        .items_center()
        .justify_center()
        .text_color(Color::Accent.color().opacity(0.5020))
        .child(icon(LucideIcons::Frame, 22.))
        .children(
            [(false, false), (true, false), (false, true), (true, true)].map(|(right, bottom)| {
                div()
                    .absolute()
                    .size(px(5.))
                    .bg(Color::Accent.color())
                    .when(right, |el| el.right(px(-3.)))
                    .when(!right, |el| el.left(px(-3.)))
                    .when(bottom, |el| el.bottom(px(-3.)))
                    .when(!bottom, |el| el.top(px(-3.)))
            }),
        )
}
