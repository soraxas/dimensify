use bevy::prelude::*;
use bevy::utils::HashMap;
use na::point;
use rapier3d::prelude::RigidBodyHandle;

use crate::graphics::InstancedMaterials;
use crate::plugins::DimensifyPluginDrawArgs;
use crate::scene_graphics::graphic_node::NodeWithGraphicsAndPhysics;
use crate::scene_graphics::graphic_node::WithGraphicsExt;

use crate::plugins::DimensifyPlugin;
use crate::BevyMaterial;
use crate::DimensifyGraphics;

use na::{self, Point3, Vector3};
use rapier3d::geometry::Ray;
use rapier3d::math::Real;
use rapier3d::pipeline::QueryFilter;

pub const SELECTED_OBJECT_MATERIAL_KEY: Point3<usize> = point![42, 42, 42];

/// Register the selected object material if it doesn't exist, or if it exists, do nothing.
/// Then retrieve it
fn get_selected_object_material(
    materials: &mut Assets<BevyMaterial>,
    instanced_materials: &mut InstancedMaterials,
) -> Handle<BevyMaterial> {
    match instanced_materials.get(&SELECTED_OBJECT_MATERIAL_KEY) {
        Some(handle) => handle,
        None => {
            let selection_material = StandardMaterial {
                metallic: 0.5,
                perceptual_roughness: 0.5,
                double_sided: true, // TODO: this doesn't do anything?
                ..StandardMaterial::from(Color::from(Srgba::RED))
            };

            instanced_materials.insert(
                SELECTED_OBJECT_MATERIAL_KEY,
                materials.add(selection_material),
            );
            &instanced_materials[&SELECTED_OBJECT_MATERIAL_KEY]
        }
    }
    .clone_weak()
}

#[derive(Default)]
pub struct HighlightHoveredBodyPlugin {
    pub highlighted_body: Option<(RigidBodyHandle, Handle<StandardMaterial>)>,
}

#[inline]
fn nested_material_setter(
    graphics_context: &mut DimensifyGraphics,
    body_handle: RigidBodyHandle,
    mut callback: impl FnMut(&NodeWithGraphicsAndPhysics, &mut Handle<BevyMaterial>),
) {
    match graphics_context.graphics.body_nodes_mut(body_handle) {
        Some(nodes) => {
            for node in nodes {
                node.visit_leaf_node(&mut |node| {
                    if let Some(handle) = node
                        .data
                        .entity
                        .and_then(|entity| graphics_context.material_handles.get_mut(entity).ok())
                    {
                        callback(node, handle.into_inner());
                    }
                });
            }
        }
        None => info!(
            "No visualable body found for node (collider) {:?}",
            body_handle
        ),
    }
}

impl DimensifyPlugin for HighlightHoveredBodyPlugin {
    fn draw(&mut self, plugin_args: &mut DimensifyPluginDrawArgs) {
        let graphics_context = &mut plugin_args.graphics;

        if let Some(window) = graphics_context.window {
            // restore the highlighted body to its original material
            let highlighted_body = self.highlighted_body.take();
            if let Some((highlighted_body, material_handle)) = highlighted_body {
                nested_material_setter(graphics_context, highlighted_body, move |_node, handle| {
                    // I think this might make it so that, if a rigid body has multiple colliders/entity, they all ended up getting the same material back.
                    *handle = material_handle.clone();
                    // if let Some(material) = node.get_material() {
                    //     *handle = material.clone_weak();
                    // }
                });
            }

            // highlight the currently hovered body
            if let Some(cursor) = window.cursor_position() {
                let physics = &plugin_args.harness.physics;

                let ndc_cursor = Vec2::new(
                    cursor.x / window.width() * 2.0 - 1.0,
                    1.0 - cursor.y / window.height() * 2.0,
                );
                let ndc_to_world = graphics_context.camera_transform.compute_matrix()
                    * graphics_context.camera_view.clip_from_view().inverse();
                let ray_pt1 =
                    ndc_to_world.project_point3(Vec3::new(ndc_cursor.x, ndc_cursor.y, -1.0));
                let ray_pt2 =
                    ndc_to_world.project_point3(Vec3::new(ndc_cursor.x, ndc_cursor.y, 1.0));
                let ray_dir = ray_pt2 - ray_pt1;
                let ray_origin = Point3::new(ray_pt1.x, ray_pt1.y, ray_pt1.z);
                let ray_dir = Vector3::new(ray_dir.x, ray_dir.y, ray_dir.z);

                let ray = Ray::new(ray_origin, ray_dir);
                let hit = physics.query_pipeline.cast_ray(
                    &physics.bodies,
                    &physics.colliders,
                    &ray,
                    Real::MAX,
                    true,
                    QueryFilter::only_dynamic(),
                );

                if let Some((handle, _)) = hit {
                    let collider = &physics.colliders[handle];

                    if let Some(parent_handle) = collider.parent() {
                        let selection_material = get_selected_object_material(
                            graphics_context.materials,
                            &mut graphics_context.graphics.instanced_materials,
                        );

                        nested_material_setter(graphics_context, parent_handle, |_, handle| {
                            let original_handle =
                                std::mem::replace(handle, selection_material.clone_weak());
                            self.highlighted_body = Some((parent_handle, original_handle));
                        });
                    }
                }
            }
        }
    }
}
