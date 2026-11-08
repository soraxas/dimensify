use std::{collections::BTreeMap, future::Future, num::ParseIntError, sync::Arc};

use async_lock::Semaphore;

use super::{ByteSource, ByteSourceError};

pub struct HttpSource {
    pub(crate) url: String,
    range_request_limiter: Option<RangeRequestLimiter>,
}

impl HttpSource {
    pub fn open(url: &str) -> Result<Self, ByteSourceError> {
        Self::open_with_concurrency(url, None)
    }

    pub fn open_with_concurrency(
        url: &str,
        max_concurrent_requests: Option<usize>,
    ) -> Result<Self, ByteSourceError> {
        Ok(Self {
            url: url.to_string(),
            range_request_limiter: max_concurrent_requests.map(RangeRequestLimiter::new),
        })
    }

    /// Plain GET, no Range header — relies on and requires a 200 response.
    ///
    /// Used only for multi-file octree node fetches: Chrome's HTTP cache is
    /// unreliable for concurrent 206 Partial Content responses that share
    /// one cache key (new nodes lost, existing entries evicted, under
    /// concurrent cold loads). A distinct URL + plain 200 response per node
    /// caches correctly regardless of concurrency. Kept as its own method
    /// (rather than reusing `read_to_end`) so this "no Range header, ever"
    /// contract stays explicit and auditable.
    pub async fn read_whole_no_range(&self) -> Result<Vec<u8>, ByteSourceError> {
        ehttp_get(&self.url, None).await
    }
}

#[derive(Clone)]
struct RangeRequestLimiter(Arc<Semaphore>);

impl RangeRequestLimiter {
    fn new(max_concurrent_requests: usize) -> Self {
        assert!(max_concurrent_requests > 0);
        Self(Arc::new(Semaphore::new(max_concurrent_requests)))
    }

    async fn run<T>(&self, network_future: impl Future<Output = T>) -> T {
        let permit = self.0.acquire_arc().await;
        let output = network_future.await;
        drop(permit);
        output
    }
}

impl ByteSource for HttpSource {
    async fn read_to_end(&self, offset: u64) -> Result<Vec<u8>, ByteSourceError> {
        let mut headers = BTreeMap::new();
        if offset > 0 {
            headers.insert("range".into(), format!("bytes={}-", offset));
        }

        ehttp_get(&self.url, Some(headers)).await
    }

    async fn read_range(&self, offset: u64, length: u64) -> Result<Vec<u8>, ByteSourceError> {
        let end = offset.checked_add(length).map(|v| v - 1).ok_or_else(|| {
            ByteSourceError::ByteSource(HttpSourceError("Range overflow".into()).into())
        })?;

        let mut headers = BTreeMap::new();
        headers.insert("range".into(), format!("bytes={}-{}", offset, end));

        let request = ehttp_get(&self.url, Some(headers));
        if let Some(limiter) = &self.range_request_limiter {
            limiter.run(request).await
        } else {
            request.await
        }
    }

    async fn size(&self) -> Result<Option<u64>, ByteSourceError> {
        ehttp_get_size(&self.url, None).await
    }
}

#[derive(Debug)]
pub struct HttpSourceError(String);

impl std::fmt::Display for HttpSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for HttpSourceError {}

async fn ehttp_get_size(
    url: &str,
    headers: Option<BTreeMap<String, String>>,
) -> Result<Option<u64>, ByteSourceError> {
    let headers = build_headers(headers);
    let request = ehttp::Request {
        method: "HEAD".to_owned(),
        url: url.to_string(),
        body: vec![],
        headers,
        #[cfg(target_arch = "wasm32")]
        mode: ehttp::Mode::default(),
    };

    let response = send_request(request).await?;

    response
        .headers
        .get("Content-Length")
        .map(&str::parse)
        .transpose()
        .map_err(|e: ParseIntError| ByteSourceError::ByteSource(e.into()))
}

async fn ehttp_get(
    url: &str,
    headers: Option<BTreeMap<String, String>>,
) -> Result<Vec<u8>, ByteSourceError> {
    let headers = build_headers(headers);
    let request = ehttp::Request {
        method: "GET".to_owned(),
        url: url.to_string(),
        body: vec![],
        headers,
        #[cfg(target_arch = "wasm32")]
        mode: ehttp::Mode::default(),
    };

    let response = send_request(request).await?;
    Ok(response.bytes)
}

/// Convert an optional header map into `ehttp::Headers`.
fn build_headers(map: Option<BTreeMap<String, String>>) -> ehttp::Headers {
    let mut headers = ehttp::Headers::default();
    if let Some(m) = map {
        for (k, v) in m {
            headers.insert(k, v);
        }
    }
    headers
}

/// Send an `ehttp` request and return the response, mapping errors uniformly.
async fn send_request(request: ehttp::Request) -> Result<ehttp::Response, ByteSourceError> {
    let method = request.method.clone();
    let url = request.url.clone();
    let (tx, rx) = futures::channel::oneshot::channel();
    ehttp::fetch(request, move |res| {
        let _ = tx.send(res);
    });

    let result = rx.await.map_err(|_| {
        ByteSourceError::ByteSource(HttpSourceError("channel closed".into()).into())
    })?;

    let response = result
        .map_err(|e| ByteSourceError::ByteSource(HttpSourceError(format!("{e:?}")).into()))?;

    if !(200..300).contains(&(response.status as usize)) {
        return Err(ByteSourceError::ByteSource(
            http_status_error(&method, &url, response.status).into(),
        ));
    }

    Ok(response)
}

fn http_status_error(method: &str, url: &str, status: u16) -> HttpSourceError {
    HttpSourceError(format!("HTTP {status} for {method} {url}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_request_limiter_caps_only_concurrent_network_futures() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        use futures::{executor::block_on, future::poll_fn, join};

        async fn yield_once() {
            let mut yielded = false;
            poll_fn(move |cx| {
                if yielded {
                    std::task::Poll::Ready(())
                } else {
                    yielded = true;
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
            })
            .await;
        }

        async fn observed_request(
            limiter: RangeRequestLimiter,
            active: Arc<AtomicUsize>,
            peak: Arc<AtomicUsize>,
        ) {
            limiter
                .run(async {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    yield_once().await;
                    active.fetch_sub(1, Ordering::SeqCst);
                })
                .await;
        }

        let limiter = RangeRequestLimiter::new(2);
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        block_on(async {
            join!(
                observed_request(limiter.clone(), active.clone(), peak.clone()),
                observed_request(limiter.clone(), active.clone(), peak.clone()),
                observed_request(limiter.clone(), active.clone(), peak.clone()),
                observed_request(limiter, active, peak.clone()),
            );
        });

        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn status_errors_identify_the_failed_request() {
        let error = http_status_error("HEAD", "https://example.com/pcd/octree.bin", 404);

        assert_eq!(
            error.to_string(),
            "HTTP 404 for HEAD https://example.com/pcd/octree.bin"
        );
    }
}
