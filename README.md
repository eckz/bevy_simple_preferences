# bevy_simple_preferences

## About
Bevy Preferences Simple Abstraction

Provides a simple Preferences API for Bevy that allows different crates to
relay in a simple preferences abstraction giving the final app crate full control on how
and where to store the preferences.

This crate is heavily based on the [Bevy Preferences API proposal](https://github.com/bevyengine/bevy/issues/13311).

## Examples

You first need to define a struct / enum that represents your preferences.
```rust
#[derive(Reflect, Default)]
struct ExampleSettings {
    field_u32: u32,
    some_str: String,
    some_option: Option<String>,
}
```

And in your code, you just need to add the [`PreferencesPlugin`] and call [`RegisterPreferencesExt::register_preferences`]
```rust
App::new()
    .add_plugins(PreferencesPlugin::persisted_with_app_name("YourAppName"))
    .register_preferences::<ExampleSettings>();
```

If you are implementing a library, you don't need to add [`PreferencesPlugin`], since it's
up the final user to add it themselves. But you can rely on the [`Preferences`] param to read and write preferences,
even if the final user does not add the plugin.

If the final user does not add the [`PreferencesPlugin`], the effect is that preferences are simply not stored.

```rust

impl Plugin for MyCratePlugin {
    fn build(&self, app: &mut App) {
        app.register_preferences::<MyCratePreferences>()
            .add_systems(Update, |my_preferences: Preferences<MyCratePreferences>| {
                // Do stuff with your preferences
                assert!(my_preferences.some_field >= 0);
            });
        ;
    }
}
```

## Supported Bevy Versions
| Bevy | `bevy_simple_preferences` |
| ---- | -----------------------   |
| 0.14 | 0.1                       |

## Details
[`PreferencesPlugin`] is responsible to define where the preferences are stored,
but sensible defaults are chosen for the user, like the storage path and format.

### Storage path
By default, the following paths are used to store the preferences

|Platform | Value                                                    | Example                                   |
| ------- | -------------------------------------------------------- | ----------------------------------------- |
| Native  | `dirs::preference_dir/{app_name}/preferences.toml`       | /home/alice/.config/MyApp/preferences.toml |
| Wasm    | `LocalStorage:{app_name}_preferences`                    | `LocalStorage:MyApp_preferences`          |

Final user can personalize this paths by using [`PreferencesPlugin::with_storage_type`] and use any convinient
value of [`PreferencesStorageType`].

### Storage format

By default, the following formats are used:

| Platform | Format      | Example                                     |
| -------- | ----------- | ------------------------------------------- |
| Native   | `toml`      | `[MyPluginPreferences]\nvalue = 3`          |
| Wasm     | `json`      | `{ "MyPluginPreferences": { "value": 3 } }` |

A different format (only for native) can be configured by implementing [`crate::storage::fs::FileStorageFormat`];

Go to the [`crate::storage::fs::FileStorageFormat`] documentation for more information on how to do it.


License: MIT OR Apache-2.0
