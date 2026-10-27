//! Optional standalone URDF kinematic viewer for Dimensify.

mod loading;
mod viewer;

use bevy::prelude::*;
use eyre::{Result, bail, ensure};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
use urdf_rs::JointType;

/// Self-contained demo, available even when launched outside the asset directory.
pub const DEMO_URDF: &str = include_str!("../../../../assets/robots/demo.urdf");

/// Parsed robot and joint positions in the original right-handed Z-up frame.
#[derive(Resource)]
pub struct RobotModel {
    robot: urdf_rs::Robot,
    positions: Vec<f32>,
    visuals: Vec<Vec<loading::VisualPart>>,
    root: usize,
}

impl RobotModel {
    /// Parse a local URDF and load its visual geometry before opening a window.
    pub fn from_urdf(xml: &str, directory: &Path) -> Result<Self> {
        let robot = urdf_rs::read_from_string(xml)?;
        let root = validate(&robot)?;
        let visuals = loading::load_visuals(&robot, directory)?;
        let positions = robot
            .joints
            .iter()
            .map(|joint| {
                if matches!(joint.joint_type, JointType::Revolute | JointType::Prismatic) {
                    0.0_f32.clamp(joint.limit.lower as f32, joint.limit.upper as f32)
                } else {
                    0.0
                }
            })
            .collect();
        Ok(Self {
            robot,
            positions,
            visuals,
            root,
        })
    }

    /// Load a URDF, resolving meshes relative to its containing directory.
    pub fn from_file(path: &Path) -> Result<Self> {
        let path = path.canonicalize()?;
        Self::from_urdf(&std::fs::read_to_string(&path)?, path.parent().unwrap())
    }

    /// Return independently controllable joint indices and names in URDF order.
    pub fn controllable_joints(&self) -> impl Iterator<Item = (usize, &str)> {
        self.robot
            .joints
            .iter()
            .enumerate()
            .filter(|(_, joint)| joint.joint_type != JointType::Fixed && joint.mimic.is_none())
            .map(|(i, joint)| (i, joint.name.as_str()))
    }

    /// Return a joint's current position, including any mimic multiplier and offset.
    pub fn joint_position(&self, index: usize) -> f32 {
        let joint = &self.robot.joints[index];
        if let Some(mimic) = &joint.mimic {
            let source = self
                .robot
                .joints
                .iter()
                .position(|j| j.name == mimic.joint)
                .unwrap();
            self.joint_position(source) * mimic.multiplier.unwrap_or(1.0) as f32
                + mimic.offset.unwrap_or(0.0) as f32
        } else {
            self.positions[index]
        }
    }

    /// Set an independent joint, clamping bounded joints to their URDF limits.
    pub fn set_joint_position(&mut self, index: usize, value: f32) -> Result<()> {
        ensure!(value.is_finite(), "Joint position must be finite");
        let joint = self
            .robot
            .joints
            .get(index)
            .ok_or_else(|| eyre::eyre!("Unknown joint index {index}"))?;
        ensure!(
            joint.mimic.is_none() && joint.joint_type != JointType::Fixed,
            "Joint {} is not independently controllable",
            joint.name
        );
        self.positions[index] = match joint.joint_type {
            JointType::Revolute | JointType::Prismatic => {
                value.clamp(joint.limit.lower as f32, joint.limit.upper as f32)
            }
            _ => value,
        };
        Ok(())
    }
}

/// Install robot entities and forward-kinematics updates without window or UI dependencies.
pub struct RobotPlugin;

impl Plugin for RobotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_robot).add_systems(
            PostUpdate,
            sync_joints.before(bevy::transform::TransformSystems::Propagate),
        );
    }
}

/// Open the standalone native viewer, or render one offscreen screenshot and exit.
pub fn run(model: RobotModel, screenshot: Option<std::path::PathBuf>) -> Result<()> {
    viewer::run(model, screenshot)
}

/// Spawn and tick the robot without a window or GPU, checking finite world transforms.
pub fn check(model: RobotModel) -> Result<()> {
    let name = model.robot.name.clone();
    let joint_count = model.controllable_joints().count();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .insert_resource(model)
        .add_plugins(RobotPlugin);
    app.update();
    let world = app.world_mut();
    let count = world.query::<&Mesh3d>().iter(world).count();
    for transform in world.query::<&GlobalTransform>().iter(world) {
        ensure!(
            transform.to_matrix().is_finite(),
            "Non-finite robot transform"
        );
    }
    println!("Loaded {name}: {count} visual meshes, {joint_count} controllable joints");
    Ok(())
}

#[derive(Component)]
struct JointIndex(usize);

fn vector(v: &[f64; 3]) -> Vec3 {
    Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)
}

fn pose(p: &urdf_rs::Pose) -> Transform {
    Transform::from_translation(vector(&p.xyz.0)).with_rotation(Quat::from_euler(
        EulerRot::ZYX,
        p.rpy[2] as f32,
        p.rpy[1] as f32,
        p.rpy[0] as f32,
    ))
}

