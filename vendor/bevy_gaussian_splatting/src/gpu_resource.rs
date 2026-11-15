//! Thread-affine ownership for raw GPU objects shared with render extraction.

/// Wrap a raw GPU object. In threaded browsers, access and drop must happen on
/// its creating thread; other platforms retain the object's normal thread traits.
#[derive(Debug, Clone)]
pub struct GpuResource<T>(
    #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))] T,
    #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))] send_wrapper::SendWrapper<T>,
);

impl<T> GpuResource<T> {
    /// Retain a GPU object on its creating thread.
    pub fn new(value: T) -> Self {
        #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
        {
            Self(value)
        }
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        {
            Self(send_wrapper::SendWrapper::new(value))
        }
    }

    /// Release ownership of the wrapped GPU object on its creating thread.
    pub fn into_inner(self) -> T {
        #[cfg(not(all(target_arch = "wasm32", target_feature = "atomics")))]
        {
            self.0
        }
        #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
        {
            self.0.take()
        }
    }
}

impl<T> core::ops::Deref for GpuResource<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> core::ops::DerefMut for GpuResource<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}
