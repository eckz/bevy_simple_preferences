//! Contains [`PreferencesSerializableMap`] that allows preferences to be serialize and deserialize using reflection.
//!
use crate::registry::PreferencesRegistryData;
use crate::{PreferencesError, PreferencesType};
use bevy::prelude::*;
use bevy::reflect::serde::{TypedReflectDeserializer, TypedReflectSerializer};
use bevy::reflect::{TypeInfo, TypeRegistry, TypeRegistryArc};
use serde::de::{DeserializeSeed, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// A preferences serializable map that allows to serialize and deserialize preferences.
///
/// Preferences are strongly typed, and defined independently by any `Plugin` that needs persistent
/// preferences. Choice of serialization format and behavior is up to the application developer. The
/// preferences storage map simply provides a common API surface to consolidate preferences for all
/// plugins in one location.
///
/// Generally speaking neither final user nor crate developers need to use the [`PreferencesSerializableMap`] directly.
/// It will be used internally when using [`crate::PreferencesPlugin`] and [`crate::RegisterPreferencesExt::register_preferences`]
///
/// ### Usage
///
/// Preferences only require that a type being added derives [`Reflect`].
///
/// ```
/// # use bevy::reflect::Reflect;
/// #[derive(Reflect)]
/// struct MyPluginPreferences {
///     do_things: bool,
///     fizz_buzz_count: usize
/// }
/// ```
/// You can [`Self::get`] or [`Self::set`] preferences by accessing this type as a [`Resource`]
/// ```
/// # use bevy::prelude::*;
/// # use bevy_simple_preferences::*;
/// # use bevy_simple_preferences::serializable_map::PreferencesSerializableMap;
///
/// #[derive(Reflect)]
/// struct MyPluginPreferences {
///     do_things: bool,
///     fizz_buzz_count: usize
/// }
///
/// fn update(mut prefs: ResMut<PreferencesSerializableMap>) {
///     let settings = MyPluginPreferences {
///         do_things: false,
///         fizz_buzz_count: 9000,
///     };
///     prefs.set(settings);
///
///     // Accessing preferences only requires the type:
///     let mut new_settings = prefs.get::<MyPluginPreferences>();
///
///     // If you are updating an existing struct, all type information can be inferred:
///     new_settings = prefs.get();
/// }
/// ```
///
/// ### Serialization
///
/// The preferences map is build on `bevy_reflect`. This makes it possible to serialize preferences
/// into a dynamic structure, and deserialize it back into this map, while retaining a
/// strongly-typed API. It's not required that the inner types implement [`Serialize`], but
/// if they do, and they register it as a reflect type data, it will be used.
///
/// It implements [`serde::Serialize`] so it can be serialized using any format.
///
/// ```
/// # use bevy::reflect::{Reflect, TypeRegistryArc};
/// # use serde::Serialize;
///
/// # use bevy_simple_preferences::serializable_map::PreferencesSerializableMap;
///
/// # #[derive(Reflect)]
/// # struct MyPluginPreferences {
/// #     do_things: bool,
/// # }
///
/// # let register_type = TypeRegistryArc::default();
/// # register_type.write().register::<MyPluginPreferences>();
///
/// let mut map = PreferencesSerializableMap::empty(register_type);
/// map.set(MyPluginPreferences {
///     do_things: true
/// });
/// let contents = toml::to_string(&map).unwrap();
///
/// assert_eq!(&contents, "[MyPluginPreferences]\ndo_things = true\n");
/// ```
///

#[derive(Resource, TypePath)]
pub struct PreferencesSerializableMap {
    values: BTreeMap<String, Box<dyn Reflect>>,
    type_registry_arc: TypeRegistryArc,
}

impl Debug for PreferencesSerializableMap {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_map();
        for (key, value) in self.values.iter() {
            debug.entry(key, &value as &dyn Debug);
        }
        debug.finish()
    }
}

