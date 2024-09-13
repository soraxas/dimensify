pub mod mesh;
pub mod urdf;

// Use material which is defined as root materials if found.
// Root material is used for PR2, but not documented.
pub fn rgba_from_visual(urdf_robot: &urdf_rs::Robot, visual: &urdf_rs::Visual) -> urdf_rs::Vec4 {
    match urdf_robot
        .materials
        .iter()
        .find(|mat| {
            visual
                .material
                .as_ref()
                .map_or(false, |m| mat.name == m.name)
        })
        .cloned()
    {
        Some(ref material) => material
            .color
            .as_ref()
            .map(|color| color.rgba)
            .unwrap_or_default(),
        None => visual
            .material
            .as_ref()
            .and_then(|material| material.color.as_ref().map(|color| color.rgba))
            .unwrap_or_default(),
    }
}

// Use material which is defined as root materials if found.
// Root material is used for PR2, but not documented.
pub fn get_material_from_urdf_root<'a>(
    urdf_robot: &'a urdf_rs::Robot,
    visual: &urdf_rs::Visual,
) -> Option<&'a urdf_rs::Material> {
    urdf_robot.materials.iter().find(|mat| {
        visual
            .material
            .as_ref()
            .map_or(false, |m| mat.name == m.name)
    })
}
