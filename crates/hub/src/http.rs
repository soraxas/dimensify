//! Optional HTTP ingress for reverse-proxy hosting. Only a fixed loopback
//! WebSocket listener is proxied; room protocol and authorization remain there.
use bevy::prelude::*;
use bytes::Bytes;
use dimensify_session::server::CollaborationListener;
use http_body_util::Full;
use hyper::{Request, Response, StatusCode, body::Incoming, header, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use lightyear::prelude::{Linked, LocalAddr};
use std::{
    convert::Infallible,
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio::{
    net::TcpStream,
    sync::{OwnedSemaphorePermit, Semaphore, oneshot},
    time::timeout,
};

const DEADLINE: Duration = Duration::from_secs(5);
type Readiness = Arc<Mutex<Option<(SocketAddr, Instant)>>>;
type Body = Full<Bytes>;
type LiveWebSocketListener = (
    With<CollaborationListener>,
    With<Linked>,
    Without<lightyear::prelude::server::WebTransportServerIo>,
);

#[derive(Resource)]
pub(super) struct HttpIngress {
    ready: Readiness,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl HttpIngress {
    /// Bind the HTTP port and own its runtime until the Bevy app is dropped.
    pub(super) fn start(address: SocketAddr, description: &'static str) -> io::Result<Self> {
        let listener = std::net::TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let ready: Readiness = Arc::default();
        let state = ready.clone();
        let (stop, mut stopped) = oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("collaboration-http".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let listener =
                        tokio::net::TcpListener::from_std(listener).expect("HTTP listener");
                    // Bound slow handshakes and upgraded streams, leaving probe capacity.
                    let slots = Arc::new(Semaphore::new(128));
                    loop {
                        let accepted = tokio::select! {
                            _ = &mut stopped => break,
                            accepted = listener.accept() => accepted,
                        };
                        let (socket, _) = match accepted {
                            Ok(socket) => socket,
                            Err(error) => {
                                error!("HTTP accept failed: {error}");
                                break;
                            }
                        };
                        let Ok(slot) = slots.clone().try_acquire_owned() else {
                            continue;
                        };
                        let slot = Arc::new(slot);
                        let state = state.clone();
                        tokio::spawn(async move {
                            let service = service_fn(move |request| {
                                handle(request, state.clone(), slot.clone(), description)
                            });
                            let result = hyper::server::conn::http1::Builder::new()
                                .timer(TokioTimer::new())
                                .header_read_timeout(DEADLINE)
                                .max_buf_size(16 * 1024)
                                .serve_connection(TokioIo::new(socket), service)
                                .with_upgrades()
                                .await;
                            if let Err(error) = result {
                                debug!("HTTP connection ended: {error}");
                            }
                        });
                    }
                });
            })?;
        info!("Collaboration HTTP ingress listening on {address}");
        Ok(Self {
            ready,
            stop: Some(stop),
            thread: Some(thread),
        })
    }
}

impl Drop for HttpIngress {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Publish listener readiness each tick; a stuck/stopped app must not look healthy.
pub(super) fn heartbeat(
    ingress: Option<Res<HttpIngress>>,
    listeners: Query<&LocalAddr, LiveWebSocketListener>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(ingress) = ingress else { return };
    *ingress.ready.lock().unwrap() = listeners
        .single()
        .ok()
        .map(|address| (address.0, Instant::now()));
    if ingress.thread.as_ref().is_some_and(JoinHandle::is_finished) {
        error!("Collaboration HTTP ingress stopped");
        exit.write(AppExit::error());
    }
}

fn backend(state: &Readiness) -> Option<SocketAddr> {
    state
        .lock()
        .unwrap()
        .filter(|(_, updated)| updated.elapsed() < DEADLINE)
        .map(|(address, _)| address)
}

fn response(status: StatusCode, text: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Full::new(Bytes::from_static(text.as_bytes())))
        .unwrap()
}

