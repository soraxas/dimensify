use pyo3::{exceptions::PyValueError, prelude::*, types::PyTuple};
use std::{collections::HashSet, time::Duration};

use crate::{client::TransportClient, components::prelude::*};
use dimensify_transport::{
    SceneRequest, TransportConfig, TransportConnection, TransportController, TransportEndpoint,
    TransportMode, ViewerResponse,
};

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

    #[pyo3(signature = (*components, timeout_ms=None))]
    pub fn spawn(
        &mut self,
        components: &Bound<'_, PyTuple>,
        timeout_ms: Option<u64>,
    ) -> PyResult<String> {
        // let mut components = Vec::new();
        // components.push(Component::Name {
        //     value: name.clone(),
        // });

        // if let Some(mesh) = mesh {
        //     let position = transform
        //         .as_ref()
        //         .map(|t| t.position)
        //         .or(mesh.position)
        //         .unwrap_or([0.0, 0.0, 0.0]);
        //     let scale = transform
        //         .as_ref()
        //         .map(|t| t.scale)
        //         .or(mesh.scale)
        //         .unwrap_or([1.0, 1.0, 1.0]);
        //     components.push(Component::Mesh3d {
        //         name: mesh.name.or_else(|| Some(name.clone())),
        //         position,
        //         scale,
        //     });
        // }
        // if let Some(line) = line3d {
        //     components.push(Component::Line3d {
        //         name: line.name.or_else(|| Some(name.clone())),
        //         points: line.points,
        //         color: line.color,
        //         width: line.width,
        //     });
        // }
        // if let Some(line) = line2d {
        //     components.push(Component::Line2d {
        //         name: line.name.or_else(|| Some(name.clone())),
        //         points: line.points,
        //         color: line.color,
        //         width: line.width,
        //     });
        // }
        // if let Some(text) = text3d {
        //     components.push(Component::Text3d {
        //         name: text.name.or_else(|| Some(name.clone())),
        //         text: text.text,
        //         position: text.position,
        //         color: text.color,
        //     });
        // }
        // if let Some(text) = text2d {
        //     components.push(Component::Text2d {
        //         name: text.name.or_else(|| Some(name.clone())),
        //         text: text.text,
        //         position: text.position,
        //         color: text.color,
        //     });
        // }
        // if let Some(rect) = rect2d {
        //     components.push(Component::Rect2d {
        //         name: rect.name.or_else(|| Some(name.clone())),
        //         position: rect.position,
        //         size: rect.size,
        //         rotation: rect.rotation,
        //         color: rect.color,
        //     });
        // }
        // if let Some(transform) = transform {
        //     components.push(Component::Transform3d {
        //         position: transform.position,
        //         rotation: transform.rotation,
        //         scale: transform.scale,
        //     });
        // }
        Ok(("hello".to_string()))

        // let has_scene_components = components
        //     .iter()
        //     .any(|component| !matches!(component, Component::Name { .. }));
        // if !has_scene_components {
        //     return Err(PyValueError::new_err(
        //         "spawn() requires at least one non-Name component",
        //     ));
        // }

        // let payload = serde_json::to_string(&WorldCommand::Spawn { components })
        //     .map_err(|err| PyValueError::new_err(err.to_string()))?;
        // self.client.send_and_wait(SceneRequest::Apply { payload }, timeout_ms)?;
        // Ok(name)
    }

    pub fn list(&self, timeout_ms: Option<u64>) -> PyResult<Vec<EntityInfo>> {
        let response = self.client.send_and_wait(SceneRequest::List, timeout_ms)?;
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
        self.client
            .expect_ack(SceneRequest::Remove { name }, timeout_ms)
    }

    pub fn clear(&self, timeout_ms: Option<u64>) -> PyResult<()> {
        self.client.expect_ack(SceneRequest::Clear, timeout_ms)
    }
}
