//! Headless native TLS fixture / browser-WASM client for transport acceptance.
//! The native fixture and browser client share the neutral cube schema.
mod common;
use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use common::Cube;
#[cfg(target_family = "wasm")]
use dimensify_session::*;
use std::time::Duration;

#[cfg(all(not(target_family = "wasm"), feature = "server"))]
fn main() {
    use aeronet_websocket::rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
    use dimensify_session::server::*;
    let dir = std::env::var("DIMENSIFY_SMOKE_DIR")
        .expect("DIMENSIFY_SMOKE_DIR for ephemeral certificate");
    let address = std::env::var("DIMENSIFY_SMOKE_BIND")
        .unwrap_or_else(|_| "127.0.0.1:5443".into())
        .parse()
        .unwrap();
    let identity = WebTransportIdentity::self_signed(["localhost", "127.0.0.1"]).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        format!("{dir}/cert.pem"),
        identity.certificate_chain().as_slice()[0].to_pem(),
    )
    .unwrap();
    std::fs::write(
        format!("{dir}/hash.txt"),
        identity.certificate_chain().as_slice()[0]
            .hash()
            .to_string(),
    )
    .unwrap();
    let socket_identity = lightyear::websocket::server::Identity::new(
        identity
            .certificate_chain()
            .as_slice()
            .iter()
            .map(|c| CertificateDer::from(c.der().to_vec())),
        PrivatePkcs8KeyDer::from(identity.private_key().secret_der().to_vec()).into(),
    );
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(5))))
        .add_plugins(CollaborationServerPlugin::<Cube>::default());
    listen_websocket(
        app.world_mut(),
        lightyear::websocket::server::ServerConfig::builder()
            .with_bind_address(address)
            .with_identity(socket_identity),
    );
    if std::env::var("DIMENSIFY_SMOKE_QUIC").as_deref() != Ok("0") {
        listen_webtransport(app.world_mut(), address, identity);
    }
    println!("Transport fixture on {address}");
    app.run();
}

#[cfg(target_family = "wasm")]
fn main() {
    let endpoint = js_sys::Reflect::get(&js_sys::global(), &"COLLAB_ENDPOINT".into())
        .unwrap()
        .as_string()
        .unwrap();
    let hash = js_sys::Reflect::get(&js_sys::global(), &"COLLAB_CERT_HASH".into())
        .unwrap()
        .as_string()
        .unwrap();
    let hash: Vec<u8> = hash
        .replace(':', "")
        .as_bytes()
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let config = TransportClientConfig {
        webtransport: std::sync::Arc::new(move || {
            use aeronet_webtransport::xwt_web::{CertificateHash, HashAlgorithm};
            aeronet_webtransport::client::ClientConfig {
                server_certificate_hashes: vec![CertificateHash {
                    algorithm: HashAlgorithm::Sha256,
                    value: hash.clone(),
                }],
                ..default()
            }
        }),
        ..default()
    };
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(5))))
        .insert_resource(config)
        .add_plugins(CollaborationClientPlugin::<Cube>::default())
        .add_systems(Update, report.after(CollaborationSystems::Transport));
    app.finish();
    app.cleanup();
    connect::<Cube>(app.world_mut(), &endpoint).unwrap();
    app.world_mut()
        .resource_mut::<ClientSession<Cube>>()
        .send(ClientRequest::Create {
            name: "browser-smoke".into(),
        })
        .unwrap();
    app.run();
}

#[cfg(target_family = "wasm")]
fn report(session: Res<ClientSession<Cube>>) {
    let value = format!(
        "{:?}:{:?}:{:?}",
        session.status,
        session.transport_kind(),
        session.error
    );
    js_sys::Reflect::set(&js_sys::global(), &"COLLAB_RESULT".into(), &value.into()).unwrap();
}

#[cfg(all(not(target_family = "wasm"), not(feature = "server")))]
fn main() {
    eprintln!("Run with --features server to start the native fixture.");
}
