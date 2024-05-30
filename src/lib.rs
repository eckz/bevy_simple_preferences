use bevy::prelude::*;
use bevy::reflect::{FromType, GetTypeRegistration, TypePathTable};
use bevy::utils::HashMap;
use thiserror::Error;

mod map;
pub mod param;
mod plugin;
mod storage;

pub use crate::map::PreferencesMap;
pub use crate::param::Preferences;
pub use crate::plugin::PreferencesPlugin;

#[derive(Error, Debug)]
pub enum PreferencesError {
    #[cfg(not(target_family = "wasm"))]
    #[error("I/O Error")]
    IoError(#[from] std::io::Error),

    #[cfg(not(target_family = "wasm"))]
    #[error("Toml deserialization Error")]
    TomlDeError(#[from] toml::de::Error),

    #[cfg(not(target_family = "wasm"))]
    #[error("Toml serialization Error")]
    TomlSerError(#[from] toml::ser::Error),

    #[error("Type {0} not registered")]
    UnregisteredType(String),

    #[cfg(target_family = "wasm")]
    #[error("Error getting from storage")]
    GlooError(#[from] gloo_storage::errors::StorageError),
}

pub(crate) type Result<T> = std::result::Result<T, PreferencesError>;

#[derive(Resource, Default)]
pub(crate) struct PreferencesRegistry {
    from_reflect_map: HashMap<String, ReflectFromReflect>,
    default_map: Vec<(
        TypePathTable,
        Box<dyn Fn() -> Box<dyn Reflect + 'static> + 'static + Send + Sync>,
    )>,
}

impl PreferencesRegistry {
    fn apply_from_reflect(&self, preferences: &mut PreferencesMap) {
        let mut new_values = Vec::new();

        for value in preferences.iter_values() {
            if let Some(type_info) = value.get_represented_type_info() {
                let type_path = type_info.type_path();

                let reflect_from_reflect =
                    self.from_reflect_map.get(type_path).unwrap_or_else(|| {
                        panic!("Type {type_path} not registered using `register_preferences`")
                    });

                let new_value = reflect_from_reflect
                    .from_reflect(value)
                    .expect("Error using ReflectFromReflect");

                debug_assert!(!new_value.is_dynamic(), "Dynamic value generated");

                new_values.push(new_value);
            }
        }

        for new_value in new_values {
            preferences.set_dyn(new_value);
        }
    }

    fn add_defaults(&self, preferences: &mut PreferencesMap) {
        for (type_path, default_fn) in &self.default_map {
            preferences.set_if_missing(type_path, default_fn);
        }
    }

    fn apply_from_reflect_and_add_defaults(&self, preferences: &mut PreferencesMap) {
        self.apply_from_reflect(preferences);
        self.add_defaults(preferences);
    }
}

pub trait RegisterPreferences {
    fn register_preferences<T>(&mut self) -> &mut Self
    where
        T: GetTypeRegistration + TypePath + FromReflect + Default,
    {
        self.register_preferences_with(T::default)
    }

    fn register_preferences_with_default<T>(&mut self, default: T) -> &mut Self
    where
        T: GetTypeRegistration + TypePath + FromReflect + Clone,
    {
        self.register_preferences_with(move || default.clone())
    }

    fn register_preferences_with<T, F>(&mut self, default_fn: F) -> &mut Self
    where
        T: GetTypeRegistration + TypePath + FromReflect,
        F: Fn() -> T + Send + Sync + 'static;
}

impl RegisterPreferences for App {
    fn register_preferences_with<T, F>(&mut self, default_fn: F) -> &mut Self
    where
        T: GetTypeRegistration + TypePath + FromReflect,
        F: Fn() -> T + Send + Sync + 'static,
    {
        self.register_type::<T>();

        if !self.world.contains_resource::<PreferencesRegistry>() {
            self.world.init_resource::<PreferencesRegistry>();
        }
        let mut registry = self
            .world
            .get_resource_mut::<PreferencesRegistry>()
            .unwrap();

        registry.from_reflect_map.insert(
            T::type_path().into(),
            <ReflectFromReflect as FromType<T>>::from_type(),
        );
        registry.default_map.push((
            TypePathTable::of::<T>(),
            Box::new(move || Box::new(default_fn())),
        ));

        self
    }
}

#[derive(Clone, Debug, Default)]
pub enum PreferencesStorageType {
    NoStorage,
    #[default]
    DefaultStorage,
    #[cfg(not(target_family = "wasm"))]
    ParentDirectory(std::path::PathBuf),

    #[cfg(target_family = "wasm")]
    LocalStorage,
    #[cfg(target_family = "wasm")]
    SessionStorage,
}

impl PreferencesStorageType {
    #[cfg(not(target_family = "wasm"))]
    fn storage_path(&self) -> Option<std::path::PathBuf> {
        match self {
            PreferencesStorageType::NoStorage => None,
            PreferencesStorageType::DefaultStorage => {
                Some(dirs::preference_dir().expect("Cannot resolve preference_dir"))
            }
            PreferencesStorageType::ParentDirectory(path) => Some(path.clone()),
        }
    }

    #[cfg(target_family = "wasm")]
    fn gloo_storage(
        &self,
        preferences_key: impl Into<String>,
    ) -> Option<storage::gloo::GlooStorage> {
        match self {
            PreferencesStorageType::NoStorage => None,
            PreferencesStorageType::DefaultStorage => {
                Some(storage::gloo::GlooStorage::local(preferences_key))
            }

            PreferencesStorageType::LocalStorage => {
                Some(storage::gloo::GlooStorage::local(preferences_key))
            }
            PreferencesStorageType::SessionStorage => {
                Some(storage::gloo::GlooStorage::session(preferences_key))
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, SystemSet)]
pub enum PreferencesSet {
    Load,
    Save,
}
