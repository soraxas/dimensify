//! Server-authoritative presentation sharing with an application-owned document.
//!
//! Select a [`SharedDocument`] for client/server plugins. Applications explicitly
//! submit state; role checks and validation precede acceptance. See the crate
//! README and shared-cube examples. No renderer, entity replication or prediction
//! is required by the default client or server features.
mod cursors;
pub use cursors::{
    CursorSample, CursorSubscription, PointerKind, RemoteCursor, SpatialPresence, WorldPointer,
};
mod resume;
pub use resume::ResumeCredential;
mod wire;
pub use wire::*;
mod invite_codes;
pub use invite_codes::{INVITE_CODE_LIFETIME_SECONDS, InviteCode, InviteCodes};
#[cfg(feature = "server")]
pub mod authority;

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "client")]
mod probe;
#[cfg(feature = "client")]
mod transport;
#[cfg(feature = "client")]
pub use probe::*;
#[cfg(feature = "client")]
pub use transport::{
    TransportClientConfig, TransportFailure, TransportKind, retry_webtransport_on_next_connect,
};
mod protocol;
#[cfg(feature = "client")]
pub use client::*;
#[cfg(all(feature = "server", not(target_family = "wasm")))]
pub mod server;

mod registration;
pub use registration::*;
