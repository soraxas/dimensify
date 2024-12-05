/// a macro that defines a simple struct,
/// as a configuration struct that has a single field `enabled`.
/// It is useful for defining a configuration struct that is used to enable/disable a feature.
#[macro_export]
macro_rules! define_config_state {
    ($struct_name:ident) => {
        // use bevy::ecs::system::SystemParam;
        // use bevy::prelude::{Statse, Res, ResMut, NextState};
        // use paste::paste;

        #[derive(Default, Debug, Hash, Eq, PartialEq, Clone, bevy::prelude::States)]
        pub enum $struct_name {
            On,
            #[default]
            Off,
        }

        impl $struct_name {
            /// Takes a closuse with mutable bool to set value of the state.
            ///
            /// Useful for changing the state based on some condition.
            ///
            /// # Example
            ///
            /// ```
            /// fn ui(world: &mut World, mut cx: EditorWindowContext, ui: &mut egui::Ui) {
            ///     ...
            ///
            ///     ConfCollidingObjects::with_bool(world, |val| {
            ///         ui.checkbox(val, "Show colliding objects with colour");
            ///     });
            ///
            /// }
            ///
            /// ```
            pub fn with_bool(world: &mut bevy::prelude::World, functor: impl FnOnce(&mut bool)) {
                world.resource_scope(
                    |world, state: bevy::prelude::Mut<bevy::prelude::State<Self>>| {
                        let ori_val = state.bool();
                        let mut val = ori_val;
                        // allow caller to mutate the value
                        functor(&mut val);
                        // if the value has changed, set the next state
                        if val != ori_val {
                            world.resource_scope(
                                |_,
                                 mut next_state: bevy::prelude::Mut<
                                    bevy::prelude::NextState<Self>,
                                >| {
                                    next_state.set($struct_name::from_bool(val));
                                },
                            )
                        }
                    },
                );
            }
        }

        paste::paste! {
            // define a system params that takes the mutable state, for easy access
            #[derive(bevy::ecs::system::SystemParam)]
            pub struct [<SystemParams $struct_name>] <'w> {
                pub state: bevy::prelude::Res<'w, bevy::prelude::State<$struct_name>>,
                pub next_state: bevy::prelude::ResMut<'w, bevy::prelude::NextState<$struct_name>>,
            }

            impl<'w> [<SystemParams $struct_name>]<'w> {

                /// a helper function that takes a closure that mutates the bool value of the state
                pub fn with_bool(&mut self, functor: impl FnOnce(&mut bool)) {
                    let mut val = self.state.bool();
                    functor(&mut val);
                    if val != self.state.bool() {
                        self.next_state.set($struct_name::from_bool(val));
                    }
                }
            }
        }

        impl $struct_name {
            pub fn from_bool(val: bool) -> Self {
                if val {
                    $struct_name::On
                } else {
                    $struct_name::Off
                }
            }

            pub fn bool(&self) -> bool {
                match self {
                    $struct_name::On => true,
                    $struct_name::Off => false,
                }
            }

            pub fn set_bool(&mut self, val: bool) {
                *self = if val {
                    $struct_name::Off
                } else {
                    $struct_name::On
                };
            }
        }
    };
}
