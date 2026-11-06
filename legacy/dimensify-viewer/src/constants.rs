pub const SCENE_FLOOR_NAME: &str = "SCENE_FLOOR";

pub fn system_info() -> String {
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
