//! Live controls over the existing point-cloud renderer components.
use super::*;
use bevy::{
    input_focus::InputFocus,
    ui_widgets::{Activate, ValueChange},
};

mod ui;
use renderer::{PointCloud3d, PointSizeMode, SplatOrientation, SplatSettings};

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Field {
    Open,
    Size,
    MinSize,
    MaxSize,
    Units,
    Adaptive,
    Shape,
    Orientation,
    Budget,
    NodeRadius,
    Reset,
    SizingInfo,
}

#[derive(Component)]
struct Control(Field, i32);
#[derive(Component)]
struct Caption(Field);
#[derive(Component)]
struct Panel;
#[derive(Resource)]
struct CircleMesh(Handle<Mesh>);

#[derive(Resource)]
pub(super) struct Settings {
    open: bool,
    splats: SplatSettings,
    initial: SplatSettings,
    budget: usize,
    node_radius: f32,
    spacing_available: bool,
    pixels_per_world: f32,
    cloud_scale: f32,
}

impl Settings {
    pub(super) fn close(&mut self) {
        self.open = false;
    }
    pub(super) fn new(splats: SplatSettings) -> Self {
        Self {
            open: false,
            initial: splats.clone(),
            splats,
            budget: 5_000_000,
            node_radius: 6.,
            spacing_available: false,
            pixels_per_world: 1.,
            cloud_scale: 1.,
        }
    }

    fn number(&self, field: Field) -> f32 {
        match field {
            Field::Size => self.splats.point_size,
            Field::MinSize => self.splats.min_point_size.unwrap_or(0.),
            Field::MaxSize => self.splats.max_point_size.unwrap_or(0.),
            Field::Budget => self.budget as f32,
            Field::NodeRadius => self.node_radius,
            _ => 0.,
        }
    }

    fn edit(&mut self, field: Field, value: f32) {
        if !value.is_finite() {
            return;
        }
        match field {
            Field::Size => self.splats.point_size = value.clamp(0.001, 1024.),
            Field::MinSize => {
                let value = value.clamp(0., 1024.);
                self.splats.min_point_size = (value > 0.).then_some(value);
                if self.splats.max_point_size.is_some_and(|max| max < value) {
                    self.splats.max_point_size = Some(value);
                }
            }
            Field::MaxSize => {
                let value = value.clamp(0., 1024.);
                self.splats.max_point_size = (value > 0.).then_some(value);
                if value > 0. && self.splats.min_point_size.is_some_and(|min| min > value) {
                    self.splats.min_point_size = Some(value);
                }
            }
            Field::Budget => self.budget = value.round().clamp(10_000., 20_000_000.) as usize,
            Field::NodeRadius => self.node_radius = value.clamp(0.1, 256.),
            _ => {}
        }
    }

    fn caption(&self, field: Field) -> String {
        match field {
            Field::Open => if self.open {
                "Close settings"
            } else {
                "Settings"
            }
            .into(),
            Field::Units => format!(
                "Units: {}",
                match self.splats.point_size_mode {
                    PointSizeMode::ScreenPixels => "Screen pixels",
                    PointSizeMode::ScreenPixelsLocal => "Scaled pixels",
                    PointSizeMode::WorldSpace => "World units",
                    PointSizeMode::LocalSpace => "Local units",
                }
            ),
            Field::Adaptive => format!(
                "Adaptive size: {}",
                if !self.spacing_available {
                    "unavailable"
                } else if self.splats.adaptive_point_size {
                    "On"
                } else {
                    "Off"
                }
            ),
            Field::SizingInfo => if self.spacing_available {
                "Adaptive uses source spacing and selected LOD. Pixel limits can reduce surface coverage when zoomed in."
            } else {
                "This source has no spacing metadata: manual sizing is active. World/local units grow with zoom; screen pixels stay fixed. Pixel limits can change coverage."
            }.into(),
            Field::Shape => format!(
                "Shape: {}",
                if self.splats.splat.is_some() {
                    "Circle mesh"
                } else if self.splats.radius.is_some() {
                    "Circular billboard"
                } else {
                    "Square billboard"
                }
            ),
            Field::Orientation => format!(
                "Orientation: {}",
                if self.splats.orientation == SplatOrientation::Billboard {
                    "Camera-facing"
                } else {
                    "Normal-aligned"
                }
            ),
            _ => String::new(),
        }
    }
}

pub(super) fn install(app: &mut App) {
    app.add_observer(activate)
        .add_observer(edit_number)
        .add_systems(Startup, ui::setup)
        .add_systems(
            Update,
            (apply, ui::sync_ui, ui::sync_numbers, ui::scroll).chain(),
        )
        .add_systems(
            Update,
            ui::capture_input
                .before(dimensify_camera::viewer::NavigationSystems)
                .before(super::keyboard),
        );
}

fn edit_number(
    event: On<ValueChange<f32>>,
    controls: Query<&Control>,
    mut state: ResMut<Settings>,
) {
    // Keep valid edits live so the next Update cannot replace a just-blurred
    // buffer before Feathers emits its final focus-loss notification.
    if let Ok(control) = controls.get(event.source) {
        state.edit(control.0, event.value);
    }
}