fn joint_transform(joint: &urdf_rs::Joint, value: f32) -> Transform {
    let axis = vector(&joint.axis.xyz.0).normalize_or_zero();
    let motion = match joint.joint_type {
        JointType::Revolute | JointType::Continuous => {
            Transform::from_rotation(Quat::from_axis_angle(axis, value))
        }
        JointType::Prismatic => Transform::from_translation(axis * value),
        _ => Transform::IDENTITY,
    };
    pose(&joint.origin).mul_transform(motion)
}

fn spawn_robot(
    mut commands: Commands,
    mut model: ResMut<RobotModel>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // All link, joint and visual transforms remain Z-up under this one boundary.
    let root = commands
        .spawn((
            Name::new(model.robot.name.clone()),
            Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            Visibility::default(),
        ))
        .id();
    let mut links = HashMap::new();
    for link in &model.robot.links {
        let entity = commands
            .spawn((
                Name::new(link.name.clone()),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        links.insert(link.name.clone(), entity);
    }
    commands
        .entity(links[&model.robot.links[model.root].name])
        .insert(ChildOf(root));
    for (index, joint) in model.robot.joints.iter().enumerate() {
        commands.entity(links[&joint.child.link]).insert((
            ChildOf(links[&joint.parent.link]),
            JointIndex(index),
            joint_transform(joint, model.joint_position(index)),
        ));
    }
    let visuals = std::mem::take(&mut model.visuals);
    for (link, parts) in model.robot.links.iter().zip(visuals) {
        for part in parts {
            commands.spawn((
                ChildOf(links[&link.name]),
                Mesh3d(meshes.add(part.mesh)),
                MeshMaterial3d(materials.add(part.material)),
                part.transform,
            ));
        }
    }
}

fn sync_joints(model: Res<RobotModel>, mut joints: Query<(&JointIndex, &mut Transform)>) {
    if !model.is_changed() {
        return;
    }
    for (index, mut transform) in &mut joints {
        *transform = joint_transform(&model.robot.joints[index.0], model.joint_position(index.0));
    }
}

fn validate(robot: &urdf_rs::Robot) -> Result<usize> {
    ensure!(!robot.links.is_empty(), "URDF has no links");
    let links: HashSet<_> = robot.links.iter().map(|l| l.name.as_str()).collect();
    ensure!(links.len() == robot.links.len(), "Duplicate link names");
    let joint_names: HashSet<_> = robot.joints.iter().map(|j| j.name.as_str()).collect();
    ensure!(
        joint_names.len() == robot.joints.len(),
        "Duplicate joint names"
    );
    let mut parents = HashMap::new();
    for joint in &robot.joints {
        ensure!(
            links.contains(joint.parent.link.as_str()) && links.contains(joint.child.link.as_str()),
            "Joint {} refers to a missing link",
            joint.name
        );
        ensure!(
            parents
                .insert(joint.child.link.as_str(), joint.parent.link.as_str())
                .is_none(),
            "Link {} has multiple parents",
            joint.child.link
        );
        ensure!(
            pose(&joint.origin).to_matrix().is_finite(),
            "Joint {} has a non-finite origin",
            joint.name
        );
        match joint.joint_type {
            JointType::Fixed => (),
            JointType::Revolute | JointType::Continuous | JointType::Prismatic => {
                let axis = vector(&joint.axis.xyz.0);
                ensure!(
                    axis.is_finite() && axis.length_squared() > 1e-12,
                    "Invalid axis for {}",
                    joint.name
                );
                if joint.joint_type != JointType::Continuous {
                    let lower = joint.limit.lower as f32;
                    let upper = joint.limit.upper as f32;
                    ensure!(
                        lower.is_finite() && upper.is_finite() && lower <= upper,
                        "Invalid limits for {}",
                        joint.name
                    );
                }
            }
            _ => bail!(
                "Joint {} uses unsupported type {:?}",
                joint.name,
                joint.joint_type
            ),
        }
        let mut seen = HashSet::new();
        let mut current = joint;
        while let Some(mimic) = &current.mimic {
            ensure!(seen.insert(current.name.as_str()), "Cyclic mimic joints");
            ensure!(
                (mimic.multiplier.unwrap_or(1.0) as f32).is_finite()
                    && (mimic.offset.unwrap_or(0.0) as f32).is_finite(),
                "Invalid mimic parameters"
            );
            current = robot
                .joints
                .iter()
                .find(|j| j.name == mimic.joint)
                .ok_or_else(|| eyre::eyre!("Missing mimic joint {}", mimic.joint))?;
        }
    }
    let roots: Vec<_> = robot
        .links
        .iter()
        .enumerate()
        .filter(|(_, l)| !parents.contains_key(l.name.as_str()))
        .collect();
    ensure!(roots.len() == 1, "URDF must have exactly one root link");
    for link in &robot.links {
        let mut seen = HashSet::new();
        let mut current = link.name.as_str();
        while let Some(parent) = parents.get(current) {
            ensure!(seen.insert(current), "Cyclic joint hierarchy");
            current = parent;
        }
    }
    Ok(roots[0].0)
}

#[cfg(test)]
mod tests;
