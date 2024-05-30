use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy_inspector_egui::quick::ResourceInspectorPlugin;
use bevy_simple_preferences::{PreferencesMap, PreferencesPlugin, RegisterPreferences};

#[derive(Reflect, Default)]
struct ExampleSettings {
    field_u32: u32,
    some_str: String,
    some_option: Option<String>,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(LogPlugin {
            filter: "wgpu=error,naga=warn,bevy_eckz_preferences=debug".into(),
            ..default()
        }))
        .add_plugins(PreferencesPlugin::with_app_name("PreferencesExampleEgui"))
        .add_plugins(ResourceInspectorPlugin::<PreferencesMap>::default())
        .register_preferences::<ExampleSettings>()
        .run();
}
