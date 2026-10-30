use bevy::{
    ecs::{
        query::{QueryFilter, With},
        system::{SystemParam, SystemState},
        world::{Mut, World},
    },
    prelude::{Entity, Resource},
    window::PrimaryWindow,
};
use bevy_egui::{EguiContext, egui};
use std::collections::HashMap;

pub trait RootWidgetSystemExt {
    /// Adds a root widget to the primary window.
    fn add_root_widget<S: RootWidgetSystem<Args = ()> + 'static>(&mut self, id: &str) -> S::Output {
        self.add_root_widget_with::<S, With<PrimaryWindow>>(id, ())
            .expect("missing window")
    }

    /// Add a root widget with a specific filter.
    fn add_root_widget_with<S: RootWidgetSystem + 'static, F: QueryFilter>(
        &mut self,
        id: &str,
        args: S::Args,
    ) -> Option<S::Output>;

    /// Add a root widget to a specific entity.
    fn add_root_widget_to<S: RootWidgetSystem + 'static>(
        &mut self,
        entity: Entity,
        id: &str,
        args: S::Args,
    ) -> Option<S::Output>;

    /// Execute a closure with the egui context from a query filter.
    fn egui_context_scope<R, F: QueryFilter>(
        &mut self,
        f: impl FnOnce(&mut Self, egui::Context) -> R,
    ) -> Option<R>;

    /// Execute a closure with the egui context from a specific entity.
    fn egui_context_scope_for<R>(
        &mut self,
        id: Entity,
        f: impl FnOnce(&mut Self, egui::Context) -> R,
    ) -> Option<R>;
}

impl RootWidgetSystemExt for World {
    fn add_root_widget_with<S: RootWidgetSystem + 'static, F: QueryFilter>(
        &mut self,
        id: &str,
        args: S::Args,
    ) -> Option<S::Output> {
        self.egui_context_scope::<_, F>(|world, mut ctx| {
            let id = WidgetId::new(id);

            if !world.contains_resource::<StateInstances<S>>() {
                world.insert_resource(StateInstances::<S> {
                    instances: HashMap::new(),
                });
            }

            world.resource_scope(|world, mut states: Mut<StateInstances<S>>| {
                let cached_state = states
                    .instances
                    .entry(id)
                    .or_insert_with(|| SystemState::new(world));
                let output = S::ctx_system(world, cached_state, &mut ctx, args);
                cached_state.apply(world);
                output
            })
        })
    }

    fn add_root_widget_to<S: RootWidgetSystem + 'static>(
        &mut self,
        entity: Entity,
        id: &str,
        args: S::Args,
    ) -> Option<S::Output> {
        self.egui_context_scope_for::<_>(entity, |world, mut ctx| {
            let id = WidgetId::new(id);

            if !world.contains_resource::<StateInstances<S>>() {
                world.insert_resource(StateInstances::<S> {
                    instances: HashMap::new(),
                });
            }

            world.resource_scope(|world, mut states: Mut<StateInstances<S>>| {
                let cached_state = states
                    .instances
                    .entry(id)
                    .or_insert_with(|| SystemState::new(world));
                let output = S::ctx_system(world, cached_state, &mut ctx, args);
                cached_state.apply(world);
                output
            })
        })
    }

    fn egui_context_scope<R, F: QueryFilter>(
        &mut self,
        f: impl FnOnce(&mut Self, egui::Context) -> R,
    ) -> Option<R> {
        let mut state = self.query_filtered::<&mut EguiContext, (With<EguiContext>, F)>();
        let Ok(mut egui_ctx) = state.single_mut(self) else {
            return None;
        };
        let ctx = egui_ctx.get_mut().clone();
        Some(f(self, ctx))
    }

    fn egui_context_scope_for<R>(
        &mut self,
        id: Entity,
        f: impl FnOnce(&mut Self, egui::Context) -> R,
    ) -> Option<R> {
        let mut egui_ctx = self.get_mut::<EguiContext>(id)?;
        let ctx = egui_ctx.get_mut().clone();
        Some(f(self, ctx))
    }
}

