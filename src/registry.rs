use crate::reflect_map::PreferencesReflectMap;
use crate::resource::PreferencesResource;
use crate::{PreferencesSet, PreferencesType, ReflectPreferences};
use bevy::prelude::*;
use bevy::reflect::{GetTypeRegistration, TypeInfo, TypeRegistration, TypeRegistry};
use std::any::TypeId;
use std::sync::Mutex;

pub(crate) struct PreferencesRegistryData<'a> {
    type_id: TypeId,
    _preferences: &'a ReflectPreferences,
    from_reflect: &'a ReflectFromReflect,
    default: Option<&'a ReflectDefault>,
}

#[cold]
fn preferences_registry_fail(full_path: &str, short_path: &str, msg: &str) -> ! {
    panic!("Type {full_path} {msg}.\nYou can try to call `.register_preferences::<{short_path}>()`\n or `.register_type::<{short_path}>()` with the type annotated `#[reflect(Preferences)]`")
}

impl<'a> PreferencesRegistryData<'a> {
    pub fn from_type_info(type_registry: &'a TypeRegistry, type_info: &TypeInfo) -> Self {
        let Some(type_registration) = type_registry.get(type_info.type_id()) else {
            preferences_registry_fail(
                type_info.type_path(),
                type_info.type_path_table().short_path(),
                "is not registered",
            );
        };
        Self::from_type_registration(type_registration)
    }

    pub fn from_type_registration(type_registration: &'a TypeRegistration) -> Self {
        #[cold]
        fn fail(type_info: &TypeInfo, msg: &str) -> ! {
            preferences_registry_fail(
                type_info.type_path(),
                type_info.type_path_table().short_path(),
                msg,
            )
        }

        let type_info = type_registration.type_info();

        let Some(_preferences) = type_registration.data() else {
            fail(type_info, "does not implement Preferences");
        };

        let Some(from_reflect) = type_registration.data() else {
            fail(type_info, "does not implement FromReflect");
        };

        let default = type_registration.data();

        let type_id = type_info.type_id();

        Self {
            type_id,
            _preferences,
            from_reflect,
            default,
        }
    }

    pub fn apply_from_reflect(&self, value: Box<dyn Reflect>) -> Box<dyn Reflect> {
        if value.as_any().type_id() == self.type_id {
            return value;
        }
        let type_path = value.reflect_type_path();

        self.from_reflect
            .from_reflect(value.as_reflect())
            .or_else(|| {
                debug!(
                    "FromReflect did not work for type :{type_path}\nValue:{:#?}",
                    &value
                );

                if let Some(reflect_default) = self.default {
                    let mut default_value = reflect_default.default();
                    match default_value.try_apply(value.as_reflect()) {
                        Ok(_) => Some(default_value),
                        Err(err) => {
                            error!("try_apply did not work for type: {type_path}: {err}");
                            None
                        }
                    }
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                // TODO: Return None
                panic!("Error using ReflectFromReflect:\nTypePath: {type_path}\nValue: {value:#?}")
            })
    }
}

/// Extension for App to allow registering preference types.
pub trait RegisterPreferencesExt {
    /// Registers a type as a [`PreferencesType`] type.
    /// Uses [`Default::default`] as the default value.
    #[track_caller]
    fn register_preferences<T>(&mut self) -> &mut Self
    where
        T: GetTypeRegistration + PreferencesType + Default;

    /// Registers a type as a [`PreferencesType`] type.
    /// Uses the specified value if nothing is loaded from disk.
    #[track_caller]
    fn register_preferences_with_default_value<T>(&mut self, default_value: T) -> &mut Self
    where
        T: GetTypeRegistration + PreferencesType;
}

impl RegisterPreferencesExt for App {
    #[track_caller]
    fn register_preferences<T>(&mut self) -> &mut Self
    where
        T: GetTypeRegistration + PreferencesType + Default,
    {
        self.register_type::<T>()
            .register_type_data::<T, ReflectPreferences>()
            .register_type_data::<T, ReflectFromReflect>()
            .register_type_data::<T, ReflectDefault>();

        self.register_type::<PreferencesResource<T>>();

        self.add_plugins(RegisteredPreferencesPlugin::<T>::new(Default::default()));
        self
    }

