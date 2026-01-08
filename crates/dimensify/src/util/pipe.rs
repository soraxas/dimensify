use bevy::{log::error, prelude::*};

pub(crate) fn error(In(result): In<Result<(), eyre::Error>>) {
    if let Err(err) = result {
        error!("Error: {err:?}")
    }
}
