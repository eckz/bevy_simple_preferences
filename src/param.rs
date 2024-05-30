use crate::map::PreferencesMap;
use bevy::ecs::system::{Command, SystemParam};
use bevy::prelude::*;
use std::ops::{Deref, DerefMut};

struct SetPreferencesCommand<T>(T);

impl<T: Reflect> Command for SetPreferencesCommand<T> {
    fn apply(self, world: &mut World) {
        let mut preferences = world.get_resource_mut::<PreferencesMap>().unwrap();
        preferences.set(self.0);
    }
}

pub struct MutatePreferencesGuard<'a, 'w, 's, T: Clone + Reflect> {
    value: &'a mut T,
    commands: &'a mut Commands<'w, 's>,
}

impl<'a, 'w, 's, T: Clone + Reflect> Deref for MutatePreferencesGuard<'a, 'w, 's, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'a, 'w, 's, T: Clone + Reflect> DerefMut for MutatePreferencesGuard<'a, 'w, 's, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
    }
}

impl<'a, 'w, 's, T: Clone + Reflect> Drop for MutatePreferencesGuard<'a, 'w, 's, T> {
    fn drop(&mut self) {
        self.commands.add(SetPreferencesCommand(self.value.clone()));
    }
}

#[derive(SystemParam)]
pub struct Preferences<'w, 's, T: Send + Sync + 'static> {
    preferences: ResMut<'w, PreferencesMap>,
    last_value: Local<'s, Option<T>>,
    commands: Commands<'w, 's>,
}

impl<'w, 's, T: Reflect + TypePath + Send + Sync + 'static> Deref for Preferences<'w, 's, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.preferences.get()
    }
}

impl<'w, 's, T: Clone + Reflect + TypePath + Send + Sync + 'static> Preferences<'w, 's, T> {
    pub fn mutate<'a>(&'a mut self) -> MutatePreferencesGuard<'a, 'w, 's, T> {
        MutatePreferencesGuard {
            value: self.last_value.insert(self.preferences.get::<T>().clone()),
            commands: &mut self.commands,
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use std::sync::{Arc, RwLock};
    use tempfile::tempdir;

    use crate::map::PreferencesMap;
    use crate::param::Preferences;
    use crate::{PreferencesPlugin, RegisterPreferences};

    #[derive(Reflect, Debug, Default, Clone)]
    struct MyPreferences {
        some_value: u32,
    }

    #[test]
    fn preferences_plugin_reads_from_disk() {
        let temp_dir = tempdir().unwrap();

        let mut app = App::new();
        app.register_preferences::<MyPreferences>();

        app.add_plugins(
            PreferencesPlugin::with_app_name("PreferencesTest")
                .with_storage_parent_directory(temp_dir.path()),
        );

        let shared_value: Arc<RwLock<u32>> = Default::default();
        let system_shared_value = shared_value.clone();
        app.add_systems(
            Update,
            move |mut preferences: Preferences<MyPreferences>| {
                preferences.mutate().some_value = *system_shared_value.read().unwrap();
            },
        );

        *shared_value.write().unwrap() = 8;

        app.update();

        // We expect the preferences to have been updated
        {
            let preferences = app.world.get_resource::<PreferencesMap>().unwrap();

            let my_preferences = preferences.get::<MyPreferences>();

            assert_eq!(my_preferences.some_value, 8);
        }
    }
}
