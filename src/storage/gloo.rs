use crate::map::PreferencesMapDeserializeSeed;
use bevy::log::*;
use serde::de::DeserializeSeed;

use crate::storage::PreferencesStorageImpl;
use crate::{PreferencesMap, Result};

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
enum GlooStorageType {
    Local,
    Session,
}

pub(crate) struct GlooStorage {
    storage_type: GlooStorageType,
    preferences_key: String,
    load_preferences: fn(&str, PreferencesMapDeserializeSeed) -> Result<PreferencesMap>,
    save_preferences: fn(&str, map: &PreferencesMap) -> Result<()>,
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

impl PreferencesStorageImpl for GlooStorage {
    fn load_preferences(&self, seed: PreferencesMapDeserializeSeed) -> Result<PreferencesMap> {
        let preferences = (self.load_preferences)(&self.preferences_key, seed)?;
        info!("Loaded preferences from {:?}Storage", self.storage_type);
        Ok(preferences)
    }

    fn save_preferences(&self, map: &PreferencesMap) -> Result<()> {
        (self.save_preferences)(&self.preferences_key, map)?;
        debug!("Saved preferences on {:?}Storage", self.storage_type);
        Ok(())
    }
}

fn load_preferences<T: gloo_storage::Storage>(
    key: &str,
    seed: PreferencesMapDeserializeSeed,
) -> Result<PreferencesMap> {
    Ok(T::get_by_seed(key, seed)?)
}

fn save_preferences<T: gloo_storage::Storage>(key: &str, map: &PreferencesMap) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use crate::storage::gloo::GlooStorage;
    use crate::storage::PreferencesStorageImpl;
    use crate::PreferencesMap;
    use bevy::prelude::Reflect;
    use bevy::reflect::TypeRegistryArc;
    use std::borrow::Cow;
    use wasm_bindgen_test::{wasm_bindgen_test as test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);

    #[derive(Reflect, PartialEq, Debug)]
    struct Foo {
        a: u32,
        b: u32,
    }

    #[derive(Reflect, PartialEq, Debug)]
    struct Bar(Cow<'static, str>);

    impl Bar {
        fn new(value: impl Into<Cow<'static, str>>) -> Self {
            Self(value.into())
        }
    }

    fn get_registry() -> TypeRegistryArc {
        let type_registry_arc = TypeRegistryArc::default();

        {
            let mut type_registry = type_registry_arc.write();

            type_registry.register::<Foo>();
            type_registry.register::<Bar>();
            type_registry.register::<Cow<'static, str>>();
        }

        type_registry_arc
    }

    const PREFERENCES_KEY: &str = "TEST_PREFERENCES_KEY";

    fn write_and_read_storage(storage: GlooStorage) {
        let registry = get_registry();

        let mut map = PreferencesMap::new(registry.clone());

        map.set(Foo { a: 1, b: 2 });
        map.set(Bar::new("BarValue"));

        storage.save_preferences(&map).unwrap();

        let new_map = storage
            .load_preferences(PreferencesMap::deserialize_seed(registry))
            .unwrap();

        assert_eq!(new_map, map);
    }

    #[test]
    fn write_and_read_from_local_storage() {
        write_and_read_storage(GlooStorage::local(PREFERENCES_KEY));
    }

    #[test]
    fn write_and_read_from_session_storage() {
        write_and_read_storage(GlooStorage::session(PREFERENCES_KEY));
    }
}
