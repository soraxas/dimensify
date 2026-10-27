//! Two headless clients exercising an external shared-cube authority on native or WASM.
mod common;
use bevy::prelude::*;
use common::Cube;
use dimensify_session::*;
use std::time::Duration;

struct Check {
    clients: [App; 2],
    endpoint: String,
    phase: u8,
    paused_at: Duration,
    credential: Option<ResumeCredential>,
    identity: Option<ParticipantId>,
}

impl Check {
    fn new(endpoint: String) -> Self {
        let mut clients = std::array::from_fn(|_| {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(CollaborationClientPlugin::<Cube>::default());
            app.finish();
            app.cleanup();
            connect::<Cube>(app.world_mut(), &endpoint).unwrap();
            app
        });
        clients[0]
            .world_mut()
            .resource_mut::<ClientSession<Cube>>()
            .send(ClientRequest::Create {
                name: "host".into(),
            })
            .unwrap();
        Self {
            clients,
            endpoint,
            phase: 0,
            paused_at: Duration::ZERO,
            credential: None,
            identity: None,
        }
    }

    fn tick(&mut self, elapsed: Duration) -> Option<Result<(), String>> {
        for client in &mut self.clients {
            client.update();
        }
        let sessions = self
            .clients
            .each_ref()
            .map(|c| c.world().resource::<ClientSession<Cube>>());
        if elapsed > Duration::from_secs(30) || sessions.iter().any(|s| s.error.is_some()) {
            return Some(Err(format!(
                "phase {}: {:?}",
                self.phase,
                sessions.map(|s| (s.status, s.error.clone()))
            )));
        }
        match self.phase {
            0 if sessions[0].invitations.is_some() => {
                let code = sessions[0]
                    .invitations
                    .as_ref()
                    .unwrap()
                    .codes
                    .as_ref()
                    .unwrap()
                    .editor
                    .clone();
                self.clients[1]
                    .world_mut()
                    .resource_mut::<ClientSession<Cube>>()
                    .send(ClientRequest::JoinWithCode {
                        code,
                        name: "editor".into(),
                    })
                    .unwrap();
                self.phase = 1;
            }
            1 if sessions.iter().all(|s| {
                s.snapshot
                    .as_ref()
                    .is_some_and(|r| r.participants.len() == 2)
            }) =>
            {
                self.clients[1]
                    .world_mut()
                    .resource_mut::<ClientSession<Cube>>()
                    .send(ClientRequest::SetDocument {
                        expected_revision: 0,
                        data: Cube {
                            center: [0.5, 0., 0.],
                            ..default()
                        },
                    })
                    .unwrap();
                self.phase = 2;
            }
            2 if sessions.iter().all(|s| {
                s.snapshot.as_ref().is_some_and(|r| {
                    r.document.revision == 1 && r.document.data.center == [0.5, 0., 0.]
                })
            }) =>
            {
                for client in &mut self.clients {
                    client
                        .world_mut()
                        .resource_mut::<ClientSession<Cube>>()
                        .set_cursor_sharing(true)
                        .unwrap();
                }
                self.phase = 3;
            }
            3 => {
                if sessions.iter().all(|s| {
                    s.remote_cursors()
                        .any(|c| c.sample.position == Some([0.2, 0.3]))
                }) {
                    self.identity = sessions[1].you;
                    self.credential = sessions[1].resume_credential().cloned();
                    assert!(self.credential.is_some());
                    disconnect::<Cube>(self.clients[1].world_mut());
                    self.paused_at = elapsed;
                    self.phase = 4;
                } else {
                    // Presence is latest-value and unreliable: publish until both peers observe it.
                    for client in &mut self.clients {
                        match client
                            .world_mut()
                            .resource_mut::<ClientSession<Cube>>()
                            .publish_cursor(Some([0.2, 0.3]))
                        {
                            Ok(()) | Err(SessionError::NotJoined) => {} // Subscription acknowledgement is still in flight.
                            Err(error) => return Some(Err(format!("presence: {error:?}"))),
                        }
                    }
                }
            }
            4 if elapsed.saturating_sub(self.paused_at) > Duration::from_millis(200)
                && sessions[0].snapshot.as_ref().unwrap().participants.len() == 1 =>
            {
                let guest = self.clients[1].world_mut();
                guest
                    .resource_mut::<ClientSession<Cube>>()
                    .restore_resume_credential(self.credential.take().unwrap());
                connect::<Cube>(guest, &self.endpoint).unwrap();
                self.phase = 5;
            }
            5 if sessions[1].status == ConnectionStatus::Joined => {
                if sessions[1].you != self.identity
                    || sessions[1].snapshot.as_ref().unwrap().document.revision != 1
                {
                    return Some(Err("resume lost identity or document revision".into()));
                }
                self.clients[1]
                    .world_mut()
                    .resource_mut::<ClientSession<Cube>>()
                    .leave()
                    .unwrap();
                self.phase = 6;
            }
            6 if sessions[1].status == ConnectionStatus::Connected => {
                self.clients[0]
                    .world_mut()
                    .resource_mut::<ClientSession<Cube>>()
                    .leave()
                    .unwrap();
                self.phase = 7;
            }
            7 if sessions[0].status == ConnectionStatus::Connected => {
                for client in &mut self.clients {
                    disconnect::<Cube>(client.world_mut());
                }
                return Some(Ok(()));
            }
            _ => {}
        }
        None
    }
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:5002".into());
    let mut check = Check::new(endpoint);
    let start = std::time::Instant::now();
    loop {
        if let Some(result) = check.tick(start.elapsed()) {
            result.expect("session acceptance");
            println!("PASS: invite, edit, bidirectional presence, identity resume, leave");
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(target_family = "wasm")]
fn main() {
    let endpoint = js_sys::Reflect::get(&js_sys::global(), &"COLLAB_ENDPOINT".into())
        .unwrap()
        .as_string()
        .expect("COLLAB_ENDPOINT");
    let mut app = App::new();
    app.add_plugins(
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(
            Duration::from_millis(5),
        )),
    )
    .insert_non_send(Check::new(endpoint))
    .add_systems(Update, browser_tick);
    app.run();
}

#[cfg(target_family = "wasm")]
fn browser_tick(
    mut check: NonSendMut<Check>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    if let Some(result) = check.tick(time.elapsed()) {
        let value = match &result {
            Ok(()) => "PASS: invite, edit, bidirectional presence, identity resume, leave".into(),
            Err(error) => format!("FAIL: {error}"),
        };
        js_sys::Reflect::set(&js_sys::global(), &"COLLAB_RESULT".into(), &value.into()).unwrap();
        exit.write(if result.is_ok() {
            AppExit::Success
        } else {
            AppExit::error()
        });
    }
}
