use eyre::Result;
use k::urdf;
use urdf_rs;
use crate::util::replace_package_with_base_dir;


pub struct Robot {
    pub name: String,
    // links: Vec<Link>,
    // joints: Vec<Joint>,
}



// fn process_meshes<'a, GeomIterator, P>(
//     iterator: GeomIterator,
//     load_context: &mut LoadContext<'_>,
//     meshes_and_materials: &mut MeshMaterialMapping,
//     base_dir: &Option<P>,
//     mesh_type: MeshType,
//     link_idx: usize,
// )
// // -> Result<(Mesh, Option<StandardMaterial>)>
// where
//     GeomIterator: Iterator<Item = (&'a urdf_rs::Geometry, Option<&'a urdf_rs::Material>)>,
//     P: std::fmt::Display,
// {
//     // let meshes_and_materials = HashMap::new();

//     for (j, (geom_element, material)) in iterator.enumerate() {
//         if let urdf_rs::Geometry::Mesh {
//             ref filename,
//             scale: _,
//         } = geom_element
//         {
//             // try to replace any filename with prefix, and correctly handle relative paths
//             let filename = replace_package_with_base_dir(filename, base_dir);

//             let meshes = match load_context.read_asset_bytes(&filename).await {
//                 Ok(bytes) => {
//                     let loader = mesh_loader::Loader::default();
//                     let scene = loader
//                         .load_from_slice(&bytes, &filename)
//                         .expect("failed to load mesh");
//                     // scene.meshes

//                     load_meshes(scene, material, load_context)
//                 }
//                 Err(e) => {
//                     error!("cannot load mesh at {}: {}", &filename, e);
//                     vec![]
//                 }
//             };

//             meshes_and_materials.insert((mesh_type, link_idx, j), meshes);
//         };
//     }
// }


impl Robot {

    pub fn from_file(urdf_path: String) -> Result<Self> {

        let urdf_robot = urdf_rs::read_file(urdf_path)?;




            // // let mut vector =  Vec::new();
            // for (link_idx, l) in urdf_robot.links.iter().enumerate() {
            //     process_meshes(
            //         l.collision.iter().map(|item| (&item.geometry, None)),
            //         &base_dir,
            //         MeshType::Collision,
            //         link_idx,
            //     );

            //     process_meshes(
            //         l.visual
            //             .iter()
            //             .map(|item| (&item.geometry, item.material.as_ref())),
            //         load_context,
            //         &mut meshes_and_materials,
            //         &base_dir,
            //         MeshType::Visual,
            //         link_idx,
            //     );
            // }

            // // Ok(UrdfAsset {
            // //     robot: urdf_robot,
            // //     meshes_and_materials,
            // // })



        // let links = urdf.links.iter().map(|l| Link::from_urdf(l)).collect();
        // let joints = urdf.joints.iter().map(|j| Joint::from_urdf(j)).collect();
        Ok(Self {
            name: urdf_robot.name,
            // links,
            // joints,
        })
    }
}