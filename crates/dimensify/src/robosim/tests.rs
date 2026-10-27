use super::*;

fn app_with_model(xml: &str) -> App {
    let model = RobotModel::from_urdf(xml, Path::new(".")).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .insert_resource(model)
        .add_plugins(RobotPlugin);
    app.update();
    app
}

const ARM: &str = r#"<robot name="test">
  <link name="base"><visual><geometry><box size="0.2 0.4 0.6"/></geometry></visual></link>
  <link name="arm"/>
  <link name="tip"/>
  <joint name="hinge" type="revolute">
    <parent link="base"/><child link="arm"/><origin xyz="0 0 1"/>
    <axis xyz="0 0 1"/><limit lower="-1.57" upper="1.57" effort="1" velocity="1"/>
  </joint>
  <joint name="tool" type="fixed"><parent link="arm"/><child link="tip"/><origin xyz="1 0 0"/></joint>
</robot>"#;

fn position(app: &mut App, name: &str) -> Vec3 {
    app.world_mut()
        .query::<(&Name, &GlobalTransform)>()
        .iter(app.world())
        .find(|(n, _)| n.as_str() == name)
        .unwrap()
        .1
        .translation()
}

#[test]
fn joint_motion_updates_descendants_with_one_z_up_conversion() {
    let mut app = app_with_model(ARM);
    assert!(position(&mut app, "tip").abs_diff_eq(Vec3::new(1.0, 1.0, 0.0), 1e-5));
    app.world_mut()
        .resource_mut::<RobotModel>()
        .set_joint_position(0, 1.0)
        .unwrap();
    app.update();
    assert!(
        position(&mut app, "tip").abs_diff_eq(Vec3::new(1.0_f32.cos(), 1.0, -1.0_f32.sin()), 1e-5)
    );
}

#[test]
fn joint_limits_clamp_and_invalid_values_leave_state_unchanged() {
    let mut model = RobotModel::from_urdf(ARM, Path::new(".")).unwrap();
    model.set_joint_position(0, 10.0).unwrap();
    assert_eq!(model.joint_position(0), 1.57);
    assert!(model.set_joint_position(0, f32::NAN).is_err());
    assert_eq!(model.joint_position(0), 1.57);
}

#[test]
fn invalid_joint_graph_is_rejected_before_spawning() {
    let missing = ARM.replace("parent link=\"base\"", "parent link=\"missing\"");
    assert!(RobotModel::from_urdf(&missing, Path::new(".")).is_err());
    let cycle = ARM.replace("parent link=\"base\"", "parent link=\"tip\"");
    assert!(RobotModel::from_urdf(&cycle, Path::new(".")).is_err());
}

#[test]
fn bundled_demo_has_renderable_geometry_and_movable_joints() {
    let mut app = app_with_model(DEMO_URDF);
    assert!(app.world_mut().query::<&Mesh3d>().iter(app.world()).count() >= 3);
    assert!(
        app.world()
            .resource::<RobotModel>()
            .controllable_joints()
            .count()
            >= 2
    );
}

#[test]
fn missing_mesh_reports_an_error_instead_of_an_empty_robot() {
    let xml = ARM.replace(
        "<box size=\"0.2 0.4 0.6\"/>",
        "<mesh filename=\"missing-robosim-mesh.stl\"/>",
    );
    assert!(RobotModel::from_urdf(&xml, Path::new(".")).is_err());
}

#[test]
fn prismatic_joint_moves_along_urdf_axis_in_world_space() {
    let xml = ARM.replace("type=\"revolute\"", "type=\"prismatic\"");
    let mut app = app_with_model(&xml);
    app.world_mut()
        .resource_mut::<RobotModel>()
        .set_joint_position(0, 0.5)
        .unwrap();
    app.update();
    assert!(position(&mut app, "tip").abs_diff_eq(Vec3::new(1.0, 1.5, 0.0), 1e-5));
}

#[test]
fn mimic_joint_uses_source_multiplier_and_offset() {
    let xml = ARM.replace("name=\"tool\" type=\"fixed\"", "name=\"tool\" type=\"revolute\"")
        .replace("<origin xyz=\"1 0 0\"/>", "<origin xyz=\"1 0 0\"/><axis xyz=\"0 0 1\"/><limit lower=\"-3\" upper=\"3\" effort=\"1\" velocity=\"1\"/><mimic joint=\"hinge\" multiplier=\"-2\" offset=\"0.1\"/>");
    let mut model = RobotModel::from_urdf(&xml, Path::new(".")).unwrap();
    model.set_joint_position(0, 0.5).unwrap();
    assert!((model.joint_position(1) + 0.9).abs() < 1e-5);
    assert_eq!(model.controllable_joints().count(), 1);
    assert!(model.set_joint_position(1, 0.0).is_err());
    let cycle = xml.replace("mimic joint=\"hinge\"", "mimic joint=\"tool\"");
    assert!(RobotModel::from_urdf(&cycle, Path::new(".")).is_err());
}

#[test]
fn local_stl_and_package_uri_load_real_triangles() {
    let directory = std::env::temp_dir().join(format!("robosim-mesh-test-{}", std::process::id()));
    let package = directory.join("test_robot");
    let urdf_dir = package.join("urdf");
    std::fs::create_dir_all(&urdf_dir).unwrap();
    let mesh = package.join("triangle.stl");
    std::fs::write(&mesh, "solid test\nfacet normal 0 0 0\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid test\n").unwrap();
    for filename in ["../triangle.stl", "package://test_robot/triangle.stl"] {
        let xml = ARM.replace(
            "<box size=\"0.2 0.4 0.6\"/>",
            &format!("<mesh filename=\"{filename}\"/>"),
        );
        let model = RobotModel::from_urdf(&xml, &urdf_dir).unwrap();
        assert_eq!(model.visuals[0][0].mesh.count_vertices(), 3);
        let bevy::mesh::VertexAttributeValues::Float32x3(normals) = model.visuals[0][0]
            .mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .unwrap()
        else {
            panic!("Expected normals");
        };
        assert!(
            normals
                .iter()
                .all(|normal| Vec3::from(*normal).abs_diff_eq(Vec3::Z, 1e-5))
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn primitive_dimensions_are_not_swapped_or_halved() {
    use bevy::camera::primitives::MeshAabb;
    let model = RobotModel::from_urdf(ARM, Path::new(".")).unwrap();
    let bounds = model.visuals[0][0].mesh.compute_aabb().unwrap();
    assert!(Vec3::from(bounds.half_extents).abs_diff_eq(Vec3::new(0.1, 0.2, 0.3), 1e-6));
}

#[test]
fn collada_normals_follow_baked_node_rotation() {
    use bevy::mesh::VertexAttributeValues;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let xml = ARM.replace(
        "<box size=\"0.2 0.4 0.6\"/>",
        "<mesh filename=\"rotated.dae\"/>",
    );
    let model = RobotModel::from_urdf(&xml, &directory).unwrap();
    let mesh = &model.visuals[0][0].mesh;
    let VertexAttributeValues::Float32x3(positions) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
    else {
        panic!("Expected positions");
    };
    let VertexAttributeValues::Float32x3(normals) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap()
    else {
        panic!("Expected normals");
    };
    let expected = (Vec3::from(positions[1]) - Vec3::from(positions[0]))
        .cross(Vec3::from(positions[2]) - Vec3::from(positions[0]))
        .normalize();
    assert!(
        normals
            .iter()
            .all(|normal| Vec3::from(*normal).abs_diff_eq(expected, 1e-5))
    );
}
