use pyo3::{exceptions::PyValueError, prelude::*};
use std::time::Duration;

use crate::components::prelude::*;
use dimensify_transport::{
    SceneRequest, TransportConfig, TransportConnection, TransportController, TransportEndpoint,
    TransportMode, ViewerResponse,
};

#[pyclass(unsendable)]
pub struct TransportClient {
    controller: TransportController,
}

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
            match mode.try_into() {
                Ok(parsed_mode) => config.mode = parsed_mode,
                Err(err) => return Err(PyValueError::new_err(err.to_string())),
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

    /// Send a scene command to the server.
    pub fn apply(&self, payload: String, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(SceneRequest::Apply { payload }, timeout_ms)
    }

    /// Remove an entity from the server.
    pub fn remove(&self, name: String, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(SceneRequest::Remove { name }, timeout_ms)
    }

    /// Clear all entities from the server.
    pub fn clear(&self, timeout_ms: Option<u64>) -> PyResult<()> {
        self.expect_ack(SceneRequest::Clear, timeout_ms)
    }

    /// List all entities on the server.
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

impl TransportClient {
    pub(crate) fn expect_ack(
        &self,
        request: SceneRequest,
        timeout_ms: Option<u64>,
    ) -> PyResult<()> {
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

    pub(crate) fn send_and_wait(
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

fn parse_connection(value: Option<&str>) -> Option<TransportConnection> {
    match value?.to_ascii_lowercase().as_str() {
        "server" => Some(TransportConnection::Server),
        "client" => Some(TransportConnection::Client),
        _ => None,
    }
}

fn parse_endpoint(value: Option<&str>) -> Option<TransportEndpoint> {
    match value?.to_ascii_lowercase().as_str() {
        "viewer" => Some(TransportEndpoint::Viewer),
        "controller" => Some(TransportEndpoint::Controller),
        _ => None,
    }
}
