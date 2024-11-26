use std::collections::HashMap;

use numpy::{Ix1, Ix2, PyArrayLike1};
use numpy::{PyArrayLike, PyArrayLikeDyn};
// use crfs_rs::{Attribute, Model};
use pyo3::prelude::*;

// use robotsim::robot::{ColliderHandle, CollisionResult, Robot, RobotError};

use eyre::Result;

// #[feature(visualiser)]
// mod visualiser;

// #[pyclass(module = "robotsim", name = "Robot")]
// // #[self_referencing]
// struct PyRobot {
//     #[pyo3(get, set)]
//     pub data: Vec<u8>,
//     // #[borrows(data)]
//     // #[covariant]
//     robot: Robot,
// }
//
// #[pymethods]
// impl PyRobot {
//     #[new]
//     fn py_new(path: &str) -> PyResult<Self> {
//         Ok(PyRobot {
//             data: vec![5, 9],
//             robot: Robot::from_file(path)?,
//         })
//     }
//
//     #[getter]
//     fn name(&self) -> &str {
//         self.robot.name()
//     }
//
//     #[getter]
//     fn joints(&self) -> Vec<f32> {
//         // self.robot.name()
//         self.robot.robot_chain.joint_positions()
//     }
//
//     #[getter]
//     fn joint_limits_by_order(&self) -> Option<Vec<(f32, f32)>> {
//         self.robot
//             .robot_chain
//             .iter_joints()
//             .map(|joint| joint.limits.map(|limit| (limit.min, limit.max)))
//             .collect()
//     }
//
//     #[getter]
//     fn joint_limits(&self) -> Option<HashMap<String, (f32, f32)>> {
//         self.robot
//             .robot_chain
//             .iter_joints()
//             .map(|joint| {
//                 joint
//                     .limits
//                     .map(|limit| (joint.name.clone(), (limit.min, limit.max)))
//             })
//             .collect()
//     }
//
//     #[getter]
//     fn joint_link_map(&self) -> HashMap<String, String> {
//         // self.robot.name()
//         self.robot.joint_link_map.clone()
//     }
//
//     #[getter]
//     fn joint_names(&self) -> Vec<String> {
//         // self.robot.name()
//         self.robot
//             .robot_chain
//             .iter_joints()
//             .map(|joint| joint.name.clone())
//             .collect()
//     }
//
//     #[getter]
//     fn link_names(&self) -> Vec<String> {
//         // self.robot.name()
//         self.robot
//             .robot_chain
//             .iter_links()
//             .map(|link| link.name.clone())
//             .collect()
//     }
//
//     // #[pyfunction]
//     // fn sum_up<'py>(py: Python<'py>, array: PyArrayLike2<'py, f32, AllowTypeChange>) -> f32 {
//
//     fn set_joints(&mut self, array: PyArrayLike1<f32, AllowTypeChange>) -> Result<()> {
//         self.robot.set_joints(array.as_slice()?)
//     }
//
//     fn is_colliding(&mut self) -> Result<bool> {
//         self.robot.has_collision().map(|result| result.into())
//     }
//
//     fn get_colliding_pairs(&mut self) -> Vec<(String, String)> {
//         dbg!(self.robot.collision_checker.get_colliding_pairs());
//         dbg!(self.robot.collision_checker.print_collision_info());
//
//         let collider_mappings: HashMap<ColliderHandle, String> = self
//             .robot
//             .colliders
//             .iter()
//             .flat_map(|(link_name, collidre_handles)| {
//                 collidre_handles
//                     .iter()
//                     .map(|(handle)| (*handle, link_name.clone()))
//             })
//             .collect();
//
//         // self.robot.collision_checker.print_collision_info();
//         // self.robot.collision_checker.print_collision_info();
//         // self.robot.collision_checker.print_collision_info();
//         // dbg!(self.robot.collision_checker.get_colliding_pairs());
//         // self.robot.collision_checker.print_collision_info();
//
//
//         self.robot
//             .collision_checker
//             .get_colliding_pairs()
//             .iter()
//             .map(|(a, b)| {
//                 (
//                     collider_mappings.get(a).unwrap().clone(),
//                     collider_mappings.get(b).unwrap().clone(),
//                 )
//             })
//             .collect()
//
//         // self.robot.has_collision(&self.robot.robot_chain.joint_positions()).map(|result| result.into())
//     }
//
//     fn has_collision(&mut self, array: PyArrayLike2<f32, AllowTypeChange>) -> Result<Vec<bool>> {
//         array
//             .as_array()
//             .rows()
//             .into_iter()
//             .map(|row| {
//                 let joints = row
//                     .as_slice()
//                     .ok_or_eyre("Failed to get slice (array is not contiguous?)")?;
//
//                 if let Err(err) = self.robot.set_joints(joints) {
//                     // we can recovery from mapping this to a CollisionResult
//                     err.downcast::<RobotError>().and_then(|err| match err {
//                         RobotError::SetJointLimitViolation => {
//                             log::debug!("Joint limit violation: {}", err);
//                             Ok(CollisionResult::JointLimitViolation.into())
//                         }
//                         e => Err(e.into()),
//                     })
//                 } else {
//                     self.robot.has_collision().map(Into::<bool>::into)
//                 }
//             })
//             .collect()
//     }
//
//     fn __repr__(&self) -> String {
//         format!("<Robot '{}'>", self.name())
//     }
// }
//
// use eyre::OptionExt;
// use numpy::{get_array_module, AllowTypeChange, PyArrayLike2};
//
// #[pyfunction]
// fn sum_up<'py>(py: Python<'py>, array: PyArrayLike2<'py, f32, AllowTypeChange>) -> f32 {
//     array.as_array().rows().into_iter().for_each(|row| {
//         let a = row.as_slice();
//
//         dbg!(a.ok_or_eyre("Failed to get slice (array is not contiguous?)")).unwrap();
//         println!("{:?}", row);
//     });
//
//     dbg!(array.as_slice().unwrap());
//     array.as_array().sum()
// }
//
// #[pyfunction]
// fn double(x: usize) -> usize {
//     x * 2
// }