fn activate(
    event: On<Activate>,
    controls: Query<&Control>,
    circle: Res<CircleMesh>,
    mut state: ResMut<Settings>,
    mut focus: ResMut<InputFocus>,
    panel: Option<ResMut<super::open::OpenPanel>>,
) {
    let Ok(&Control(field, direction)) = controls.get(event.entity) else {
        return;
    };
    match field {
        Field::Open => {
            state.open = !state.open;
            if state.open
                && let Some(mut panel) = panel
            {
                panel.open = false;
            }
            focus.clear();
        }
        Field::Reset => {
            state.splats = state.initial.clone();
            state.budget = 5_000_000;
            state.node_radius = 6.;
            focus.clear();
        }
        Field::Units => {
            // Preserve the footprint at the cloud centre when changing units.
            let scale = state.cloud_scale;
            let projection = state.pixels_per_world;
            let world_size = match state.splats.point_size_mode {
                PointSizeMode::ScreenPixels => state.splats.point_size / projection,
                PointSizeMode::ScreenPixelsLocal => state.splats.point_size * scale / projection,
                PointSizeMode::WorldSpace => state.splats.point_size,
                PointSizeMode::LocalSpace => state.splats.point_size * scale,
            };
            state.splats.point_size_mode = match state.splats.point_size_mode {
                PointSizeMode::ScreenPixels => PointSizeMode::ScreenPixelsLocal,
                PointSizeMode::ScreenPixelsLocal => PointSizeMode::WorldSpace,
                PointSizeMode::WorldSpace => PointSizeMode::LocalSpace,
                PointSizeMode::LocalSpace => PointSizeMode::ScreenPixels,
            };
            state.splats.point_size = match state.splats.point_size_mode {
                PointSizeMode::ScreenPixels => world_size * projection,
                PointSizeMode::ScreenPixelsLocal => world_size * projection / scale,
                PointSizeMode::WorldSpace => world_size,
                PointSizeMode::LocalSpace => world_size / scale,
            }
            .clamp(0.001, 1024.);
        }
        Field::Adaptive if state.spacing_available => {
            state.splats.adaptive_point_size = !state.splats.adaptive_point_size
        }
        Field::Orientation => {
            state.splats.orientation = if state.splats.orientation == SplatOrientation::Billboard {
                SplatOrientation::FaceNormal
            } else {
                SplatOrientation::Billboard
            }
        }
        Field::Shape => {
            if state.splats.splat.is_some() {
                state.splats.splat = None;
                state.splats.radius = None;
            } else if state.splats.radius.is_none() {
                state.splats.radius = Some(0.5);
            } else {
                state.splats.splat = Some(circle.0.clone());
                state.splats.radius = None;
            }
        }
        _ if direction != 0 => {
            let step = match field {
                Field::Budget => 250_000.,
                Field::Size if state.splats.point_size <= 1. => 0.1,
                _ => 1.,
            };
            let value = state.number(field) + step * direction as f32;
            state.edit(field, value);
            focus.clear();
        }
        _ => {}
    }
}

fn apply(
    mut state: ResMut<Settings>,
    assets: Res<Assets<renderer::PointCloud>>,
    mut clouds: Query<(&PointCloud3d, &mut SplatSettings, Option<&GlobalTransform>)>,
    mut cameras: Query<&mut PointCloudVisibilitySettings, With<ViewerCamera>>,
    views: Query<(&Camera, &Projection, &GlobalTransform), With<ViewerCamera>>,
) {
    state.spacing_available = false;
    for (handle, mut splats, transform) in &mut clouds {
        let asset = assets.get(&handle.0);
        let available = asset
            .and_then(|a| a.spacing)
            .is_some_and(|s| s.is_finite() && s > 0.);
        state.spacing_available |= available;
        let mut effective = state.splats.clone();
        effective.adaptive_point_size &= available;
        if *splats != effective {
            *splats = effective;
        }
        if let (Some(transform), Some((camera, projection, view))) =
            (transform, views.iter().next())
        {
            state.cloud_scale = transform
                .compute_transform()
                .scale
                .abs()
                .max_element()
                .max(0.0001);
            let centre = asset
                .and_then(|a| a.aabb)
                .map_or(Vec3::ZERO, |a| a.center.into());
            let view_position = view
                .affine()
                .inverse()
                .transform_point3(transform.transform_point(centre));
            let clip = projection.get_clip_from_view() * view_position.extend(1.);
            if let Some(viewport) = camera.physical_viewport_size() {
                let factor = (0.5 * viewport.y as f32 * projection.get_clip_from_view().y_axis.y
                    / clip.w)
                    .abs();
                if factor.is_finite() && factor > 0.0001 {
                    state.pixels_per_world = factor;
                }
            }
        }
    }
    for mut camera in &mut cameras {
        if camera.point_budget != Some(state.budget) {
            camera.point_budget = Some(state.budget);
        }
        if camera.min_radius != Some(state.node_radius) {
            camera.min_radius = Some(state.node_radius);
        }
    }
}

#[cfg(test)]
mod tests;
