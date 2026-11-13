use super::*;
use std::time::Duration;

fn app() -> App {
    let mut app = App::new();
    app.init_resource::<Time<Real>>()
        .init_resource::<InputFocus>()
        .init_resource::<Motion>()
        .add_systems(PostUpdate, (animate_panels, block_panel_input).chain());
    app
}

fn step(app: &mut App, seconds: f32) {
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_secs_f32(seconds));
    app.update();
}

#[test]
fn closing_panel_keeps_rendering_but_blocks_descendant_input_until_reopened() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn((PanelMotion::new(true), Node::default()))
        .id();
    let button = app
        .world_mut()
        .spawn((Node::default(), TabIndex(3), ChildOf(root)))
        .id();
    let disabled = app
        .world_mut()
        .spawn((Node::default(), InteractionDisabled, ChildOf(root)))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(button, bevy::input_focus::FocusCause::Navigated);
    app.world_mut()
        .get_mut::<PanelMotion>(root)
        .unwrap()
        .set_open(false);
    step(&mut app, 0.05);
    assert_eq!(
        app.world().get::<Node>(root).unwrap().display,
        Display::Flex
    );
    assert_ne!(
        app.world().get::<UiTransform>(root).unwrap().translation,
        Val2::ZERO
    );
    assert!(app.world().get::<InteractionDisabled>(button).is_some());
    assert_eq!(app.world().get::<TabIndex>(button).unwrap().0, -1);
    assert_eq!(app.world().get::<Pickable>(button), Some(&Pickable::IGNORE));
    assert_eq!(app.world().resource::<InputFocus>().get(), None);
    step(&mut app, 1.);
    assert_eq!(
        app.world().get::<Node>(root).unwrap().display,
        Display::None
    );
    app.world_mut()
        .get_mut::<PanelMotion>(root)
        .unwrap()
        .set_open(true);
    step(&mut app, 0.05);
    assert_eq!(
        app.world().get::<Node>(root).unwrap().display,
        Display::Flex
    );
    assert!(app.world().get::<InteractionDisabled>(button).is_none());
    assert_eq!(app.world().get::<TabIndex>(button).unwrap().0, 3);
    assert!(app.world().get::<Pickable>(button).is_none());
    assert!(app.world().get::<InteractionDisabled>(disabled).is_some());
}

#[test]
fn reduced_motion_hides_immediately_and_new_hidden_descendants_are_blocked() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn((PanelMotion::new(true), Node::default()))
        .id();
    app.world_mut()
        .get_mut::<PanelMotion>(root)
        .unwrap()
        .set_open(false);
    app.world_mut().resource_mut::<Motion>().reduced = true;
    step(&mut app, 0.);
    assert_eq!(
        app.world().get::<Node>(root).unwrap().display,
        Display::None
    );
    let child = app.world_mut().spawn((TabIndex(0), ChildOf(root))).id();
    step(&mut app, 0.);
    assert!(app.world().get::<InteractionDisabled>(child).is_some());
    assert_eq!(app.world().get::<TabIndex>(child).unwrap().0, -1);
}

#[test]
fn nested_closed_panels_remain_blocked_when_outer_panel_opens() {
    let mut app = app();
    let outer = app
        .world_mut()
        .spawn((PanelMotion::new(false), Node::default()))
        .id();
    let inner = app
        .world_mut()
        .spawn((PanelMotion::new(false), Node::default(), ChildOf(outer)))
        .id();
    let child = app.world_mut().spawn((TabIndex(0), ChildOf(inner))).id();
    step(&mut app, 0.);
    app.world_mut()
        .get_mut::<PanelMotion>(outer)
        .unwrap()
        .set_open(true);
    step(&mut app, 1.);
    assert!(app.world().get::<InteractionDisabled>(child).is_some());
    assert_eq!(app.world().get::<TabIndex>(child).unwrap().0, -1);
}

