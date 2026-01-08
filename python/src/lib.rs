use pyo3::prelude::*;

mod client;

/// Return compile-time build metadata and enabled feature flags.
#[pyfunction]
fn system_info() -> String {
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

#[pyfunction]
fn transport_enabled() -> bool {
    cfg!(any(
        feature = "transport_webtransport",
        feature = "transport_websocket",
        feature = "transport_udp"
    ))
}

#[pyfunction]
fn transport_features() -> Vec<String> {
    let mut features = Vec::new();
    if cfg!(feature = "transport_webtransport") {
        features.push("webtransport".to_string());
    }
    if cfg!(feature = "transport_websocket") {
        features.push("websocket".to_string());
    }
    if cfg!(feature = "transport_udp") {
        features.push("udp".to_string());
    }
    features
}

#[pymodule]
fn dimensify(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<client::DataSource>()?;
    m.add_class::<client::ViewerClient>()?;
    m.add_class::<client::TransportClient>()?;
    m.add_class::<client::EntityInfo>()?;
    m.add_class::<client::World>()?;
    m.add_class::<client::Name>()?;
    m.add_class::<client::Transform3d>()?;
    m.add_class::<client::Mesh3d>()?;
    m.add_class::<client::Line3d>()?;
    m.add_class::<client::Line2d>()?;
    m.add_class::<client::Text3d>()?;
    m.add_class::<client::Text2d>()?;
    m.add_class::<client::Rect2d>()?;
    m.add_function(wrap_pyfunction!(system_info, m)?)?;
    m.add_function(wrap_pyfunction!(transport_enabled, m)?)?;
    m.add_function(wrap_pyfunction!(transport_features, m)?)?;
    Ok(())
}
