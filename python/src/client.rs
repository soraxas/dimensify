use pyo3::{exceptions::PyValueError, prelude::*};
use serde::Serialize;
#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
use std::collections::HashSet;
use std::{
    fs::File,
    io::{BufWriter, Write},
    time::Duration,
};

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
use pyo3::types::PyTuple;

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
use dimensify_protocol::{SceneRequest, ViewerEntityInfo, ViewerEntityKind, ViewerResponse};
use dimensify_transport::{
    TransportConfig, TransportConnection, TransportController, TransportEndpoint, TransportMode,
};

#[derive(Clone, Debug)]
enum DataSourceKind {
    Local,
    File { path: String },
    Db { addr: String },
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct DataSource {
    inner: DataSourceKind,
}

#[pymethods]
impl DataSource {
    #[staticmethod]
    pub fn local() -> Self {
        Self {
            inner: DataSourceKind::Local,
        }
    }

    #[staticmethod]
    pub fn file(path: String) -> Self {
        Self {
            inner: DataSourceKind::File { path },
        }
    }

    #[staticmethod]
    pub fn db(addr: String) -> Self {
        Self {
            inner: DataSourceKind::Db { addr },
        }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct EntityInfo {
    #[pyo3(get)]
    pub id: u64,
    #[pyo3(get)]
    pub name: Option<String>,
    #[pyo3(get)]
    pub components: Vec<String>,
}

#[pymethods]
impl EntityInfo {
    pub fn to_dict<'a>(&self, py: Python<'a>) -> PyResult<pyo3::Bound<'a, pyo3::types::PyDict>> {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("id", self.id)?;
        dict.set_item("name", self.name.clone())?;
        dict.set_item("components", self.components.clone())?;
        Ok(dict)
    }

    fn __repr__(&self) -> PyResult<String> {
        Ok(format!(
            "EntityInfo(id={}, name={}, components={})",
            self.id,
            self.name
                .as_ref()
                .map(|value| format!("{:?}", value))
                .unwrap_or_else(|| "None".to_string()),
            self.components.join(",")
        ))
    }

