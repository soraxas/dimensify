use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde::Serialize;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Duration;

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
use dimensify_transport::{
    TransportConfig, TransportConnection, TransportController, TransportEndpoint, ViewerRequest,
    ViewerResponse,
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

#[derive(Serialize)]
#[serde(tag = "type")]
enum Command {
    Line3d {
        points: Vec<[f32; 3]>,
        color: [f32; 4],
        width: f32,
    },
    Line2d {
        points: Vec<[f32; 2]>,
        color: [f32; 4],
        width: f32,
    },
    Text3d {
        text: String,
        position: [f32; 3],
        color: [f32; 4],
    },
    Text2d {
        text: String,
        position: [f32; 2],
        color: [f32; 4],
    },
    Mesh3d {
        name: String,
        position: [f32; 3],
        scale: [f32; 3],
    },
    Rect2d {
        position: [f32; 2],
        size: [f32; 2],
        rotation: f32,
        color: [f32; 4],
    },
    Transform {
        entity: String,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
}

#[pyclass]
pub struct ViewerClient {
    source: DataSourceKind,
    commands: Vec<Command>,
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
        self.commands.push(Command::Line3d {
            points: points.into_iter().map(|p| [p.0, p.1, p.2]).collect(),
            color: [color.0, color.1, color.2, color.3],
            width,
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
        self.commands.push(Command::Line2d {
            points: points.into_iter().map(|p| [p.0, p.1]).collect(),
            color: [color.0, color.1, color.2, color.3],
            width,
        });
    }

    pub fn log_text_3d(
        &mut self,
        text: String,
        position: (f32, f32, f32),
        color: Option<(f32, f32, f32, f32)>,
    ) {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        self.commands.push(Command::Text3d {
            text,
            position: [position.0, position.1, position.2],
            color: [color.0, color.1, color.2, color.3],
        });
    }

    pub fn log_text_2d(
        &mut self,
        text: String,
        position: (f32, f32),
        color: Option<(f32, f32, f32, f32)>,
    ) {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        self.commands.push(Command::Text2d {
            text,
            position: [position.0, position.1],
            color: [color.0, color.1, color.2, color.3],
        });
    }

    pub fn log_mesh_3d(
        &mut self,
        name: String,
        position: (f32, f32, f32),
        scale: Option<(f32, f32, f32)>,
    ) {
        let scale = scale.unwrap_or((1.0, 1.0, 1.0));
        self.commands.push(Command::Mesh3d {
            name,
            position: [position.0, position.1, position.2],
            scale: [scale.0, scale.1, scale.2],
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
        self.commands.push(Command::Rect2d {
            position: [position.0, position.1],
            size: [size.0, size.1],
            rotation,
            color: [color.0, color.1, color.2, color.3],
        });
    }

    pub fn set_transform(
        &mut self,
        entity: String,
        position: (f32, f32, f32),
        rotation: (f32, f32, f32, f32),
        scale: (f32, f32, f32),
    ) {
        self.commands.push(Command::Transform {
            entity,
            position: [position.0, position.1, position.2],
            rotation: [rotation.0, rotation.1, rotation.2, rotation.3],
            scale: [scale.0, scale.1, scale.2],
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

#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
#[pyclass]
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
    #[pyo3(signature = (server_addr=None, cert_digest=None, tick_hz=None, connection=None, endpoint=None))]
    #[new]
    pub fn new(
        server_addr: Option<String>,
        cert_digest: Option<String>,
        tick_hz: Option<f32>,
        connection: Option<String>,
        endpoint: Option<String>,
    ) -> PyResult<Self> {
        let mut config = TransportConfig::from_env();
        config.connection =
            parse_connection(connection.as_deref()).unwrap_or(TransportConnection::Client);
        config.endpoint =
            parse_endpoint(endpoint.as_deref()).unwrap_or(TransportEndpoint::Controller);

        if let Some(addr) = server_addr {
            config.server_addr = addr
                .parse()
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
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

    pub fn apply_json(&self, payload: String, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(ViewerRequest::ApplyJson { payload }, timeout_ms)
    }

    pub fn remove(&self, name: String, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(ViewerRequest::Remove { name }, timeout_ms)
    }

    pub fn clear(&self, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(ViewerRequest::Clear, timeout_ms)
    }

    pub fn list(&self, timeout_ms: Option<u64>) -> PyResult<Vec<String>> {
        let response = self.send_and_wait(ViewerRequest::List, timeout_ms)?;
        match response {
            ViewerResponse::Entities { names } => Ok(names),
            ViewerResponse::Error { message } => Err(PyValueError::new_err(message)),
            other => Err(PyValueError::new_err(format!(
                "unexpected response: {:?}",
                other
            ))),
        }
    }

    fn expect_ack(&self, request: ViewerRequest, timeout_ms: Option<u64>) -> PyResult<()> {
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
        request: ViewerRequest,
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
    #[new]
    pub fn new(
        _server_addr: Option<String>,
        _cert_digest: Option<String>,
        _tick_hz: Option<f32>,
        _connection: Option<String>,
        _endpoint: Option<String>,
    ) -> PyResult<Self> {
        Err(PyValueError::new_err(
            "transport support is disabled; enable transport_webtransport in maturin/uv",
        ))
    }
}
