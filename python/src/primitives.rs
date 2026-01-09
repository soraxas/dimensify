/// A set of primitive types that are wrapped in Python classes.
/// These are used to represent the primitive types in the protocol.
/// They wrap the protocol POD primitives.
///
/// Example:
/// ```python
/// vec2 = d.Vec2(x=1.0, y=2.0)
/// vec3 = d.Vec3(x=1.0, y=2.0, z=3.0)
/// vec4 = d.Vec4(x=1.0, y=2.0, z=3.0, w=4.0)
/// quat = d.Quat(x=1.0, y=2.0, z=3.0, w=4.0)
/// dir2 = d.Dir2(x=1.0, y=2.0)
/// dir3 = d.Dir3(x=1.0, y=2.0, z=3.0)
/// dir4 = d.Dir4(x=1.0, y=2.0, z=3.0, w=4.0)
/// ```
use dimensify_protocol::prelude::{Dir2, Dir3, Dir4, Quat, Vec2, Vec3, Vec4};
use pyo3::{exceptions::PyValueError, prelude::*};

macro_rules! py_wrap_infallible {
    (
        $(#[$docs:meta])*
        $py_ty:ident, $py_name:literal, $inner:ty, $n:literal,
        new($($arg:ident = $default:expr),+ $(,)?) => $ctor:expr,
        fields: [$(($field:ident, $setter:ident)),+ $(,)?],
        extract($arr:ident) => $extract_ctor:expr $(,)?
    ) => {
        $(#[$docs])*
        #[pyo3::prelude::pyclass(name = $py_name, str)]
        pub struct $py_ty(pub(crate) $inner);

        #[pyo3::prelude::pymethods]
        impl $py_ty {
            #[new]
            #[pyo3(signature = ($($arg = $default),+))]
            pub fn new($($arg: f32),+) -> Self {
                Self($ctor)
            }

            $(
                #[getter]
                pub fn $field(&self) -> f32 { self.0.$field }

                #[setter]
                pub fn $setter(&mut self, v: f32) { self.0.$field = v; }
            )+

            pub fn __repr__(&self) -> pyo3::PyResult<String> {
                Ok(format!("{}", self.0))
            }
        }

        impl std::fmt::Display for $py_ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }


        /// Convert a Python object to a $py_ty.
        /// All objects that are convertible to a $n float array are valid.
        impl FromPyObject<'_, '_> for $py_ty {
            type Error = PyErr;

            fn extract(obj: Borrowed<'_, '_, PyAny>) -> PyResult<Self> {
                let $arr: [f32; $n] = obj.extract()?;
                Ok(Self($extract_ctor))
            }
        }
    };
}

macro_rules! py_wrap_fallible_no_setters {
    (
        $(#[$docs:meta])*
        $py_ty:ident, $py_name:literal, $inner:ty, $n:literal,
        new($($arg:ident = $default:expr),+ $(,)?) => $ctor:expr,
        getters: [$($field:ident),+ $(,)?],
        extract($arr:ident) => $extract_ctor:expr,
        map_err($map_err:expr) $(,)?
    ) => {
        $(#[$docs])*
        #[pyo3::prelude::pyclass(name = $py_name, str)]
        pub struct $py_ty(pub(crate) $inner);

        #[pyo3::prelude::pymethods]
        impl $py_ty {
            #[new]
            #[pyo3(signature = ($($arg = $default),+))]
            pub fn new($($arg: f32),+) -> pyo3::PyResult<Self> {
                ($ctor).map(Self).map_err($map_err)
            }

            $(
                #[getter]
                pub fn $field(&self) -> f32 { self.0.$field }
            )+

            pub fn __repr__(&self) -> pyo3::PyResult<String> {
                Ok(format!("{}", self.0))
            }
        }

        impl std::fmt::Display for $py_ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        /// Convert a Python object to a $py_ty.
        /// All objects that are convertible to a $n float array are valid.
        impl FromPyObject<'_, '_> for $py_ty {
            type Error = PyErr;

            fn extract(obj: Borrowed<'_, '_, PyAny>) -> PyResult<Self> {
                let $arr: [f32; $n] = obj.extract()?;
                Ok($extract_ctor.map(Self).map_err($map_err)?)
            }
        }
    };
}

py_wrap_infallible!(
    /// A 2D vector.
    PyVec2, "Vec2", Vec2, 2,
    new(x = 0.0, y = 0.0) => Vec2::new(x, y),
    fields: [(x, set_x), (y, set_y)],
    extract(arr) => Vec2::from(arr),
);

py_wrap_infallible!(
    /// A 3D vector.
    PyVec3, "Vec3", Vec3, 3,
    new(x = 0.0, y = 0.0, z = 0.0) => Vec3::new(x, y, z),
    fields: [(x, set_x), (y, set_y), (z, set_z)],
    extract(arr) => Vec3::from(arr),
);

py_wrap_infallible!(
    /// A 4D vector.
    PyVec4, "Vec4", Vec4, 4,
    new(x = 0.0, y = 0.0, z = 0.0, w = 0.0) => Vec4::new(x, y, z, w),
    fields: [(x, set_x), (y, set_y), (z, set_z), (w, set_w)],
    extract(arr) => Vec4::from(arr),
);

py_wrap_infallible!(
    /// A quaternion, in xyzw order.
    PyQuat, "Quat", Quat, 4,
    new(x = 0.0, y = 0.0, z = 0.0, w = 1.0) => Quat::from_xyzw(x, y, z, w),
    fields: [(x, set_x), (y, set_y), (z, set_z), (w, set_w)],
    extract(arr) => Quat::from_xyzw(arr[0], arr[1], arr[2], arr[3]),
);

// fallible as direction constructors are fallible

fn dir_err_to_py(e: dimensify_protocol::prelude::InvalidDirectionError) -> pyo3::PyErr {
    PyValueError::new_err(e.to_string())
}

py_wrap_fallible_no_setters!(
    /// A 2D direction.
    PyDir2, "Dir2", Dir2, 2,
    new(x = 0.0, y = 0.0) => Dir2::new(Vec2::new(x, y)),
    getters: [x, y],
    extract(arr) => Dir2::new(Vec2::from(arr)),
    map_err(dir_err_to_py),
);

py_wrap_fallible_no_setters!(
    /// A 3D direction.
    PyDir3, "Dir3", Dir3, 3,
    new(x = 0.0, y = 0.0, z = 0.0) => Dir3::new(Vec3::new(x, y, z)),
    getters: [x, y, z],
    extract(arr) => Dir3::new(Vec3::from(arr)),
    map_err(dir_err_to_py),
);

py_wrap_fallible_no_setters!(
    /// A 4D direction.
    PyDir4, "Dir4", Dir4, 4,
    new(x = 0.0, y = 0.0, z = 0.0, w = 0.0) => Dir4::new(Vec4::new(x, y, z, w)),
    getters: [x, y, z, w],
    extract(arr) => Dir4::new(Vec4::from(arr)),
    map_err(dir_err_to_py),
);
