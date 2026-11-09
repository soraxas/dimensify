use super::*;
use bevy::{picking::hover::HoverMap, text::EditableText};

fn app() -> App {
    let mut app = App::new();
    app.insert_resource(Settings::new(PointcloudSettings::default().splats))
        .insert_resource(CircleMesh(Handle::default()))
        .init_resource::<InputFocus>()
        .add_observer(activate)
        .add_observer(edit_number)
        .add_systems(Update, apply);
    app
}

fn click(app: &mut App, field: Field) {
    let entity = app.world_mut().spawn(Control(field, 0)).id();
    app.world_mut().trigger(Activate { entity });
    app.update();
}

#[test]
fn live_shape_orientation_and_budget_edits_reach_existing_and_new_clouds() {
    let mut app = app();
    let cloud = app.world_mut().spawn(PointCloud3d(Handle::default())).id();
    let camera = app
        .world_mut()
        .spawn((ViewerCamera, PointCloudVisibilitySettings::default()))
        .id();
    click(&mut app, Field::Shape);
    let splats = app.world().get::<SplatSettings>(cloud).unwrap();
    assert!(
        splats.splat.is_some(),
        "circle mesh must replace the billboard geometry"
    );
    assert_eq!(
        splats.radius, None,
        "circle mesh does not need fragment clipping"
    );
    click(&mut app, Field::Shape);
    let splats = app.world().get::<SplatSettings>(cloud).unwrap();
    assert!(splats.splat.is_none());
    assert_eq!(
        splats.radius, None,
        "square points must disable circular clipping"
    );
    click(&mut app, Field::Shape);
    assert_eq!(
        app.world().get::<SplatSettings>(cloud).unwrap().radius,
        Some(0.5)
    );
    click(&mut app, Field::Orientation);
    click(&mut app, Field::Adaptive);
    let budget = app.world_mut().spawn(Control(Field::Budget, 0)).id();
    app.world_mut().trigger(ValueChange {
        source: budget,
        value: 750_000_f32,
        is_final: true,
    });
    let next_cloud = app.world_mut().spawn(PointCloud3d(Handle::default())).id();
    app.update();
    for entity in [cloud, next_cloud] {
        let splats = app.world().get::<SplatSettings>(entity).unwrap();
        assert_eq!(splats.orientation, SplatOrientation::FaceNormal);
        assert!(!splats.adaptive_point_size);
    }
    assert_eq!(
        app.world()
            .get::<PointCloudVisibilitySettings>(camera)
            .unwrap()
            .point_budget,
        Some(750_000)
    );
    click(&mut app, Field::Reset);
    assert_eq!(
        *app.world().get::<SplatSettings>(cloud).unwrap(),
        PointcloudSettings::default().splats
    );
    assert_eq!(
        app.world()
            .get::<PointCloudVisibilitySettings>(camera)
            .unwrap()
            .point_budget,
        Some(5_000_000)
    );
}

#[test]
fn size_units_cycle_and_reset_restores_the_source_settings() {
    let mut app = app();
    let initial = SplatSettings {
        point_size: 7.,
        point_size_mode: PointSizeMode::WorldSpace,
        ..default()
    };
    app.insert_resource(Settings::new(initial.clone()));
    let cloud = app.world_mut().spawn(PointCloud3d(Handle::default())).id();
    for want in [
        PointSizeMode::LocalSpace,
        PointSizeMode::ScreenPixels,
        PointSizeMode::ScreenPixelsLocal,
        PointSizeMode::WorldSpace,
    ] {
        click(&mut app, Field::Units);
        assert_eq!(
            app.world()
                .get::<SplatSettings>(cloud)
                .unwrap()
                .point_size_mode,
            want
        );
    }
    click(&mut app, Field::Adaptive);
    click(&mut app, Field::Reset);
    assert_eq!(*app.world().get::<SplatSettings>(cloud).unwrap(), initial);
}

#[test]
fn number_editor_blocks_navigation_and_shortcuts_until_a_canvas_press() {
    let mut app = app();
    app.init_resource::<NavigationConfig>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<Touches>()
        .init_resource::<HoverMap>()
        .add_message::<CameraAction>()
        .add_systems(Update, (ui::capture_input, super::super::keyboard).chain());
    let editor = app.world_mut().spawn(EditableText::default()).id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, bevy::input_focus::FocusCause::Navigated);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyR);
    app.update();
    assert!(app.world().resource::<NavigationConfig>().blocked);
    assert!(app.world().resource::<Messages<CameraAction>>().is_empty());
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    assert!(!app.world().resource::<NavigationConfig>().blocked);
    assert_eq!(app.world().resource::<InputFocus>().get(), None);
}

// A minimum larger than the maximum must never reach the GPU as reversed bounds.
#[test]
fn numeric_edits_keep_pixel_bounds_ordered_and_reject_nonfinite_values() {
    let mut app = App::new();
    app.insert_resource(Settings::new(PointcloudSettings::default().splats))
        .add_observer(edit_number);
    let min = app.world_mut().spawn(Control(Field::MinSize, 0)).id();
    let max = app.world_mut().spawn(Control(Field::MaxSize, 0)).id();
    app.world_mut().trigger(ValueChange {
        source: min,
        value: 64_f32,
        is_final: true,
    });
    let settings = &app.world().resource::<Settings>().splats;
    assert_eq!(settings.min_point_size, Some(64.));
    assert_eq!(settings.max_point_size, Some(64.));
    app.world_mut().trigger(ValueChange {
        source: max,
        value: 4_f32,
        is_final: true,
    });
    let settings = &app.world().resource::<Settings>().splats;
    assert_eq!(settings.min_point_size, Some(4.));
    assert_eq!(settings.max_point_size, Some(4.));
    app.world_mut().trigger(ValueChange {
        source: min,
        value: f32::NAN,
        is_final: true,
    });
    assert_eq!(
        app.world().resource::<Settings>().splats.min_point_size,
        Some(4.)
    );
    app.world_mut().trigger(ValueChange {
        source: max,
        value: 0_f32,
        is_final: true,
    });
    assert_eq!(
        app.world().resource::<Settings>().splats.max_point_size,
        None
    );
    app.world_mut().trigger(ValueChange {
        source: min,
        value: 8_f32,
        is_final: false,
    });
    assert_eq!(
        app.world().resource::<Settings>().splats.min_point_size,
        Some(8.)
    );
}
