mod math_trait_ext;
mod pipe;
mod urdf;

use eyre::Result;

pub fn initialise() -> Result<()> {
    color_eyre::install()
}

pub(crate) use self::pipe::*;

macro_rules! single {
    ($query:expr) => {
        match $query.get_single() {
            Ok(q) => q,
            _ => {
                return;
            }
        }
    };
}

macro_rules! single_mut {
    ($query:expr) => {
        match $query.get_single_mut() {
            Ok(q) => q,
            _ => {
                return;
            }
        }
    };
}

pub(crate) use urdf::*;