impl PartialEq for PreferencesSerializableMap {
    fn eq(&self, other: &Self) -> bool {
        let iter = self.values.iter().zip(other.values.iter());

        for ((k1, v1), (k2, v2)) in iter {
            if k1 != k2 {
                return false;
            }
            if !v1
                .reflect_partial_eq(v2.as_partial_reflect())
                .unwrap_or(false)
            {
                return false;
            }
        }

        true
    }
}

impl FromWorld for PreferencesSerializableMap {
    fn from_world(world: &mut World) -> Self {
        let type_registry_arc = world.resource::<AppTypeRegistry>().0.clone();
        Self::empty(type_registry_arc)
    }
}

fn effective_type_path<'a>(
    type_path: &'a str,
    short_type_path: &'a str,
    type_registry: &TypeRegistry,
) -> &'a str {
    if let Some(type_registration) = type_registry.get_with_short_type_path(short_type_path) {
        let registered_type_path = type_registration.type_info().type_path();
        assert_eq!(registered_type_path, type_path, "Short type path {short_type_path} corresponds to {registered_type_path}, not to {type_path}. Perhaps you missed to call register_preferences in a type");
        short_type_path
    } else if type_registry.get_with_type_path(type_path).is_some() {
        type_path
    } else {
        panic!("Type {type_path} ({short_type_path}) not registered in type_registry. Use register_preferences to register it")
    }
}

impl PreferencesSerializableMap {
    /// Creates a new empty storage map
    pub fn empty(type_registry_arc: TypeRegistryArc) -> Self {
        Self {
            values: BTreeMap::new(),
            type_registry_arc,
        }
    }

    /// Creates a storage map using the specified dynamic values.
    /// Values are converted into concrete types using the `FromReflect` implementation.
    pub fn from_dynamic_values(
        values: impl IntoIterator<Item = (String, Box<dyn PartialReflect>)>,
        type_registry_arc: TypeRegistryArc,
    ) -> Self {
        let values = values.into_iter();

        // This is scope is to make the borrow checker happy
        let values = {
            let type_registry = type_registry_arc.read();

            values
                .flat_map(|(key, value)| {
                    if let Some(type_info) = value.get_represented_type_info() {
                        let registry_data =
                            PreferencesRegistryData::from_type_info(&type_registry, type_info);

                        let new_value = registry_data.convert_to_concrete_type(value);

                        debug_assert!(!new_value.is_dynamic(), "Dynamic value generated");

                        Some((key, new_value))
                    } else {
                        // TODO: Should we panic instead?, or at least a warning
                        None
                    }
                })
                .collect()
        };

        Self {
            values,
            type_registry_arc,
        }
    }