    fn __str__(&self) -> PyResult<String> {
        self.__repr__()
    }
}

impl EntityInfo {
    fn from_viewer(info: ViewerEntityInfo) -> Self {
        Self {
            id: info.id,
            name: info.name,
            components: info.components,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum Component {
    Name {
        value: String,
    },
    Line3d {
        name: Option<String>,
        points: Vec<[f32; 3]>,
        color: [f32; 4],
        width: f32,
    },
    Line2d {
        name: Option<String>,
        points: Vec<[f32; 2]>,
        color: [f32; 4],
        width: f32,
    },
    Text3d {
        name: Option<String>,
        text: String,
        position: [f32; 3],
        color: [f32; 4],
    },
    Text2d {
        name: Option<String>,
        text: String,
        position: [f32; 2],
        color: [f32; 4],
    },
    Mesh3d {
        name: Option<String>,
        position: [f32; 3],
        scale: [f32; 3],
    },
    Rect2d {
        name: Option<String>,
        position: [f32; 2],
        size: [f32; 2],
        rotation: f32,
        color: [f32; 4],
    },
    Transform3d {
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
}

#[derive(Serialize)]
enum SceneCommand {
    Spawn {
        components: Vec<Component>,
    },
    Insert {
        entity: String,
        components: Vec<Component>,
    },
    Update {
        entity: String,
        component: Component,
    },
    Remove {
        entity: String,
        component: ComponentKind,
    },
    Despawn {
        entity: String,
    },
    Clear,
}

#[derive(Serialize)]
#[allow(dead_code)]
enum ComponentKind {
    Name,
    Line3d,
    Line2d,
    Text3d,
    Text2d,
    Mesh3d,
    Rect2d,
    Transform3d,
    Binary,
}

#[pyclass]
pub struct ViewerClient {
    source: DataSourceKind,
    commands: Vec<SceneCommand>,
}

#[pymethods]
impl ViewerClient {
    #[new]
    pub fn new(source: DataSource) -> PyResult<Self> {
        match &source.inner {
            DataSourceKind::Local | DataSourceKind::File { .. } => Ok(Self {
                source: source.inner,
                commands: Vec::new(),
            }),
            DataSourceKind::Db { .. } => {
                Err(PyValueError::new_err("db source is not implemented yet"))
            }
        }
    }

    pub fn log_line_3d(
        &mut self,
        points: Vec<(f32, f32, f32)>,
        color: Option<(f32, f32, f32, f32)>,
        width: Option<f32>,
    ) {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        let width = width.unwrap_or(1.0);
        self.commands.push(SceneCommand::Spawn {
            components: vec![Component::Line3d {
                name: None,
                points: points.into_iter().map(|p| [p.0, p.1, p.2]).collect(),
                color: [color.0, color.1, color.2, color.3],
                width,
            }],
        });
    }

    pub fn log_line_2d(
        &mut self,
        points: Vec<(f32, f32)>,
        color: Option<(f32, f32, f32, f32)>,
        width: Option<f32>,
    ) {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        let width = width.unwrap_or(1.0);
        self.commands.push(SceneCommand::Spawn {
            components: vec![Component::Line2d {
                name: None,
                points: points.into_iter().map(|p| [p.0, p.1]).collect(),
                color: [color.0, color.1, color.2, color.3],
                width,
            }],
        });
    }

    pub fn log_text_3d(
        &mut self,
        text: String,
        position: (f32, f32, f32),
        color: Option<(f32, f32, f32, f32)>,
    ) {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        self.commands.push(SceneCommand::Spawn {
            components: vec![Component::Text3d {
                name: None,
                text,
                position: [position.0, position.1, position.2],
                color: [color.0, color.1, color.2, color.3],
            }],
        });
    }

    pub fn log_text_2d(
        &mut self,
        text: String,
        position: (f32, f32),
        color: Option<(f32, f32, f32, f32)>,
    ) {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        self.commands.push(SceneCommand::Spawn {
            components: vec![Component::Text2d {
                name: None,
                text,
                position: [position.0, position.1],
                color: [color.0, color.1, color.2, color.3],
            }],
        });
    }

    pub fn log_mesh_3d(
        &mut self,
        name: String,
        position: (f32, f32, f32),
        scale: Option<(f32, f32, f32)>,
    ) {
        let scale = scale.unwrap_or((1.0, 1.0, 1.0));
        self.commands.push(SceneCommand::Spawn {
            components: vec![Component::Mesh3d {
                name: Some(name),
                position: [position.0, position.1, position.2],
                scale: [scale.0, scale.1, scale.2],
            }],
        });
    }

    pub fn log_rect_2d(
        &mut self,
        position: (f32, f32),
        size: (f32, f32),
        rotation: Option<f32>,
        color: Option<(f32, f32, f32, f32)>,
    ) {
        let rotation = rotation.unwrap_or(0.0);
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        self.commands.push(SceneCommand::Spawn {
            components: vec![Component::Rect2d {
                name: None,
                position: [position.0, position.1],
                size: [size.0, size.1],
                rotation,
                color: [color.0, color.1, color.2, color.3],
            }],
        });
    }

    pub fn set_transform(
        &mut self,
        entity: String,
        position: (f32, f32, f32),
        rotation: (f32, f32, f32, f32),
        scale: (f32, f32, f32),
    ) {
        self.commands.push(SceneCommand::Update {
            entity,
            component: Component::Transform3d {
                position: [position.0, position.1, position.2],
                rotation: [rotation.0, rotation.1, rotation.2, rotation.3],
                scale: [scale.0, scale.1, scale.2],
            },
        });
    }

    pub fn save(&self, path: Option<String>) -> PyResult<()> {
        let path = match (&self.source, path) {
            (DataSourceKind::File { path }, None) => path.clone(),
            (_, Some(path)) => path,
            _ => return Err(PyValueError::new_err("no output path provided for save()")),
        };

        let file = File::create(&path).map_err(|err| PyValueError::new_err(err.to_string()))?;
        let mut writer = BufWriter::new(file);
        for command in &self.commands {
            let line = serde_json::to_string(command)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            writer
                .write_all(line.as_bytes())
                .and_then(|_| writer.write_all(b"\n"))
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Name {
    value: String,
}

#[pymethods]
impl Name {
    #[new]
    pub fn new(value: String) -> Self {
        Self { value }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Transform3d {
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

#[pymethods]
impl Transform3d {
    #[new]
    #[pyo3(signature = (position=(0.0, 0.0, 0.0), rotation=(0.0, 0.0, 0.0, 1.0), scale=(1.0, 1.0, 1.0)))]
    pub fn new(
        position: (f32, f32, f32),
        rotation: (f32, f32, f32, f32),
        scale: (f32, f32, f32),
    ) -> Self {
        Self {
            position: [position.0, position.1, position.2],
            rotation: [rotation.0, rotation.1, rotation.2, rotation.3],
            scale: [scale.0, scale.1, scale.2],
        }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Mesh3d {
    name: Option<String>,
    position: Option<[f32; 3]>,
    scale: Option<[f32; 3]>,
}

#[pymethods]
impl Mesh3d {
    #[new]
    #[pyo3(signature = (name=None, position=None, scale=None))]
    pub fn new(
        name: Option<String>,
        position: Option<(f32, f32, f32)>,
        scale: Option<(f32, f32, f32)>,
    ) -> Self {
        Self {
            name,
            position: position.map(|p| [p.0, p.1, p.2]),
            scale: scale.map(|s| [s.0, s.1, s.2]),
        }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Line3d {
    name: Option<String>,
    points: Vec<[f32; 3]>,
    color: [f32; 4],
    width: f32,
}

#[pymethods]
impl Line3d {
    #[new]
    #[pyo3(signature = (points, color=None, width=None, name=None))]
    pub fn new(
        points: Vec<(f32, f32, f32)>,
        color: Option<(f32, f32, f32, f32)>,
        width: Option<f32>,
        name: Option<String>,
    ) -> Self {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        Self {
            name,
            points: points.into_iter().map(|p| [p.0, p.1, p.2]).collect(),
            color: [color.0, color.1, color.2, color.3],
            width: width.unwrap_or(1.0),
        }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Line2d {
    name: Option<String>,
    points: Vec<[f32; 2]>,
    color: [f32; 4],
    width: f32,
}

#[pymethods]
impl Line2d {
    #[new]
    #[pyo3(signature = (points, color=None, width=None, name=None))]
    pub fn new(
        points: Vec<(f32, f32)>,
        color: Option<(f32, f32, f32, f32)>,
        width: Option<f32>,
        name: Option<String>,
    ) -> Self {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        Self {
            name,
            points: points.into_iter().map(|p| [p.0, p.1]).collect(),
            color: [color.0, color.1, color.2, color.3],
            width: width.unwrap_or(1.0),
        }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Text3d {
    name: Option<String>,
    text: String,
    position: [f32; 3],
    color: [f32; 4],
}

#[pymethods]
impl Text3d {
    #[new]
    #[pyo3(signature = (text, position, color=None, name=None))]
    pub fn new(
        text: String,
        position: (f32, f32, f32),
        color: Option<(f32, f32, f32, f32)>,
        name: Option<String>,
    ) -> Self {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        Self {
            name,
            text,
            position: [position.0, position.1, position.2],
            color: [color.0, color.1, color.2, color.3],
        }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Text2d {
    name: Option<String>,
    text: String,
    position: [f32; 2],
    color: [f32; 4],
}

#[pymethods]
impl Text2d {
    #[new]
    #[pyo3(signature = (text, position, color=None, name=None))]
    pub fn new(
        text: String,
        position: (f32, f32),
        color: Option<(f32, f32, f32, f32)>,
        name: Option<String>,
    ) -> Self {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        Self {
            name,
            text,
            position: [position.0, position.1],
            color: [color.0, color.1, color.2, color.3],
        }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct Rect2d {
    name: Option<String>,
    position: [f32; 2],
    size: [f32; 2],
    rotation: f32,
    color: [f32; 4],
}

#[pymethods]
impl Rect2d {
    #[new]
    #[pyo3(signature = (position, size, rotation=None, color=None, name=None))]
    pub fn new(
        position: (f32, f32),
        size: (f32, f32),
        rotation: Option<f32>,
        color: Option<(f32, f32, f32, f32)>,
        name: Option<String>,
    ) -> Self {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        Self {
            name,
            position: [position.0, position.1],
            size: [size.0, size.1],
            rotation: rotation.unwrap_or(0.0),
            color: [color.0, color.1, color.2, color.3],
        }
    }
}

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
#[pyclass(unsendable)]
pub struct TransportClient {
    controller: TransportController,
}

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
#[pymethods]
impl TransportClient {
    #[pyo3(signature = (server_addr=None, mode=None, client_addr=None, cert_digest=None, tick_hz=None, connection=None, endpoint=None))]
    #[new]
    pub fn new(
        server_addr: Option<String>,
        mode: Option<String>,
        client_addr: Option<String>,
        cert_digest: Option<String>,
        tick_hz: Option<f32>,
        connection: Option<String>,
        endpoint: Option<String>,
    ) -> PyResult<Self> {
        let mut config = TransportConfig::default();
        config.connection =
            parse_connection(connection.as_deref()).unwrap_or(TransportConnection::Client);
        config.endpoint =
            parse_endpoint(endpoint.as_deref()).unwrap_or(TransportEndpoint::Controller);

        if let Some(addr) = server_addr {
            config.server_addr = addr
                .parse()
                .map_err(|err: std::net::AddrParseError| PyValueError::new_err(err.to_string()))?;
        }

        if let Some(mode) = mode {
            if let Some(parsed_mode) = parse_mode(Some(mode.as_str())) {
                config.mode = parsed_mode;
            }
        }

        if let Some(addr) = client_addr {
            config.client_addr =
                Some(addr.parse().map_err(|err: std::net::AddrParseError| {
                    PyValueError::new_err(err.to_string())
                })?);
        }

        if let Some(digest) = cert_digest {
            config.certificate_digest = digest;
        }

        if let Some(hz) = tick_hz {
            config.tick_hz = hz;
        }

        Ok(Self {
            controller: TransportController::start(config),
        })
    }

    pub fn apply(&self, payload: String, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(SceneRequest::Apply { payload }, timeout_ms)
    }

    pub fn remove(&self, name: String, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(SceneRequest::Remove { name }, timeout_ms)
    }

    pub fn clear(&self, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(SceneRequest::Clear, timeout_ms)
    }

    pub fn list(&self, timeout_ms: Option<u64>) -> PyResult<Vec<EntityInfo>> {
        let response = self.send_and_wait(SceneRequest::List, timeout_ms)?;
        match response {
            ViewerResponse::Entities { entities } => {
                Ok(entities.into_iter().map(EntityInfo::from_viewer).collect())
            }
            ViewerResponse::Error { message } => Err(PyValueError::new_err(message)),
            other => Err(PyValueError::new_err(format!(
                "unexpected response: {:?}",
                other
            ))),
        }
    }
}

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
impl TransportClient {
    fn expect_ack(&self, request: SceneRequest, timeout_ms: Option<u64>) -> PyResult<()> {
        let response = self.send_and_wait(request, timeout_ms)?;
        match response {
            ViewerResponse::Ack => Ok(()),
            ViewerResponse::Error { message } => Err(PyValueError::new_err(message)),
            other => Err(PyValueError::new_err(format!(
                "unexpected response: {:?}",
                other
            ))),
        }
    }

    fn send_and_wait(
        &self,
        request: SceneRequest,
        timeout_ms: Option<u64>,
    ) -> PyResult<ViewerResponse> {
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(1_000));
        self.controller
            .send_and_wait(request, timeout)
            .ok_or_else(|| PyValueError::new_err("transport response timed out"))
    }
}

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
#[pyclass(unsendable)]
pub struct World {
    controller: TransportController,
    used_names: HashSet<String>,
    next_id: u64,
}

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
#[pymethods]
impl World {
    #[pyo3(signature = (server_addr=None, mode=None, client_addr=None, cert_digest=None, tick_hz=None))]
    #[new]
    pub fn new(
        server_addr: Option<String>,
        mode: Option<String>,
        client_addr: Option<String>,
        cert_digest: Option<String>,
        tick_hz: Option<f32>,
    ) -> PyResult<Self> {
        let mut config = TransportConfig::default();
        config.connection = TransportConnection::Client;
        config.endpoint = TransportEndpoint::Controller;

        if let Some(addr) = server_addr {
            config.server_addr = addr
                .parse()
                .map_err(|err: std::net::AddrParseError| PyValueError::new_err(err.to_string()))?;
        }

        if let Some(mode) = mode {
            if let Some(parsed_mode) = parse_mode(Some(mode.as_str())) {
                config.mode = parsed_mode;
            }
        }

        if let Some(addr) = client_addr {
            config.client_addr =
                Some(addr.parse().map_err(|err: std::net::AddrParseError| {
                    PyValueError::new_err(err.to_string())
                })?);
        }

        if let Some(digest) = cert_digest {
            config.certificate_digest = digest;
        }

        if let Some(hz) = tick_hz {
            config.tick_hz = hz;
        }

        Ok(Self {
            controller: TransportController::start(config),
            used_names: HashSet::new(),
            next_id: 0,
        })
    }

    #[pyo3(signature = (*components, timeout_ms=None))]
    pub fn spawn(
        &mut self,
        components: &Bound<'_, PyTuple>,
        timeout_ms: Option<u64>,
    ) -> PyResult<String> {
        let mut name_override: Option<String> = None;
        let mut transform: Option<Transform3d> = None;
        let mut mesh: Option<Mesh3d> = None;
        let mut line3d: Option<Line3d> = None;
        let mut line2d: Option<Line2d> = None;
        let mut text3d: Option<Text3d> = None;
        let mut text2d: Option<Text2d> = None;
        let mut rect2d: Option<Rect2d> = None;

        for component in components.iter() {
            if let Ok(value) = component.extract::<PyRef<Name>>() {
                if name_override.replace(value.value.clone()).is_some() {
                    return Err(PyValueError::new_err(
                        "Name component provided more than once",
                    ));
                }
                continue;
            }
            if let Ok(value) = component.extract::<PyRef<Transform3d>>() {
                if transform.replace(value.clone()).is_some() {
                    return Err(PyValueError::new_err(
                        "Transform3d component provided more than once",
                    ));
                }
                continue;
            }
            if let Ok(value) = component.extract::<PyRef<Mesh3d>>() {
                if mesh.replace(value.clone()).is_some() {
                    return Err(PyValueError::new_err(
                        "Mesh3d component provided more than once",
                    ));
                }
                continue;
            }
            if let Ok(value) = component.extract::<PyRef<Line3d>>() {
                if line3d.replace(value.clone()).is_some() {
                    return Err(PyValueError::new_err(
                        "Line3d component provided more than once",
                    ));
                }
                continue;
            }
            if let Ok(value) = component.extract::<PyRef<Line2d>>() {
                if line2d.replace(value.clone()).is_some() {
                    return Err(PyValueError::new_err(
                        "Line2d component provided more than once",
                    ));
                }
                continue;
            }
            if let Ok(value) = component.extract::<PyRef<Text3d>>() {
                if text3d.replace(value.clone()).is_some() {
                    return Err(PyValueError::new_err(
                        "Text3d component provided more than once",
                    ));
                }
                continue;
            }
            if let Ok(value) = component.extract::<PyRef<Text2d>>() {
                if text2d.replace(value.clone()).is_some() {
                    return Err(PyValueError::new_err(
                        "Text2d component provided more than once",
                    ));
                }
                continue;
            }
            if let Ok(value) = component.extract::<PyRef<Rect2d>>() {
                if rect2d.replace(value.clone()).is_some() {
                    return Err(PyValueError::new_err(
                        "Rect2d component provided more than once",
                    ));
                }
                continue;
            }
            let type_name = component
                .get_type()
                .name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(PyValueError::new_err(format!(
                "unknown component type: {}",
                type_name
            )));
        }

        let name = self.allocate_name(
            name_override
                .clone()
                .or_else(|| mesh.as_ref().and_then(|m| m.name.clone()))
                .or_else(|| line3d.as_ref().and_then(|l| l.name.clone()))
                .or_else(|| line2d.as_ref().and_then(|l| l.name.clone()))
                .or_else(|| text3d.as_ref().and_then(|t| t.name.clone()))
                .or_else(|| text2d.as_ref().and_then(|t| t.name.clone()))
                .or_else(|| rect2d.as_ref().and_then(|r| r.name.clone())),
        );

        let mut components = Vec::new();
        components.push(Component::Name {
            value: name.clone(),
        });

        if let Some(mesh) = mesh {
            let position = transform
                .as_ref()
                .map(|t| t.position)
                .or(mesh.position)
                .unwrap_or([0.0, 0.0, 0.0]);
            let scale = transform
                .as_ref()
                .map(|t| t.scale)
                .or(mesh.scale)
                .unwrap_or([1.0, 1.0, 1.0]);
            components.push(Component::Mesh3d {
                name: mesh.name.or_else(|| Some(name.clone())),
                position,
                scale,
            });
        }
        if let Some(line) = line3d {
            components.push(Component::Line3d {
                name: line.name.or_else(|| Some(name.clone())),
                points: line.points,
                color: line.color,
                width: line.width,
            });
        }
        if let Some(line) = line2d {
            components.push(Component::Line2d {
                name: line.name.or_else(|| Some(name.clone())),
                points: line.points,
                color: line.color,
                width: line.width,
            });
        }
        if let Some(text) = text3d {
            components.push(Component::Text3d {
                name: text.name.or_else(|| Some(name.clone())),
                text: text.text,
                position: text.position,
                color: text.color,
            });
        }
        if let Some(text) = text2d {
            components.push(Component::Text2d {
                name: text.name.or_else(|| Some(name.clone())),
                text: text.text,
                position: text.position,
                color: text.color,
            });
        }
        if let Some(rect) = rect2d {
            components.push(Component::Rect2d {
                name: rect.name.or_else(|| Some(name.clone())),
                position: rect.position,
                size: rect.size,
                rotation: rect.rotation,
                color: rect.color,
            });
        }
        if let Some(transform) = transform {
            components.push(Component::Transform3d {
                position: transform.position,
                rotation: transform.rotation,
                scale: transform.scale,
            });
        }

        let has_scene_components = components
            .iter()
            .any(|component| !matches!(component, Component::Name { .. }));
        if !has_scene_components {
            return Err(PyValueError::new_err(
                "spawn() requires at least one non-Name component",
            ));
        }

        let payload = serde_json::to_string(&SceneCommand::Spawn { components })
            .map_err(|err| PyValueError::new_err(err.to_string()))?;
        self.send_and_wait(SceneRequest::Apply { payload }, timeout_ms)?;
        Ok(name)
    }

    pub fn list(&self, timeout_ms: Option<u64>) -> PyResult<Vec<EntityInfo>> {
        let response = self.send_and_wait(SceneRequest::List, timeout_ms)?;
        match response {
            ViewerResponse::Entities { entities } => {
                Ok(entities.into_iter().map(EntityInfo::from_viewer).collect())
            }
            ViewerResponse::Error { message } => Err(PyValueError::new_err(message)),
            other => Err(PyValueError::new_err(format!(
                "unexpected response: {:?}",
                other
            ))),
        }
    }

    pub fn remove(&self, name: String, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(SceneRequest::Remove { name }, timeout_ms)
    }

    pub fn clear(&self, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(SceneRequest::Clear, timeout_ms)
    }
}

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
impl World {
    fn allocate_name(&mut self, preferred: Option<String>) -> String {
        if let Some(name) = preferred {
            if !self.used_names.contains(&name) {
                self.used_names.insert(name.clone());
                return name;
            }
            let mut suffix = 1;
            loop {
                let candidate = format!("{}_{}", name, suffix);
                if !self.used_names.contains(&candidate) {
                    self.used_names.insert(candidate.clone());
                    return candidate;
                }
                suffix += 1;
            }
        }
        loop {
            self.next_id += 1;
            let candidate = format!("entity_{}", self.next_id);
            if !self.used_names.contains(&candidate) {
                self.used_names.insert(candidate.clone());
                return candidate;
            }
        }
    }

    fn expect_ack(&self, request: SceneRequest, timeout_ms: Option<u64>) -> PyResult<()> {
        let response = self.send_and_wait(request, timeout_ms)?;
        match response {
            ViewerResponse::Ack => Ok(()),
            ViewerResponse::Error { message } => Err(PyValueError::new_err(message)),
            other => Err(PyValueError::new_err(format!(
                "unexpected response: {:?}",
                other
            ))),
        }
    }

    fn send_and_wait(
        &self,
        request: SceneRequest,
        timeout_ms: Option<u64>,
    ) -> PyResult<ViewerResponse> {
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(1_000));
        self.controller
            .send_and_wait(request, timeout)
            .ok_or_else(|| PyValueError::new_err("transport response timed out"))
    }
}

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
fn parse_connection(value: Option<&str>) -> Option<TransportConnection> {
    match value?.to_ascii_lowercase().as_str() {
        "server" => Some(TransportConnection::Server),
        "client" => Some(TransportConnection::Client),
        _ => None,
    }
}

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
fn parse_endpoint(value: Option<&str>) -> Option<TransportEndpoint> {
    match value?.to_ascii_lowercase().as_str() {
        "viewer" => Some(TransportEndpoint::Viewer),
        "controller" => Some(TransportEndpoint::Controller),
        _ => None,
    }
}

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
fn parse_mode(value: Option<&str>) -> Option<TransportMode> {
    match value?.to_ascii_lowercase().as_str() {
        "webtransport" => Some(TransportMode::WebTransport),
        "websocket" => Some(TransportMode::WebSocket),
        "udp" => Some(TransportMode::Udp),
        _ => None,
    }
}

#[cfg(not(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
)))]
#[pyclass]
pub struct TransportClient;

#[cfg(not(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
)))]
#[pymethods]
impl TransportClient {
    #[pyo3(signature = (server_addr=None, mode=None, client_addr=None, cert_digest=None, tick_hz=None, connection=None, endpoint=None))]
    #[new]
    pub fn new(
        server_addr: Option<String>,
        mode: Option<String>,
        client_addr: Option<String>,
        cert_digest: Option<String>,
        tick_hz: Option<f32>,
        connection: Option<String>,
        endpoint: Option<String>,
    ) -> PyResult<Self> {
        Err(PyValueError::new_err(
            "transport support is disabled; enable transport_webtransport/transport_websocket/transport_udp in maturin/uv",
        ))
    }
}

#[cfg(not(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
)))]
#[pyclass]
pub struct World;

#[cfg(not(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
)))]
#[pymethods]
impl World {
    #[new]
    pub fn new(
        _server_addr: Option<String>,
        _mode: Option<String>,
        _client_addr: Option<String>,
        _cert_digest: Option<String>,
        _tick_hz: Option<f32>,
    ) -> PyResult<Self> {
        Err(PyValueError::new_err(
            "transport support is disabled; enable transport_webtransport/transport_websocket/transport_udp in maturin/uv",
        ))
    }
}
