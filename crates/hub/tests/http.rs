#[path = "../src/bin/cube.rs"]
mod cube;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Server {
    child: Child,
    address: SocketAddr,
}

impl Server {
    fn start() -> Self {
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let child = Command::new(env!("CARGO_BIN_EXE_dimensify-room-server"))
            .env("DIMENSIFY_COLLAB_BIND", address.to_string())
            .env("DIMENSIFY_COLLAB_HTTP", "1")
            .env_remove("DIMENSIFY_COLLAB_TLS_CERT")
            .env_remove("DIMENSIFY_COLLAB_TLS_KEY")
            .env_remove("DIMENSIFY_COLLAB_QUIC_BIND")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut server = Self { child, address };
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(server.child.try_wait().unwrap().is_none(), "server exited");
            // TCP acceptance alone can race child startup or a reused ephemeral port.
            // Require the actual HTTP backend to be ready before exercising requests.
            if server
                .try_raw_request(
                    "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                )
                .is_ok_and(|response| response.starts_with("HTTP/1.1 200"))
            {
                return server;
            }
            assert!(Instant::now() < until, "listener did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn request(&self, path: &str) -> String {
        self.raw_request(&format!(
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        ))
    }

    fn raw_request(&self, request: &str) -> String {
        self.try_raw_request(request)
            .expect("ready HTTP server response")
    }

    fn try_raw_request(&self, request: &str) -> std::io::Result<String> {
        let mut socket = TcpStream::connect(self.address)?;
        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
        socket.write_all(request.as_bytes())?;
        let mut response = String::new();
        socket.read_to_string(&mut response)?;
        Ok(response)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn browser_requests_receive_http_responses_without_stopping_the_server() {
    let mut server = Server::start();
    let response = server.request("/");
    assert!(response.starts_with("HTTP/1.1 200"), "{response:?}");
    assert!(server.request("/favicon.ico").starts_with("HTTP/1.1 404"));
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if server.request("/health").starts_with("HTTP/1.1 200") {
            break;
        }
        assert!(Instant::now() < until, "backend never became ready");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(server.child.try_wait().unwrap().is_none());
}

#[test]
fn invalid_upgrades_and_methods_get_http_errors() {
    let server = Server::start();
    let response = server.raw_request("GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close, Upgrade\r\nUpgrade: websocket\r\n\r\n");
    assert!(response.starts_with("HTTP/1.1 400"), "{response:?}");
    assert!(server.raw_request("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").starts_with("HTTP/1.1 405"));
    let head =
        server.raw_request("HEAD / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert!(head.starts_with("HTTP/1.1 200"));
    assert!(head.split_once("\r\n\r\n").unwrap().1.is_empty());
    assert!(server.request("/").starts_with("HTTP/1.1 200"));
}

#[test]
fn two_clients_sync_and_disconnect_through_http_ingress() {
    let server = Server::start();
    let ready_until = Instant::now() + Duration::from_secs(5);
    while !server.request("/health").starts_with("HTTP/1.1 200") {
        assert!(Instant::now() < ready_until);
        std::thread::sleep(Duration::from_millis(10));
    }
    exercise_room(&format!("ws://{}/", server.address), || {
        // HTTP traffic must not disturb existing room streams.
        assert!(server.request("/favicon.ico").starts_with("HTTP/1.1 404"));
        assert!(server.request("/health").starts_with("HTTP/1.1 200"));
    });
    assert!(server.request("/health").starts_with("HTTP/1.1 200"));
}

#[test]
#[ignore = "creates a temporary room on COLLAB_TEST_ENDPOINT; run only with deployment-test authorization"]
fn deployed_two_clients_sync_and_disconnect() {
    let endpoint = std::env::var("COLLAB_TEST_ENDPOINT")
        .expect("set COLLAB_TEST_ENDPOINT to the deployed wss:// URL");
    assert!(
        endpoint.starts_with("wss://"),
        "deployed test must use validated TLS"
    );
    exercise_room(&endpoint, || {});
}

fn exercise_room(endpoint: &str, before_disconnect: impl FnOnce()) {
    use crate::cube::Cube;
    use bevy::prelude::*;
    use dimensify_session::*;
    type ClientSession = dimensify_session::ClientSession<Cube>;

    fn client() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CollaborationClientPlugin::<Cube>::default());
        app.finish();
        app.cleanup();
        app
    }
    fn drive(
        host: &mut App,
        viewer: &mut App,
        condition: impl Fn(&ClientSession, &ClientSession) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            host.update();
            viewer.update();
            let h = host.world().resource::<ClientSession>();
            let v = viewer.world().resource::<ClientSession>();
            if condition(h, v) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "states: {:?}/{:?}; errors: {:?}/{:?}",
                h.status,
                v.status,
                h.error,
                v.error
            );
            std::thread::sleep(Duration::from_millis(3));
        }
    }

    let (mut host, mut viewer) = (client(), client());
    connect::<Cube>(host.world_mut(), endpoint).unwrap();
    connect::<Cube>(viewer.world_mut(), endpoint).unwrap();
    host.world_mut()
        .resource_mut::<ClientSession>()
        .send(ClientRequest::Create {
            name: "host".into(),
        })
        .unwrap();
    drive(&mut host, &mut viewer, |h, _| h.snapshot.is_some());
    println!(
        "Host created room via {:?}",
        host.world().resource::<ClientSession>().transport_kind()
    );
    let session = host.world().resource::<ClientSession>();
    let room = session.snapshot.as_ref().unwrap().room.clone();
    let invitation = session.invitations.as_ref().unwrap().viewer.clone();
    viewer
        .world_mut()
        .resource_mut::<ClientSession>()
        .send(ClientRequest::Join {
            room,
            invitation,
            name: "viewer".into(),
        })
        .unwrap();
    drive(&mut host, &mut viewer, |h, v| {
        v.snapshot.is_some() && h.snapshot.as_ref().unwrap().participants.len() == 2
    });
    println!("Viewer joined: both clients see two participants");
    host.world_mut()
        .resource_mut::<ClientSession>()
        .send(ClientRequest::SetDocument {
            expected_revision: 0,
            data: Cube {
                center: [0.5, 0., 0.],
                ..default()
            },
        })
        .unwrap();
    drive(&mut host, &mut viewer, |_, v| {
        v.snapshot.as_ref().unwrap().document.revision == 1
    });
    println!("Viewer received shared document revision 1");
    before_disconnect();
    disconnect::<Cube>(viewer.world_mut());
    drive(&mut host, &mut viewer, |h, _| {
        h.snapshot.as_ref().unwrap().participants.len() == 1
    });
    println!("Host observed viewer disconnect");
    // Explicit leave revokes the test host's resume credential instead of keeping
    // a disconnected owner session around on the deployed service.
    host.world_mut()
        .resource_mut::<ClientSession>()
        .leave()
        .unwrap();
    drive(&mut host, &mut viewer, |h, _| {
        h.status == ConnectionStatus::Connected
    });
    disconnect::<Cube>(host.world_mut());
    println!("Host left; test clients disconnected");
}