    fn effective_type_path_from_type<T: TypePath>(&self) -> &'static str {
        let type_registry = self.type_registry_arc.read();
        effective_type_path(T::type_path(), T::short_type_path(), &type_registry)
    }

    fn effective_type_path_from_dyn<'a>(&self, value: &'a dyn PartialReflect) -> &'a str {
        let type_registry = self.type_registry_arc.read();
        effective_type_path(
            value.reflect_type_path(),
            value.reflect_short_type_path(),
            &type_registry,
        )
    }

    fn effective_type_path_from_type_info<'a>(&self, type_info: &'a TypeInfo) -> &'a str {
        let type_registry = self.type_registry_arc.read();
        effective_type_path(
            type_info.type_path(),
            type_info.type_path_table().short_path(),
            &type_registry,
        )
    }

    /// Set preferences entry of type `P`, potentially overwriting an existing entry.
    pub fn set<T: PreferencesType>(&mut self, value: T) {
        self.values.insert(
            self.effective_type_path_from_dyn(&value).to_owned(),
            Box::new(value),
        );
    }

    /// Set preferences entry from a boxed trait object of unknown type.
    pub fn set_dyn(&mut self, value: Box<dyn PartialReflect>) {
        if value.is_dynamic() {
            let type_info = value
                .get_represented_type_info()
                .expect("Provided dynamic value without a a represented type info");

            let key = self.effective_type_path_from_type_info(type_info);

            let type_registry = &self.type_registry_arc.read();
            let registry_data = PreferencesRegistryData::from_type_info(type_registry, type_info);

            let value = registry_data.convert_to_concrete_type(value);

            self.values.insert(key.to_owned(), value);
        } else {
            match value.try_into_reflect() {
                Ok(value) => {
                    self.values.insert(
                        self.effective_type_path_from_dyn(value.as_partial_reflect())
                            .to_owned(),
                        value,
                    );
                }
                Err(_) => {
                    panic!("PartialReflect cannot be converted into Reflect")
                }
            }
        }
    }

    /// Get preferences entry of type `T`.
    #[track_caller]
    pub fn get<T: PreferencesType>(&self) -> Option<&T> {
        self.values
            .get(self.effective_type_path_from_type::<T>())
            .and_then(|val| val.downcast_ref())
    }

    /// Get a mutable reference to a preferences entry of type `T`.
    #[track_caller]
    pub fn get_mut<T: PreferencesType>(&mut self) -> Option<&mut T> {
        let type_path = self.effective_type_path_from_type::<T>();
        self.values
            .get_mut(type_path)
            .and_then(|val| val.downcast_mut())
    }

    /// Iterator over all preference values as [`Reflect`] trait objects.
    pub fn iter_values(&self) -> impl Iterator<Item = &dyn Reflect> {
        self.values.values().map(|v| &**v)
    }

    /// Iterator over all preference entries as a tuple of ['&str'], [`&dyn Reflect`] objects.
    pub fn iter_entries(&mut self) -> impl Iterator<Item = (&str, &dyn Reflect)> {
        self.values.iter_mut().map(|(k, v)| (k.as_str(), &**v))
    }

    /// Remove and return an entry from the map, if it exists.
    pub fn take<T: PreferencesType>(&mut self) -> Option<T> {
        let type_path = self.effective_type_path_from_type::<T>();

        self.values
            .remove(type_path)
            .and_then(|val| val.downcast().ok())
            .map(|val| *val)
    }

    /// Returns if the map is empty
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns how many preferences are in the map
    pub fn len(&self) -> usize {
        self.values.len()
    }
}

impl Serialize for PreferencesSerializableMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let type_registry = self.type_registry_arc.read();
        let values = &self.values;

        let mut map_serializer = serializer.serialize_map(Some(values.len()))?;

        for (type_path, value) in values.iter() {
            let reflect_serializer =
                TypedReflectSerializer::new(value.as_partial_reflect(), &type_registry);
            map_serializer.serialize_entry(type_path, &reflect_serializer)?;
        }

        map_serializer.end()
    }
}

/// [`DeserializeSeed`] used to deserialize a [`PreferencesSerializableMap`].
/// Is required to deserialize this way in order to have a reference to
/// the [`TypeRegistry`].
///
/// Best way to get a new seed is to call [`PreferencesSerializableMap::deserialize_seed`]
pub struct PreferencesSerializableMapSeed {
    type_registry_arc: TypeRegistryArc,
}

impl PreferencesSerializableMapSeed {
    pub(crate) fn new(type_registry_arc: TypeRegistryArc) -> Self {
        Self { type_registry_arc }
    }
}

impl PreferencesSerializableMap {
    /// Creates an [`PreferencesSerializableMapSeed`] that allows deserialization of [`PreferencesSerializableMap`].
    pub fn deserialize_seed(type_registry_arc: TypeRegistryArc) -> PreferencesSerializableMapSeed {
        PreferencesSerializableMapSeed::new(type_registry_arc)
    }
}

impl<'de> DeserializeSeed<'de> for PreferencesSerializableMapSeed {
    type Value = PreferencesSerializableMap;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MapVisitor {
            type_registry_arc: TypeRegistryArc,
        }

        impl<'de> Visitor<'de> for MapVisitor {
            type Value = BTreeMap<String, Box<dyn PartialReflect>>;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let type_registry = self.type_registry_arc.read();

                let mut values = BTreeMap::new();

