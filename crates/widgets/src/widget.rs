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
    ///
    /// This is a convenience method that calls `add_root_widget_with` using the primary window filter.
    /// Panics if no primary window exists in the world.
    ///
    /// # Arguments
    /// * `id` - A unique string identifier for the widget instance
    ///
    /// # Returns
    /// The output of the widget system
    fn add_root_widget<S: RootWidgetSystem<Args = ()> + 'static>(&mut self, id: &str) -> S::Output {
        self.add_root_widget_with::<S, With<PrimaryWindow>>(id, ())
            .expect("missing window")
    }

    /// Add a root widget with a specific filter
    /// For example, you can add a root widget to a specific window
    /// by passing a filter that queries certain entities
    fn add_root_widget_with<S: RootWidgetSystem + 'static, F: QueryFilter>(
        &mut self,
        id: &str,
        args: S::Args,
    ) -> Option<S::Output>;

    /// Add a root widget to a specific entity
    fn add_root_widget_to<S: RootWidgetSystem + 'static>(
        &mut self,
        entity: Entity,
        id: &str,
        args: S::Args,
    ) -> Option<S::Output>;

    /// Executes a closure with an egui context obtained through a query filter.
    ///
    /// This method queries for an `EguiContext` component using the provided filter `F`,
    /// clones the context, and passes it to the closure along with a mutable reference to self.
    /// Returns `None` if no matching entity with an `EguiContext` is found.
    ///
    /// # Arguments
    /// * `f` - A closure that receives a mutable reference to self and a cloned egui context
    ///
    /// # Returns
    /// `Some(R)` if a matching context is found, `None` otherwise
    fn egui_context_scope<R, F: QueryFilter>(
        &mut self,
        f: impl FnOnce(&mut Self, egui::Context) -> R,
    ) -> Option<R>;

    /// Executes a closure with an egui context from a specific entity.
    ///
    /// Retrieves the `EguiContext` component from the specified entity, clones it,
    /// and passes it to the closure along with a mutable reference to self.
    /// Returns `None` if the entity doesn't exist or doesn't have an `EguiContext` component.
    ///
    /// # Arguments
    /// * `id` - The entity ID to retrieve the egui context from
    /// * `f` - A closure that receives a mutable reference to self and a cloned egui context
    ///
    /// # Returns
    /// `Some(R)` if the entity has an egui context, `None` otherwise
    fn egui_context_scope_for<R>(
        &mut self,
        id: Entity,
        f: impl FnOnce(&mut Self, egui::Context) -> R,
    ) -> Option<R>;
}

impl RootWidgetSystemExt for World {
    /// Adds a root widget system to the world using a query filter to find the egui context.
    ///
    /// This method manages widget state instances per widget ID, ensuring each widget
    /// maintains its own isolated system state. The state is cached and reused across frames
    /// for the same widget ID, allowing widgets to maintain persistent state.
    ///
    /// # Arguments
    /// * `id` - A unique string identifier for the widget instance
    /// * `args` - Arguments to pass to the widget system
    ///
    /// # Returns
    /// `Some(S::Output)` if the egui context is found via the filter, `None` otherwise
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

    /// Adds a root widget system to a specific entity.
    ///
    /// Similar to `add_root_widget_with`, but targets a specific entity by ID rather than
    /// using a query filter. This is useful when you know the exact entity that has the
    /// egui context you want to use.
    ///
    /// # Arguments
    /// * `entity` - The entity ID that contains the egui context
    /// * `id` - A unique string identifier for the widget instance
    /// * `args` - Arguments to pass to the widget system
    ///
    /// # Returns
    /// `Some(S::Output)` if the entity exists and has an egui context, `None` otherwise
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

    /// Executes a closure with an egui context obtained through a query filter.
    ///
    /// Queries for a single entity matching the filter `F` that has an `EguiContext` component,
    /// clones the context, and executes the closure with it. Returns `None` if no matching
    /// entity is found or if multiple entities match.
    ///
    /// # Arguments
    /// * `f` - A closure that receives a mutable reference to self and a cloned egui context
    ///
    /// # Returns
    /// `Some(R)` if exactly one matching entity is found, `None` otherwise
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

    /// Executes a closure with an egui context from a specific entity.
    ///
    /// Retrieves the `EguiContext` component directly from the specified entity,
    /// clones it, and executes the closure. Returns `None` if the entity doesn't exist
    /// or doesn't have an `EguiContext` component.
    ///
    /// # Arguments
    /// * `id` - The entity ID to retrieve the egui context from
    /// * `f` - A closure that receives a mutable reference to self and a cloned egui context
    ///
    /// # Returns
    /// `Some(R)` if the entity has an egui context, `None` otherwise
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
///
/// This trait provides methods to add widget systems to an existing egui UI context,
/// allowing widgets to be nested within other UI elements.
pub trait WidgetSystemExt {
    /// Adds a widget to the UI without arguments.
    ///
    /// This is a convenience method that calls `add_widget_with` with empty arguments.
    ///
    /// # Arguments
    /// * `world` - A mutable reference to the Bevy world
    /// * `id` - A unique string identifier for the widget instance
    ///
    /// # Returns
    /// The output of the widget system
    fn add_widget<S: WidgetSystem<Args = ()> + 'static>(
        &mut self,
        world: &mut World,
        id: &str,
    ) -> S::Output {
        self.add_widget_with::<S>(world, id, ())
    }