async fn handle(
    mut request: Request<Incoming>,
    state: Readiness,
    slot: Arc<OwnedSemaphorePermit>,
    description: &'static str,
) -> Result<Response<Body>, Infallible> {
    let is_get = request.method() == hyper::Method::GET;
    let is_head = request.method() == hyper::Method::HEAD;
    let mut result = if !is_get && !is_head {
        let mut result = response(StatusCode::METHOD_NOT_ALLOWED, "Use GET or HEAD.\n");
        result
            .headers_mut()
            .insert(header::ALLOW, "GET, HEAD".parse().unwrap());
        result
    } else if request.uri().path() == "/health" {
        match backend(&state) {
            Some(_) => response(StatusCode::OK, "ready\n"),
            None => response(StatusCode::SERVICE_UNAVAILABLE, "not ready\n"),
        }
    } else if request.uri().path() != "/" {
        response(StatusCode::NOT_FOUND, "Not found.\n")
    } else if request.headers().contains_key(header::UPGRADE) {
        let websocket = is_get
            && request
                .headers()
                .get(header::UPGRADE)
                .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"websocket"))
            && request
                .headers()
                .get_all(header::CONNECTION)
                .iter()
                .any(|v| {
                    v.to_str().is_ok_and(|s| {
                        s.split(',')
                            .any(|s| s.trim().eq_ignore_ascii_case("upgrade"))
                    })
                })
            && request
                .headers()
                .get("sec-websocket-version")
                .is_some_and(|v| v == "13")
            && request.headers().contains_key("sec-websocket-key")
            && !request.headers().contains_key(header::TRANSFER_ENCODING)
            && request
                .headers()
                .get(header::CONTENT_LENGTH)
                .is_none_or(|v| v == "0");
        if !websocket {
            response(StatusCode::BAD_REQUEST, "Invalid WebSocket upgrade.\n")
        } else if let Some(address) = backend(&state) {
            let downstream = hyper::upgrade::on(&mut request);
            match timeout(DEADLINE, upgrade(request, address)).await {
                Ok(Ok((result, upstream))) => {
                    tokio::spawn(async move {
                        let _slot = slot;
                        if let Ok(Ok(client)) = timeout(DEADLINE, downstream).await {
                            let _ = tokio::io::copy_bidirectional(
                                &mut TokioIo::new(client),
                                &mut TokioIo::new(upstream),
                            )
                            .await;
                        }
                    });
                    result
                }
                Ok(Err(error)) => {
                    warn!("WebSocket upstream handshake failed: {error}");
                    response(StatusCode::BAD_GATEWAY, "WebSocket backend unavailable.\n")
                }
                Err(_) => {
                    warn!("WebSocket upstream handshake timed out");
                    response(
                        StatusCode::GATEWAY_TIMEOUT,
                        "WebSocket backend timed out.\n",
                    )
                }
            }
        } else {
            response(StatusCode::SERVICE_UNAVAILABLE, "not ready\n")
        }
    } else {
        response(StatusCode::OK, description)
    };
    // Disabling Hyper keep-alive globally overwrites even a 101's Connection:
    // Upgrade with Connection: close. Close only ordinary HTTP responses.
    if result.status() != StatusCode::SWITCHING_PROTOCOLS {
        result
            .headers_mut()
            .insert(header::CONNECTION, "close".parse().unwrap());
    }
    Ok(result)
}

async fn upgrade(
    request: Request<Incoming>,
    address: SocketAddr,
) -> Result<(Response<Body>, hyper::upgrade::Upgraded), Box<dyn std::error::Error + Send + Sync>> {
    let socket = TcpStream::connect(address).await?;
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(TokioIo::new(socket)).await?;
    // Dropping this guard aborts the driver on error/timeout, avoiding orphaned requests.
    struct Driver(tokio::task::JoinHandle<()>);
    impl Drop for Driver {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let _driver = Driver(tokio::spawn(async {
        let _ = connection.with_upgrades().await;
    }));
    let (mut parts, _) = request.into_parts();
    // Do not forward downstream upgrade extensions or any request body.
    parts.extensions.clear();
    let mut result = sender
        .send_request(Request::from_parts(parts, Full::new(Bytes::new())))
        .await?;
    if result.status() != StatusCode::SWITCHING_PROTOCOLS {
        return Err("backend rejected upgrade".into());
    }
    let upstream = hyper::upgrade::on(&mut result).await?;
    let (parts, _) = result.into_parts();
    Ok((
        Response::from_parts(parts, Full::new(Bytes::new())),
        upstream,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_requires_a_recent_live_listener() {
        let state: Readiness = Arc::default();
        assert!(backend(&state).is_none());
        let address = "127.0.0.1:1234".parse().unwrap();
        *state.lock().unwrap() = Some((address, Instant::now()));
        assert_eq!(backend(&state), Some(address));
        *state.lock().unwrap() = Some((address, Instant::now() - Duration::from_secs(6)));
        assert!(backend(&state).is_none());
    }
}