                while let Some(type_path) = map.next_key::<String>()? {
                    let type_registration = type_registry
                        .get_with_short_type_path(&type_path)
                        .or_else(|| type_registry.get_with_type_path(&type_path))
                        .ok_or_else(|| {
                            serde::de::Error::custom(PreferencesError::UnregisteredType(
                                type_path.clone(),
                            ))
                        })?;

                    let reflect_deserializer =
                        TypedReflectDeserializer::new(type_registration, &type_registry);

                    let value = map.next_value_seed(reflect_deserializer)?;

                    values.insert(type_path, value);
                }

                Ok(values)
            }
        }

        let type_registry_arc = self.type_registry_arc;
        let values = deserializer.deserialize_map(MapVisitor {
            type_registry_arc: type_registry_arc.clone(),
        })?;

        Ok(PreferencesSerializableMap::from_dynamic_values(
            values,
            type_registry_arc,
        ))
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::reflect::TypeRegistryArc;
    use std::fmt::Debug;

    use super::{PreferencesSerializableMap, PreferencesSerializableMapSeed};
    use crate::ReflectPreferences;
    use serde_test::{assert_ser_tokens, Token};

    #[derive(Reflect, Clone, PartialEq, Debug, Default)]
    #[reflect(Preferences)]
    struct Foo {
        field: u32,
        option: Option<u32>,
    }

    #[derive(Reflect, Clone, PartialEq, Debug, Default)]
    #[reflect(Preferences)]
    struct Bar(String);

    mod ambiguous {
        use crate::ReflectPreferences;
        use bevy::prelude::*;

        #[derive(Reflect, Clone, PartialEq, Default, Debug)]
        #[reflect(Preferences)]
        pub(super) struct Bar(pub(super) String);
    }

    fn get_registry() -> TypeRegistryArc {
        let type_registry = TypeRegistryArc::default();

        {
            let mut type_registry = type_registry.write();
            type_registry.register::<Foo>();
            type_registry.register::<Bar>();
        }

        type_registry
    }

    fn new_map() -> PreferencesSerializableMap {
        PreferencesSerializableMap::empty(get_registry())
    }

    #[test]
    fn test_sets_and_gets() {
        let mut map = new_map();
        map.set(Foo {
            field: 4,
            option: Some(2),
        });

        let value: &Foo = map.get().unwrap();

        assert_eq!(value.field, 4);
        assert_eq!(value.option, Some(2));
    }

    #[test]
    fn test_sets_and_gets_with_ambiguous() {
        let mut map = new_map();
        map.type_registry_arc.write().register::<ambiguous::Bar>();

        map.set(Bar("Bar".into()));
        map.set(ambiguous::Bar("ambiguousBar".into()));

        let bar: &Bar = map.get().unwrap();
        let ambiguous_bar: &ambiguous::Bar = map.get().unwrap();

        assert_eq!(bar.0, "Bar");
        assert_eq!(ambiguous_bar.0, "ambiguousBar");
    }

    #[test]
    fn test_sets_dyn_and_gets() {
        let mut map = new_map();
        map.set_dyn(
            Box::new(Foo {
                field: 4,
                option: Some(2),
            })
            .into_partial_reflect(),
        );

        let value: &Foo = map.get().unwrap();

        assert_eq!(value.field, 4);
        assert_eq!(value.option, Some(2));
    }

    #[test]
    fn test_take() {
        let mut map = new_map();
        map.set(Bar("H".into()));

        let taken_bar = map.take::<Bar>().unwrap();
        assert_eq!(taken_bar.0, "H");

        assert!(map.get::<Bar>().is_none());
        assert!(map.is_empty());
    }

    #[test]
    fn test_partial_eq() {
        let bar = Bar("reflect_partial_eq".into());
        let mut map_1 = new_map();
        map_1.set(bar.clone());

        let mut map_2 = new_map();
        map_2.set(bar.clone());

        assert_eq!(map_1, map_2);
    }

    #[test]
    fn test_apply_from_reflect_converts_dynamic_values() {
        let mut map = new_map();
        let foo = Foo {
            field: 3,
            option: None,
        };
        map.set_dyn(foo.clone_value());

        assert_eq!(map.get::<Foo>(), Some(&foo));
    }

    #[test]
    fn test_ser_empty() {
        let map = new_map();

        assert_ser_tokens(&map, &[Token::Map { len: Some(0) }, Token::MapEnd]);
    }

    #[test]
    fn test_ser_foo() {
        let mut map = new_map();
        map.set(Foo {
            field: 3,
            option: None,
        });

        assert_ser_tokens(
            &map,
            &[
                Token::Map { len: Some(1) },
                Token::Str("Foo"),
                Token::Struct {
                    name: "Foo",
                    len: 2,
                },
                Token::Str("field"),
                Token::U32(3),
                Token::Str("option"),
                Token::None,
                Token::StructEnd,
                Token::MapEnd,
            ],
        );
    }

    #[test]
    fn test_ser_bar() {
        let mut map = new_map();
        map.set(Bar("Hello".to_string()));

        assert_ser_tokens(
            &map,
            &[
                Token::Map { len: Some(1) },
                Token::Str("Bar"),
                Token::NewtypeStruct { name: "Bar" },
                Token::Str("Hello"),
                Token::MapEnd,
            ],
        );
    }

    #[test]
    fn test_ser_bar_with_ambiguous() {
        let mut map = new_map();
        map.type_registry_arc.write().register::<ambiguous::Bar>();

        map.set(Bar("Bar".to_string()));
        map.set(ambiguous::Bar("ambiguousBar".to_string()));

        assert_ser_tokens(
            &map,
            &[
                Token::Map { len: Some(2) },
                Token::Str("bevy_simple_preferences::serializable_map::tests::Bar"),
                Token::NewtypeStruct { name: "Bar" },
                Token::Str("Bar"),
                Token::Str("bevy_simple_preferences::serializable_map::tests::ambiguous::Bar"),
                Token::NewtypeStruct { name: "Bar" },
                Token::Str("ambiguousBar"),
                Token::MapEnd,
            ],
        );
    }

    #[test]
    fn test_ser_foo_bar() {
        let mut map = new_map();
        map.set(Foo {
            field: 3,
            option: None,
        });
        map.set(Bar("Hello".to_string()));

        assert_ser_tokens(
            &map,
            &[
                Token::Map { len: Some(2) },
                // Bar
                Token::Str("Bar"),
                Token::NewtypeStruct { name: "Bar" },
                Token::Str("Hello"),
                // Foo
                Token::Str("Foo"),
                Token::Struct {
                    name: "Foo",
                    len: 2,
                },
                Token::Str("field"),
                Token::U32(3),
                Token::Str("option"),
                Token::None,
                Token::StructEnd,
                Token::MapEnd,
            ],
        );
    }

    #[track_caller]
    pub fn assert_de_seed_tokens<'de, T>(
        value: &<T as serde::de::DeserializeSeed<'de>>::Value,
        seed: T,
        tokens: impl IntoIterator<Item = serde_assert::Token> + Clone,
    ) where
        T: serde::de::DeserializeSeed<'de>,
        T::Value: PartialEq + Debug,
    {
        let mut de = serde_assert::Deserializer::builder(tokens).build();
        match T::deserialize(seed, &mut de) {
            Ok(v) => {
                assert_eq!(v, *value);
            }
            Err(e) => panic!("tokens failed to deserialize: {}", e),
        };
    }

    #[test]
    fn test_de_foo() {
        use serde_assert::Token;

        let mut map = new_map();
        map.set(Foo {
            field: 3,
            option: None,
        });

        let deserializer = PreferencesSerializableMapSeed {
            type_registry_arc: map.type_registry_arc.clone(),
        };

        // It takes the default value for Bar
        assert_de_seed_tokens(
            &map,
            deserializer,
            [
                Token::Map { len: Some(1) },
                // Foo
                Token::Str("Foo".into()),
                Token::Struct {
                    name: "Foo",
                    len: 2,
                },
                Token::Str("field".into()),
                Token::U32(3),
                Token::Str("option".into()),
                Token::None,
                Token::StructEnd,
                Token::MapEnd,
            ],
        );
    }
}
