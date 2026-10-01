use super::*;
use crate::workspace::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn zoom_menu_edits_view_without_changing_document_and_discards_drafts(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-rectangle");
    let (shape, undo) = window
        .update(&mut visual.cx, |this, _, _| {
            (this.shapes[0].clone(), this.history.borrow().undo_len())
        })
        .unwrap();
    click(&mut visual, "inspector-zoom");
    click(&mut visual, "zoom-200");
    assert!(visual.debug_bounds("zoom-menu").is_none());
    window
        .update(&mut visual.cx, |this, _, _| assert_eq!(this.view.zoom, 2.))
        .unwrap();
    click(&mut visual, "inspector-zoom");
    click(&mut visual, "zoom-input");
    for value in ["NaN", "0", "-20", "inf", "abc"] {
        window
            .update(&mut visual.cx, |this, _, cx| {
                this.zoom_menu
                    .input
                    .update(cx, |input, cx| input.set_value(value, cx));
            })
            .unwrap();
        visual.simulate_keystrokes("enter");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert!(this.zoom_menu.invalid);
                assert!(this.zoom_menu.popover.read(cx).is_open());
                assert_eq!(this.view.zoom, 2.);
            })
            .unwrap();
    }
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.zoom_menu
                .input
                .update(cx, |input, cx| input.set_value("62.5%", cx));
        })
        .unwrap();
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.view.zoom, 0.625);
            assert!(!this.zoom_menu.popover.read(cx).is_open());
            assert_eq!(this.shapes[0], shape);
            assert_eq!(this.history.borrow().undo_len(), undo);
        })
        .unwrap();
    click(&mut visual, "inspector-zoom");
    click(&mut visual, "zoom-input");
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.zoom_menu
                .input
                .update(cx, |input, cx| input.set_value("300%", cx));
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    click(&mut visual, "inspector-zoom");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.view.zoom, 0.625);
            assert_eq!(this.zoom_menu.input.read(cx).value().as_ref(), "62.5%");
        })
        .unwrap();
}

#[gpui::test]
fn fit_content_accounts_for_panels_rotation_and_hidden_layers(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-rectangle");
    create(&mut visual, "add-rectangle");
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.sidebar.collapsed = false;
            this.shapes[0].rect = Rect {
                x: -600.,
                y: -200.,
                width: 1200.,
                height: 700.,
            };
            this.shapes[0].layer.rotation = 35.;
            this.shapes[1].rect.x = 10000.;
            this.shapes[1].layer.hidden = true;
            cx.notify();
        })
        .unwrap();
    for width in [1280., 840.] {
        visual.simulate_resize(size(px(width), px(800.)));
        draw(&mut visual);
        let tab = visual.debug_bounds("inspector-design-tab").unwrap();
        let zoom = visual.debug_bounds("inspector-zoom").unwrap();
        let toggle = visual.debug_bounds("toggle-properties").unwrap();
        assert!(tab.right() <= zoom.left());
        assert!(zoom.right() <= toggle.left());
        click(&mut visual, "inspector-zoom");
        click(&mut visual, "zoom-fit");
        window
            .update(&mut visual.cx, |this, _, _| {
                let rect = this.world_bounds(this.shapes[0].id).unwrap();
                let top_left = this.view.screen(point(rect.x, rect.y));
                let bottom_right = this
                    .view
                    .screen(point(rect.x + rect.width, rect.y + rect.height));
                let (left, right) = this.canvas_insets();
                assert!(top_left.x >= left);
                assert!(bottom_right.x <= width - right);
                assert!(top_left.y >= 24.);
                assert!(bottom_right.y <= 728.);
            })
            .unwrap();
    }
}
