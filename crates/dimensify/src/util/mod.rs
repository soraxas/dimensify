#![allow(unused)]

pub mod math_trait_ext;
pub mod pipe;
pub mod spatial;
pub mod urdf;

pub mod bihashmap;
pub mod exponential_iterator;
pub mod macros;
pub mod traits;

pub mod mesh_tools;

use eyre::Result;

pub fn initialise() -> Result<()> {
    setup_hooks();
    color_eyre::install()
}

pub(crate) use spatial::*;
pub(crate) use urdf::*;

pub fn setup_hooks() {
    #[cfg(debug_assertions)]
    #[cfg(target_arch = "wasm32")]
    {
        // console_error_panic_hook::set_once();
    }
}

pub fn log(_msg: &str) {
    #[cfg(debug_assertions)]
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::console::log_1(&_msg.into());
    }
    #[cfg(debug_assertions)]
    #[cfg(not(target_arch = "wasm32"))]
    {
        println!("{}", _msg);
    }
}
