//! Standalone room host for the neutral shared-cube schema.
mod cube;
fn main() -> bevy::app::AppExit {
    let config =
        dimensify_hub::HostConfig::from_env("DIMENSIFY_COLLAB", "127.0.0.1:5002".parse().unwrap())
            .expect("valid host configuration");
    dimensify_hub::run::<cube::Cube>(config).expect("host startup")
}
