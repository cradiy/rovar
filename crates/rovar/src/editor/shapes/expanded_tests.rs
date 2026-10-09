use super::*;
use crate::editor::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn polygon_and_star_parameters_flip_and_vector_conversion_share_history(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    for tool in ["add-polygon", "add-star"] {
        create(&mut visual, tool);
        click(&mut visual, "property-9");
        visual.simulate_keystrokes("secondary-a 7 enter");
        draw(&mut visual);
        let before = window
            .update(&mut visual.cx, |this, _, _| {
                this.selected_shape().unwrap().clone()
            })
            .unwrap();
        assert_eq!(before.vertices, 7);
        assert_eq!(
            before.editable_nodes().len(),
            if tool == "add-star" { 14 } else { 7 }
        );
        window
            .update(&mut visual.cx, |this, window, cx| {
                let original = this.selected_shape().unwrap().polygon_points();
                this.flip_selection(true, cx);
                let flipped = this.selected_shape().unwrap().polygon_points();
                for (a, b) in original.iter().zip(&flipped) {
                    assert!((a.x + b.x - 1.).abs() < 0.0001);
                }
                this.replay_history(false, window, cx);
                assert_eq!(this.selected_shape().unwrap(), &before);
                this.enter_vector_edit(before.id, window, cx);
                this.selected_node = Some((before.id, 0));
                this.nudge_node(point(10., 0.), cx);
                assert_eq!(this.selected_shape().unwrap().kind, ShapeKind::Bezier);
                this.replay_history(false, window, cx);
                assert_eq!(this.selected_shape().unwrap(), &before);
                this.exit_vector_edit(cx);
            })
            .unwrap();
    }
    click(&mut visual, "property-10");
    visual.simulate_keystrokes("secondary-a 6 0 enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!((this.selected_shape().unwrap().inner_radius - 0.6).abs() < 0.0001)
        })
        .unwrap();
}

#[gpui::test]
fn arrow_draws_endpoints_in_both_directions_and_converts_its_head(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "draw-arrow");
    window
        .update(&mut visual.cx, |this, _, _| {
            let shape = this.selected_shape().unwrap();
            assert_eq!(shape.kind, ShapeKind::Arrow);
            assert!(shape.stroke.enabled);
            assert!(!shape.can_fill());
            assert_eq!(shape.editable_nodes().len(), 5);
        })
        .unwrap();
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("secondary-a - 4 0 enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.selected_shape().unwrap().display_path_point(1).x, -40.);
            this.replay_history(false, window, cx);
            let id = this.selected_shape.unwrap();
            this.shapes
                .last_mut()
                .unwrap()
                .set_path(&[point(0., 0.), point(0., 120.)]);
            this.enter_vector_edit(id, window, cx);
            let original = this.selected_shape().unwrap().clone();
            let handles = this.vector_handle_shape(&original);
            assert!(handles.nodes.0[2].anchor.x.abs() > 0.);
            assert_eq!(handles.rect.width, 1.);
            this.selected_node = Some((id, 2));
            this.nudge_node(point(10., 0.), cx);
            assert_eq!(this.selected_shape().unwrap().kind, ShapeKind::Bezier);
            assert!(!this.selected_shape().unwrap().closed);
            this.replay_history(false, window, cx);
            assert_eq!(this.selected_shape().unwrap(), &original);
        })
        .unwrap();
}
