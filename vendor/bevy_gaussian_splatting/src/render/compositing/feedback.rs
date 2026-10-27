//! One bounded render-to-main-world observation per requested camera.
use bevy::{prelude::*, render::extract_component::ExtractComponent};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositingFallback {
    PassUnavailable,
    TargetFormat,
    Msaa,
    Oit,
    ResolutionOverride,
    TransferPipeline,
    GaussianPipeline,
}

/// Describes issued rendering work, not GPU completion or visible-pixel proof.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompositingStatus {
    #[default]
    AwaitingRenderer,
    LinearFallback(CompositingFallback),
    NoVisibleDraws,
    Error,
    Encoded {
        runs: u32,
        linear_fallback_draws: u32,
        /// Estimated texel read+write traffic, excluding hardware compression.
        transfer_texel_bytes: u64,
    },
}

/// Clones share the last render observation without readback or growing history.
#[derive(Component, Clone, Default, ExtractComponent)]
pub struct CompositingFeedback(Arc<Mutex<CompositingStatus>>);

impl CompositingFeedback {
    pub fn snapshot(&self) -> CompositingStatus {
        self.0
            .lock()
            .map(|state| *state)
            .unwrap_or(CompositingStatus::Error)
    }

    pub(super) fn publish(&self, state: CompositingStatus) {
        if let Ok(mut current) = self.0.lock() {
            *current = state;
        }
    }
}
