use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use bevy::log::*;
use serde::de::DeserializeSeed;

use tempfile::NamedTempFile;

use crate::map::{PreferencesMap, PreferencesMapDeserializeSeed};
use crate::storage::PreferencesStorageImpl;
use crate::Result;

pub(crate) fn write_atomically(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> io::Result<()> {
    let path = path.as_ref();
    let mut temp_file = NamedTempFile::new_in(path.parent().expect("path has no parent"))?;

    temp_file.write_all(contents.as_ref())?;
    temp_file.as_file().sync_all()?;

    temp_file.persist(path)?;

    Ok(())
}

pub(crate) struct FileStorage {
    path: PathBuf,
}

impl FileStorage {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(Self { path })
    }
}

impl PreferencesStorageImpl for FileStorage {
    fn load_preferences(
        &self,
        deserialize_seed: PreferencesMapDeserializeSeed,
    ) -> Result<PreferencesMap> {
        let contents = std::fs::read_to_string(&self.path)?;
        info!("Loading preferences from {}", self.path.display());
        let toml_deserializer = toml::de::Deserializer::new(&contents);
        Ok(deserialize_seed.deserialize(toml_deserializer)?)
    }

    fn save_preferences(&self, map: &PreferencesMap) -> Result<()> {
        let output = toml::to_string(map)?;
        debug!("Storing preferences to {}", self.path.display());
        write_atomically(&self.path, output)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::reflect::TypeRegistryArc;
    use tempfile::NamedTempFile;

    use crate::map::PreferencesMap;

    use super::{FileStorage, PreferencesStorageImpl};

    #[derive(Reflect, PartialEq, Debug)]
    struct Foo {
        size: usize,
        option: Option<usize>,
    }

    #[derive(Reflect, PartialEq, Debug)]
    struct Bar(String);

    fn get_registry() -> TypeRegistryArc {
        let type_registry_arc = TypeRegistryArc::default();

        {
            let mut type_registry = type_registry_arc.write();

            type_registry.register::<Foo>();
            type_registry.register::<Bar>();
            type_registry.register::<Option<usize>>();
        }

        type_registry_arc
    }

    #[test]
    fn write_and_read() {
        let temp_file = NamedTempFile::new().unwrap();
        let registry = get_registry();

        let storage = FileStorage::new(temp_file.path()).unwrap();

        let mut map = PreferencesMap::new(registry.clone());

        map.set(Foo {
            size: 3,
            option: Some(27),
        });
        map.set(Bar("Bar".into()));

        storage.save_preferences(&map).unwrap();

        let new_map = storage
            .load_preferences(PreferencesMap::deserialize_seed(registry))
            .unwrap();

        assert_eq!(new_map, map);
    }
}
