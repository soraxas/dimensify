use crate::widget::{PanelLocation, PanelRegistry, WidgetPane};
use bevy::prelude::*;

/// Trait for registering widgets in the bevy app.
pub trait AppRegisterWidgetTrait {
    fn register_widget<T>(
        &mut self,
        title: &'static str,
        default_enabled: bool,
        location: PanelLocation,
    ) -> &mut Self
    where
        T: WidgetPane + Default + 'static,
        Self: Sized;

    fn register_widget_left<T>(&mut self, title: &'static str, default_enabled: bool) -> &mut Self
    where
        T: WidgetPane + Default + 'static,
        Self: Sized,
    {
        self.register_widget::<T>(title, default_enabled, PanelLocation::Left)
    }

    fn register_widget_right<T>(&mut self, title: &'static str, default_enabled: bool) -> &mut Self
    where
        T: WidgetPane + Default + 'static,
        Self: Sized,
    {
        self.register_widget::<T>(title, default_enabled, PanelLocation::Right)
    }

    fn register_widget_bottom<T>(&mut self, title: &'static str, default_enabled: bool) -> &mut Self
    where
        T: WidgetPane + Default + 'static,
        Self: Sized,
    {
        self.register_widget::<T>(title, default_enabled, PanelLocation::Bottom)
    }

    fn register_widget_floating<T>(
        &mut self,
        title: &'static str,
        default_enabled: bool,
    ) -> &mut Self
    where
        T: WidgetPane + Default + 'static,
        Self: Sized,
    {
        self.register_widget::<T>(title, default_enabled, PanelLocation::Floating)
    }
}

impl AppRegisterWidgetTrait for App {
    fn register_widget<T>(
        &mut self,
        title: &'static str,
        default_enabled: bool,
        location: PanelLocation,
    ) -> &mut Self
    where
        T: WidgetPane + Default + 'static,
    {
        self.add_systems(Startup, move |mut registry: ResMut<PanelRegistry>| {
            registry.register_panel::<T>(title, location, default_enabled);
        })
    }
}