#[derive(Resource, Default)]
struct LayoutChanges(usize);
fn count_layout_changes(nodes: Query<(), Changed<Node>>, mut count: ResMut<LayoutChanges>) {
    count.0 = nodes.iter().count();
}

#[test]
fn settled_panels_do_not_invalidate_layout() {
    let mut app = app();
    app.init_resource::<LayoutChanges>()
        .add_systems(PostUpdate, count_layout_changes.after(animate_panels));
    app.world_mut()
        .spawn((PanelMotion::new(false), Node::default()));
    app.world_mut()
        .spawn((PanelMotion::new(true), Node::default()));
    step(&mut app, 1.);
    step(&mut app, 1.);
    assert_eq!(app.world().resource::<LayoutChanges>().0, 0);
}

#[test]
fn reveal_updates_grid_then_leaves_settled_layout_untouched() {
    let mut reveal = Reveal::new(false);
    let mut node = Node {
        display: Display::None,
        ..default()
    };
    assert!(reveal.update_node(&mut node, true, 0.1, false));
    assert_eq!(node.display, Display::Grid);
    assert_eq!(node.grid_template_rows, vec![GridTrack::flex(0.5)]);
    assert!(reveal.update_node(&mut node, true, 1., false));
    assert!(!reveal.update_node(&mut node, true, 1., false));
    assert!(reveal.update_node(&mut node, false, 0., true));
    assert_eq!(node.display, Display::None);
    assert!(!reveal.update_node(&mut node, false, 1., false));
}

#[test]
fn opening_preserves_field_focus_at_zero_delta_and_through_motion() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn((PanelMotion::new(false), Node::default()))
        .id();
    let field = app.world_mut().spawn((TabIndex(0), ChildOf(root))).id();
    step(&mut app, 0.);
    app.world_mut()
        .get_mut::<PanelMotion>(root)
        .unwrap()
        .set_open(true);
    // URL panel Update systems assign focus once when opening, before motion.
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(field, bevy::input_focus::FocusCause::Navigated);
    for seconds in [0., 0.05, 1.] {
        step(&mut app, seconds);
        assert_eq!(
            app.world().get::<Node>(root).unwrap().display,
            Display::Flex
        );
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
        assert!(app.world().get::<InteractionDisabled>(field).is_none());
        assert_eq!(app.world().get::<TabIndex>(field).unwrap().0, 0);
    }
}

#[test]
fn responsive_panels_clear_wrapped_toolbar_and_status_at_physical_scale() {
    let mut app = app();
    app.add_systems(PostUpdate, place_panels.before(animate_panels));
    let bar = app
        .world_mut()
        .spawn((
            Toolbar,
            ComputedNode {
                size: Vec2::new(656., 520.),
                inverse_scale_factor: 0.5,
                ..default()
            },
            UiGlobalTransform::from_xy(360., 292.),
        ))
        .id();
    app.world_mut().spawn((
        StatusCard,
        ComputedNode {
            size: Vec2::new(656., 240.),
            inverse_scale_factor: 0.5,
            ..default()
        },
    ));
    let panel = app
        .world_mut()
        .spawn((PanelMotion::new(true), panel_node(420.)))
        .id();
    step(&mut app, 0.);
    let node = app.world().get::<Node>(panel).unwrap();
    assert_eq!(node.top, px(284.)); // physical bottom 552 / 2 + 8
    assert_eq!(node.bottom, px(144.)); // status height 240 / 2 + 16 + 8
    // Shrinking the toolbar restores the default top inset, rather than retaining
    // the previous mobile height after a resize.
    app.world_mut().get_mut::<ComputedNode>(bar).unwrap().size.y = 80.;
    *app.world_mut().get_mut::<UiGlobalTransform>(bar).unwrap() =
        UiGlobalTransform::from_xy(360., 72.);
    step(&mut app, 0.);
    assert_eq!(app.world().get::<Node>(panel).unwrap().top, px(104.));
}
