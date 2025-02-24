use crate::serializable_map::PreferencesSerializableMap;
use crate::storage::{PreferencesStorage, PreferencesStorageResource};
use std::sync::Arc;

use crate::{PreferencesSet, PreferencesStorageType};
use bevy::app::MainScheduleOrder;
use bevy::ecs::component::Tick;
use bevy::ecs::schedule::{ExecutorKind, ScheduleLabel};
use bevy::ecs::system::SystemChangeTick;
use bevy::prelude::*;
use bevy::reflect::TypeRegistryArc;

use std::time::Duration;

/// Struct responsible for deciding where to store the preferences.
#[derive(Clone)]
struct PreferencesStorageBuilder {
    pub app_name: Option<&'static str>,
    pub org_name: Option<&'static str>,
    pub storage_type: PreferencesStorageType,
}

impl PreferencesStorageBuilder {
    fn full_app_name(&self) -> Option<String> {
        match (self.app_name, self.org_name) {
            (None, None) => None,
            (Some(app_name), Some(org_unit)) => Some(format!("{}.{}", org_unit, app_name)),
            (Some(app_name), None) => Some(app_name.into()),
            _ => None,
        }
    }

    #[cfg(not(target_family = "wasm"))]
    fn get_storage_parent_path_and_format(
        &self,
    ) -> Option<(std::path::PathBuf, crate::storage::fs::FileStorageFormatFns)> {
        let file_storage_path = self.storage_type.file_storage_path()?;
        let file_storage_format = self.storage_type.file_storage_format()?;
        let app_name = self.full_app_name()?;
        Some((file_storage_path.join(app_name), file_storage_format))
    }

    fn create_storage(&self) -> Option<PreferencesStorageResource> {
        if let PreferencesStorageType::Custom(custom) = &self.storage_type {
            return Some(PreferencesStorageResource::from_arc(custom.clone()));
        }
        self.create_native_storage()
    }

    #[cfg(not(target_family = "wasm"))]
    fn create_native_storage(&self) -> Option<PreferencesStorageResource> {
        let storage =
            self.get_storage_parent_path_and_format()
                .and_then(|(parent_path, format)| {
                    crate::storage::fs::FileStorage::new_with_format(parent_path, format).ok()
                });

        storage.map(PreferencesStorageResource::new)
    }

    #[cfg(target_family = "wasm")]
    fn create_native_storage(&self) -> Option<PreferencesStorageResource> {
        let app_name = self.full_app_name()?;
        let storage = self
            .storage_type
            .gloo_storage(format!("{app_name}_preferences"))?;
        Some(PreferencesStorageResource::new(storage))
    }
}

/// Schedule label that is executed before `PreStartup`
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LoadPreferences;

/// Preferences Plugin that configures how preferences are stored.
/// It should be only added by final applications, not libraries, since it's not their responsibility.
///
/// If you want to persist the preferences, you need to include an `app_name` that
/// uniquely represents your application
/// ```
/// # use bevy::prelude::*;
/// # use bevy_simple_preferences::PreferencesPlugin;
/// App::new()
///         .add_plugins(MinimalPlugins)
///         .add_plugins(PreferencesPlugin::persisted_with_app_name("MyPreferencesAppName"))
/// # ;
/// ```
/// In case you don't want to store any preferences on disk
/// ```
/// # use bevy::prelude::*;
/// # use bevy_simple_preferences::PreferencesPlugin;
/// App::new()
///         .add_plugins(MinimalPlugins)
///         .add_plugins(PreferencesPlugin::with_no_persistence())
/// # ;
/// ```
pub struct PreferencesPlugin {
    /// Name of the application, required unless `storage_type` is [`PreferencesStorageType::NoStorage`]
    pub app_name: Option<&'static str>,
    /// Organization name, optional. It will be used to construct the final file name.
    pub org_name: Option<&'static str>,
    /// Type of storage, [`PreferencesStorageType::DefaultStorage`] by default.
    pub storage_type: PreferencesStorageType,
}

impl PreferencesPlugin {
    /// Creates a [`PreferencesPlugin`] with specified app name and default storage.
    ///
    /// |Platform | Value                                                    | Example                                   |
    /// | ------- | -------------------------------------------------------- | ----------------------------------------- |
    /// | Native  | `{dirs::preference_dir}/{app_name}/preferences.toml`     | /home/alice/.config/MyApp/preferences.toml |
    /// | Wasm    | `LocalStorage:{app_name}_preferences`                    | `LocalStorage:MyApp_preferences`          |
    ///
    pub fn persisted_with_app_name(app_name: &'static str) -> Self {
        Self {
            app_name: Some(app_name),
            org_name: None,
            storage_type: Default::default(),
        }
    }

    /// Creates a [`PreferencesPlugin`] that doesn't store preferences anywhere
    /// Take into consideration that this is exactly the same as not adding the Plugin.
    pub fn with_no_persistence() -> Self {
        Self {
            app_name: None,
            org_name: None,
            storage_type: PreferencesStorageType::NoStorage,
        }
    }

    /// Specifies the storage type
    pub fn with_storage_type(mut self, storage_type: PreferencesStorageType) -> Self {
        self.storage_type = storage_type;
        self
    }

