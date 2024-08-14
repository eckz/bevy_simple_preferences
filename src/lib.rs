//! # About
//! Bevy Preferences Simple Abstraction
//!
//! Provides a simple Preferences API for Bevy that allows different crates to
//! relay in a simple preferences abstraction giving the final app crate full control on how
//! and where to store the preferences.
//!
//! This crate is heavily based on the [Bevy Preferences API proposal](https://github.com/bevyengine/bevy/issues/13311).
//!
//! # Examples
//!
//! You first need to define a struct / enum that represents your preferences.
//! ```
//!# use bevy::prelude::*;
//!# use bevy_simple_preferences::*;
//! #[derive(Reflect, Default)]
//! struct ExampleSettings {
//!     field_u32: u32,
//!     some_str: String,
//!     some_option: Option<String>,
//! }
//! ```
//!
//! And in your code, you just need to add the [`PreferencesPlugin`] and call [`RegisterPreferencesExt::register_preferences`]
//! ```
//!# use bevy::prelude::*;
//!# use bevy_simple_preferences::*;
//!# #[derive(Reflect, Default)]
//!# struct ExampleSettings {
//!#     field_u32: u32,
//!# }
//! App::new()
//!     .add_plugins(PreferencesPlugin::persisted_with_app_name("YourAppName"))
//!     .register_preferences::<ExampleSettings>();
//! ```
//!
//! If you are implementing a library, you don't need to add [`PreferencesPlugin`], since it's
//! up the final user to add it themselves. But you can rely on the [`Preferences`] param to read and write preferences,
//! even if the final user does not add the plugin.
//!
//! If the final user does not add the [`PreferencesPlugin`], the effect is that preferences are simply not stored.
//!
//! ```
//!# use bevy::prelude::*;
//!# use bevy_simple_preferences::*;
//!# #[derive(Reflect, Default)]
//!# struct MyCratePreferences {
//!#     some_field: i32,
//!# }
//!# struct MyCratePlugin;
//!
//! impl Plugin for MyCratePlugin {
//!     fn build(&self, app: &mut App) {
//!         app.register_preferences::<MyCratePreferences>()
//!             .add_systems(Update, |my_preferences: Preferences<MyCratePreferences>| {
//!                 // Do stuff with your preferences
//!                 assert!(my_preferences.some_field >= 0);
//!             });
//!         ;
//!     }
//! }
//! # App::new().add_plugins(MyCratePlugin).run();
//! ```
//!
//! # Supported Bevy Versions
//!| Bevy | `bevy_simple_preferences` |
//!| ---- | -----------------------   |
//!| 0.14 | 0.1                       |
//!
//! # Details
//! [`PreferencesPlugin`] is responsible to define where the preferences are stored,
//! but sensible defaults are chosen for the user, like the storage path and format.
//!
//! ## Storage path
//! By default, the following paths are used to store the preferences
//!
//!|Platform | Value                                                    | Example                                   |
//!| ------- | -------------------------------------------------------- | ----------------------------------------- |
//!| Native  | `dirs::preference_dir/{app_name}/preferences.toml`       | /home/alice/.config/MyApp/preferences.toml |
//!| Wasm    | `LocalStorage:{app_name}_preferences`                    | `LocalStorage:MyApp_preferences`          |
//!
//! Final user can personalize this paths by using [`PreferencesPlugin::with_storage_type`] and use any convinient
//! value of [`PreferencesStorageType`].
//!
//! ## Storage format
//!
//! By default, the following formats are used:
//!
//!| Platform | Format      | Example                                     |
//!| -------- | ----------- | ------------------------------------------- |
//!| Native   | `toml`      | `[MyPluginPreferences]\nvalue = 3`          |
//!| Wasm     | `json`      | `{ "MyPluginPreferences": { "value": 3 } }` |
//!
//! A different format (only for native) can be configured by implementing [`crate::storage::fs::FileStorageFormat`];
//!
//! Go to the [`crate::storage::fs::FileStorageFormat`] documentation for more information on how to do it.
//!
use bevy::prelude::*;
use bevy::reflect::FromType;
use std::sync::Arc;
use thiserror::Error;

pub mod serializable_map;

mod plugin;
mod registry;
mod resource;
pub mod storage;

pub use crate::plugin::PreferencesPlugin;
pub use crate::registry::RegisterPreferencesExt;
pub use crate::resource::{Preferences, PreferencesResource};

use crate::storage::PreferencesStorage;

#[cfg(not(target_family = "wasm"))]
use crate::storage::fs::{DefaultFileStorageFormat, FileStorageFormatFns};

