use std::fs;
use std::io::{self, Write};

// use crfs_rs::{Attribute, Model};
use ouroboros::self_referencing;
use pyo3::prelude::*;

use robotsim::robot::Robot;

// #[pyclass(module = "crfs", name = "Attribute")]
// #[derive(FromPyObject)]
// struct PyAttribute {
//     /// Attribute name
//     #[pyo3(get, set)]
//     name: String,
//     /// Value of the attribute
//     #[pyo3(get, set)]
//     value: f64,
// }

// #[pymethods]
// impl PyAttribute {
//     #[new]
//     #[pyo3(signature = (name, value = 1.0))]
//     fn new(name: String, value: f64) -> Self {
//         Self { name, value }
//     }
// }

// #[derive(FromPyObject)]
// enum PyAttributeInput {
//     #[pyo3(transparent)]
//     Attr(PyAttribute),
//     Dict {
//         /// Attribute name
//         #[pyo3(item("name"))]
//         name: String,
//         /// Value of the attribute
//         #[pyo3(item("value"))]
//         value: f64,
//     },
//     Tuple(String, f64),
//     #[pyo3(transparent)]
//     NameOnly(String),
// }

// impl From<PyAttributeInput> for Attribute {
//     fn from(attr: PyAttributeInput) -> Self {
//         match attr {
//             PyAttributeInput::Attr(PyAttribute { name, value }) => Attribute::new(name, value),
//             PyAttributeInput::Dict { name, value } => Attribute::new(name, value),
//             PyAttributeInput::Tuple(name, value) => Attribute::new(name, value),
//             PyAttributeInput::NameOnly(name) => Attribute::new(name, 1.0),
//         }
//     }
// }


#[pyclass(module = "robotsim", name = "Robot")]
// #[self_referencing]
struct PyRobot {
    #[pyo3(get, set)]
    pub data: Vec<u8>,
    // #[borrows(data)]
    // #[covariant]
    robot: Robot,
}

#[pymethods]
impl PyRobot {

    #[new]
    fn py_new(path: &str) -> PyResult<Self> {
        Ok(PyRobot {
            data: vec![5, 9],
            robot: Robot::from_file(path)?,
        })
    }

    #[getter]
    fn name(&self) -> &str {
        &self.robot.name()
    }

    fn __repr__(&self) -> String {
        format!("<Robot '{}'>", self.name())
    }
}


// #[pyclass(module = "crfs", name = "Model")]
// #[self_referencing]
// struct PyModel {
//     data: Vec<u8>,
//     #[borrows(data)]
//     #[covariant]
//     model: Model<'this>,
// }

// #[pymethods]
// impl PyModel {
//     /// Create an instance of a model object from a model in memory
//     #[new]
//     fn new_py(data: Vec<u8>) -> PyResult<Self> {
//         let model = PyModel::try_new(data, |data| Model::new(data))?;
//         Ok(model)
//     }

//     /// Create an instance of a model object from a local file
//     #[staticmethod]
//     fn open(path: &str) -> PyResult<Self> {
//         let data = fs::read(path)?;
//         Self::new_py(data)
//     }

//     /// Predict the label sequence for the item sequence.
//     pub fn tag(&self, xseq: Vec<Vec<PyAttributeInput>>) -> PyResult<Vec<String>> {
//         let mut tagger = self.borrow_model().tagger()?;
//         let xseq: Vec<Vec<Attribute>> = xseq
//             .into_iter()
//             .map(|xs| xs.into_iter().map(Into::into).collect())
//             .collect();
//         let labels = tagger.tag(&xseq)?;
//         Ok(labels.iter().map(|l| l.to_string()).collect())
//     }

//     /// Print the model in human-readable format
//     pub fn dump(&self) -> PyResult<()> {
//         let mut out = Vec::new();
//         self.borrow_model().dump(&mut out)?;
//         let stdout = io::stdout();
//         let mut handle = stdout.lock();
//         handle.write_all(&out)?;
//         Ok(())
//     }
// }


#[pyfunction]
fn double(x: usize) -> usize {
    x * 2
}

#[pymodule]
#[pyo3(name = "robotsim")]
mod py_robotsim {
    use super::*;

    #[pymodule_export]
    use super::double; // Exports the double function as part of the module

    #[pymodule_export]
    use super::PyRobot;

    #[pyfunction] // This will be part of the module
    fn triple(x: usize) -> usize {
        x * 3
    }

    #[pyclass] // This will be part of the module
    struct Unit;

    #[pymodule]
    mod submodule {
        // This is a submodule
    }

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        // Arbitrary code to run at the module initialization
        m.add("double2", m.getattr("double")?)
    }
}