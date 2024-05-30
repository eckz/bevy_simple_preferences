use crate::map::PreferencesMap;
use crate::storage::PreferencesStorage;
use crate::{PreferencesRegistry, PreferencesSet, PreferencesStorageType};
use bevy::prelude::*;
use bevy::reflect::TypeRegistryArc;

#[derive(Clone, Debug)]
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
    fn get_storage_final_path(&self) -> Option<std::path::PathBuf> {
        let preferences_dir = self.storage_type.storage_path()?;
        let app_name = self.full_app_name()?;

        let final_path = preferences_dir.join(app_name).join("preferences.toml");
        Some(final_path)
    }

    #[cfg(not(target_family = "wasm"))]
    fn create_storage(&self) -> Option<PreferencesStorage> {
        let storage = self
            .get_storage_final_path()
            .and_then(|final_path| crate::storage::fs::FileStorage::new(final_path).ok());

        storage.map(PreferencesStorage::new)
    }

    #[cfg(target_family = "wasm")]
    fn create_storage(&self) -> Option<PreferencesStorage> {
        let app_name = self.full_app_name()?;
        let storage = self
            .storage_type
            .gloo_storage(format!("{app_name}_preferences"))?;
        Some(PreferencesStorage::new(storage))
    }
}

#[derive(Debug, Default)]
pub struct PreferencesPlugin {
    pub app_name: Option<&'static str>,
    pub org_name: Option<&'static str>,
    pub storage_type: PreferencesStorageType,
}

impl PreferencesPlugin {
    pub fn with_app_name(app_name: &'static str) -> Self {
        Self {
            app_name: Some(app_name),
            ..Default::default()
        }
    }

