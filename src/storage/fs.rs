//! Provides all necessary to reads and writes preferences to disk.
//! Custom serializations can be provided by implementing [`FileStorageFormat`].
//!
//! A default `toml` format is provided by the [`TomlFormat`] struct.

use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use bevy::log::*;
use serde::de::DeserializeSeed;
use tempfile::NamedTempFile;

use crate::reflect_map::{PreferencesReflectMap, PreferencesReflectMapDeserializeSeed};
use crate::storage::PreferencesStorage;
use crate::{PreferencesError, Result};

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

/// Trait used to serialize or deserialize from disk.
/// By default, only a toml format is provided, but you may provide any other format
/// by just implementing this trait
///
/// ```
/// # use serde::de::DeserializeSeed;
/// # use bevy_simple_preferences::{PreferencesError};
/// # use bevy_simple_preferences::storage::fs::FileStorageFormat;
/// # use bevy_simple_preferences::reflect_map::{PreferencesReflectMap, PreferencesReflectMapDeserializeSeed};
///
/// struct MyJsonFormat;
/// impl FileStorageFormat for MyJsonFormat {
///    fn serialize_preferences(map: &PreferencesReflectMap) -> Result<String, PreferencesError> {
///         serde_json::to_string(map).map_err(|json_err| PreferencesError::SerializationError(json_err.into()))
///     }
///
///    fn deserialize_preferences(deserialize_seed: PreferencesReflectMapDeserializeSeed, input: &str) -> Result<PreferencesReflectMap, PreferencesError> {
///         let mut deserializer = serde_json::de::Deserializer::from_str(input);
///         deserialize_seed.deserialize(&mut deserializer).map_err(|json_err| PreferencesError::DeserializationError(json_err.into()))
///     }
///
///     fn file_name() -> &'static str {
///         "preferences.json"
///     }
/// }
/// ```
pub trait FileStorageFormat {
    /// Serialize the preferences map into a String
    fn serialize_preferences(map: &PreferencesReflectMap) -> Result<String>;

    /// Deserialize the preferences map from a string
    fn deserialize_preferences(
        deserialize_seed: PreferencesReflectMapDeserializeSeed,
        input: &str,
    ) -> Result<PreferencesReflectMap>;

    /// Default file name, e.g: `preferences.json`
    fn file_name() -> &'static str;
}

/// Virtual table that represents a single [`FileStorageFormat`] type.
#[derive(Copy, Clone)]
pub struct FileStorageFormatFns {
    serialize_preferences: fn(&PreferencesReflectMap) -> Result<String>,
    deserialize_preferences:
        fn(PreferencesReflectMapDeserializeSeed, input: &str) -> Result<PreferencesReflectMap>,
    file_name: &'static str,
}

impl FileStorageFormatFns {
    /// Creates a [`FileStorageFormatFns`] from a type that implements [`FileStorageFormat`].
    pub fn from_format<F: FileStorageFormat>() -> Self {
        Self {
            serialize_preferences: F::serialize_preferences,
            deserialize_preferences: F::deserialize_preferences,
            file_name: F::file_name(),
        }
    }
}

pub(crate) type DefaultFileStorageFormat = TomlFormat;

/// Default format using `toml`.
pub struct TomlFormat;

impl FileStorageFormat for TomlFormat {
    fn serialize_preferences(map: &PreferencesReflectMap) -> Result<String> {
        toml::to_string_pretty(map).map_err(|err| PreferencesError::SerializationError(err.into()))
    }

    fn deserialize_preferences(
        deserialize_seed: PreferencesReflectMapDeserializeSeed,
        input: &str,
    ) -> Result<PreferencesReflectMap> {
        deserialize_seed
            .deserialize(toml::de::Deserializer::new(input))
            .map_err(|err| PreferencesError::DeserializationError(err.into()))
    }

    fn file_name() -> &'static str {
        "preferences.toml"
    }
}

pub(crate) struct FileStorage {
    path: PathBuf,
    format: FileStorageFormatFns,
}

impl FileStorage {
    pub(crate) fn new_with_format(
        parent_path: impl Into<PathBuf>,
        format: FileStorageFormatFns,
    ) -> Result<Self> {
        let parent_path = parent_path.into();
        std::fs::create_dir_all(&parent_path)?;

        let path = parent_path.join(format.file_name);

        Ok(Self { path, format })
    }

    #[cfg(test)]
    pub(crate) fn new_from_format<F: FileStorageFormat>(
        parent_path: impl Into<PathBuf>,
    ) -> Result<Self> {
        Self::new_with_format(parent_path, FileStorageFormatFns::from_format::<F>())
    }

    #[cfg(test)]
    pub(crate) fn new(parent_path: impl Into<PathBuf>) -> Result<Self> {
        Self::new_from_format::<TomlFormat>(parent_path)
    }
}

impl PreferencesStorage for FileStorage {
    fn load_preferences(
        &self,
        deserialize_seed: PreferencesReflectMapDeserializeSeed,
    ) -> Result<PreferencesReflectMap> {
        let contents = std::fs::read_to_string(&self.path)?;
        info!("Loading preferences from {}", self.path.display());
        (self.format.deserialize_preferences)(deserialize_seed, &contents)
    }

    fn save_preferences(&self, map: &PreferencesReflectMap) -> Result<()> {
        debug!("Storing preferences to {}", self.path.display());

        let output = (self.format.serialize_preferences)(map)?;
        write_atomically(&self.path, output)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::reflect::TypeRegistryArc;
    use tempfile::TempDir;

    use super::{FileStorage, PreferencesStorage};
    use crate::reflect_map::PreferencesReflectMap;
    use crate::ReflectPreferences;

    #[derive(Reflect, PartialEq, Debug, Default)]
    #[reflect(Preferences)]
    struct Foo {
        size: usize,
        option: Option<usize>,
    }

    #[derive(Reflect, PartialEq, Debug, Default)]
    #[reflect(Preferences)]
    struct Bar(String);

    fn get_registry() -> TypeRegistryArc {
        let type_registry_arc = TypeRegistryArc::default();

        {
            let mut type_registry = type_registry_arc.write();

            type_registry.register::<Foo>();
            type_registry.register::<Bar>();
        }

        type_registry_arc
    }

    #[test]
    fn fs_writes_and_reads_from_disk() {
        let temp_dir = TempDir::new().unwrap();
        let registry = get_registry();

        let storage = FileStorage::new(temp_dir.path()).unwrap();

        let mut written_map = PreferencesReflectMap::empty(registry.clone());

        written_map.set(Foo {
            size: 3,
            option: Some(27),
        });
        written_map.set(Bar("Bar".into()));

        storage.save_preferences(&written_map).unwrap();

        let read_map = storage
            .load_preferences(PreferencesReflectMap::deserialize_seed(registry))
            .unwrap();

        assert_eq!(read_map, written_map);
    }
}
