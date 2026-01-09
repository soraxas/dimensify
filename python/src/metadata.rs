use dimensify_protocol::{ComponentInfo, WorldCommand};
use dimensify_transport::EntityInfo;
use pyo3::{exceptions::PyValueError, prelude::*};
use std::{
    fs::File,
    io::{BufWriter, Write},
};

#[pyclass(name = "ComponentInfo")]
#[derive(Clone, Debug)]
pub struct PyComponentInfo(ComponentInfo);

#[pymethods]
impl PyComponentInfo {
    pub fn to_dict<'a>(&self, py: Python<'a>) -> PyResult<pyo3::Bound<'a, pyo3::types::PyDict>> {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("id", self.0.id)?;
        dict.set_item("name", self.0.name.clone())?;
        Ok(dict)
    }
}
impl From<ComponentInfo> for PyComponentInfo {
    fn from(info: ComponentInfo) -> Self {
        Self(info)
    }
}

use dimensify_protocol::prelude::Entity;

#[pyclass(name = "Entity")]
#[derive(Clone, Debug)]
pub struct PyEntity(pub(crate) Entity);
#[pymethods]
impl PyEntity {
    #[getter]
    pub fn id(&self) -> u64 {
        self.0.to_bits()
    }
    #[getter]
    pub fn index(&self) -> u32 {
        self.0.index()
    }
    #[getter]
    pub fn generation(&self) -> u32 {
        self.0.generation().to_bits()
    }

    fn __repr__(&self) -> String {
        format!("Entity({})", self.0)
    }

    #[staticmethod]
    pub fn from_bits(bits: u64) -> Self {
        Self(Entity::from_bits(bits))
    }
}

impl From<Entity> for PyEntity {
    fn from(entity: Entity) -> Self {
        Self(entity)
    }
}

impl std::fmt::Display for PyEntity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[pyclass(name = "EntityInfo")]
#[derive(Clone, Debug)]
pub struct PyEntityInfo(EntityInfo);

#[pymethods]
impl PyEntityInfo {
    #[getter]
    pub fn entity(&self) -> PyEntity {
        PyEntity::from_bits(self.0.id)
    }

    #[getter]
    pub fn name(&self) -> Option<&String> {
        self.0.name.as_ref()
    }

    #[getter]
    pub fn get_components(&self) -> Vec<PyComponentInfo> {
        self.0
            .components
            .clone()
            .into_iter()
            .map(PyComponentInfo::from)
            .collect()
    }

    pub fn to_dict<'a>(&self, py: Python<'a>) -> PyResult<pyo3::Bound<'a, pyo3::types::PyDict>> {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("id", self.0.id)?;
        dict.set_item("name", self.0.name.clone())?;

        let components = pyo3::types::PyDict::new(py);
        for c in &self.0.components {
            components.set_item(c.id, c.name.clone())?;
        }
        dict.set_item("components", components)?;
        Ok(dict)
    }
}

impl From<EntityInfo> for PyEntityInfo {
    fn from(info: EntityInfo) -> Self {
        Self(info)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum DataSourceKind {
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
pub struct ViewerClient {
    source: DataSourceKind,
    commands: Vec<WorldCommand>,
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

    // pub fn log_line_3d(
    //     &mut self,
    //     points: Vec<(f32, f32, f32)>,
    //     color: Option<(f32, f32, f32, f32)>,
    //     width: Option<f32>,
    // ) {
    //     let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
    //     let width = width.unwrap_or(1.0);
    //     self.commands.push(WorldCommand::Spawn {
    //         components: vec![Component::Line3d {
    //             name: None,
    //             points: points.into_iter().map(|p| [p.0, p.1, p.2]).collect(),
    //             color: [color.0, color.1, color.2, color.3],
    //             width,
    //         }],
    //     });
    // }

    // pub fn log_line_2d(
    //     &mut self,
    //     points: Vec<(f32, f32)>,
    //     color: Option<(f32, f32, f32, f32)>,
    //     width: Option<f32>,
    // ) {
    //     let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
    //     let width = width.unwrap_or(1.0);
    //     self.commands.push(WorldCommand::Spawn {
    //         components: vec![Component::Line2d {
    //             name: None,
    //             points: points.into_iter().map(|p| [p.0, p.1]).collect(),
    //             color: [color.0, color.1, color.2, color.3],
    //             width,
    //         }],
    //     });
    // }

    // pub fn log_text_3d(
    //     &mut self,
    //     text: String,
    //     position: (f32, f32, f32),
    //     color: Option<(f32, f32, f32, f32)>,
    // ) {
    //     let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
    //     self.commands.push(WorldCommand::Spawn {
    //         components: vec![Component::Text3d {
    //             name: None,
    //             text,
    //             position: [position.0, position.1, position.2],
    //             color: [color.0, color.1, color.2, color.3],
    //         }],
    //     });
    // }

    // pub fn log_text_2d(
    //     &mut self,
    //     text: String,
    //     position: (f32, f32),
    //     color: Option<(f32, f32, f32, f32)>,
    // ) {
    //     let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
    //     self.commands.push(WorldCommand::Spawn {
    //         components: vec![Component::Text2d {
    //             name: None,
    //             text,
    //             position: [position.0, position.1],
    //             color: [color.0, color.1, color.2, color.3],
    //         }],
    //     });
    // }

    // pub fn log_mesh_3d(
    //     &mut self,
    //     name: String,
    //     position: (f32, f32, f32),
    //     scale: Option<(f32, f32, f32)>,
    // ) {
    //     let scale = scale.unwrap_or((1.0, 1.0, 1.0));
    //     self.commands.push(WorldCommand::Spawn {
    //         components: vec![Component::Mesh3d {
    //             name: Some(name),
    //             position: [position.0, position.1, position.2],
    //             scale: [scale.0, scale.1, scale.2],
    //         }],
    //     });
    // }

    // pub fn log_rect_2d(
    //     &mut self,
    //     position: (f32, f32),
    //     size: (f32, f32),
    //     rotation: Option<f32>,
    //     color: Option<(f32, f32, f32, f32)>,
    // ) {
    //     let rotation = rotation.unwrap_or(0.0);
    //     let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
    //     self.commands.push(WorldCommand::Spawn {
    //         components: vec![Component::Rect2d {
    //             name: None,
    //             position: [position.0, position.1],
    //             size: [size.0, size.1],
    //             rotation,
    //             color: [color.0, color.1, color.2, color.3],
    //         }],
    //     });
    // }

    // pub fn set_transform(
    //     &mut self,
    //     entity: String,
    //     position: (f32, f32, f32),
    //     rotation: (f32, f32, f32, f32),
    //     scale: (f32, f32, f32),
    // ) {
    //     self.commands.push(WorldCommand::Update {
    //         entity,
    //         component: Component::Transform3d {
    //             transform: TransformPod {
    //                 position: [position.0, position.1, position.2],
    //                 rotation: [rotation.0, rotation.1, rotation.2, rotation.3],
    //                 scale: [scale.0, scale.1, scale.2],
    //             },
    //         },
    //     });
    // }

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
