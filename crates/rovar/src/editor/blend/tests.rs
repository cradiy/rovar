use super::*;
use crate::{
    document::{Page, export::Format},
    editor::tests::{click, draw, open},
};
use gpui::{TestAppContext, VisualTestContext};
use std::collections::BTreeSet;

fn rect(x: f32, width: f32) -> Rect {
    Rect {
        x,
        y: 0.,
        width,
        height: 40.,
    }
}

fn setup(this: &mut Workspace, cx: &mut Context<Workspace>) {
    this.boards.clear();
    this.shapes = vec![
        Shape::new(1, None, ShapeKind::Rectangle, rect(0., 40.)),
        Shape::new(2, None, ShapeKind::Rectangle, rect(20., 40.)),
    ];
    this.shapes[0].color = rgb(0x4080c0);
    this.shapes[1].color = rgb(0xc06040);
    this.next_id = 3;
    this.set_selection(BTreeSet::from([1, 2]), cx);
    this.view.zoom = 1.;
    this.view.pan = point(350., 250.);
}

fn expected(mode: Mode, b: f32, s: f32) -> f32 {
    match mode {
        Mode::Normal => s,
        Mode::Multiply => b * s,
        Mode::Screen => b + s - b * s,
        Mode::Overlay => {
            if b <= 0.5 {
                2. * b * s
            } else {
                1. - 2. * (1. - b) * (1. - s)
            }
        }
        Mode::Darken => b.min(s),
        Mode::Lighten => b.max(s),
    }
}

#[gpui::test]
fn blend_exports_match_modes_alpha_and_group_isolation(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            setup(this, cx);
            this.group_selection(cx);
            let root = *this.selection_ids().first().unwrap();
            let options = crate::render::svg::render_options(false).unwrap();
            for mode in Mode::ALL {
                this.shapes[1].layer.blend = mode;
                this.shapes[1].layer.opacity = 0.5;
                let jobs = this.component_export_jobs(window, cx).unwrap();
                let png =
                    image::load_from_memory(&jobs[0].render(Format::Png, 1, &options).unwrap())
                        .unwrap()
                        .to_rgba8();
                for (channel, (b, s)) in [64., 128., 192.]
                    .into_iter()
                    .zip([192., 96., 64.])
                    .enumerate()
                {
                    let expected = ((expected(mode, b / 255., s / 255.) + b / 255.) * 0.5 * 255.)
                        .round() as u8;
                    assert!(
                        png.get_pixel(30, 20).0[channel].abs_diff(expected) <= 2,
                        "{mode:?}: {:?}",
                        png.get_pixel(30, 20)
                    );
                }
                assert_eq!(png.get_pixel(10, 20).0, [64, 128, 192, 255]);
                assert!(png.get_pixel(50, 20).0[3].abs_diff(128) <= 1);
                assert!(
                    png.get_pixel(50, 20).0[0].abs_diff(192) <= 2,
                    "A transparent backdrop must preserve the source color"
                );
                let svg =
                    String::from_utf8(jobs[0].render(Format::Svg, 1, &options).unwrap()).unwrap();
                assert!(svg.contains(&format!("mix-blend-mode:{}", mode.css())));
            }
            this.shapes[1].layer = LayerState::default();
            this.hierarchy.groups.get_mut(&root).unwrap().layer.opacity = 0.5;
            let png = image::load_from_memory(
                &this.component_export_jobs(window, cx).unwrap()[0]
                    .render(Format::Png, 1, &options)
                    .unwrap(),
            )
            .unwrap()
            .to_rgba8();
            for x in [10, 30, 50] {
                assert!(
                    png.get_pixel(x, 20).0[3].abs_diff(128) <= 1,
                    "Group opacity must apply once, including overlaps"
                );
            }
            // Exporting a child by itself must ignore its parent's opacity.
            this.set_selection(BTreeSet::from([2]), cx);
            let png = image::load_from_memory(
                &this.component_export_jobs(window, cx).unwrap()[0]
                    .render(Format::Png, 1, &options)
                    .unwrap(),
            )
            .unwrap()
            .to_rgba8();
            assert_eq!(png.get_pixel(20, 20).0, [192, 96, 64, 255]);
        })
        .unwrap();
}