#[pymodule]
#[pyo3(name = "dimensify")]
mod py_dimensify {
    use super::*;

    // #[pymodule_export]
    // use super::double; // Exports the double function as part of the module
    //
    // #[pymodule_export]
    // use super::sum_up; // Exports the double function as part of the module
    //
    // #[pymodule_export]
    // use super::PyRobot;

    #[pyfunction] // This will be part of the module
    fn triple(x: usize) -> usize {
        x * 3
    }

    #[pyfunction] // This will be part of the module
    fn start() {
        use bevy::prelude::*;
        use bevy::winit::WinitPlugin;
        use dimensify::test_scene;
        use dimensify::util;
        use dimensify::SimPlugin;

        std::thread::spawn(move || {
            if let Err(e) = util::initialise() {
                log::error!("{}", e);
            }

            let mut app = App::new();

            let mut p = WinitPlugin::default();
            p.run_on_any_thread = true;

            app.add_plugins(SimPlugin.set::<bevy_winit::WinitPlugin>(p))
                .add_plugins(test_scene::plugin)
                // .add_event::<StreamEvent>()
                // .insert_resource(StreamReceiver(rx))
                // .add_systems(
                //     Update,
                //     (
                //         read_stream,
                //         update_robot_state
                //             .pipe(error_handler)
                //             .run_if(resource_exists::<RobotState>),
                //     ),
                // )
                .run();
        });
    }

    // #[pymodule_export]
    // use visualiser::PyVisualiser;

    // #[pyfunction] // This will be part of the module
    // fn start_visualiser() {
    //     visualiser::start_visualiser();
    // }

    #[pyclass] // This will be part of the module
    struct Unit;

    #[pymodule]
    mod submodule {
        // This is a submodule
    }

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        // Arbitrary code to run at the module initialization
        // m.add("double2", m.getattr("double")?)
        Ok(())
    }
}
