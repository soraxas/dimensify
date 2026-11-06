//! Bounded downloads of a URDF and its meshes for URL sources.
use super::{
    RobotModel,
    loading::{MeshFile, MeshFiles, mesh_references},
    source::{mesh_candidates, url_file_name},
};
use eyre::{Result, bail, ensure};
use std::{
    collections::{HashMap, VecDeque},
    ops::ControlFlow,
    sync::{Arc, Mutex},
};
use url::Url;

const URDF_LIMIT: usize = 8 << 20;
const MESH_LIMIT: usize = 64 << 20;
const TOTAL_LIMIT: usize = 256 << 20;
const MESH_COUNT_LIMIT: usize = 256;
const IN_FLIGHT: usize = 6;

pub(super) type Reply = Box<dyn FnOnce(Result<Vec<u8>, String>) + Send>;

/// Byte transport; tests substitute an in-memory implementation.
pub(super) trait Transport: Send + Sync {
    /// Fetch at most `limit` bytes and call `reply` exactly once.
    fn get(&self, url: &Url, limit: usize, reply: Reply);
}

/// Streaming HTTP(S) with the platform's TLS and the browser's CORS policy.
struct Http;

impl Transport for Http {
    fn get(&self, url: &Url, limit: usize, reply: Reply) {
        // The streaming callback is shared, so the body and reply live behind a lock.
        let state = Mutex::new((Some(reply), Vec::new()));
        ehttp::streaming::fetch(ehttp::Request::get(url.as_str()), move |part| {
            use ehttp::streaming::Part;
            let mut state = state.lock().unwrap();
            let (reply, body) = &mut *state;
            let result = match part {
                Ok(Part::Response(response)) if (200..300).contains(&response.status) => {
                    return ControlFlow::Continue(());
                }
                Ok(Part::Response(response)) => Err(format!("HTTP {}", response.status)),
                Ok(Part::Chunk(chunk)) if chunk.is_empty() => Ok(std::mem::take(body)),
                Ok(Part::Chunk(chunk)) if chunk.len() <= limit.saturating_sub(body.len()) => {
                    body.extend_from_slice(&chunk);
                    return ControlFlow::Continue(());
                }
                Ok(Part::Chunk(_)) => Err(format!("exceeds the {} MiB limit", limit >> 20)),
                Err(error) => Err(error),
            };
            if let Some(reply) = reply.take() {
                reply(result);
            }
            ControlFlow::Break(())
        });
    }
}

enum Key {
    Urdf,
    Mesh(String),
}

type Inbox = Arc<Mutex<Vec<(Key, Result<Vec<u8>, String>)>>>;

struct Pending {
    candidates: Vec<Url>,
    attempt: usize,
}

/// Download state for one URL source. Poll it each frame until it finishes.
pub struct RobotLoad {
    transport: Arc<dyn Transport>,
    url: Url,
    packages: Vec<(String, Url)>,
    inbox: Inbox,
    in_flight: usize,
    robot: Option<urdf_rs::Robot>,
    references: usize,
    queue: VecDeque<String>,
    pending: HashMap<String, Pending>,
    /// Package directory proven by an earlier download.
    found: HashMap<String, Url>,
    files: MeshFiles,
    bytes: usize,
    done: bool,
}

impl RobotLoad {
    /// Start downloading a URDF; `packages` maps package names to base URLs.
    pub fn start(url: Url, packages: Vec<(String, Url)>) -> Self {
        Self::with_transport(Arc::new(Http), url, packages)
    }

    pub(super) fn with_transport(
        transport: Arc<dyn Transport>,
        url: Url,
        packages: Vec<(String, Url)>,
    ) -> Self {
        let mut load = Self {
            transport,
            url,
            packages,
            inbox: Arc::default(),
            in_flight: 0,
            robot: None,
            references: 0,
            queue: VecDeque::new(),
            pending: HashMap::new(),
            found: HashMap::new(),
            files: MeshFiles::new(),
            bytes: 0,
            done: false,
        };
        let url = load.url.clone();
        load.request(Key::Urdf, &url, URDF_LIMIT);
        load
    }

    /// The URDF location being loaded.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Loaded and referenced unique meshes, once the URDF itself has arrived.
    pub fn progress(&self) -> Option<(usize, usize)> {
        self.robot
            .as_ref()
            .map(|_| (self.files.len(), self.references))
    }

    /// Return the finished model or the first error, exactly once.
    pub fn poll(&mut self) -> Option<Result<RobotModel>> {
        if self.done {
            return None;
        }
        let result = self.step().transpose();
        self.done = result.is_some();
        result
    }

