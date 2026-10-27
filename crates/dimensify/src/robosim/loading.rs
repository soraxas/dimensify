use super::{pose, vector};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use eyre::{Result, WrapErr, bail, ensure};
use std::path::{Path, PathBuf};

pub(super) struct VisualPart {
    pub(super) mesh: Mesh,
    pub(super) material: StandardMaterial,
    pub(super) transform: Transform,
}

/// Load all visible link geometry with local paths resolved from the URDF directory.
pub(super) fn load_visuals(
    robot: &urdf_rs::Robot,
    directory: &Path,
) -> Result<Vec<Vec<VisualPart>>> {
    robot
        .links
        .iter()
        .map(|link| {
            let mut parts = Vec::new();
            for visual in &link.visual {
                let transform = pose(&visual.origin);
                ensure!(
                    transform.to_matrix().is_finite(),
                    "Invalid visual origin on {}",
                    link.name
                );
                let material = visual
                    .material
                    .as_ref()
                    .map(|m| {
                        let m = if m.color.is_none() && m.texture.is_none() {
                            robot
                                .materials
                                .iter()
                                .find(|root| root.name == m.name)
                                .ok_or_else(|| eyre::eyre!("Undefined material {}", m.name))?
                        } else {
                            m
                        };
                        if m.texture.is_some() {
                            eprintln!(
                                "RoboSim: texture on {} is not displayed; using material color",
                                link.name
                            );
                        }
                        Ok::<_, eyre::Report>(m.color.as_ref().map(|c| {
                            Color::srgba(
                                c.rgba[0] as f32,
                                c.rgba[1] as f32,
                                c.rgba[2] as f32,
                                c.rgba[3] as f32,
                            )
                        }))
                    })
                    .transpose()?
                    .flatten();
                match &visual.geometry {
                    urdf_rs::Geometry::Mesh { filename, scale } => {
                        let path = resolve_mesh(directory, filename)?;
                        let scene = mesh_loader::Loader::default()
                            .load(&path)
                            .wrap_err_with(|| format!("Loading mesh {}", path.display()))?;
                        ensure!(
                            !scene.meshes.is_empty(),
                            "Mesh {} contains no geometry",
                            path.display()
                        );
                        let scale = scale.as_ref().map(|s| vector(&s.0)).unwrap_or(Vec3::ONE);
                        ensure!(
                            scale.is_finite() && scale.cmpgt(Vec3::ZERO).all(),
                            "Mesh scale must be positive and finite"
                        );
                        let mut source_materials = scene.materials.into_iter();
                        for source in scene.meshes {
                            ensure!(
                                !source.vertices.is_empty() && !source.faces.is_empty(),
                                "Mesh {} has no triangles",
                                path.display()
                            );
                            let source_material = source_materials.next();
                            let color = material.or_else(|| {
                                source_material.and_then(|m| {
                                    m.color.diffuse.map(|c| Color::srgb(c[0], c[1], c[2]))
                                })
                            });
                            let mut mesh = Mesh::new(
                                PrimitiveTopology::TriangleList,
                                RenderAssetUsages::default(),
                            )
                            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, source.vertices)
                            .with_inserted_indices(Indices::U32(
                                source.faces.into_iter().flatten().collect(),
                            ));
                            // STL facet normals may be zero. mesh-loader also bakes
                            // Collada node transforms into positions but not normals.
                            let recalculate = path.extension().is_some_and(|extension| {
                                extension.eq_ignore_ascii_case("stl")
                                    || extension.eq_ignore_ascii_case("dae")
                            }) || source.normals.iter().any(|normal| {
                                let normal = Vec3::from(*normal);
                                !normal.is_finite() || normal.length_squared() < 1e-12
                            });
                            if source.normals.is_empty() || recalculate {
                                mesh.duplicate_vertices();
                                mesh.compute_flat_normals();
                            } else {
                                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, source.normals);
                            }
                            parts.push(VisualPart {
                                mesh,
                                material: surface(color),
                                transform: transform.with_scale(scale),
                            });
                        }
                    }
                    geometry => {
                        let (mesh, orientation) = primitive(geometry)?;
                        parts.push(VisualPart {
                            mesh,
                            material: surface(material),
                            transform: transform
                                .mul_transform(Transform::from_rotation(orientation)),
                        });
                    }
                }
            }
            Ok(parts)
        })
        .collect()
}

fn surface(color: Option<Color>) -> StandardMaterial {
    let color = color.unwrap_or(Color::srgb(0.7, 0.73, 0.78));
    StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.65,
        alpha_mode: if color.alpha() < 1.0 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        },
        ..default()
    }
}

fn positive(value: f64) -> Result<f32> {
    let value = value as f32;
    ensure!(
        value.is_finite() && value > 0.0,
        "Geometry dimensions must be positive and finite"
    );
    Ok(value)
}

fn primitive(geometry: &urdf_rs::Geometry) -> Result<(Mesh, Quat)> {
    // Bevy cylinders/capsules are Y-aligned; construct URDF geometry along local Z.
    let z_axis = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    Ok(match geometry {
        urdf_rs::Geometry::Box { size } => (
            Cuboid::new(positive(size[0])?, positive(size[1])?, positive(size[2])?).into(),
            Quat::IDENTITY,
        ),
        urdf_rs::Geometry::Sphere { radius } => {
            (Sphere::new(positive(*radius)?).into(), Quat::IDENTITY)
        }
        urdf_rs::Geometry::Cylinder { radius, length } => (
            Cylinder::new(positive(*radius)?, positive(*length)?).into(),
            z_axis,
        ),
        urdf_rs::Geometry::Capsule { radius, length } => (
            Capsule3d::new(positive(*radius)?, positive(*length)?).into(),
            z_axis,
        ),
        _ => bail!("Expected primitive geometry"),
    })
}

/// Resolve relative paths and package URIs against the URDF's ancestor directories.
pub(super) fn resolve_mesh(directory: &Path, filename: &str) -> Result<PathBuf> {
    if let Some(relative) = filename.strip_prefix("package://") {
        for parent in directory.ancestors() {
            let candidate = parent.join(relative);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        bail!("Cannot resolve {filename} from {}", directory.display());
    }
    let path = directory.join(filename.strip_prefix("file://").unwrap_or(filename));
    ensure!(path.is_file(), "Mesh file not found: {}", path.display());
    Ok(path)
}
