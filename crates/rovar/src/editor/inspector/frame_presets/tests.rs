use super::*;
use crate::{
    editor::tests::{click, draw, open},
    scene::auto_layout::{Constraint, Constraints, Container, Limits, Sizing},
};
use gpui::{TestAppContext, VisualTestContext};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[gpui::test]
fn frame_menu_resizes_document_and_undoes_in_one_step(cx: &mut TestAppContext) {
    let handle = open(cx);
    let (original, depth) = handle
        .update(cx, |this, _, cx| {
            this.add_artboard(rect(100., 200., 600., 400.), cx);
            this.boards[0].name = "My screen".into();
            this.shapes.push(Shape::new(
                2,
                Some(1),
                ShapeKind::Rectangle,
                rect(500., 20., 50., 30.),
            ));
            this.hierarchy.sizing.insert(
                2,
                Sizing {
                    constraints: Some(Constraints {
                        horizontal: Constraint::End,
                        vertical: Constraint::Start,
                        rect: this.shapes[0].rect,
                        parent_size: [600., 400.],
                    }),
                    ..Default::default()
                },
            );
            this.next_id = 3;
            this.reflow_layout(cx);
            (
                serde_json::to_value(this.snapshot_page(cx).0).unwrap(),
                this.history.borrow().undo_len(),
            )
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "frame-presets");
    click(&mut visual, "frame-preset-iPhone 17 / 16 Pro");
    draw(&mut visual);
    let resized = handle
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.boards[0].rect, rect(100., 200., 402., 874.));
            assert_eq!(this.boards[0].name, "My screen");
            assert_eq!(this.shapes[0].rect, rect(302., 20., 50., 30.));
            assert_eq!(this.inspector.fields[3].read(cx).value(), "402");
            assert_eq!(this.inspector.fields[4].read(cx).value(), "874");
            assert_eq!(this.history.borrow().undo_len(), depth + 1);
            serde_json::to_value(this.snapshot_page(cx).0).unwrap()
        })
        .unwrap();
    handle
        .update(&mut visual.cx, |this, window, cx| {
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(
                serde_json::to_value(this.snapshot_page(cx).0).unwrap(),
                original
            );
            this.undo_redo(true, window, cx);
            this.reflow_layout(cx);
            assert_eq!(
                serde_json::to_value(this.snapshot_page(cx).0).unwrap(),
                resized
            );
        })
        .unwrap();
    click(&mut visual, "frame-presets");
    click(&mut visual, "frame-preset-iPhone 17 / 16 Pro");
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.history.borrow().undo_len(), depth + 1)
        })
        .unwrap();
    for dimensions in [[1280., 800.], [600., 640.]] {
        visual.simulate_resize(size(px(dimensions[0]), px(dimensions[1])));
        click(&mut visual, "frame-presets");
        draw(&mut visual);
        let menu = visual.debug_bounds("frame-preset-menu-0").unwrap();
        assert!(menu.left() >= px(0.) && menu.top() >= px(0.));
        assert!(menu.right() <= px(dimensions[0]) && menu.bottom() <= px(dimensions[1]));
        visual.simulate_event(gpui::ScrollWheelEvent {
            position: menu.center(),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-3000.))),
            ..Default::default()
        });
        draw(&mut visual);
        assert!(
            menu.contains(
                &visual
                    .debug_bounds("frame-preset-Apple Watch 38 mm")
                    .unwrap()
                    .center()
            )
        );
        visual.simulate_keystrokes("escape");
        draw(&mut visual);
        assert!(visual.debug_bounds("frame-preset-menu-0").is_none());
    }
}

#[gpui::test]
fn frame_preset_preserves_limits_and_restores_hug_and_wrapping_on_undo(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(0., 0., 600., 400.), cx);
            let mut layout = Container::new(this.boards[0].rect);
            layout.wrap = true;
            this.hierarchy.layouts.insert(1, layout);
            let limits = Limits {
                min_width: Some(360.),
                ..Default::default()
            };
            this.hierarchy.sizing.insert(
                1,
                Sizing {
                    height: Mode::Hug,
                    limits,
                    ..Default::default()
                },
            );
            for id in 2..=4 {
                this.shapes.push(Shape::new(
                    id,
                    Some(1),
                    ShapeKind::Rectangle,
                    rect(0., 0., 160., 60.),
                ));
            }
            this.next_id = 5;
            this.reflow_layout(cx);
            let original = serde_json::to_value(this.snapshot_page(cx).0).unwrap();
            let page = this.pages.active.clone();
            this.apply_frame_preset(1, &page, [320., 568.], window, cx);
            assert_eq!(this.boards[0].rect.width, 360.);
            assert_eq!(this.boards[0].rect.height, 568.);
            assert_eq!(this.hierarchy.sizing[&1].height, Mode::Fixed);
            assert_eq!(this.hierarchy.sizing[&1].limits, limits);
            assert!(this.shapes[2].rect.y > this.shapes[0].rect.y);
            this.undo_redo(false, window, cx);
            this.reflow_layout(cx);
            assert_eq!(
                serde_json::to_value(this.snapshot_page(cx).0).unwrap(),
                original
            );
        })
        .unwrap();
}

#[gpui::test]
fn frame_preset_rejects_stale_selection_and_locked_frames(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(0., 0., 600., 400.), cx);
            let first = this.boards[0].id;
            this.add_artboard(rect(700., 0., 600., 400.), cx);
            let second = this.boards[1].id;
            let page = this.pages.active.clone();
            let depth = this.history.borrow().undo_len();
            this.apply_frame_preset(first, &page, [360., 640.], window, cx);
            this.apply_frame_preset(second, "stale-page", [360., 640.], window, cx);
            this.boards[1].layer.locked = true;
            this.apply_frame_preset(second, &page, [360., 640.], window, cx);
            assert_eq!(this.boards[0].rect.width, 600.);
            assert_eq!(this.boards[1].rect.width, 600.);
            assert_eq!(this.history.borrow().undo_len(), depth);
        })
        .unwrap();
}
