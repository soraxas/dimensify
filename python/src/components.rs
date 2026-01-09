use pyo3::{exceptions::PyValueError, prelude::*};
use serde::Serialize;
use std::{
    fs::File,
    io::{BufWriter, Write},
};

use dimensify_transport::ViewerEntityInfo;

pub(crate) mod prelude {
    pub(crate) use super::{Component, ComponentKind, WorldCommand};
    pub use super::{
        DataSource, EntityInfo, Line2d, Line3d, Mesh3d, Name, Rect2d, Text2d, Text3d, Transform3d,
        ViewerClient,
    };
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
    pub(crate) fn from_viewer(info: ViewerEntityInfo) -> Self {
        Self {
            id: info.id,
            name: info.name,
            components: info.components,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub(crate) enum Component {
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
pub(crate) enum WorldCommand {
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
pub(crate) enum ComponentKind {
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

    pub fn log_line_3d(
        &mut self,
        points: Vec<(f32, f32, f32)>,
        color: Option<(f32, f32, f32, f32)>,
        width: Option<f32>,
    ) {
        let color = color.unwrap_or((1.0, 1.0, 1.0, 1.0));
        let width = width.unwrap_or(1.0);
        self.commands.push(WorldCommand::Spawn {
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
        self.commands.push(WorldCommand::Spawn {
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
        self.commands.push(WorldCommand::Spawn {
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
        self.commands.push(WorldCommand::Spawn {
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
        self.commands.push(WorldCommand::Spawn {
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
        self.commands.push(WorldCommand::Spawn {
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
        self.commands.push(WorldCommand::Update {
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
    pub(crate) value: String,
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
    pub(crate) position: [f32; 3],
    pub(crate) rotation: [f32; 4],
    pub(crate) scale: [f32; 3],
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
    pub(crate) name: Option<String>,
    pub(crate) position: Option<[f32; 3]>,
    pub(crate) scale: Option<[f32; 3]>,
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
    pub(crate) name: Option<String>,
    pub(crate) points: Vec<[f32; 3]>,
    pub(crate) color: [f32; 4],
    pub(crate) width: f32,
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
    pub(crate) name: Option<String>,
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
