use pyo3::prelude::*;

pub(crate) mod client;
pub(crate) mod components;
pub(crate) mod telemetry;
pub(crate) mod world;

mod metadata;
mod primitives;

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

mod shapes;

#[pymodule]
fn dimensify(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<primitives::PyVec3>()?;
    m.add_class::<primitives::PyQuat>()?;
    m.add_class::<metadata::DataSource>()?;
    m.add_class::<metadata::ViewerClient>()?;
    m.add_class::<client::TransportClient>()?;
    m.add_class::<metadata::PyEntityInfo>()?;
    m.add_class::<world::World>()?;
    // m.add_class::<components::Name>()?;
    // m.add_class::<components::Transform3d>()?;
    // m.add_class::<components::Mesh3d>()?;
    // m.add_class::<components::Line3d>()?;
    // m.add_class::<components::Line2d>()?;
    // m.add_class::<components::Text3d>()?;
    // m.add_class::<components::Text2d>()?;
    // m.add_class::<components::Rect2d>()?;
    m.add_class::<telemetry::TelemetryClient>()?;
    m.add_function(wrap_pyfunction!(system_info, m)?)?;
    m.add_function(wrap_pyfunction!(transport_enabled, m)?)?;
    m.add_function(wrap_pyfunction!(transport_features, m)?)?;

    m.add_class::<components::PyComponent>()?;
    m.add_class::<shapes::PyShape3d>()?;

    Ok(())
}