#[gpui::test]
fn blend_controls_history_persistence_and_transparent_handles(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, _, cx| {
            setup(this, cx);
            this.set_selection(BTreeSet::from([2]), cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    click(&mut visual, "blend-menu");
    click(&mut visual, "blend-multiply");
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.shapes[1].layer.blend, Mode::Multiply);
            this.undo_redo(false, window, cx);
            assert_eq!(this.shapes[1].layer.blend, Mode::Normal);
            this.undo_redo(true, window, cx);
            this.history.borrow_mut().break_group();
            assert!(this.edit_layer_opacity("0"));
            assert!(!this.edit_layer_opacity("NaN"));
            assert!(!this.edit_layer_opacity("101"));
            this.sync_fields(cx);
            let page = this.snapshot_page(cx).0;
            let decoded = Page::decode(&serde_json::to_vec(&page).unwrap()).unwrap();
            assert_eq!(decoded.shapes[1].layer, this.shapes[1].layer);
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    assert!(visual.debug_bounds("shape-corner-4").is_some());
    let corner = visual.debug_bounds("shape-corner-4").unwrap().center();
    visual.simulate_mouse_down(corner, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        corner + point(px(20.), px(20.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_up(
        corner + point(px(20.), px(20.)),
        MouseButton::Left,
        Default::default(),
    );
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert!(this.shapes[1].rect.width > 40.);
            this.undo_redo(false, window, cx);
            this.undo_redo(false, window, cx);
            assert_eq!(this.shapes[1].layer.opacity, 1.);
            this.set_selection(BTreeSet::from([1, 2]), cx);
            this.set_blend(Mode::Screen, window, cx);
            assert!(this.shapes.iter().all(|s| s.layer.blend == Mode::Screen));
            this.undo_redo(false, window, cx);
            assert_eq!(this.shapes[0].layer.blend, Mode::Normal);
            assert_eq!(this.shapes[1].layer.blend, Mode::Multiply);
        })
        .unwrap();
}

#[test]
fn blend_shader_validates() {
    let source = gpui::compose_subtree_effect_wgsl(&super::paint::shader());
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[gpui::test]
fn layer_opacity_input_and_scrub_cancel_preserve_history(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, _, cx| {
            setup(this, cx);
            this.set_selection(BTreeSet::from([2]), cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    for width in [256., 480.] {
        handle
            .update(&mut visual.cx, |this, _, cx| {
                this.panels.set(super::super::panels::Side::Right, width);
                cx.notify();
            })
            .unwrap();
        draw(&mut visual);
        let mode = visual.debug_bounds("blend-menu").unwrap();
        let alpha = visual.debug_bounds("property-18").unwrap();
        assert!(mode.right() < alpha.left());
        assert!((f32::from(mode.center().y - alpha.center().y)).abs() < 3.);
        let p = visual.debug_bounds("property-drag-18").unwrap().center();
        visual.simulate_mouse_down(p, MouseButton::Left, Default::default());
        visual.simulate_mouse_move(
            p - point(px(30.), px(0.)),
            MouseButton::Left,
            Default::default(),
        );
        handle
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.shapes[1].layer.opacity, 0.7)
            })
            .unwrap();
        visual.simulate_keystrokes("escape");
        visual.simulate_mouse_up(p, MouseButton::Left, Default::default());
        handle
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.shapes[1].layer.opacity, 1.)
            })
            .unwrap();
    }
    click(&mut visual, "property-18");
    visual.simulate_keystrokes("ctrl-a 2 5 enter");
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.shapes[1].layer.opacity, 0.25);
            this.undo_redo(false, window, cx);
            assert_eq!(this.shapes[1].layer.opacity, 1.);
        })
        .unwrap();
}

#[gpui::test]
fn blend_masks_and_components_keep_group_and_instance_appearance(cx: &mut TestAppContext) {
    open(cx)
        .update(cx, |this, window, cx| {
            setup(this, cx);
            this.shapes.insert(
                0,
                Shape::new(
                    3,
                    None,
                    ShapeKind::Ellipse,
                    Rect {
                        x: 0.,
                        y: 0.,
                        width: 60.,
                        height: 40.,
                    },
                ),
            );
            this.next_id = 4;
            this.hierarchy.order = vec![3, 1, 2];
            this.shapes[2].layer.blend = Mode::Multiply;
            this.set_selection(BTreeSet::from([1, 2, 3]), cx);
            this.create_mask(window, cx);
            let root = this.selected_mask().unwrap();
            this.edit_layer_opacity("50");
            let job = this.component_export_jobs(window, cx).unwrap().remove(0);
            let png =
                image::load_from_memory(&job.render(Format::Png, 1, &Default::default()).unwrap())
                    .unwrap()
                    .to_rgba8();
            assert_eq!(png.get_pixel(0, 0).0[3], 0);
            let center = png.get_pixel(30, 20).0;
            for (actual, expected) in center.into_iter().zip([48, 48, 48, 128]) {
                assert!(actual.abs_diff(expected) <= 2, "{center:?}");
            }
            this.create_component(window, cx);
            this.sync_components(window, cx);
            let component = this.hierarchy.components[&root].component.clone();
            this.insert_document_component(&component, false, Some(point(200., 0.)), window, cx);
            let instance = this.selected_mask().unwrap();
            assert_eq!(this.hierarchy.groups[&instance].layer.opacity, 0.5);
            this.set_blend(Mode::Screen, window, cx);
            this.sync_components(window, cx);
            this.set_selection(BTreeSet::from([root]), cx);
            this.edit_layer_opacity("70");
            this.sync_components(window, cx);
            assert_eq!(this.hierarchy.groups[&instance].layer.blend, Mode::Screen);
            assert_eq!(this.hierarchy.groups[&instance].layer.opacity, 0.7);
        })
        .unwrap();
}

#[cfg(not(target_family = "wasm"))]
mod gpu;
