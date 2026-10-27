//! Read-only activity for consumer presentation.
/// Current stage of streamed loading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StreamProgressState {
    #[default]
    Loading,
    Error,
}
/// Current loading activity, separate from renderer-ready scene status.
#[derive(Clone, Debug)]
pub struct StreamProgress {
    pub label: String,
    pub detail: String,
    pub state: StreamProgressState,
}
impl StreamProgress {
    /// Create the bounded state owned by this operation.
    pub(super) fn new(label: String) -> Self {
        Self {
            label,
            detail: String::new(),
            state: StreamProgressState::Loading,
        }
    }
    /// Attach human-readable loading observations.
    pub(super) fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }
    /// Set the observable loading or failure state.
    pub(super) fn state(mut self, state: StreamProgressState) -> Self {
        self.state = state;
        self
    }
}
