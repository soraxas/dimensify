//! Shared optional ground reference for standalone viewers.
use bevy::{
    camera::visibility::VisibilitySystems,
    dev_tools::infinite_grid::{InfiniteGrid, InfiniteGridPlugin},
    prelude::*,
};

/// The infinite XZ ground grid at Y=0. Disabled until the user enables it.
#[derive(Resource, Default)]
pub struct ViewerGrid {
    pub enabled: bool,
}

/// Attach to the text child of a grid toolbar button to synchronize its label.
#[derive(Component)]
pub struct GridLabel;

#[derive(Component)]
struct GroundGrid;

pub(super) struct ViewerGridPlugin;
impl Plugin for ViewerGridPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<InfiniteGridPlugin>() {
            app.add_plugins(InfiniteGridPlugin);
        }
        app.init_resource::<ViewerGrid>()
            .add_systems(Startup, setup)
            .add_systems(
                PostUpdate,
                sync.before(VisibilitySystems::VisibilityPropagate),
            );
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((GroundGrid, InfiniteGrid, Visibility::Hidden));
}

fn sync(
    mut commands: Commands,
    grid: Res<ViewerGrid>,
    mut planes: Query<&mut Visibility, With<GroundGrid>>,
    mut labels: Query<(&mut Text, &ChildOf), With<GridLabel>>,
    buttons: Query<&AccessibleLabel>,
) {
    let visibility = if grid.enabled {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut plane in &mut planes {
        if *plane != visibility {
            *plane = visibility;
        }
    }
    let label = if grid.enabled {
        "Grid: On"
    } else {
        "Grid: Off"
    };
    for (mut text, parent) in &mut labels {
        if text.0 != label {
            text.0 = label.into();
        }
        if let Ok(accessible) = buttons.get(parent.parent())
            && accessible.0 != label
        {
            commands
                .entity(parent.parent())
                .insert(AccessibleLabel(label.into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_starts_hidden_and_reuses_the_plane_when_toggled() {
        let mut app = App::new();
        app.init_resource::<ViewerGrid>()
            .add_systems(Startup, setup)
            .add_systems(Update, sync);
        let button = app
            .world_mut()
            .spawn(AccessibleLabel("Grid: Off".into()))
            .id();
        let text = app
            .world_mut()
            .spawn((GridLabel, Text::default(), ChildOf(button)))
            .id();
        for enabled in [false, true, false, true] {
            app.world_mut().resource_mut::<ViewerGrid>().enabled = enabled;
            app.update();
            let world = app.world_mut();
            let mut planes = world.query_filtered::<&Visibility, With<GroundGrid>>();
            let visibility = planes.single(world).unwrap();
            assert_eq!(
                *visibility,
                if enabled {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                }
            );
            let label = if enabled { "Grid: On" } else { "Grid: Off" };
            assert_eq!(world.get::<Text>(text).unwrap().0, label);
            assert_eq!(world.get::<AccessibleLabel>(button).unwrap().0, label);
        }
    }
}
