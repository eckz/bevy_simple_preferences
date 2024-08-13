//! Contains everything related to store and load preferences.
//! Two main submodules are present, depending on the platform.
//!
//! For native, the submodule `fs` is present, and allows load and storing from disk.
//! For web, the submodule `gloo` is present, and allows load and storing from local and session storage.
#[cfg(not(target_family = "wasm"))]
pub mod fs;

#[cfg(target_family = "wasm")]
pub(crate) mod gloo;

use crate::reflect_map::{PreferencesReflectMap, PreferencesReflectMapDeserializeSeed};
use crate::Result;
use bevy::prelude::*;
use std::ops::Deref;
use std::sync::Arc;

/// Trait used to represent how preferences are loaded and saved.
/// Final applications can have custom storages by implementing this trait.
pub trait PreferencesStorage: Send + Sync + 'static {
    /// Loads the preferences using the [`PreferencesReflectMapDeserializeSeed`] passed as a value.
    fn load_preferences(
        &self,
        deserialize_seed: PreferencesReflectMapDeserializeSeed,
    ) -> Result<PreferencesReflectMap>;

    /// Saves the preferences
    fn save_preferences(&self, map: &PreferencesReflectMap) -> Result<()>;
}

/// Represents the current Preferences storage used.
/// If no storage is used, this Resources will not be present at all.
#[derive(Resource)]
pub struct PreferencesStorageResource(Arc<dyn PreferencesStorage>);

impl PreferencesStorageResource {
    pub(crate) fn new(storage: impl PreferencesStorage) -> Self {
        Self(Arc::new(storage))
    }

    pub(crate) fn from_arc(storage: Arc<dyn PreferencesStorage>) -> Self {
        Self(storage)
    }
}

impl Deref for PreferencesStorageResource {
    type Target = dyn PreferencesStorage;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
