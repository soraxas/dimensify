use pyo3::{exceptions::PyValueError, prelude::*};
use std::collections::HashSet;

use crate::{
    client::TransportClient,
    // shapes::PySphere3d,
};
use dimensify_transport::{ProtoRequest, ProtoResponse};

use crate::{
    components::PyComponent,
    metadata::{PyEntity, PyEntityInfo},
};
use dimensify_protocol::WorldCommand;

#[pyclass(unsendable)]
pub struct World {
    client: TransportClient,
    used_names: HashSet<String>,
    next_id: u64,
}

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
        Ok(Self {
            client: TransportClient::new(
                server_addr,
                mode,
                client_addr,
                cert_digest,
                tick_hz,
                Some("client".to_string()),
                Some("controller".to_string()),
            )?,
            used_names: HashSet::new(),
            next_id: 0,
        })
    }

    /// Spawn a new entity with the given components.
    #[pyo3(signature = (*components, timeout_ms=None, wait=true))]
    pub fn spawn(
        &mut self,
        py: Python<'_>,
        components: Vec<Py<PyComponent>>,
        timeout_ms: Option<u64>,
        wait: bool,
    ) -> PyResult<Option<PyEntity>> {
        if components.is_empty() {
            return Err(PyValueError::new_err(
                "spawn() requires at least one component",
            ));
        }

        // convert components to ProtoComponent
        let collected = components
            .into_iter()
            .map(|component| component.borrow(py).0.clone())
            .collect();

        let command = WorldCommand::Spawn {
            components: collected,
        };
        if wait {
            let response = self
                .client
                .send_and_wait(ProtoRequest::ApplyCommand(command), timeout_ms)?;

            match response {
                ProtoResponse::CommandResponseEntity(entity) => Ok(Some(entity.into())),
                ProtoResponse::Error { message } => Err(PyValueError::new_err(message)),
                other => Err(PyValueError::new_err(format!(
                    "unexpected response: {:?}",
                    other
                ))),
            }
        } else {
            self.client.send(ProtoRequest::ApplyCommand(command))?;
            Ok(None)
        }
    }

    /// Despawn an entity.
    #[pyo3(signature = (entity, timeout_ms=None))]
    pub fn despawn(&self, entity: PyEntity, timeout_ms: Option<u64>) -> PyResult<()> {
        let command = WorldCommand::Despawn { entity: entity.0 };
        self.client
            .send_and_wait(ProtoRequest::ApplyCommand(command), timeout_ms)?;
        Ok(())
    }

    /// List all entities in the world.
    #[pyo3(signature = (timeout_ms=None))]
    pub fn list(&self, timeout_ms: Option<u64>) -> PyResult<Vec<PyEntityInfo>> {
        let response = self.client.send_and_wait(ProtoRequest::List, timeout_ms)?;
        match response {
            ProtoResponse::Entities { entities } => {
                Ok(entities.into_iter().map(PyEntityInfo::from).collect())
            }
            ProtoResponse::Error { message } => Err(PyValueError::new_err(message)),
            other => Err(PyValueError::new_err(format!(
                "unexpected response: {:?}",
                other
            ))),
        }
    }
}
