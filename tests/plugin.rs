//! End-to-end test
mod utils;

use bevy::prelude::*;
use bevy::utils::HashMap;
use bevy_simple_preferences::{Preferences, PreferencesStorageType, RegisterPreferencesExt};
use rand::random;
use utils::*;

#[cfg(target_family = "wasm")]
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
#[cfg(target_family = "wasm")]
wasm_bindgen_test_configure!(run_in_browser);

#[derive(Reflect, PartialEq, Clone, Debug, Default)]
struct MyPreferenceInsideAMap {
    some_string: String,
    some_option: u32,
}

#[derive(Reflect, PartialEq, Clone, Debug, Default)]
struct MyPluginPreferences {
    some_map: HashMap<String, MyPreferenceInsideAMap>,
}

fn test_preferences_plugin_reads_and_writes(storage_type: PreferencesStorageType) {
    let some_option = random();

    let initial_preferences = MyPluginPreferences {
        some_map: HashMap::from_iter([(
            "SomeValue".to_string(),
            MyPreferenceInsideAMap {
                some_string: "SomeString".into(),
                some_option,
            },
        )]),
    };

    let expected_preferences = initial_preferences.clone();

    // First, we store the initial preferences in a system
    {
        create_test_app(storage_type.clone())
            .register_preferences::<MyPluginPreferences>()
            .add_systems(
                Update,
                move |mut preferences: Preferences<MyPluginPreferences>| {
                    *preferences = initial_preferences.clone();
                },
            )
            .run();
    }

    // We simulate restart of the app, we expect the preferences to have been loaded from disk at Startup
    {
        create_test_app(storage_type)
            .register_preferences::<MyPluginPreferences>()
            .add_systems(
                Update,
                move |preferences: Preferences<MyPluginPreferences>| {
                    assert_eq!(&*preferences, &expected_preferences);
                },
            )
            .run();
    }
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn preferences_plugin_reads_and_writes_to_disk() {
    let temp_dir = temp_dir();
    test_preferences_plugin_reads_and_writes(
        PreferencesStorageType::FileSystemWithParentDirectory(temp_dir.path().into()),
    );
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen_test]
fn preferences_plugin_reads_and_writes_to_local_storage() {
    test_preferences_plugin_reads_and_writes(PreferencesStorageType::LocalStorage);
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen_test]
fn preferences_plugin_reads_and_writes_to_session_storage() {
    test_preferences_plugin_reads_and_writes(PreferencesStorageType::SessionStorage);
}
