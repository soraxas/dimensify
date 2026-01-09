mod adopter;
mod custom;

#[cfg(feature = "bevy")]
pub mod bevy_impls {

    use crate::components::prelude::{Material, Shape3d};
    use bevy::{asset::Assets, mesh::Mesh, pbr::StandardMaterial};

    /// A wrapper type to indicate the insertion result.
    pub enum InsertionResult<'a, 'b> {
        Trivial(&'a mut bevy::prelude::EntityCommands<'b>),
        RequireResMesh(Shape3d),
        RequireResMaterial(Material),
    }

    /// Insert a ProtoComponent into a Bevy entity.
    ///
    /// This only inserts trivial components into the entity.
    ///
    /// If the component requires meshes or materials, it returns an
    /// enum indicator to schedule a mesh or material insertion.
    pub trait ProtoComponentIntoBevy {
        #[cfg(feature = "bevy")]
        fn insert_into<'a, 'b>(
            self,
            e: &'a mut bevy::prelude::EntityCommands<'b>,
        ) -> InsertionResult<'a, 'b>;
    }

    pub use super::adopter::bevy_impls::*;
}

pub mod prelude {
    pub use super::adopter::*;

    #[cfg(feature = "bevy")]
    pub use super::bevy_impls::*;
}