/// Extension trait for adding widgets to an egui UI.
pub trait WidgetSystemExt {
    /// Adds a widget to the UI without arguments.
    ///
    /// This is the recommended way to build reusable, world‑aware widgets:
    /// implement [`WidgetSystem`] in any crate, then call this from your
    /// `WidgetPane` or other egui code.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use bevy::prelude::*;
    /// # use bevy_egui::egui;
    /// # use dimensify::ui::widgets::{WidgetSystem, WidgetSystemExt};
    /// use bevy::ecs::system::SystemParam;
    ///
    /// #[derive(SystemParam)]
    /// struct MyWidget<'w, 's> {
    ///     // add queries / resources here
    ///     // my_res: Res<'w, MyResource>,
    /// }
    ///
    /// impl WidgetSystem for MyWidget<'_, '_> {
    ///     type Args = ();
    ///     type Output = egui::Response;
    ///
    ///     fn ui_system(
    ///         world: &mut World,
    ///         state: &mut bevy::ecs::system::SystemState<Self>,
    ///         ui: &mut egui::Ui,
    ///         _args: Self::Args,
    ///     ) -> Self::Output {
    ///         let _params = state.get_mut(world);
    ///         ui.button("My custom widget")
    ///     }
    /// }
    ///
    /// fn draw(ui: &mut egui::Ui, world: &mut World) {
    ///     ui.add_widget::<MyWidget>(world, "my_widget_id");
    /// }
    /// ```
    fn add_widget<S: WidgetSystem<Args = ()> + 'static>(
        &mut self,
        world: &mut World,
        id: &str,
    ) -> S::Output {
        self.add_widget_with::<S>(world, id, ())
    }

    /// Adds a widget to the UI with custom arguments.
    fn add_widget_with<S: WidgetSystem + 'static>(
        &mut self,
        world: &mut World,
        id: &str,
        args: S::Args,
    ) -> S::Output;
}

impl WidgetSystemExt for egui::Ui {
    fn add_widget_with<S: WidgetSystem + 'static>(
        &mut self,
        world: &mut World,
        id: &str,
        args: S::Args,
    ) -> S::Output {
        let id = WidgetId::new(id);

        if !world.contains_resource::<StateInstances<S>>() {
            world.insert_resource(StateInstances::<S> {
                instances: HashMap::new(),
            });
        }

        world.resource_scope(|world, mut states: Mut<StateInstances<S>>| {
            let cached_state = states
                .instances
                .entry(id)
                .or_insert_with(|| SystemState::new(world));
            let output = S::ui_system(world, cached_state, self, args);
            cached_state.apply(world);
            output
        })
    }
}

/// Trait for root widget systems that operate on the egui context level.
pub trait RootWidgetSystem: SystemParam {
    type Args;
    type Output;

    fn ctx_system(
        world: &mut World,
        state: &mut SystemState<Self>,
        ctx: &mut egui::Context,
        args: Self::Args,
    ) -> Self::Output;
}

/// Trait for widget systems that operate within an existing egui UI.
pub trait WidgetSystem: SystemParam {
    type Args;
    type Output;

    fn ui_system(
        world: &mut World,
        state: &mut SystemState<Self>,
        ui: &mut egui::Ui,
        args: Self::Args,
    ) -> Self::Output;
}

#[derive(Resource, Default)]
struct StateInstances<T: SystemParam + 'static> {
    instances: HashMap<WidgetId, SystemState<T>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u64);

impl WidgetId {
    pub const fn new(str: &str) -> Self {
        Self(const_fnv1a_hash::fnv1a_hash_str_64(str))
    }
}

pub type BoxedWidget<'a> = Box<dyn FnOnce(&mut egui::Ui) + 'a>;

pub trait BoxedWidgetExt<'a> {
    fn boxed(self) -> BoxedWidget<'a>;
}

impl<'a, T> BoxedWidgetExt<'a> for T
where
    T: egui::Widget + 'a,
{
    fn boxed(self) -> BoxedWidget<'a> {
        Box::new(move |ui| {
            self.ui(ui);
        })
    }
}

pub trait BoxedWidgetFnExt<'a> {
    fn boxed_fn(self) -> BoxedWidget<'a>;
}

impl<'a, F> BoxedWidgetFnExt<'a> for F
where
    F: FnOnce(&mut egui::Ui) + 'a,
{
    fn boxed_fn(self) -> BoxedWidget<'a> {
        Box::new(self)
    }
}

pub trait DynWidget: Send + Sync {
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response;
}

impl<F> DynWidget for F
where
    F: FnMut(&mut egui::Ui) -> egui::Response + Send + Sync + 'static,
{
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        (self)(ui)
    }
}
