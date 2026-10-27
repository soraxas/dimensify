# Collaboration

Dimensify supports server-authoritative rooms with application-defined documents.
The default client and host require no renderer, window or physics backend.

| Package | Owns |
| --- | --- |
| `dimensify_transport` | WebSocket/WebTransport connections, fallback, cancellation and listeners |
| `dimensify_session` | Schema handshake, rooms, roles, invitations, private resume, document revisions and optional presentation sharing |
| `dimensify_hub` | Headless host composition, bind/TLS configuration and optional HTTP readiness ingress |

Applications implement `SharedDocument` and register typed `DocumentProjection`s
with a `WritePolicy`. An empty `RegistrationSet` shares no document projections
or camera lane; `.presentation_camera()` explicitly enables camera sharing.
The [cube schema](../crates/session/examples/common/mod.rs) is a complete example.
Applications own asset access, domain validation and UI. An invitation authorizes
room participation; it does not grant access to referenced files or remote assets.

## Run a local room

From the workspace root, in separate terminals:

```sh
rtk cargo run --locked -p dimensify_hub --bin dimensify-room-server
rtk cargo run --locked -p dimensify_session --example session_check -- ws://127.0.0.1:5002
```

The host uses the shared-cube schema. The check creates a room, joins by editor
code, changes the document, exchanges cursors in both directions, disconnects,
resumes the same identity and explicitly leaves. For a graphical native client:

```sh
rtk cargo run --locked -p dimensify_session --features example-viewer --example shared_cube
```

For two viewers using the Dimensify camera plugins, use `rtk just collab-peer`
and follow the [two-peer walkthrough](examples.md#connect-two-dimensify-viewers).

To host a different schema, add `MinimalPlugins` to a Bevy `App`, then call
`dimensify_hub::configure::<YourDocument>(&mut app, config)`, or use
`dimensify_hub::run::<YourDocument>(config)` for the default headless runner.
`HostConfig::from_env(prefix, default_address)` lets a caller select its own
configuration namespace; no account service or application configuration is required.

## Hosting and transport

The included executable reads these environment variables:

| Variable | Meaning |
| --- | --- |
| `DIMENSIFY_COLLAB_BIND` | TCP bind address; default `127.0.0.1:5002` |
| `DIMENSIFY_COLLAB_TLS_CERT`, `DIMENSIFY_COLLAB_TLS_KEY` | PEM certificate and private key, supplied together |
| `DIMENSIFY_COLLAB_QUIC_BIND` | Optional UDP bind override; requires TLS |
| `DIMENSIFY_COLLAB_WS_PLAINTEXT=1` | Plaintext WebSocket behind a TLS proxy, while QUIC uses the configured certificate |
| `DIMENSIFY_COLLAB_HTTP=1` | HTTP information, `/health` readiness, and WebSocket upgrades; use behind a TLS proxy, without QUIC/TLS settings |

Secure endpoints attempt WebTransport first, then WebSocket if connection or
protocol verification fails. Plaintext `ws://` endpoints use WebSocket directly.
The session supplies the handshake through the transport's `ConnectionProtocol`
interface. Multiple listeners share one authority; closing one listener does
not stop the others. Controls use a bounded reliable ordered lane; presence
and camera samples use coalesced latest-value lanes. Cursor publishing waits
for the subscription acknowledgement and may return `NotJoined` while it is pending.

The active protocol remains `collaboration.v9`. Peers must agree on the document
schema and canonical registration descriptors. The package move does not change
wire identifiers. The Lightyear/Aeronet and wtransport overrides in `Cargo.toml`
retain pending-handshake cancellation and closed-stream fixes. Consumers with
their own workspace root must carry these patches until compatible releases
include the fixes; dependency-level Cargo patches do not propagate to consumers.

!!! note "Current scope"
    Rooms and documents are memory-only. Restart loses rooms, codes and resume
    state. Codes expire after 24 hours and can be renewed by the owner. A private
    resume credential preserves identity through a temporary disconnect, subject
    to a 30-minute retention window; explicit leave revokes it. Keep credentials
    private and bind saved credentials to their issuing endpoint.

Discovery/matchmaking, WebRTC peer-to-peer routing, NAT traversal, durable room
storage and host migration are future work. This host is the authoritative peer;
it does not elect a browser host or run simulation physics.

## Verification

```sh
rtk cargo test --locked -p dimensify_transport -p dimensify_session -p dimensify_hub --features dimensify_session/server,dimensify_transport/server
rtk python3 scripts/verify-standalone.py --test
```

The export check copies source to an unrelated temporary directory, resolves
manifests there and rejects renderer or robot dependencies in the host graph.
Its offline mode requires dependencies already present in Cargo's cache.

To exercise the browser client without a canvas or GPU, start the local host,
then build and serve the acceptance example:

```sh
rtk cargo build --locked -p dimensify_session --example session_check --target wasm32-unknown-unknown
rtk wasm-bindgen --target web --out-dir /tmp/dimensify-browser target/wasm32-unknown-unknown/debug/examples/session_check.wasm
rtk cp crates/session/examples/session_check.html /tmp/dimensify-browser/index.html
rtk python3 -m http.server 8000 --bind 127.0.0.1 --directory /tmp/dimensify-browser
```

Use the wasm-bindgen CLI matching the version in `Cargo.lock`. With Playwright
and its Chromium browser installed, run in another terminal:

```sh
rtk node scripts/check-browser.cjs http://127.0.0.1:8000
```

`PLAYWRIGHT_MODULE` can point to a separately installed Playwright package;
`CHROMIUM_EXECUTABLE` can select an existing compatible Chromium binary.
The page accepts `?endpoint=ws%3A%2F%2F127.0.0.1%3A5002` for another test host.
The check uses a fresh room and leaves it on success. It verifies networking
and state synchronization; it does not establish rendered-viewer, device or
browser WebTransport/TLS support.
