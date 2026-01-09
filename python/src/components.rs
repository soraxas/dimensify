use dimensify_protocol::prelude::Component;
use pyo3::prelude::*;
use std::fmt;

use crate::{
    primitives::{PyQuat, PyVec3},
    shapes::PyShape3d,
};
use dimensify_protocol::prelude::Material;

// fn mat(c: Component) -> PyResult<PyObject> {
//     match c {
//         Component::Name(value) => PyObject::new(value),
//         Component::Line3d { name, points, color, width } => PyObject::new(Line3d { name, points, color, width }),
//         Component::Line2d { name, points, color, width } => PyObject::new(Line2d { name, points, color, width }),
//         Component::Text3d { name, text, position, color } => PyObject::new(Text3d { name, text, position, color }),
//         Component::Text2d { name, text, position, color } => PyObject::new(Text2d { name, text, position, color }),
//         Component::Mesh3d { name, position, scale } => PyObject::new(Mesh3d { name, position, scale }),
//         Component::Rect2d { name, position, size, rotation, color } => PyObject::new(Rect2d { name, position, size, rotation, color }),
//         Component::Transform3d { transform } => PyObject::new(Transform3d { transform }),
//     }
// }

/// A generic component wrapper for protocol components.
#[pyclass(name = "Component", str)]
#[derive(Clone, Debug)]
pub struct PyComponent(pub Component);
#[pymethods]
impl PyComponent {
    fn __repr__(&self) -> PyResult<String> {
        let out = match &self.0 {
            Component::Name(value) => format!("Name({:?})", value),
            Component::Mesh3d(value) => format!("Mesh3d({:?})", value),
            Component::Transform {
                translation,
                rotation,
                scale,
            } => format!(
                "Transform(translation={:?}, rotation={:?}, scale={:?})",
                translation, rotation, scale
            ),
            Component::MeshMaterial3d(value) => format!("MeshMaterial3d({:?})", value),
        };
        Ok(out)
    }

    #[staticmethod]
    #[pyo3(signature = (name))]
    pub fn name(name: String) -> Self {
        Self(Component::Name(name))
    }

    /// Create a Transform component.
    #[staticmethod]
    #[pyo3(signature = (translation = PyVec3::new(0.0, 0.0, 0.0), rotation = PyQuat::new(0.0, 0.0, 0.0, 1.0), scale = PyVec3::new(1.0, 1.0, 1.0)))]
    pub fn transform(translation: PyVec3, rotation: PyQuat, scale: PyVec3) -> Self {
        Self(Component::Transform {
            translation: translation.0,
            rotation: rotation.0,
            scale: scale.0,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (r=1.0, g=1.0, b=1.0, a=1.0))]
    pub fn material_from_color(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self(Component::MeshMaterial3d(Material::Color { r, g, b, a }))
    }

    /// Create a Mesh3d component from a Shape3d.
    #[staticmethod]
    #[pyo3(signature = (shape))]
    pub fn mesh_3d(shape: PyShape3d) -> Self {
        Self(Component::Mesh3d(shape.0))
    }
}

impl fmt::Display for PyComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.__repr__() {
            Ok(out) => write!(f, "{}", out),
            Err(_) => {
                log::error!("Failed to format component: {:?}", self);
                write!(f, "Component(...)")
            }
        }
    }
}
