use super::*;
use crate::scene::layer::LayerGroup;

pub(crate) fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn setup(operation: Operation) -> (Hierarchy, Vec<Shape>) {
    let mut h = Hierarchy::default();
    h.groups.insert(
        3,
        LayerGroup {
            boolean: Some(operation),
            mask: None,
            uid: uuid::Uuid::new_v4(),
            name: "Boolean".into(),
            board: None,
            layer: Default::default(),
        },
    );
    h.parents.extend([(1, 3), (2, 3)]);
    h.order = vec![3, 1, 2];
    (
        h,
        vec![
            Shape::new(1, None, ShapeKind::Rectangle, rect(0., 0., 100., 100.)),
            Shape::new(2, None, ShapeKind::Rectangle, rect(50., 0., 100., 100.)),
        ],
    )
}

fn area(geometry: &Geometry) -> f32 {
    geometry
        .contours
        .iter()
        .map(|c| {
            c.iter()
                .zip(c.iter().cycle().skip(1))
                .map(|(a, b)| a.x * b.y - b.x * a.y)
                .sum::<f32>()
                / 2.
        })
        .sum::<f32>()
        .abs()
        * geometry.shape.rect.width
        * geometry.shape.rect.height
}

#[test]
fn four_operations_preserve_operands_and_use_paint_order_for_subtraction() {
    for (operation, expected) in [
        (Operation::Union, 15000.),
        (Operation::Subtract, 5000.),
        (Operation::Intersect, 5000.),
        (Operation::Exclude, 10000.),
    ] {
        let (mut h, shapes) = setup(operation);
        let original = shapes.clone();
        let mut cache = Cache::default();
        let g = cache.get(&h, &shapes, 3).unwrap();
        assert!((area(&g) - expected).abs() < 0.1);
        assert_eq!(shapes, original);
        if operation == Operation::Subtract {
            assert_eq!(g.shape.rect, rect(0., 0., 50., 100.));
            h.order = vec![3, 2, 1];
            assert_eq!(
                cache.get(&h, &shapes, 3).unwrap().shape.rect,
                rect(100., 0., 50., 100.)
            );
        }
    }
}

#[test]
fn holes_disjoint_regions_and_empty_results_recompute_after_child_edits() {
    let (h, mut shapes) = setup(Operation::Subtract);
    shapes[1].rect = rect(25., 25., 50., 50.);
    let mut cache = Cache::default();
    let g = cache.get(&h, &shapes, 3).unwrap();
    assert_eq!(g.contours.len(), 2);
    assert!((area(&g) - 7500.).abs() < 0.1);
    assert!(Arc::ptr_eq(
        &g.contours,
        &cache.get(&h, &shapes, 3).unwrap().contours
    ));
    shapes[1].rect = rect(0., 0., 100., 100.);
    assert!(cache.get(&h, &shapes, 3).unwrap().contours.is_empty());
    shapes[1].rect = rect(40., -10., 20., 120.);
    let g = cache.get(&h, &shapes, 3).unwrap();
    assert_eq!(g.contours.len(), 2);
    assert!((area(&g) - 8000.).abs() < 0.1);
}

#[test]
fn rounded_curves_rotation_and_nested_groups_remain_composable() {
    let (mut h, mut shapes) = setup(Operation::Union);
    shapes[0].kind = ShapeKind::Ellipse;
    shapes[0].rect = rect(0., 0., 100., 50.);
    shapes[0].layer.rotation = 90.;
    shapes[1].layer.hidden = true;
    let mut cache = Cache::default();
    let g = cache.get(&h, &shapes, 3).unwrap();
    assert!((area(&g) - std::f32::consts::PI * 50. * 25.).abs() < 3.);
    assert!((g.shape.rect.width - 50.).abs() < 0.01);
    assert!((g.shape.rect.height - 100.).abs() < 0.01);
    let mut outer = h.groups[&3].clone();
    outer.uid = uuid::Uuid::new_v4();
    outer.boolean = Some(Operation::Subtract);
    h.groups.insert(5, outer);
    h.parents.extend([(3, 5), (4, 5)]);
    h.order = vec![5, 3, 1, 2, 4];
    shapes.push(Shape::new(
        4,
        None,
        ShapeKind::Rectangle,
        rect(25., -25., 50., 100.),
    ));
    assert!(cache.get(&h, &shapes, 5).unwrap().contours.is_empty());
}
