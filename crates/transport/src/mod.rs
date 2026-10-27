//! Native/browser connections and listeners without room or document policy.
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "client")]
pub use client::{TransportClientConfig, TransportFailure, TransportKind};
#[cfg(all(feature = "server", not(target_family = "wasm")))]
pub mod server;