/// Possible errors that could happen during either loading or saving preferences
#[derive(Error, Debug)]
pub enum PreferencesError {
    #[cfg(not(target_family = "wasm"))]
    /// Input/Output error while reading or writing to disk.
    #[error("I/O Error: {0}")]
    IoError(#[from] std::io::Error),

    /// Error while deserializing the preferences
    #[error("Deserialization Error: {0}")]
    DeserializationError(Box<dyn std::error::Error + Send + Sync>),

    /// Error while serializing the preferences
    #[cfg(not(target_family = "wasm"))]
    #[error("Serialization Error: {0}")]
    SerializationError(Box<dyn std::error::Error + Send + Sync>),

    /// While serializing or deserializing, a type has not been registered in the [`bevy::reflect::TypeRegistry`].
    #[error("Type {0} not registered")]
    UnregisteredType(String),

    #[cfg(target_family = "wasm")]
    /// An error has occurred while storing in either LocalStorage or Session storage.
    #[error("Error getting from storage: {0}")]
    GlooError(#[from] gloo_storage::errors::StorageError),
}

pub(crate) type Result<T> = std::result::Result<T, PreferencesError>;

/// Type of storage that will be used.
/// Most use cases are covered by [`PreferencesStorageType::DefaultStorage`] and [`PreferencesStorageType::NoStorage`]
#[derive(Clone, Default)]
pub enum PreferencesStorageType {
    /// No storage of any type is used
    NoStorage,
    /// Default storage is used. In native, a default preferences path, and toml file format will be used.
    /// In wasm, `LocalStorage` will be used
    /// See [`PreferencesPlugin`] for more info.
    #[default]
    DefaultStorage,
    /// Fully custom Preferences storage
    Custom(Arc<dyn PreferencesStorage>),
    #[cfg(not(target_family = "wasm"))]
    /// File system storage using the default format (toml) in a specific parent directory
    /// The parent directory will get preferences.toml appended to it.
    /// Useful for tests where the parent directory can be a temporary folder.
    FileSystemWithParentDirectory(std::path::PathBuf),
    #[cfg(not(target_family = "wasm"))]
    /// Store using the default file system paths, but using the specified format.
    /// Useful if you want to only modify the format in which the files are written.
    FileSystemWithFormat(FileStorageFormatFns),
    #[cfg(not(target_family = "wasm"))]
    /// Specified parent path and file format. If you want full control on where the files are stored
    /// and in which format they are written.
    FileSystemWithParentDirectoryAndFormat(std::path::PathBuf, FileStorageFormatFns),

    #[cfg(target_family = "wasm")]
    /// Preferences will be stored in the browser local storage
    LocalStorage,
    #[cfg(target_family = "wasm")]
    /// Preferences will be stored in the browser session storage
    SessionStorage,
}

impl PreferencesStorageType {
    #[cfg(not(target_family = "wasm"))]
    fn file_storage_path(&self) -> Option<std::path::PathBuf> {
        match self {
            PreferencesStorageType::NoStorage => None,
            PreferencesStorageType::Custom(_) => None,
            PreferencesStorageType::DefaultStorage
            | PreferencesStorageType::FileSystemWithFormat(_) => {
                Some(dirs::preference_dir().expect("Cannot resolve preference_dir"))
            }
            PreferencesStorageType::FileSystemWithParentDirectory(path)
            | PreferencesStorageType::FileSystemWithParentDirectoryAndFormat(path, _) => {
                Some(path.clone())
            }
        }
    }

    #[cfg(not(target_family = "wasm"))]
    fn file_storage_format(&self) -> Option<FileStorageFormatFns> {
        match self {
            PreferencesStorageType::NoStorage => None,
            PreferencesStorageType::Custom(_) => None,
            PreferencesStorageType::DefaultStorage
            | PreferencesStorageType::FileSystemWithParentDirectory(_) => {
                Some(FileStorageFormatFns::from_format::<DefaultFileStorageFormat>())
            }
            PreferencesStorageType::FileSystemWithFormat(format)
            | PreferencesStorageType::FileSystemWithParentDirectoryAndFormat(_, format) => {
                Some(*format)
            }
        }
    }

    #[cfg(target_family = "wasm")]
    fn gloo_storage(
        &self,
        preferences_key: impl Into<String>,
    ) -> Option<storage::gloo::GlooStorage> {
        match self {
            PreferencesStorageType::NoStorage => None,
            PreferencesStorageType::Custom(_) => None,
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

/// System Set used by [`PreferencesPlugin`]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, SystemSet)]
pub enum PreferencesSet {
    /// System set used to load preferences, it happens before [`PreStartup`].
    Load,
    /// System set used to create resources of type [`crate::resource::Preferences`]
    AssignResources,
    /// Assign values into [`crate::serializable_map::PreferencesSerializableMap`].
    SetReflectMapValues,
    /// System set used to save preferences, it happens on [`Last`].
    Save,
}

/// Marker trait to indicate that the type can work as Preferences.
/// Is automatically implemented for anything that implements [`FromReflect`] and [`TypePath`] .
///
#[diagnostic::on_unimplemented(
    message = "`{Self}` can not be used as preferences",
    note = "consider annotating `{Self}` with `#[derive(Reflect)]`"
)]
pub trait PreferencesType: FromReflect + TypePath {}

impl<T> PreferencesType for T where T: FromReflect + TypePath {}

/// Represents the type data registration of a [`PreferencesType`] type.
/// It doesn't contain any data, so it only serves as a marker type to make sure
/// a type has been registered as Preferences.
#[derive(Clone)]
pub struct ReflectPreferences;

impl<T: PreferencesType> FromType<T> for ReflectPreferences {
    fn from_type() -> Self {
        Self
    }
}