    pub fn without_storage() -> Self {
        Self {
            storage_type: PreferencesStorageType::NoStorage,
            ..Default::default()
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn with_storage_parent_directory(
        mut self,
        storage_dir: impl Into<std::path::PathBuf>,
    ) -> Self {
        self.storage_type = PreferencesStorageType::ParentDirectory(storage_dir.into());
        self
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
        app.register_type::<PreferencesMap>();

        app.add_systems(
            PreStartup,
            load_preferences(self.storage_builder())
                .in_set(PreferencesSet::Load)
                .run_if(resource_exists::<PreferencesRegistry>),
        );

        app.add_systems(
            PostUpdate,
            save_preferences.in_set(PreferencesSet::Save).run_if(
                resource_exists::<PreferencesStorage>.and_then(
                    resource_changed::<PreferencesMap>
                        .and_then(not(resource_added::<PreferencesMap>)),
                ),
            ),
        );
    }
}

fn load_preferences(
    storage_builder: PreferencesStorageBuilder,
) -> impl Fn(Commands, Res<AppTypeRegistry>, Res<PreferencesRegistry>) {
    move |mut commands: Commands,
          app_type_registry: Res<AppTypeRegistry>,
          preferences_registry: Res<PreferencesRegistry>| {
        let type_registry = TypeRegistryArc::clone(&app_type_registry);
        let Some(storage) = storage_builder.create_storage() else {
            let mut preferences = PreferencesMap::new(type_registry);
            preferences_registry.add_defaults(&mut preferences);
            commands.insert_resource(preferences);
            return;
        };

        let seed = PreferencesMap::deserialize_seed(type_registry.clone());

        let mut preferences = match storage.load_preferences(seed) {
            Ok(preferences) => preferences,
            #[cfg(not(target_family = "wasm"))]
            Err(crate::PreferencesError::IoError(io_error)) => {
                if io_error.kind() != std::io::ErrorKind::NotFound {
                    error!("I/O Error loading preferences: {io_error}");
                }
                PreferencesMap::new(type_registry)
            }
            #[cfg(target_family = "wasm")]
            Err(crate::PreferencesError::GlooError(
                gloo_storage::errors::StorageError::KeyNotFound(_),
            )) => PreferencesMap::new(type_registry),
            Err(err) => {
                error!("Error loading preferences: {err:?}");
                PreferencesMap::new(type_registry)
            }
        };

        preferences_registry.apply_from_reflect_and_add_defaults(&mut preferences);

        commands.insert_resource(preferences);
        commands.insert_resource(storage);
    }
}

pub fn save_preferences(preferences: Res<PreferencesMap>, storage: Res<PreferencesStorage>) {
    if let Err(err) = storage.save_preferences(&preferences) {
        error!("Error saving preferences: {err}");
    }
}

#[cfg(not(target_family = "wasm"))]
#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::utils::HashMap;
    use rand::random;

    use crate::map::{PreferencesMap, PreferencesMapDeserializeSeed};
    use crate::storage::PreferencesStorage;
    use crate::{PreferencesPlugin, PreferencesRegistry, RegisterPreferences};

    #[derive(Reflect, Debug, Default)]
    struct MyPreferences {
        some_string: String,
        some_option: u32,
    }

    #[derive(Reflect, Debug, Default)]
    struct MyPreferencesMap {
        some_map: HashMap<String, MyPreferences>,
    }

    fn load_preferences_from_world(
        storage: &PreferencesStorage,
        world: &World,
    ) -> crate::Result<PreferencesMap> {
        let type_registry_arc = world.get_resource::<AppTypeRegistry>().unwrap().0.clone();
        let seed = PreferencesMapDeserializeSeed::new(type_registry_arc);
        storage.load_preferences(seed)
    }

    #[test]
    fn preferences_plugin_reads_from_disk() {
        let temp_dir = tempfile::tempdir().unwrap();

        let mut app = App::new();
        let type_registry = app.world.resource::<AppTypeRegistry>().0.clone();

        app.register_preferences::<MyPreferences>();

        let preferences_plugin = PreferencesPlugin::with_app_name("PreferencesTest")
            .with_storage_parent_directory(temp_dir.path());
        let storage_builder = preferences_plugin.storage_builder();

        let some_option = random();

        // First, we store the initial preferences
        {
            let storage = storage_builder.create_storage().unwrap();
            let mut preferences = PreferencesMap::new(type_registry.clone());

            let my_settings: &mut MyPreferences = preferences.get_mut_or_default();
            my_settings.some_string = "TestReadFromDisk".into();
            my_settings.some_option = some_option;

            storage.save_preferences(&preferences).unwrap();
        }

        app.add_plugins(preferences_plugin);

        app.update();
        // We expect the preferences to have been loaded from disk
        {
            let preferences = app.world.get_resource::<PreferencesMap>().unwrap();

            let my_preferences = preferences.get::<MyPreferences>();

            assert_eq!(my_preferences.some_string, "TestReadFromDisk");
            assert_eq!(my_preferences.some_option, some_option);
        }
    }

    #[test]
    fn preferences_plugin_saves_to_disk() {
        let temp_dir = tempfile::tempdir().unwrap();

        let mut app = App::new();

        app.register_preferences::<MyPreferences>();

        let preferences_plugin = PreferencesPlugin::with_app_name("PreferencesTest")
            .with_storage_parent_directory(temp_dir.path());
        let storage_builder = preferences_plugin.storage_builder();

        let some_option = random();

        app.add_plugins(preferences_plugin);

        app.update();
        {
            let mut preferences = app.world.get_resource_mut::<PreferencesMap>().unwrap();

            let my_settings: &mut MyPreferences = preferences.get_mut();
            my_settings.some_string = "TestWriteToDisk".into();
            my_settings.some_option = some_option;
        }

        // This should save to disk
        app.update();

        // We verify the preferences were stored to disk
        {
            let storage = storage_builder.create_storage().unwrap();
            let mut preferences = load_preferences_from_world(&storage, &app.world).unwrap();
            let registry = app.world.get_resource::<PreferencesRegistry>().unwrap();
            registry.apply_from_reflect_and_add_defaults(&mut preferences);

            let my_preferences = preferences.get::<MyPreferences>();
            assert_eq!(my_preferences.some_string, "TestWriteToDisk");
            assert_eq!(my_preferences.some_option, some_option);
        }
    }
}