    fn request(&mut self, key: Key, url: &Url, limit: usize) {
        self.in_flight += 1;
        let inbox = Arc::downgrade(&self.inbox);
        self.transport.get(
            url,
            limit,
            Box::new(move |result| {
                // A dropped load cancels delivery of late responses.
                if let Some(inbox) = inbox.upgrade() {
                    inbox.lock().unwrap().push((key, result));
                }
            }),
        );
    }

    fn step(&mut self) -> Result<Option<RobotModel>> {
        let replies = std::mem::take(&mut *self.inbox.lock().unwrap());
        for (key, result) in replies {
            self.in_flight -= 1;
            match key {
                Key::Urdf => {
                    let bytes = result.map_err(|e| eyre::eyre!("Downloading URDF: {e}"))?;
                    let xml = String::from_utf8(bytes)
                        .map_err(|_| eyre::eyre!("The URDF is not UTF-8 text"))?;
                    let robot = RobotModel::parse(&xml)?;
                    let references = mesh_references(&robot);
                    ensure!(
                        references.len() <= MESH_COUNT_LIMIT,
                        "The URDF references more than {MESH_COUNT_LIMIT} mesh files"
                    );
                    self.references = references.len();
                    self.queue = references.into();
                    self.robot = Some(robot);
                }
                Key::Mesh(filename) => self.receive(filename, result)?,
            }
        }
        while self.in_flight < IN_FLIGHT
            && let Some(filename) = self.queue.pop_front()
        {
            self.dispatch(filename)?;
        }
        if self.in_flight > 0 || !self.queue.is_empty() {
            return Ok(None);
        }
        let Some(robot) = self.robot.take() else {
            return Ok(None);
        };
        // URL sources never read OBJ material side files from the local disk.
        let side_file = |_: &std::path::Path| Err(std::io::ErrorKind::NotFound.into());
        RobotModel::build(robot, &self.files, &side_file).map(Some)
    }

    fn dispatch(&mut self, filename: String) -> Result<()> {
        if !self.pending.contains_key(&filename) {
            let mut candidates = mesh_candidates(&self.url, &self.packages, &filename)?;
            if let Some((package, rest)) = package_parts(&filename)
                && let Some(base) = self.found.get(package)
            {
                let proven = base.join(rest)?;
                candidates.retain(|candidate| *candidate != proven);
                candidates.insert(0, proven);
            }
            self.pending.insert(
                filename.clone(),
                Pending {
                    candidates,
                    attempt: 0,
                },
            );
        }
        let pending = &self.pending[&filename];
        let url = pending.candidates[pending.attempt].clone();
        self.request(Key::Mesh(filename), &url, MESH_LIMIT);
        Ok(())
    }

    fn receive(&mut self, filename: String, result: Result<Vec<u8>, String>) -> Result<()> {
        let pending = self
            .pending
            .get_mut(&filename)
            .expect("replies match requests");
        let url = pending.candidates[pending.attempt].clone();
        match result {
            Ok(bytes) => {
                self.bytes += bytes.len();
                ensure!(
                    self.bytes <= TOTAL_LIMIT,
                    "Meshes exceed the {} MiB total download limit",
                    TOTAL_LIMIT >> 20
                );
                if let Some((package, rest)) = package_parts(&filename)
                    && let Some(base) = url.as_str().strip_suffix(rest)
                {
                    self.found.insert(package.to_owned(), Url::parse(base)?);
                }
                self.pending.remove(&filename);
                self.files.insert(
                    filename,
                    MeshFile {
                        bytes,
                        path: url_file_name(&url),
                    },
                );
            }
            Err(_) if pending.attempt + 1 < pending.candidates.len() => {
                pending.attempt += 1;
                self.queue.push_front(filename);
            }
            Err(error) => match package_parts(&filename) {
                Some((package, _)) => bail!(
                    "Cannot download {filename} ({error}). Add package={package}=<base URL> to locate the package."
                ),
                None => bail!("Cannot download {filename} from {url}: {error}"),
            },
        }
        Ok(())
    }
}

fn package_parts(filename: &str) -> Option<(&str, &str)> {
    filename.strip_prefix("package://")?.split_once('/')
}

/// Download a URL source on the calling thread, for command-line checks.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_blocking(url: Url, packages: Vec<(String, Url)>) -> Result<RobotModel> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    let mut load = RobotLoad::start(url, packages);
    loop {
        if let Some(result) = load.poll() {
            return result;
        }
        ensure!(
            std::time::Instant::now() < deadline,
            "Timed out downloading the robot"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
