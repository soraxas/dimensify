use pyo3::prelude::*;

mod client;

#[pymodule]
fn dimensify(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<client::DataSource>()?;
    m.add_class::<client::ViewerClient>()?;
    Ok(())
}
