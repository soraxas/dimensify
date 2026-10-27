#![cfg(all(feature = "client", feature = "server", not(target_family = "wasm")))]
use bevy::prelude::*;
use dimensify_session::{server::*, *};
use lightyear::prelude::{Linked, LocalAddr};
use std::time::{Duration, Instant};
#[path = "../examples/common/mod.rs"]
mod common;
use common::Cube;
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Other;
impl SharedDocument for Other {
    const SCHEMA: &'static str = "other.v1";
    fn validate(&self) -> bool {
        true
    }
}
fn server<D: SharedDocument>() -> (App, String) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(CollaborationServerPlugin::<D>::default());
    let listener = listen(app.world_mut(), "127.0.0.1:0".parse().unwrap());
    app.finish();
    app.cleanup();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.world().get::<Linked>(listener).is_none() {
        app.update();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(3));
    }
    let endpoint = format!("ws://{}", app.world().get::<LocalAddr>(listener).unwrap().0);
    (app, endpoint)
}
fn probe(endpoint: &str, mut server: Option<&mut App>) -> ProbeResult {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(CollaborationClientPlugin::<Cube>::default());
    app.finish();
    app.cleanup();
    probe_endpoint::<Cube>(app.world_mut(), endpoint).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(server) = server.as_mut() {
            server.update();
        }
        app.update();
        if let Some(result) = app
            .world_mut()
            .resource_mut::<EndpointProbe<Cube>>()
            .take_result()
        {
            assert_eq!(
                app.world().resource::<ClientSession<Cube>>().status,
                ConnectionStatus::Disconnected
            );
            assert!(
                app.world()
                    .resource::<ClientSession<Cube>>()
                    .snapshot
                    .is_none()
            );
            assert!(!app.world().resource::<EndpointProbe<Cube>>().checking());
            return result;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(3));
    }
}
#[test]
fn real_socket_probe_requires_compatible_protocol_without_joining() {
    let (mut good, endpoint) = server::<Cube>();
    assert_eq!(probe(&endpoint, Some(&mut good)).result, Ok(()));
    let (mut other, endpoint) = server::<Other>();
    assert!(probe(&endpoint, Some(&mut other)).result.is_err());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    drop(listener);
    assert!(probe(&endpoint, None).result.is_err());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 2048];
        let _ = stream.read(&mut request);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    assert!(probe(&endpoint, None).result.is_err());
    thread.join().unwrap();
}
