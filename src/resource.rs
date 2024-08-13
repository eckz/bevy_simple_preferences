use crate::PreferencesType;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::ops::{Deref, DerefMut};

/// Stores the specific values of a preferences of type T.
/// Is preferable to use [`Preferences`] system param, but if you need fine-grained
/// control over the change detection, you might need to use the Resource directly
#[derive(Resource, Deref, DerefMut, Reflect)]
#[reflect(Resource)]
pub struct PreferencesResource<T: PreferencesType>(T);

impl<T: PreferencesType> PreferencesResource<T> {
    pub(crate) fn new(value: T) -> Self {
        Self(value)
    }
}

/// System param that allows to read and write preferences of a type `T`.
/// ```
/// # use bevy::prelude::*;
/// # use bevy_simple_preferences::*;
/// # #[derive(Reflect, Default)]
/// # struct MyPreferences;
/// let app = App::new()
///     .register_preferences::<MyPreferences>()
///     .add_systems(Update, |mut preferences: Preferences<MyPreferences>| {
///         // Do stuff with your preferences
///     })
///     .run();
///
/// ```
#[derive(SystemParam)]
pub struct Preferences<'w, T: PreferencesType> {
    resource: ResMut<'w, PreferencesResource<T>>,
}

impl<'w, T> Deref for Preferences<'w, T>
where
    T: PreferencesType,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.resource
    }
}

impl<'w, T> DerefMut for Preferences<'w, T>
where
    T: PreferencesType,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.resource
    }
}

impl<'w, T> PartialEq<T> for Preferences<'w, T>
where
    T: PreferencesType + PartialEq,
{
    fn eq(&self, other: &T) -> bool {
        (**self.resource).eq(other)
    }
}
