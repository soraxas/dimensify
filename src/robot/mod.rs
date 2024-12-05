// use k::nalgebra::Isometry;
use log::debug;
use std::collections::HashMap;
use std::path::Path;

use crate::collision_checker::SimpleCollisionPipeline;
use crate::util::replace_package_with_base_dir;
use eyre::{Context, ContextCompat, OptionExt, Result};
use rapier3d::math::Real;
pub use rapier3d::prelude::ColliderHandle;
use rapier3d::{
    math::{Isometry, Point, Vector},
    na::{self},
    prelude::{ColliderBuilder, MeshConverter, SharedShape, TriMeshFlags},
};
use urdf_rs::{self, Geometry, Pose};

pub mod plugin;

pub struct Robot {
    // links: Vec<Link>,
    // joints: Vec<Joint>,
    pub collision_checker: SimpleCollisionPipeline,
    pub robot_chain: k::Chain<f32>,
    pub urdf_robot: urdf_rs::Robot,
    pub colliders: HashMap<String, Vec<ColliderHandle>>,
    pub joint_link_map: HashMap<String, String>,
}

fn pose_to_isometry(pose: &Pose) -> Isometry<Real> {
    Isometry::from_parts(
        Point::new(
            pose.xyz[0] as Real,
            pose.xyz[1] as Real,
            pose.xyz[2] as Real,
        )
        .into(),
        na::UnitQuaternion::from_euler_angles(
            pose.rpy[0] as Real,
            pose.rpy[1] as Real,
            pose.rpy[2] as Real,
        ),
    )
}

fn k_isometry_to_rapier(isometry: &k::Isometry3<f32>) -> Isometry<Real> {
    Isometry::from_parts(
        Point::new(
            isometry.translation.vector.x,
            isometry.translation.vector.y,
            isometry.translation.vector.z,
        )
        .into(),
        na::UnitQuaternion::from_euler_angles(
            isometry.rotation.euler_angles().0,
            isometry.rotation.euler_angles().1,
            isometry.rotation.euler_angles().2,
        ),
    )
}

pub fn geometry_to_colliders(
    mesh_dir: &Option<&str>,
    geometry: &Geometry,
    origin: &Pose,
) -> Vec<ColliderBuilder> {
    todo!("deal with bevy coordinate system transformation");

    let mut shape_transform = Isometry::identity();

    let collider_blueprint = ColliderBuilder::default().density(0.0);
    let trimesh_flags = TriMeshFlags::all();

    let mut colliders = Vec::new();
    match &geometry {
        Geometry::Box { size } => {
            todo!("deal with bevy coordinate system transformation");
            shape_transform = Isometry::rotation(Vector::x() * -std::f32::consts::FRAC_PI_2);
            colliders.push(SharedShape::cuboid(
                size[0] as Real / 2.0,
                size[1] as Real / 2.0,
                size[2] as Real / 2.0,
            ));
        }
        Geometry::Cylinder { radius, length } => {
            // This rotation will make the cylinder Z-up as per the URDF spec,
            // instead of rapier’s default Y-up.
            shape_transform = Isometry::rotation(Vector::x() * -std::f32::consts::FRAC_PI_2);
            colliders.push(SharedShape::cylinder(
                *length as Real / 2.0,
                *radius as Real,
            ));
        }
        Geometry::Sphere { radius } => {
            colliders.push(SharedShape::ball(*radius as Real));
        }
        Geometry::Mesh { filename, scale } => {
            let _scale = scale
                .map(|s| Vector::new(s[0] as Real, s[1] as Real, s[2] as Real))
                .unwrap_or_else(|| Vector::<Real>::repeat(1.0));

            let loader = mesh_loader::Loader::default();
            let full_path = replace_package_with_base_dir(filename, mesh_dir);
            if let Ok(scene) = loader.load(full_path) {
                for (raw_mesh, _) in scene.meshes.into_iter().zip(scene.materials) {
                    let vertices: Vec<_> = raw_mesh
                        .vertices
                        .iter()
                        .map(|xyz| Point::new(xyz[0], xyz[1], xyz[2]))
                        .collect();
                    let indices: Vec<_> = raw_mesh.faces;
                    let converter = MeshConverter::TriMeshWithFlags(trimesh_flags);
                    if let Ok((shape, _)) = converter.convert(vertices, indices) {
                        colliders.push(shape)
                    }
                }
            }
        }
        Geometry::Capsule { radius, length } => todo!(),
    };

    colliders
        .drain(..)
        .map(move |shape| {
            let mut builder = collider_blueprint.clone();
            builder.shape = shape;
            builder.position(pose_to_isometry(origin) * shape_transform)
        })
        .collect()
}

