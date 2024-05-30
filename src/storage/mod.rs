#[cfg(not(target_family = "wasm"))]
pub(crate) mod fs;

#[cfg(target_family = "wasm")]
pub(crate) mod gloo;

use bevy::prelude::*;
use std::ops::Deref;

use crate::map::{PreferencesMap, PreferencesMapDeserializeSeed};
use crate::Result;

pub trait PreferencesStorageImpl: Send + Sync + 'static {
    fn load_preferences(
        &self,
        deserialize_seed: PreferencesMapDeserializeSeed,
    ) -> Result<PreferencesMap>;

    fn save_preferences(&self, map: &PreferencesMap) -> Result<()>;
}

#[derive(Resource)]
pub struct PreferencesStorage(Box<dyn PreferencesStorageImpl>);

impl PreferencesStorage {
    pub fn new(storage: impl PreferencesStorageImpl) -> Self {
        Self(Box::new(storage))
    }
}

impl Deref for PreferencesStorage {
    type Target = dyn PreferencesStorageImpl;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