    #[track_caller]
    fn register_preferences_with_default_value<T>(&mut self, default_value: T) -> &mut Self
    where
        T: GetTypeRegistration + PreferencesType,
    {
        self.register_type::<T>()
            .register_type_data::<T, ReflectPreferences>()
            .register_type_data::<T, ReflectFromReflect>();

        self.register_type::<PreferencesResource<T>>();

        self.add_plugins(RegisteredPreferencesPlugin::new(default_value));
        self
    }
}

struct RegisteredPreferencesPlugin<T> {
    default_value: Mutex<Option<T>>,
}

impl<T> RegisteredPreferencesPlugin<T> {
    pub fn new(value: T) -> Self {
        Self {
            default_value: Mutex::new(Some(value)),
        }
    }
}

impl<T> Plugin for RegisteredPreferencesPlugin<T>
where
    T: PreferencesType + GetTypeRegistration,
{
    fn build(&self, app: &mut App) {
        let initial_value = {
            let mut lock = self.default_value.try_lock().unwrap();
            lock.take().expect("Cannot build Plugin more than once")
        };
        app.register_type::<PreferencesResource<T>>()
            .add_systems(
                PreStartup,
                Self::assign_initial_value(initial_value).in_set(PreferencesSet::AssignResources),
            )
            .add_systems(
                Last,
                Self::set_reflect_map_value
                    .in_set(PreferencesSet::SetReflectMapValues)
                    .run_if(
                        preferences_changed::<T>.and_then(resource_exists::<PreferencesReflectMap>),
                    ),
            );
    }
}

// Detect if preferences have changed
fn preferences_changed<T: PreferencesType>(
    preferences: Option<Res<PreferencesResource<T>>>,
) -> bool {
    preferences.is_some_and(|res| res.is_changed())
}

impl<T> RegisteredPreferencesPlugin<T>
where
    T: PreferencesType,
{
    fn assign_initial_value(
        default_value: T,
    ) -> impl FnMut(Commands, Option<ResMut<PreferencesReflectMap>>) {
        let mut default_value = Some(default_value);
        move |mut commands, storage_map| {
            let stored_value: Option<T> =
                storage_map.and_then(|mut storage_map| storage_map.take());

            let value = stored_value.unwrap_or_else(|| {
                default_value
                    .take()
                    .expect("This system should not be executed more than once")
            });
            commands.insert_resource(PreferencesResource::new(value));
        }
    }

    fn set_reflect_map_value(
        value: Res<PreferencesResource<T>>,
        mut storage_map: ResMut<PreferencesReflectMap>,
    ) {
        let cloned_value = T::from_reflect(&**value).expect("Error while trying to clone value");
        storage_map.set(cloned_value);
    }
}

#[cfg(test)]
mod tests {
    use crate::reflect_map::PreferencesReflectMap;
    use crate::{Preferences, PreferencesSet, RegisterPreferencesExt};
    use bevy::prelude::*;

    #[derive(Reflect)]
    struct MyPreferences {
        value: &'static str,
    }

    impl Default for MyPreferences {
        fn default() -> Self {
            Self {
                value: "DefaultValue",
            }
        }
    }

    #[test]
    fn test_register_preferences_using_default() {
        App::new()
            .register_preferences::<MyPreferences>()
            .add_systems(Update, |pref: Preferences<MyPreferences>| {
                assert_eq!(pref.value, "DefaultValue");
            })
            .run();
    }

    #[test]
    fn test_register_preferences_using_specified_default_value() {
        App::new()
            .register_preferences_with_default_value(MyPreferences {
                value: "SpecifiedValue",
            })
            .add_systems(Update, |pref: Preferences<MyPreferences>| {
                assert_eq!(pref.value, "SpecifiedValue");
            })
            .run();
    }

    #[test]
    fn test_register_preferences_takes_value_from_reflect_map() {
        let mut app = App::new();
        app.register_preferences::<MyPreferences>();

        let mut reflect_map = {
            let type_registry_arc = app.world().resource::<AppTypeRegistry>().0.clone();
            PreferencesReflectMap::empty(type_registry_arc)
        };

        reflect_map.set(MyPreferences {
            value: "ValueFromPreferencesReflect",
        });

        app.insert_resource(reflect_map)
            .add_systems(Update, |pref: Preferences<MyPreferences>| {
                assert_eq!(pref.value, "ValueFromPreferencesReflect");
            })
            .run();
    }

    #[test]
    fn test_register_preferences_saves_back_to_reflect_map() {
        App::new()
            .register_preferences::<MyPreferences>()
            .init_resource::<PreferencesReflectMap>()
            .add_systems(Update, |mut pref: Preferences<MyPreferences>| {
                pref.value = "ValueFromSystem";
            })
            .add_systems(
                Last,
                (|map: Res<PreferencesReflectMap>| {
                    assert_eq!(map.get::<MyPreferences>().unwrap().value, "ValueFromSystem");
                })
                .after(PreferencesSet::SetReflectMapValues),
            )
            .run();
    }
}