    /// Adds a widget to the UI with custom arguments.
    ///
    /// Manages widget state instances per widget ID, ensuring each widget maintains
    /// its own isolated system state that persists across frames.
    ///
    /// # Arguments
    /// * `world` - A mutable reference to the Bevy world
    /// * `id` - A unique string identifier for the widget instance
    /// * `args` - Arguments to pass to the widget system
    ///
    /// # Returns
    /// The output of the widget system
    fn add_widget_with<S: WidgetSystem + 'static>(
        &mut self,
        world: &mut World,
        id: &str,
        args: S::Args,
    ) -> S::Output;
}

impl WidgetSystemExt for egui::Ui {
    /// Adds a widget system to this UI with custom arguments.
    ///
    /// Creates or retrieves a cached system state for the widget ID, executes the widget
    /// system with the provided UI context, and applies any state changes back to the world.
    /// Each widget ID maintains its own isolated state, allowing multiple instances of the
    /// same widget type to coexist with independent state.
    ///
    /// # Arguments
    /// * `world` - A mutable reference to the Bevy world
    /// * `id` - A unique string identifier for the widget instance
    /// * `args` - Arguments to pass to the widget system
    ///
    /// # Returns
    /// The output of the widget system
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
///
/// Root widgets are top-level widgets that can create windows, panels, and other
/// top-level UI elements. They receive the full egui context rather than a specific UI.
///
/// # Type Parameters
/// * `Args` - The argument type passed to the widget system
/// * `Output` - The return type of the widget system
pub trait RootWidgetSystem: SystemParam {
    type Args;
    type Output;

    /// Executes the root widget system with the given egui context.
    ///
    /// This method is called by the widget framework to render the widget.
    /// The system state is managed by the framework and persists across frames.
    ///
    /// # Arguments
    /// * `world` - A mutable reference to the Bevy world
    /// * `state` - The cached system state for this widget instance
    /// * `ctx` - A mutable reference to the egui context
    /// * `args` - Arguments passed to the widget system
    ///
    /// # Returns
    /// The output of the widget system
    fn ctx_system(
        world: &mut World,
        state: &mut SystemState<Self>,
        ctx: &mut egui::Context,
        args: Self::Args,
    ) -> Self::Output;
}

/// Trait for widget systems that operate within an existing egui UI.
///
/// Regular widgets are nested within other UI elements and receive a specific UI context
/// rather than the full egui context. They can be added to any UI element.
///
/// # Type Parameters
/// * `Args` - The argument type passed to the widget system
/// * `Output` - The return type of the widget system
pub trait WidgetSystem: SystemParam {
    type Args;
    type Output;

    /// Executes the widget system with the given UI context.
    ///
    /// This method is called by the widget framework to render the widget within
    /// the provided UI. The system state is managed by the framework and persists across frames.
    ///
    /// # Arguments
    /// * `world` - A mutable reference to the Bevy world
    /// * `state` - The cached system state for this widget instance
    /// * `ui` - A mutable reference to the egui UI context
    /// * `args` - Arguments passed to the widget system
    ///
    /// # Returns
    /// The output of the widget system
    fn ui_system(
        world: &mut World,
        state: &mut SystemState<Self>,
        ui: &mut egui::Ui,
        args: Self::Args,
    ) -> Self::Output;
}

/// Resource that stores cached system state instances for each widget.
///
/// Each widget type `T` has its own `StateInstances` resource that maps widget IDs
/// to their corresponding `SystemState`. This allows widgets to maintain persistent
/// state across frames while keeping different widget instances isolated from each other.
#[derive(Resource, Default)]
struct StateInstances<T: SystemParam + 'static> {
    instances: HashMap<WidgetId, SystemState<T>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Unique identifier for a widget
/// Each widget has a unique id, which is used to identify the widget in the UI
pub struct WidgetId(pub u64);

impl WidgetId {
    /// Creates a new widget ID from a string identifier.
    ///
    /// The string is hashed using FNV-1a 64-bit hash algorithm to produce a unique
    /// numeric ID. The same string will always produce the same ID, allowing widgets
    /// to be identified consistently across frames.
    ///
    /// # Arguments
    /// * `str` - The string identifier to hash
    ///
    /// # Returns
    /// A `WidgetId` containing the hashed value
    pub const fn new(str: &str) -> Self {
        Self(const_fnv1a_hash::fnv1a_hash_str_64(str))
    }
}
