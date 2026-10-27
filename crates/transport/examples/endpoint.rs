//! Validate an endpoint before opening a connection; no server is needed.
use dimensify_transport::client::Endpoint;

fn main() -> std::process::ExitCode {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "wss://EXAMPLE.com:443/collab/?tenant=demo".into());
    match Endpoint::parse(&address) {
        Ok(endpoint) => {
            println!("Canonical WebSocket endpoint: {}", endpoint.websocket);
            println!(
                "Secure connections may try WebTransport first; rooms belong to dimensify_session."
            );
            std::process::ExitCode::SUCCESS
        }
        Err(_) => {
            eprintln!("Expected ws:// or wss:// with a host and no credentials or fragment.");
            std::process::ExitCode::FAILURE
        }
    }
}
