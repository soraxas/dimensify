//! Streaming HTTP transport used by the SOG loader.

use std::{
    ops::ControlFlow,
    sync::{Arc, Mutex},
};

use ehttp::streaming::Part;

/// Shared state for one streaming download. The callback runs on a native
/// worker thread or a wasm `ReadableStream`, so updates cross a lock.
struct FetchState<T> {
    received: u64,
    total: Option<u64>,
    buf: Vec<u8>,
    result: Option<Result<T, String>>,
    finished: bool,
}

impl<T> Default for FetchState<T> {
    fn default() -> Self {
        Self {
            received: 0,
            total: None,
            buf: Vec::new(),
            result: None,
            finished: false,
        }
    }
}

/// Handle to an in-flight streaming fetch. Clones share the same state.
#[derive(Clone)]
pub struct FetchSlot<T = Vec<u8>>(Arc<Mutex<FetchState<T>>>);

impl FetchSlot {
    /// Starts a streaming GET and returns immediately.
    pub fn fetch(url: &str) -> Self {
        Self::fetch_limited(url, 256 * 1024 * 1024)
    }

    /// Start an HTTP request with a bounded response body.
    pub fn fetch_limited(url: &str, max_bytes: usize) -> Self {
        Self::fetch_mapped(url, max_bytes, Ok)
    }
}

impl<T: Send + 'static> FetchSlot<T> {
    /// Create a pending completion slot for deterministic acceptance tests.
    #[cfg(test)]
    pub fn pending() -> Self {
        Self(Arc::new(Mutex::new(FetchState::default())))
    }

    /// Create a completed acceptance-test fetch slot.
    #[cfg(test)]
    pub fn completed(result: Result<T, String>) -> Self {
        Self(Arc::new(Mutex::new(FetchState {
            result: Some(result),
            finished: true,
            ..Default::default()
        })))
    }

    /// Transform a completed response immediately, without waiting for an app
    /// update. Keep this bounded to metadata/transport work, never heavy decode.
    /// Dropping the last slot cancels unfinished transport and its transform.
    pub fn fetch_mapped(
        url: &str,
        max_bytes: usize,
        map: impl Fn(Vec<u8>) -> Result<T, String> + Send + 'static,
    ) -> Self {
        let state = Arc::new(Mutex::new(FetchState::default()));
        let shared = Arc::downgrade(&state);
        let request_url = url.to_string();
        ehttp::streaming::fetch(ehttp::Request::get(url), move |part| {
            let Some(shared) = shared.upgrade() else {
                return ControlFlow::Break(());
            };
            let mut state = shared.lock().unwrap();
            if state.finished {
                return ControlFlow::Break(());
            }
            match part {
                Ok(Part::Response(response)) => {
                    if !(200..300).contains(&response.status) {
                        state.finished = true;
                        state.result = Some(Err(format!(
                            "HTTP {} fetching {request_url}",
                            response.status
                        )));
                        return ControlFlow::Break(());
                    }
                    // Both values are compressed, on-the-wire byte counts.
                    state.total = response
                        .headers
                        .get("content-length")
                        .and_then(|value| value.trim().parse::<u64>().ok());
                    ControlFlow::Continue(())
                }
                Ok(Part::Chunk(chunk)) => {
                    if chunk.is_empty() {
                        let bytes = std::mem::take(&mut state.buf);
                        state.finished = true;
                        // A transform may start other fetches; don't hold this
                        // response's mutex while doing so.
                        drop(state);
                        let result = map(bytes);
                        shared.lock().unwrap().result = Some(result);
                        ControlFlow::Break(())
                    } else {
                        if chunk.len() > max_bytes.saturating_sub(state.buf.len()) {
                            state.finished = true;
                            state.buf.clear();
                            state.result =
                                Some(Err(format!("download exceeds byte budget: {request_url}")));
                            return ControlFlow::Break(());
                        }
                        state.received += chunk.len() as u64;
                        state.buf.extend_from_slice(&chunk);
                        ControlFlow::Continue(())
                    }
                }
                Err(err) => {
                    state.finished = true;
                    state.buf.clear();
                    state.result =
                        Some(Err(format!("network error fetching {request_url}: {err}")));
                    ControlFlow::Break(())
                }
            }
        });
        Self(state)
    }

    /// Takes the final result once it is available.
    pub fn take_result(&self) -> Option<Result<T, String>> {
        self.0.lock().unwrap().result.take()
    }

    /// Report completion without consuming the response.
    pub fn is_done(&self) -> bool {
        self.0.lock().unwrap().result.is_some()
    }

    /// Returns received bytes and the optional content length.
    pub fn progress(&self) -> (u64, Option<u64>) {
        let state = self.0.lock().unwrap();
        (state.received, state.total)
    }
}