use thiserror::Error;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum RobotError {
    #[error("Failed to set joint positions: {0}")]
    FailedToSetJointPositions(#[from] k::Error),

    #[error("Failed to set joint positions: Joint limit out of bound")]
    SetJointLimitViolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UrdfRobotOption {
    pub collision_exclude_neighbour: bool,
}

impl Default for UrdfRobotOption {
    fn default() -> Self {
        Self {
            collision_exclude_neighbour: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionResult {
    Free,
    Collision,
    JointLimitViolation,
}

impl From<CollisionResult> for bool {
    fn from(val: CollisionResult) -> Self {
        match val {
            CollisionResult::Free => false,
            CollisionResult::Collision => true,
            CollisionResult::JointLimitViolation => true,
        }
    }
}

impl Robot {
    pub fn name(&self) -> &str {
        self.urdf_robot.name.as_str()
    }

    pub fn from_file(urdf_path: impl AsRef<Path>) -> Result<Self> {
        let urdf_robot: urdf_rs::Robot = urdf_rs::read_file(urdf_path.as_ref())?;
        Self::from_urdf_robot(
            urdf_robot,
            urdf_path.as_ref().parent().and_then(|p| p.to_str()),
        )
    }

    pub fn from_urdf_robot(urdf_robot: urdf_rs::Robot, base_dir: Option<&str>) -> Result<Self> {
        let colliders_mappings = HashMap::new();

        let collision_checker = SimpleCollisionPipeline::default();

        let option = UrdfRobotOption::default();

        let mut mapping_parent_to_child = HashMap::new();
        let mut mapping_child_to_parent = HashMap::new();
        // build a mappint to mapping if we want to exclude neighbour collision
        if option.collision_exclude_neighbour {
            let mut link_name_to_idx = HashMap::new();
            // build a mapping from link_name to link_idx
            for (link_idx, link) in urdf_robot.links.iter().enumerate() {
                link_name_to_idx.insert(link.name.as_str(), link_idx);
            }

            for joint in urdf_robot.joints.iter() {
                mapping_child_to_parent.insert(
                    joint.child.link.as_str(),
                    *link_name_to_idx
                        .get(joint.parent.link.as_str())
                        .expect("internal logic error: failed to map link name to index"),
                );
                mapping_parent_to_child.insert(
                    joint.parent.link.as_str(),
                    *link_name_to_idx
                        .get(joint.child.link.as_str())
                        .expect("internal logic error: failed to map link name to index"),
                );
            }
        }

        Ok(Self {
            joint_link_map: k::urdf::joint_to_link_map(&urdf_robot),
            robot_chain: urdf_robot.clone().into(),
            urdf_robot,
            colliders: colliders_mappings,
            collision_checker,
        })
    }

    pub fn set_joints(&mut self, joints: &[f32]) -> Result<()> {
        let result = self.robot_chain.set_joint_positions(joints);

        // this error is mapped to collided result
        if let Err(k::Error::OutOfLimitError {
            joint_name: _,
            position: _,
            max_limit: _,
            min_limit: _,
        }) = &result
        {
            debug!("{:#?}", &result);
            return Err(RobotError::SetJointLimitViolation.into());
        }

        // if there's error in setting joint positions, return error
        if let Err(e) = result {
            return Err(RobotError::FailedToSetJointPositions(e).into());
        }

        Ok(())
    }

    pub fn has_collision(&mut self) -> Result<CollisionResult> {
        // .map_err(|e| match e {
        //         k::Error::OutOfLimitError { joint_name, position, max_limit, min_limit } => return CollisionResult::OutOfJointLimit,
        //         // // k::Error::SetToFixedError { joint_name } => todo!(),
        //         // k::Error::SizeMismatchError { input, required } => todo!(),
        //         // k::Error::MimicError { from, to } => todo!(),
        //         // k::Error::NotConvergedError { num_tried, position_diff, rotation_diff } => todo!(),
        //         // k::Error::InverseMatrixError => todo!(),
        //         // k::Error::PreconditionError { dof, necessary_dof } => todo!(),
        //         // k::Error::InvalidJointNameError { joint_name } => todo!(),
        //         e => e,
        //     })?;

        // self.robot_chain
        //     .set_joint_positions(joints)
        //     .wrap_err("Failed to set joint positions")?;

        self.robot_chain.update_transforms();

        for link_node in self.robot_chain.iter() {
            let trans = link_node
                .world_transform()
                .wrap_err("Failed to get world transform")?;
            let joint_name = &link_node.joint().name;
            let link_name = self
                .joint_link_map
                .get(joint_name)
                .wrap_err("Failed to map joint to link_node (internal error)")?;

            // link_node.link().unwrap().
            // dbg!(trans);
            // dbg!(joint_name);
            // dbg!(link_name);
            // dbg!(&self.colliders);

            let collider_handles = self
                .colliders
                .get(link_name)
                .wrap_err_with(|| format!("Couldn't find colliders for link: {}", link_name))?;

            let trans = k_isometry_to_rapier(&trans);

            for handle in collider_handles {
                let collider = self
                    .collision_checker
                    .collider_set
                    .get_mut(*handle)
                    .ok_or_eyre("cannot find collider")?;

                collider.set_position(trans);
            }

            // link.link().

            // if let Some(id) = robot_state.link_names_to_entity.get(link_name) {
            //     // query.get_mut(*id).unwrap();
            //     query.get_mut(*id).unwrap();
            //     if let Ok((link, mut transform)) = query.get_mut(*id) {
            //         dbg!(&id);
            //         //    dbg!(&query);
            //         *transform = Transform {
            //             translation: [
            //                 trans.translation.vector.x,
            //                 trans.translation.vector.y,
            //                 trans.translation.vector.z,
            //             ]
            //             .into(),
            //             rotation: Quat::from_xyzw(
            //                 trans.rotation.i as f32,
            //                 trans.rotation.j as f32,
            //                 trans.rotation.k as f32,
            //                 trans.rotation.w as f32,
            //             ),
            //             ..Default::default()
            //         };
            //         dbg!(&transform);

            //         //  * transform = trans.into();
            //     }
            // }
        }

        self.collision_checker.update();

        // self.collision_checker.print_collision_info();

        Ok(if self.collision_checker.has_collision() {
            CollisionResult::Collision
        } else {
            CollisionResult::Free
        })
    }

    // fn apply_joint_state(&mut self, joint_idx: usize, position: f32) {
    //     let joint = &self.urdf_robot.joints[joint_idx];
    //     let link = &self.urdf_robot.links[joint.child];
    //     let colliders = self.colliders.get(&joint.child).unwrap();

    //     let joint_pose = pose_to_isometry(&joint.origin);
    //     let link_pose = pose_to_isometry(&link.visual[0].origin);

    //     let link_transform = joint_pose * link_pose;

    //     for (collider, handle) in colliders.iter().zip(colliders) {
    //         self.collision_checker
    //             .collider_set
    //             .get_mut(handle)
    //             .unwrap()
    //             .set_position(link_transform);
    //     }
    // }
}
