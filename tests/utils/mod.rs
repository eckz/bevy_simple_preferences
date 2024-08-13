use bevy::core::FrameCount;
use bevy::prelude::*;
use bevy_simple_preferences::{PreferencesPlugin, PreferencesStorageType};

#[cfg(not(target_family = "wasm"))]
pub fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

pub fn create_test_app(storage_type: PreferencesStorageType) -> App {
    let mut app = App::new();

    app.add_plugins(MinimalPlugins)
        .add_plugins(
            PreferencesPlugin::persisted_with_app_name("PreferencesTest")
                .with_storage_type(storage_type),
        )
        .add_systems(
            PostUpdate,
            |mut app_exit: EventWriter<AppExit>, frame_count: Res<FrameCount>| {
                if frame_count.0 > 1 {
                    app_exit.send_default();
                }
            },
        );

    app
}
