use bevy::prelude::*;
use bevy::winit::WinitPlugin;
use crossbeam_channel::Sender;
use eyre::Result;
// use eyre::Result;
use pyo3::prelude::*;

use crossbeam_channel::{bounded, Receiver};
use robotsim::robot_vis::RobotState;
// use rand::{Rng, SeedableRng};
// use rand_chacha::ChaCha8Rng;
use std::time::{Duration, Instant};

use robotsim::util;
use robotsim::SimPlugin;

// fn main() -> Result<()> {
//     util::initialise()?;

//     let mut app = App::new();
//     app.add_plugins(SimPlugin).run();

//     Ok(())
// }

type JointState = Vec<f32>;

#[derive(Resource, Deref)]
struct StreamReceiver(Receiver<JointState>);

#[derive(Event)]
struct StreamEvent(JointState);

// This system reads from the receiver and sends events to Bevy
fn read_stream(receiver: Res<StreamReceiver>, mut events: EventWriter<StreamEvent>) {
    for from_stream in receiver.try_iter() {
        events.send(StreamEvent(from_stream));
    }
}

fn error_handler(In(result): In<Result<()>>) {
    if let Err(err) = result {
        log::error!("encountered an error {:?}", err);
    }
}

fn update_robot_state(
    mut reader: EventReader<StreamEvent>,
    mut robot_state: ResMut<RobotState>,
) -> Result<()> {
    if let Some(event) = reader.read().next() {
        let robot_state_inner = robot_state.bypass_change_detection();
        robot_state_inner
            .robot_chain
            .set_joint_positions(&event.0)?;

        // if we reached this piont, the set joint positions was successful
        robot_state.set_changed();
    }
    Ok(())
}

#[pyclass(module = "robotsim", name = "Visualiser")]
// #[self_referencing]
pub struct PyVisualiser {
    #[pyo3(get, set)]
    pub data: Vec<u8>,
    // #[borrows(data)]
    // #[covariant]
    stream_seder: Sender<JointState>,
}

#[pymethods]
impl PyVisualiser {
    #[new]
    fn py_new() -> PyResult<Self> {
        Ok(PyVisualiser {
            data: vec![5, 9],
            stream_seder: start_visualiser(),
        })
    }

    fn set_joints(&mut self, joints: JointState) -> Result<bool> {
        self.stream_seder.send(joints)?;
        Ok(true)
    }
}

pub fn start_visualiser() -> Sender<JointState> {
    let (tx, rx) = bounded::<JointState>(10);

    std::thread::spawn(move || {
        if let Err(e) = util::initialise() {
            log::error!("{}", e);
        }

        let mut app = App::new();

        let mut p = WinitPlugin::default();
        p.run_on_any_thread = true;

        app.add_plugins(SimPlugin.set::<bevy_winit::WinitPlugin>(p))
            .add_event::<StreamEvent>()
            .insert_resource(StreamReceiver(rx))
            .add_systems(
                Update,
                (
                    read_stream,
                    update_robot_state
                        .pipe(error_handler)
                        .run_if(resource_exists::<RobotState>),
                ),
            )
            .run();

        // We're seeding the PRNG here to make this example deterministic for testing purposes.
        // This isn't strictly required in practical use unless you need your app to be deterministic.
        // let mut rng = ChaCha8Rng::seed_from_u64(19878367467713);
        // loop {
        //     // // Everything here happens in another thread
        //     // // This is where you could connect to an external data source
        //     // let start_time = Instant::now();
        //     // let duration = Duration::from_secs_f32(rng.gen_range(0.0..0.2));
        //     // while start_time.elapsed() < duration {
        //     //     // Spinning for 'duration', simulating doing hard work!
        //     // }

        //     // tx.send(rng.gen_range(0..2000)).unwrap();
        // }
    });

    tx
}
