//! Headless authority for the shared_cube example. See ../README.md.
mod common;
use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use dimensify_session::server::{CollaborationListener, CollaborationServerPlugin, listen};
use lightyear::prelude::{Linked, LocalAddr, Unlinked};
use std::{net::SocketAddr, time::Duration};
fn main() {
    let address: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:5002".into())
        .parse()
        .expect("bind address, e.g. 127.0.0.1:5002");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))))
        .add_plugins(CollaborationServerPlugin::<common::Cube>::default())
        .add_systems(Update, report_listener);
    listen(app.world_mut(), address);
    app.run();
}
fn report_listener(
    started: Query<&LocalAddr, (With<CollaborationListener>, Added<Linked>)>,
    failed: Query<(), (With<CollaborationListener>, Added<Unlinked>)>,
    mut exit: MessageWriter<AppExit>,
) {
    for addr in &started {
        println!("Cube collaboration server listening at ws://{}", addr.0);
    }
    if !failed.is_empty() {
        eprintln!("Listener failed or stopped");
        exit.write(AppExit::error());
    }
}
