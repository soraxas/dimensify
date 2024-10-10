#![allow(dead_code)]

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use inflector::Inflector;

use dimensify::{Dimensify, DimensifyApp};
use std::cmp::Ordering;

mod scenario;
use scenario::*;

fn demo_name_from_command_line() -> Option<String> {
    let mut args = std::env::args();

    while let Some(arg) = args.next() {
        if &arg[..] == "--example" {
            return args.next();
        }
    }

    None
}

#[cfg(target_arch = "wasm32")]
fn demo_name_from_url() -> Option<String> {
    None
    //    let window = stdweb::web::window();
    //    let hash = window.location()?.search().ok()?;
    //    if hash.len() > 0 {
    //        Some(hash[1..].to_string())
    //    } else {
    //        None
    //    }
}

#[cfg(not(target_arch = "wasm32"))]
fn demo_name_from_url() -> Option<String> {
    None
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn main() {
    let demo = demo_name_from_command_line()
        .or_else(demo_name_from_url)
        .unwrap_or_default()
        .to_camel_case();

    let mut builders: Vec<(_, fn(&mut Dimensify))> = vec![
        ("Fountain", fountain3::init_world),
        ("Primitives", primitives3::init_world),
        ("CCD", ccd3::init_world),
        ("Collision groups", collision_groups3::init_world),
        ("Compound", compound3::init_world),
        // ("Convex decomposition", convex_decomposition3::init_world),
        ("Convex polyhedron", convex_polyhedron3::init_world),
        ("Damping", damping3::init_world),
        ("Domino", domino3::init_world),
        ("Heightfield", heightfield3::init_world),
        // ("Joints", joints3::init_world),
        ("Locked rotations", locked_rotations3::init_world),
        ("One-way platforms", one_way_platforms3::init_world),
        ("Platform", platform3::init_world),
        ("Restitution", restitution3::init_world),
        ("Sensor", sensor3::init_world),
        ("TriMesh", trimesh3::init_world),
        ("Keva tower", keva3::init_world),
        (
            "(Debug) add/rm collider",
            debug_add_remove_collider3::init_world,
        ),
        ("(Debug) big colliders", debug_big_colliders3::init_world),
        ("(Debug) boxes", debug_boxes3::init_world),
        (
            "(Debug) dyn. coll. add",
            debug_dynamic_collider_add3::init_world,
        ),
        ("(Debug) friction", debug_friction3::init_world),
        ("(Debug) triangle", debug_triangle3::init_world),
        ("(Debug) trimesh", debug_trimesh3::init_world),
        ("(Debug) cylinder", debug_cylinder3::init_world),
        ("(Debug) infinite fall", debug_infinite_fall3::init_world),
        ("(Debug) prismatic", debug_prismatic3::init_world),
        ("(Debug) rollback", debug_rollback3::init_world),
        (
            "(Debug) shape modification",
            debug_shape_modification3::init_world,
        ),
    ];

    // Lexicographic sort, with stress tests moved at the end of the list.
    builders.sort_by(|a, b| match (a.0.starts_with("("), b.0.starts_with("(")) {
        (true, true) | (false, false) => a.0.cmp(b.0),
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
    });

    let i = builders
        .iter()
        .position(|builder| builder.0.to_camel_case().as_str() == demo.as_str())
        .unwrap_or(0);

    let viewer = DimensifyApp::from_builders(i, builders);
    viewer.run()
}
