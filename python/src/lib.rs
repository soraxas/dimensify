use pyo3::prelude::*;

mod client;

/// Return compile-time build metadata and enabled feature flags.
#[pyfunction]
fn compile_info() -> String {
    let mut features = Vec::new();
    if cfg!(feature = "transport_webtransport") {
        features.push("transport_webtransport");
    }
    if cfg!(feature = "transport_websocket") {
        features.push("transport_websocket");
    }
    if cfg!(feature = "transport_udp") {
        features.push("transport_udp");
    }
    let features = if features.is_empty() {
        "none".to_string()
    } else {
        features.join(",")
    };
    format!(
        "crate={} version={} features={}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        features
    )
}

#[pymodule]
fn dimensify(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<client::DataSource>()?;
    m.add_class::<client::ViewerClient>()?;
    m.add_class::<client::TransportClient>()?;
    m.add_function(wrap_pyfunction!(compile_info, m)?)?;
    Ok(())
}
