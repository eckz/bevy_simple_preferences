//! Shows an example that uses egui to modify preferences.
//! Also stores egui internals in the preferences, like window positions.

use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy_inspector_egui::bevy_egui::{EguiContexts, EguiPlugin};
use bevy_inspector_egui::{egui, reflect_inspector, DefaultInspectorConfigPlugin};
use bevy_simple_preferences::PreferencesResource;
use bevy_simple_preferences::*;
use std::marker::PhantomData;
use std::ops::DerefMut;

struct PreferencesInspectorPlugin<T> {
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for PreferencesInspectorPlugin<T> {
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<T: PreferencesType> Plugin for PreferencesInspectorPlugin<T> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<DefaultInspectorConfigPlugin>() {
            app.add_plugins(DefaultInspectorConfigPlugin);
        }
        if !app.is_plugin_added::<EguiPlugin>() {
            app.add_plugins(EguiPlugin);
        }
        app.add_systems(Update, preferences_ui::<T>);
    }
}

fn preferences_ui<T: PreferencesType>(
    app_type_registry: Res<AppTypeRegistry>,
    mut egui_contexts: EguiContexts,
    mut preferences: ResMut<PreferencesResource<T>>,
) {
    let type_registry = app_type_registry.read();
    let ctx = egui_contexts.ctx_mut();

    egui::Window::new(format!("Preferences ({})", T::short_type_path()))
        .default_size((100., 100.))
        .show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                let value = preferences.bypass_change_detection().deref_mut();

                if reflect_inspector::ui_for_value(value, ui, &type_registry) {
                    preferences.set_changed();
                }

                ui.allocate_space(ui.available_size());
            });
        });
}

#[derive(Reflect, PartialEq, Clone, Default)]
#[reflect(PartialEq)]
struct EguiPreferences {
    memory: String,
}

#[derive(Reflect, Default)]
struct MyPreferences {
    field_u32: u32,
    some_str: String,
    some_option: Option<String>,
}

#[derive(Reflect, Default)]
#[reflect(Default)]
struct OtherPreferencesListValue {
    foo: u32,
    bar: u32,
}
#[derive(Reflect, Default)]
struct OtherPreferences {
    field_u32: u32,
    some_str: String,
    some_list: Vec<OtherPreferencesListValue>,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(LogPlugin {
            filter: "wgpu=error,naga=warn,bevy_simple_preferences=debug".into(),
            ..default()
        }))
        .add_plugins(PreferencesPlugin::persisted_with_app_name(
            "PreferencesExampleEgui",
        ))
        .register_preferences::<EguiPreferences>()
        .register_preferences::<MyPreferences>()
        .register_preferences::<OtherPreferences>()
        .add_plugins(PreferencesInspectorPlugin::<MyPreferences>::default())
        .add_plugins(PreferencesInspectorPlugin::<OtherPreferences>::default())
        .add_systems(Startup, restore_egui_memory)
        .add_systems(PreUpdate, store_egui_memory)
        .run();
}

fn store_egui_memory(
    mut egui_contexts: EguiContexts,
    mut egui_preferences: Preferences<EguiPreferences>,
) {
    let ctx = egui_contexts.ctx_mut();
    let memory = ctx.memory(|memory| serde_json::to_string(memory).unwrap());
    *egui_preferences = EguiPreferences { memory };
}

fn restore_egui_memory(
    mut egui_contexts: EguiContexts,
    egui_preferences: Preferences<EguiPreferences>,
) {
    let ctx = egui_contexts.ctx_mut();

    if let Ok(new_memory) = serde_json::from_str(&egui_preferences.memory) {
        ctx.memory_mut(move |memory| *memory = new_memory);
    }
}
