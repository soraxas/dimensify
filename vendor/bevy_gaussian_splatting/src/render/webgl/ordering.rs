//! Source-scoped ordering; browser objects remain inside JS, never Send/Sync Rust data.
use super::*;
use splat_sort::DepthSorter;

#[cfg(target_arch = "wasm32")]
mod browser {
    use bevy::{
        prelude::*,
        winit::{EventLoopProxyWrapper, WinitUserEvent},
    };
    use wasm_bindgen::{JsCast, prelude::*};

    #[wasm_bindgen(module = "/src/render/webgl/sort_jobs.mjs")]
    extern "C" {
        /// Retain one source/view identity in the worker queue.
        pub(super) fn sort_create(positions: &[f32], version: &str) -> u32;
        /// Queue one view order if this identity is not already outstanding.
        pub(super) fn sort_request(id: u32, axis: &[f32]) -> bool;
        /// Return pending, complete, or compatibility-fallback status.
        pub(super) fn sort_status(id: u32) -> i32;
        /// Consume the completed order for this identity.
        pub(super) fn sort_take(id: u32) -> Vec<u32>;
        /// Retire an identity and free its worker positions/scratch.
        pub(super) fn sort_remove(id: u32);
        fn sort_install(wake: &js_sys::Function);
    }

    /// Wake reactive viewers after a worker reply or compatibility fallback.
    pub(crate) fn install(proxy: Option<Res<EventLoopProxyWrapper>>) {
        let Some(proxy) = proxy else {
            return;
        };
        let proxy = (**proxy).clone();
        let callback = Closure::<dyn FnMut()>::new(move || {
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        });
        sort_install(callback.as_ref().unchecked_ref());
        callback.forget();
    }

    /// Version worker/glue/WASM requests by their compiled source inputs.
    pub(super) fn version() -> String {
        let mut hash = 0xcbf29ce484222325u64;
        for text in [
            include_str!("sort_worker.mjs"),
            include_str!("../../../../../crates/splat_sort/src/lib.rs"),
        ] {
            for byte in text.bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        format!("{hash:x}")
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) use browser::install;

pub(super) struct Ordering {
    source: Arc<Vec<Instance>>,
    complete: Option<Mat4>,
    fallback: Option<DepthSorter>,
    #[cfg(target_arch = "wasm32")]
    worker: Option<u32>,
    #[cfg(target_arch = "wasm32")]
    requested: Option<Mat4>,
}

fn positions(data: &[Instance]) -> Vec<f32> {
    data.iter()
        .flat_map(|s| s.position_opacity[..3].iter().copied())
        .collect()
}

impl Ordering {
    /// Give each source/view lifetime a distinct worker identity.
    pub(super) fn new(source: Arc<Vec<Instance>>) -> Self {
        Self {
            #[cfg(target_arch = "wasm32")]
            worker: Some(browser::sort_create(
                &positions(&source),
                &browser::version(),
            )),
            #[cfg(target_arch = "wasm32")]
            requested: None,
            source,
            complete: None,
            fallback: None,
        }
    }

    /// Publish completed orders; coalesce camera movement while one request runs.
    pub(super) fn update(&mut self, view: Mat4) -> Option<Vec<u32>> {
        if !view.is_finite() {
            return None;
        }
        let axis = [view.x_axis.z, view.y_axis.z, view.z_axis.z, view.w_axis.z];
        #[cfg(target_arch = "wasm32")]
        if let Some(id) = self.worker {
            if browser::sort_status(id) < 0 {
                browser::sort_remove(id);
                self.worker = None;
                self.requested = None;
            } else {
                let result = if browser::sort_status(id) == 1 {
                    self.complete = self.requested.take();
                    Some(browser::sort_take(id))
                } else {
                    None
                };
                if self.requested.is_none()
                    && self.complete != Some(view)
                    && browser::sort_request(id, &axis)
                {
                    self.requested = Some(view);
                }
                return result;
            }
        }
        if self.complete == Some(view) {
            return None;
        }
        let sorter = self.fallback.get_or_insert_with(|| {
            DepthSorter::new(positions(&self.source)).expect("prepared finite positions")
        });
        let result = sorter.sort(&axis).expect("finite camera Z row");
        self.complete = Some(view);
        Some(result)
    }
}

impl Drop for Ordering {
    fn drop(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(id) = self.worker {
            browser::sort_remove(id);
        }
    }
}
