use std::{hash::Hash, path::Path};

use crate::util::replace_package_with_base_dir;
use bevy::utils::hashbrown::HashMap;
use bevy_rapier3d::math::Real;
use eyre::{Context, Result};
use k::urdf;
use rapier3d::{
    math::{Isometry, Point, Vector},
    na::{self, geometry},
    prelude::{Collider, ColliderBuilder, MeshConverter, SharedShape, TriMeshFlags},
};
use urdf_rs::{self, Geometry, Pose};
use crate::collision_checker::{ColliderBuilderActivateRobotLinkCollision, SimpleCollisionPipeline};


pub struct Robot<'a> {
    // links: Vec<Link>,
    // joints: Vec<Joint>,
    collision_checker: SimpleCollisionPipeline,
    pub urdf_robot: urdf_rs::Robot,
    pub colliders: HashMap<usize, Vec<&'a Collider>>,
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

pub fn geometry_to_colliders(
    mesh_dir: &Option<&str>,
    geometry: &Geometry,
    origin: &Pose,
) -> Vec<ColliderBuilder> {
    let mut shape_transform = Isometry::identity();

    let collider_blueprint = ColliderBuilder::default().density(0.0);
    let trimesh_flags = TriMeshFlags::all();

    let mut colliders = Vec::new();
    match &geometry {
        Geometry::Box { size } => {
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


impl Robot<'_> {
    pub fn name(&self) -> &str {
        self.urdf_robot.name.as_str()
    }

    pub fn from_file(urdf_path: &str) -> Result<Self> {
        let path = Path::new(urdf_path);

        let urdf_robot = urdf_rs::read_file(path)?;

        let mut colliders_mappings = HashMap::new();

        let mut collision_checker = SimpleCollisionPipeline::default();

        for (link_idx, l) in urdf_robot.links.iter().enumerate() {
            for collision in &l.collision {
                let colliders: Vec<_> =
                    geometry_to_colliders(
                        &path.parent().and_then(|p| p.to_str()),
                        &collision.geometry,
                        &collision.origin,
                    )
                    .drain(..)
                    .map(|collider| collider.activate_as_robot_link(link_idx).build())
                    .collect();
                // colliders.insert(
                //     link_idx,
                //     geometry_to_colliders(
                //         &path.parent().and_then(|p| p.to_str()),
                //         &collision.geometry,
                //         &collision.origin,
                //     )
                //     .drain(..)
                //     .map(|collider| collider.activate_as_robot_link(link_idx).build())
                //     .collect(),
                // );

                for c in colliders {

                    collision_checker.collider_set.insert(c);
                }
                colliders_mappings.insert(link_idx, vec![&colliders[5]]);
            }
        }

        Ok(Self {
            urdf_robot,
            colliders: colliders_mappings,
            collision_checker,
        })
    }

    pub fn check_collision(&mut self) -> bool {
        self.collision_checker.update();
        self.collision_checker.has_collision()
    }
}