type AssetLookup = dyn Fn(&str) -> Result<AssetLocation, String> + Send + Sync;

/// Resolve a source reference without embedding provider authorization in the loader.
#[derive(Clone)]
pub struct AssetResolver(std::sync::Arc<AssetLookup>);
/// Authorized location or already supplied bytes for one resource.
pub enum AssetLocation {
    /// Fetch this complete URL; query credentials belong to this URL only.
    Url(String),
    /// Decode supplied bytes without network access.
    Bytes(std::sync::Arc<[u8]>),
}
impl Default for AssetResolver {
    fn default() -> Self {
        Self::new(|reference| {
            let url = url::Url::parse(reference)
                .map_err(|_| "Expected an absolute HTTP(S) resource URL".to_owned())?;
            if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                return Err("Expected an absolute HTTP(S) resource URL".into());
            }
            Ok(AssetLocation::Url(url.into()))
        })
    }
}
impl AssetResolver {
    /// Supply application-owned reference resolution or private byte access.
    pub fn new(
        resolve: impl Fn(&str) -> Result<AssetLocation, String> + Send + Sync + 'static,
    ) -> Self {
        Self(std::sync::Arc::new(resolve))
    }
    /// Resolve and fetch bounded bytes; dropping all returned handles retires completion.
    pub fn fetch(&self, reference: &str, limit: usize) -> FetchSlot {
        FetchSlot::fetch_mapped_resolved(reference, limit, self, Ok)
    }
}
impl<T: Send + 'static> FetchSlot<T> {
    /// Supply an immediate owned completion, including resolver failures.
    pub fn ready(result: Result<T, String>) -> Self {
        Self(Arc::new(Mutex::new(FetchState {
            result: Some(result),
            finished: true,
            ..Default::default()
        })))
    }
    /// Resolve one resource and transform its bounded response outside the ECS world.
    pub fn fetch_mapped_resolved(
        reference: &str,
        limit: usize,
        resolver: &AssetResolver,
        map: impl Fn(Vec<u8>) -> Result<T, String> + Send + 'static,
    ) -> Self {
        match (resolver.0)(reference) {
            Ok(AssetLocation::Url(url)) => Self::fetch_mapped(&url, limit, map),
            Ok(AssetLocation::Bytes(bytes)) if bytes.len() <= limit => {
                Self::ready(map(bytes.to_vec()))
            }
            Ok(AssetLocation::Bytes(_)) => Self::ready(Err("resource exceeds byte budget".into())),
            Err(error) => Self::ready(Err(error)),
        }
    }
}
/// Directory of a validated resource URL, without implicitly forwarding credentials.
pub fn url_directory(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rfind('/').map_or("", |index| &path[..=index])
}
/// Human-readable loading label without transport filenames.
pub fn loading_label(name: &str) -> String {
    if [".json", ".sog", ".ply", ".spz"]
        .iter()
        .any(|ext| name.to_ascii_lowercase().ends_with(ext))
    {
        "Gaussian splats".into()
    } else {
        name.to_owned()
    }
}
#[cfg(test)]
mod resolver_tests {
    use super::*;
    #[test]
    fn supplied_bytes_and_denied_access_use_the_same_completion_contract() {
        let resolver = AssetResolver::new(|reference| match reference {
            "scene/chunk" => Ok(AssetLocation::Bytes(b"bytes".as_slice().into())),
            _ => Err("denied".into()),
        });
        assert_eq!(
            resolver
                .fetch("scene/chunk", 5)
                .take_result()
                .unwrap()
                .unwrap(),
            b"bytes"
        );
        assert!(
            resolver
                .fetch("scene/chunk", 4)
                .take_result()
                .unwrap()
                .unwrap_err()
                .contains("budget")
        );
        assert_eq!(
            resolver
                .fetch("private", 100)
                .take_result()
                .unwrap()
                .unwrap_err(),
            "denied"
        );
        let request = resolver.fetch("scene/chunk", 5);
        assert!(request.take_result().is_some());
        assert!(request.take_result().is_none());
    }
}