    /// Specifies a fully custom Preferences Storage
    /// ```
    /// # use bevy::prelude::*;
    /// # use bevy_simple_preferences::PreferencesPlugin;
    /// # use bevy_simple_preferences::serializable_map::{PreferencesSerializableMap, PreferencesSerializableMapSeed};
    /// # use bevy_simple_preferences::storage::PreferencesStorage;
    ///
    /// struct MyCustomStorage;
    ///
    /// impl PreferencesStorage for MyCustomStorage {
    /// fn load_preferences(&self, deserialize_seed: PreferencesSerializableMapSeed) -> Result<PreferencesSerializableMap, bevy_simple_preferences::PreferencesError> {
    ///         todo!()
    ///     }
    ///
    /// fn save_preferences(&self, map: &PreferencesSerializableMap) -> Result<(), bevy_simple_preferences::PreferencesError> {
    ///         todo!()
    ///     }
    /// }
    ///
    /// let app = App::new()
    ///     .add_plugins(MinimalPlugins)
    ///     .add_plugins(PreferencesPlugin::with_custom_storage(MyCustomStorage));
    ///
    /// ```
    ///
    pub fn with_custom_storage(storage: impl PreferencesStorage) -> Self {
        Self {
            app_name: None,
            org_name: None,
            storage_type: PreferencesStorageType::Custom(Arc::new(storage)),
        }
    }

    fn storage_builder(&self) -> PreferencesStorageBuilder {
        PreferencesStorageBuilder {
            app_name: self.app_name,
            org_name: self.org_name,
            storage_type: self.storage_type.clone(),
        }
    }
}

impl Plugin for PreferencesPlugin {
    fn build(&self, app: &mut App) {
        {
            let world = app.world_mut();

            let mut main_schedule_order = world.resource_mut::<MainScheduleOrder>();
            main_schedule_order
                .startup_labels
                .insert(0, LoadPreferences.intern());

            let mut schedule = Schedule::new(LoadPreferences);
            schedule.set_executor_kind(ExecutorKind::Simple);
            world.add_schedule(schedule);
        }

        app.add_event::<PreferencesSaved>()
            .add_systems(
                LoadPreferences,
                load_preferences(self.storage_builder()).in_set(PreferencesSet::Load),
            )
            .configure_sets(
                Last,
                PreferencesSet::SetReflectMapValues.before(PreferencesSet::Save),
            )
            // We need to hook on Last to catch AppExit event correctly
            .add_systems(
                Last,
                save_preferences.in_set(PreferencesSet::Save).run_if(
                    resource_exists::<PreferencesStorageResource>
                        .and(resource_exists::<PreferencesSerializableMap>),
                ),
            );
    }
}

fn load_preferences(
    storage_builder: PreferencesStorageBuilder,
) -> impl Fn(Commands, Res<AppTypeRegistry>) {
    move |mut commands: Commands, app_type_registry: Res<AppTypeRegistry>| {
        let type_registry_arc = TypeRegistryArc::clone(&app_type_registry);
        let Some(storage) = storage_builder.create_storage() else {
            return;
        };

        let seed = PreferencesSerializableMap::deserialize_seed(type_registry_arc.clone());

        let preferences = match storage.load_preferences(seed) {
            Ok(preferences) => preferences,
            #[cfg(not(target_family = "wasm"))]
            Err(crate::PreferencesError::IoError(io_error)) => {
                if io_error.kind() != std::io::ErrorKind::NotFound {
                    error!("I/O Error loading preferences: {io_error}");
                }
                PreferencesSerializableMap::empty(type_registry_arc)
            }
            #[cfg(target_family = "wasm")]
            Err(crate::PreferencesError::GlooError(
                gloo_storage::errors::StorageError::KeyNotFound(_),
            )) => PreferencesSerializableMap::empty(type_registry_arc),
            Err(err) => {
                error!("Unknown Error loading preferences: {err:?}");
                PreferencesSerializableMap::empty(type_registry_arc)
            }
        };

        commands.insert_resource(preferences);
        commands.insert_resource(storage);
    }
}

/// Event triggered every time the preferences are saved to the background
#[derive(Event, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct PreferencesSaved;

#[allow(clippy::too_many_arguments)]
pub fn save_preferences(
    time: Res<Time<Real>>,
    preferences: Res<PreferencesSerializableMap>,
    storage: Res<PreferencesStorageResource>,
    mut last_save_tick: Local<Option<Tick>>,
    mut last_save_time: Local<Duration>,
    system_change_tick: SystemChangeTick,
    mut app_exit: EventReader<AppExit>,
    mut preferences_saved: EventWriter<PreferencesSaved>,
) {
    let last_save_tick = last_save_tick.get_or_insert_with(|| system_change_tick.last_run());

    let is_modified = preferences
        .last_changed()
        .is_newer_than(*last_save_tick, system_change_tick.this_run());

    let mut should_trigger_save = is_modified;

    if is_modified {
        let duration_since_last_save = time.elapsed() - *last_save_time;
        if duration_since_last_save.as_secs() < 1 {
            should_trigger_save = false;
        }
    }

    if !app_exit.is_empty() {
        app_exit.clear();
        if is_modified {
            should_trigger_save = true;
        }
    }

    if should_trigger_save {
        if let Err(err) = storage.save_preferences(&preferences) {
            error!("Error saving preferences: {err}");
        } else {
            preferences_saved.send_default();
        }
        *last_save_tick = preferences.last_changed();
        *last_save_time = time.elapsed();
    }
}
