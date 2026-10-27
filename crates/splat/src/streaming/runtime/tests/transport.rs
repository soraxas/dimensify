//! Real localhost transport regressions: no app ticks or decoder are involved.
use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const META: &[u8] = br#"{"version":2,"count":1,
    "means":{"mins":[-1,-1,-1],"maxs":[1,1,1],"files":["lo.webp","hi.webp"]},
    "scales":{"codebook":[0],"files":["scales.webp"]},
    "quats":{"files":["quats.webp"]},
    "sh0":{"codebook":[0],"files":["sh0.webp"]},
    "shN":{"count":1,"bands":1,"codebook":[0],"files":["centroids.webp","labels.webp"]}}"#;

struct Server {
    url: String,
    requests: mpsc::Receiver<String>,
    release: mpsc::Sender<()>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    fn new(body: Vec<u8>, status: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/meta.json", listener.local_addr().unwrap());
        let (sent, requests) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                };
                // Accepted sockets inherit nonblocking mode on some platforms.
                // Wait for headers instead of dropping a just-connected client.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut request = Vec::new();
                let mut block = [0; 1024];
                while !request.windows(4).any(|s| s == b"\r\n\r\n") && request.len() < 8192 {
                    match stream.read(&mut block) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&block[..n]),
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let Some(path) = request.split_whitespace().nth(1) else {
                    continue;
                };
                let _ = sent.send(path.to_owned());
                if path == "/meta.json" && released.recv_timeout(Duration::from_secs(2)).is_err() {
                    continue;
                }
                let payload = if path == "/meta.json" {
                    body.as_slice()
                } else {
                    b"not decoded"
                };
                // A deliberately cancelled download may close its socket.
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = stream.write_all(payload);
            }
        });
        Self {
            url,
            requests,
            release,
            stop,
            thread: Some(thread),
        }
    }

    fn next(&self) -> String {
        self.requests
            .recv_timeout(Duration::from_secs(2))
            .expect("request arrives")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.release.send(());
        self.thread.take().unwrap().join().unwrap();
    }
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !ready() {
        assert!(Instant::now() < deadline, "transport did not complete");
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn textures_start_after_metadata_without_a_viewer_poll() {
    for storage in [SourceStorage::Expanded, SourceStorage::Packed] {
        let server = Server::new(META.to_vec(), 200);
        let _load = ChunkLoad::new(server.url.clone(), Some(1), 1, 0., storage);
        assert_eq!(server.next(), "/meta.json");
        server.release.send(()).unwrap();
        let mut expected = vec![
            "/hi.webp",
            "/lo.webp",
            "/quats.webp",
            "/scales.webp",
            "/sh0.webp",
        ];
        if storage == SourceStorage::Packed
            || bevy_gaussian_splatting::material::spherical_harmonics::SH_COEFF_COUNT_PER_CHANNEL
                > 1
        {
            expected.extend(["/centroids.webp", "/labels.webp"]);
        }
        let mut paths: Vec<_> = (0..expected.len()).map(|_| server.next()).collect();
        paths.sort();
        expected.sort();
        assert_eq!(
            paths, expected,
            "metadata completion starts only required textures without app polling"
        );
    }
}

#[test]
fn rejected_metadata_never_starts_dependent_requests() {
    for (body, expected, max) in [
        (META.to_vec(), Some(2), 2),
        (META.to_vec(), Some(1), 0),
        (b"invalid json".to_vec(), None, 1),
    ] {
        let server = Server::new(body, 200);
        let load = ChunkLoad::new(server.url.clone(), expected, max, 0., SourceStorage::Packed);
        assert_eq!(server.next(), "/meta.json");
        server.release.send(()).unwrap();
        let Phase::Meta(slot) = &load.phase else {
            panic!("metadata request");
        };
        wait_until(|| slot.is_done());
        assert!(slot.take_result().unwrap().is_err());
        assert!(
            server.requests.try_recv().is_err(),
            "validation precedes dependent fetches"
        );
    }
}

#[test]
fn dropping_metadata_owner_prevents_late_texture_requests() {
    let server = Server::new(META.to_vec(), 200);
    let load = ChunkLoad::new(server.url.clone(), Some(1), 1, 0., SourceStorage::Packed);
    assert_eq!(server.next(), "/meta.json");
    drop(load);
    server.release.send(()).unwrap();
    assert!(
        server
            .requests
            .recv_timeout(Duration::from_millis(200))
            .is_err()
    );
}

#[test]
fn response_errors_and_byte_limits_do_not_invoke_the_completion_transform() {
    for (status, limit) in [(404, 1024), (200, 8)] {
        let server = Server::new(META.to_vec(), status);
        let calls = Arc::new(AtomicUsize::new(0));
        let called = calls.clone();
        let slot = FetchSlot::fetch_mapped(&server.url, limit, move |_| {
            called.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        assert_eq!(server.next(), "/meta.json");
        server.release.send(()).unwrap();
        wait_until(|| slot.is_done());
        assert!(slot.take_result().unwrap().is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn resolver_authorizes_metadata_and_every_texture_without_forwarding_manifest_queries() {
    use crate::assets::{AssetLocation, AssetResolver};
    use crate::streaming::runtime::SogStream;
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = calls.clone();
    let resolver = AssetResolver::new(move |reference| {
        observed.lock().unwrap().push(reference.to_owned());
        let filename = reference
            .strip_prefix("https://logical.test/scene/chunks/")
            .ok_or("unauthorized reference")?;
        let bytes = match filename {
            "meta.json" => {
                include_bytes!("../../../../tests/fixtures/hosted-stream/chunks/meta.json")
                    .as_slice()
            }
            "means_l.webp" => {
                include_bytes!("../../../../tests/fixtures/hosted-stream/chunks/means_l.webp")
                    .as_slice()
            }
            "means_u.webp" => {
                include_bytes!("../../../../tests/fixtures/hosted-stream/chunks/means_u.webp")
                    .as_slice()
            }
            "quats.webp" => {
                include_bytes!("../../../../tests/fixtures/hosted-stream/chunks/quats.webp")
                    .as_slice()
            }
            "scales.webp" => {
                include_bytes!("../../../../tests/fixtures/hosted-stream/chunks/scales.webp")
                    .as_slice()
            }
            "sh0.webp" => {
                include_bytes!("../../../../tests/fixtures/hosted-stream/chunks/sh0.webp")
                    .as_slice()
            }
            _ => return Err("denied".into()),
        };
        Ok(AssetLocation::Bytes(bytes.into()))
    });
    let stream = SogStream::parse(
        include_bytes!("../../../../tests/fixtures/hosted-stream/lod-meta.json"),
        "https://logical.test/scene/lod-meta.json?token=manifest-only",
        "authorized".into(),
        "authorized".into(),
    )
    .unwrap()
    .with_resolver(resolver);
    let mut load = ChunkLoad::new_resolved(
        format!("{}chunks/meta.json", stream.base_url),
        Some(8),
        8,
        0.,
        SourceStorage::Expanded,
        stream.resolver.clone(),
    );
    let mut decoded = None;
    for _ in 0..20 {
        if let Some(result) = load.poll_measured(0., &Default::default()) {
            decoded = Some(result.unwrap());
            break;
        }
    }
    assert_eq!(decoded.expect("authorized source decoded").count, 8);
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls.len(),
        6,
        "one metadata request and five texture requests"
    );
    assert!(calls.iter().all(|reference| !reference.contains("token=")));
    assert!(
        calls
            .iter()
            .any(|reference| reference.ends_with("means_l.webp"))
    );
}
