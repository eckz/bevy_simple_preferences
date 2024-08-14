use crate::serializable_map::{PreferencesSerializableMap, PreferencesSerializableMapSeed};
use bevy::log::*;
use serde::de::DeserializeSeed;

use crate::storage::PreferencesStorage;
use crate::Result;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
enum GlooStorageType {
    Local,
    Session,
}

pub(crate) struct GlooStorage {
    storage_type: GlooStorageType,
    preferences_key: String,
    load_preferences:
        fn(&str, PreferencesSerializableMapSeed) -> Result<PreferencesSerializableMap>,
    save_preferences: fn(&str, map: &PreferencesSerializableMap) -> Result<()>,
}

impl GlooStorage {
    fn new<T: gloo_storage::Storage>(
        storage_type: GlooStorageType,
        preferences_key: impl Into<String>,
    ) -> Self {
        Self {
            storage_type,
            preferences_key: preferences_key.into(),
            load_preferences: load_preferences::<T>,
            save_preferences: save_preferences::<T>,
        }
    }

    pub fn local(preferences_key: impl Into<String>) -> Self {
        Self::new::<gloo_storage::LocalStorage>(GlooStorageType::Local, preferences_key)
    }

    pub fn session(preferences_key: impl Into<String>) -> Self {
        Self::new::<gloo_storage::SessionStorage>(GlooStorageType::Session, preferences_key)
    }
}

impl PreferencesStorage for GlooStorage {
    fn load_preferences(
        &self,
        seed: PreferencesSerializableMapSeed,
    ) -> Result<PreferencesSerializableMap> {
        let preferences = (self.load_preferences)(&self.preferences_key, seed)?;
        info!("Loaded preferences from {:?}Storage", self.storage_type);
        Ok(preferences)
    }

    fn save_preferences(&self, map: &PreferencesSerializableMap) -> Result<()> {
        (self.save_preferences)(&self.preferences_key, map)?;
        debug!("Saved preferences on {:?}Storage", self.storage_type);
        Ok(())
    }
}

fn load_preferences<T: gloo_storage::Storage>(
    key: &str,
    seed: PreferencesSerializableMapSeed,
) -> Result<PreferencesSerializableMap> {
    Ok(T::get_by_seed(key, seed)?)
}

fn save_preferences<T: gloo_storage::Storage>(
    key: &str,
    map: &PreferencesSerializableMap,
) -> Result<()> {
    T::set(key, map)?;
    Ok(())
}

trait GlooStorageExt {
    fn get_by_seed<S>(
        key: impl AsRef<str>,
        seed: S,
    ) -> gloo_storage::Result<<S as DeserializeSeed<'static>>::Value>
    where
        S: for<'de> DeserializeSeed<'de>;
}

impl<T> GlooStorageExt for T
where
    T: gloo_storage::Storage,
{
    fn get_by_seed<S>(
        key: impl AsRef<str>,
        seed: S,
    ) -> gloo_storage::Result<<S as DeserializeSeed<'static>>::Value>
    where
        S: for<'de> DeserializeSeed<'de>,
    {
        let key = key.as_ref();
        let item_string = T::raw()
            .get_item(key)
            .expect("unreachable: get_item does not throw an exception")
            .ok_or_else(|| gloo_storage::errors::StorageError::KeyNotFound(key.to_string()))?;

        let mut deserializer = serde_json::de::Deserializer::from_reader(item_string.as_bytes());
        let item = seed.deserialize(&mut deserializer)?;

        Ok(item)
    }
}
