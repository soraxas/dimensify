use crate::primitives::{PyVec2, PyVec3};
use dimensify_protocol::{
    bm3d,
    prelude::{Dir3, Shape3d},
};
use pyo3::prelude::*;
use std::{fmt, fmt::Display};

/// A 3D shape primitive.
///
/// Example:
/// ```python
/// shape = d.Shape3d.sphere(radius=1.0)
/// world.spawn(d.Component.mesh_3d(shape))
/// ```
#[pyclass(name = "Shape3d", str)]
#[derive(Clone)]
pub struct PyShape3d(pub(crate) Shape3d);
#[pymethods]
impl PyShape3d {
    fn __repr__(&self) -> PyResult<String> {
        let out = match &self.0 {
            Shape3d::Sphere(sphere) => format!("Sphere3d(radius={})", sphere.radius),
            Shape3d::Plane3d(plane) => format!(
                "Plane3d(normal={:?}, half_size={:?})",
                plane.normal, plane.half_size
            ),
            Shape3d::Segment3d(segment) => format!("Segment3d(vertices={:?})", segment.vertices),
            Shape3d::Polyline3d(polyline) => {
                format!("Polyline3d(vertices={:?})", polyline.vertices)
            }
            Shape3d::Cuboid(cuboid) => format!("Cuboid(half_size={:?})", cuboid.half_size),
            Shape3d::Cylinder(cylinder) => format!(
                "Cylinder(radius={}, half_height={})",
                cylinder.radius, cylinder.half_height
            ),
            Shape3d::Capsule3d(capsule) => format!(
                "Capsule3d(radius={}, half_length={})",
                capsule.radius, capsule.half_length
            ),
            Shape3d::Cone(cone) => format!("Cone(radius={}, height={})", cone.radius, cone.height),
            Shape3d::ConicalFrustum(conical_frustum) => format!(
                "ConicalFrustum(radius_top={}, radius_bottom={}, height={})",
                conical_frustum.radius_top, conical_frustum.radius_bottom, conical_frustum.height
            ),
            Shape3d::Torus(torus) => format!(
                "Torus(major_radius={}, minor_radius={})",
                torus.major_radius, torus.minor_radius
            ),
            Shape3d::Triangle3d(triangle) => {
                format!("Triangle3d(vertices={:?})", triangle.vertices)
            }
            Shape3d::Tetrahedron(tetrahedron) => {
                format!("Tetrahedron(vertices={:?})", tetrahedron.vertices)
            }
        };
        Ok(out)
    }

    #[staticmethod]
    #[pyo3(signature = (radius))]
    pub fn sphere(radius: f32) -> Self {
        Self(Shape3d::Sphere(bm3d::Sphere { radius }))
    }

    #[staticmethod]
    #[pyo3(signature = (normal, half_size))]
    pub fn plane3d(normal: PyVec3, half_size: PyVec2) -> Self {
        let normal_dir = Dir3::new(normal.0).expect("normal vector must be non-zero");
        Self(Shape3d::Plane3d(bm3d::Plane3d {
            normal: normal_dir,
            half_size: half_size.0,
        }))
    }

    #[staticmethod]
    #[pyo3(signature = (vertices))]
    pub fn polyline3d(vertices: Vec<PyVec3>) -> Self {
        Self(Shape3d::Polyline3d(bm3d::Polyline3d {
            vertices: vertices.into_iter().map(|v| v.0).collect(),
        }))
    }

    #[staticmethod]
    #[pyo3(signature = (vertices))]
    pub fn segment3d(vertices: [PyVec3; 2]) -> Self {
        Self(Shape3d::Segment3d(bm3d::Segment3d {
            vertices: vertices.map(|v| v.0),
        }))
    }

    #[staticmethod]
    #[pyo3(signature = (half_size))]
    pub fn cuboid(half_size: PyVec3) -> Self {
        Self(Shape3d::Cuboid(bm3d::Cuboid {
            half_size: half_size.0,
        }))
    }

    #[staticmethod]
    #[pyo3(signature = (radius, half_height))]
    pub fn cylinder(radius: f32, half_height: f32) -> Self {
        Self(Shape3d::Cylinder(bm3d::Cylinder {
            radius,
            half_height,
        }))
    }

    #[staticmethod]
    #[pyo3(signature = (radius, half_length))]
    pub fn capsule3d(radius: f32, half_length: f32) -> Self {
        Self(Shape3d::Capsule3d(bm3d::Capsule3d {
            radius,
            half_length,
        }))
    }

    #[staticmethod]
    #[pyo3(signature = (radius, height))]
    pub fn cone(radius: f32, height: f32) -> Self {
        Self(Shape3d::Cone(bm3d::Cone { radius, height }))
    }

    #[staticmethod]
    #[pyo3(signature = (radius_top, radius_bottom, height))]
    pub fn conical_frustum(radius_top: f32, radius_bottom: f32, height: f32) -> Self {
        Self(Shape3d::ConicalFrustum(bm3d::ConicalFrustum {
            radius_top,
            radius_bottom,
            height,
        }))
    }

    #[staticmethod]
    #[pyo3(signature = (major_radius, minor_radius))]
    pub fn torus(major_radius: f32, minor_radius: f32) -> Self {
        Self(Shape3d::Torus(bm3d::Torus {
            major_radius,
            minor_radius,
        }))
    }

    #[staticmethod]
    #[pyo3(signature = (vertices))]
    pub fn triangle3d(vertices: [PyVec3; 3]) -> Self {
        Self(Shape3d::Triangle3d(bm3d::Triangle3d {
            vertices: vertices.map(|v| v.0),
        }))
    }

    #[staticmethod]
    #[pyo3(signature = (vertices))]
    pub fn tetrahedron(vertices: [PyVec3; 4]) -> Self {
        Self(Shape3d::Tetrahedron(bm3d::Tetrahedron {
            vertices: vertices.map(|v| v.0),
        }))
    }
}

impl Display for PyShape3d {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Shape3d({:?})", self.0)
    }
}
