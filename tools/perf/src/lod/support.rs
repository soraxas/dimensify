//! Packed-stream plugin and capture policy; lifecycle is shared with flat GPU runs.
use super::options::Options;
use bevy::prelude::*;
pub use dimensify_perf::offscreen::{capture, command, sha256, wait_for_gpu, write_ppm};

pub fn setup(options: &Options) -> Result<(App, Handle<Image>, Entity), String> {
    let mut app = dimensify_perf::offscreen::app();
    app.add_plugins(dimensify_splat::streaming::runtime::SplatStreamingPlugin);
    app.add_plugins(
        dimensify_splat::renderer::telemetry::SogGpuTelemetryPlugin {
            timestamps: options.timestamps,
        },
    );
    dimensify_perf::offscreen::finish(app, options.width, options.height, options.timestamps, true)
}
