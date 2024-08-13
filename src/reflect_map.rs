//! Contains [`PreferencesReflectMap`] that allows preferences to be serialize and deserialize using reflection.
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

/// A preferences reflect map that allows to serialize and deserialize preferences.
///
/// Preferences are strongly typed, and defined independently by any `Plugin` that needs persistent
/// preferences. Choice of serialization format and behavior is up to the application developer. The
/// preferences storage map simply provides a common API surface to consolidate preferences for all
/// plugins in one location.
///
/// Generally speaking neither final user nor crate developers need to use the [`PreferencesReflectMap`] directly.
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
/// # use bevy_simple_preferences::reflect_map::PreferencesReflectMap;
///
/// #[derive(Reflect)]
/// struct MyPluginPreferences {
///     do_things: bool,
///     fizz_buzz_count: usize
/// }
///
/// fn update(mut prefs: ResMut<PreferencesReflectMap>) {
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
/// # use bevy_simple_preferences::reflect_map::PreferencesReflectMap;
///
/// # #[derive(Reflect)]
/// # struct MyPluginPreferences {
/// #     do_things: bool,
/// # }
///
/// # let register_type = TypeRegistryArc::default();
/// # register_type.write().register::<MyPluginPreferences>();
///
/// let mut map = PreferencesReflectMap::empty(register_type);
/// map.set(MyPluginPreferences {
///     do_things: true
/// });
/// let contents = toml::to_string(&map).unwrap();
///
/// assert_eq!(&contents, "[MyPluginPreferences]\ndo_things = true\n");
/// ```
///
/// ### Reflection
/// It implements [`Reflect`] and [`bevy::reflect::Map`] so it can be used as any other reflectable map.
///
/// ```
/// # use bevy::reflect::{DynamicMap, Map, Reflect, TypeRegistryArc};
/// use bevy::utils::HashMap;
/// # use serde::Serialize;
///
/// # use bevy_simple_preferences::reflect_map::PreferencesReflectMap;
///
/// # #[derive(Reflect, Debug)]
/// # struct MyPluginPreferences {
/// #     do_things: bool,
/// # }
/// # let register_type = TypeRegistryArc::default();
/// # register_type.write().register::<MyPluginPreferences>();
/// let mut preferences_map = PreferencesReflectMap::empty(register_type);
/// preferences_map.set(MyPluginPreferences {
///     do_things: true
/// });
/// let mut hash_map = HashMap::<String, MyPluginPreferences>::default();
/// hash_map.try_apply(&preferences_map).unwrap();
///
/// let preferences_from_dynamic_map = hash_map.get("MyPluginPreferences").unwrap();
/// assert!(preferences_from_dynamic_map.do_things);
/// ```
///

#[derive(Resource, TypePath)]
pub struct PreferencesReflectMap {
    values: BTreeMap<String, Box<dyn Reflect>>,
    type_registry_arc: TypeRegistryArc,
}

impl Debug for PreferencesReflectMap {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Reflect::debug(self, f)
    }
}

impl PartialEq for PreferencesReflectMap {
    fn eq(&self, other: &Self) -> bool {
        if let Some(type_info) = other.get_represented_type_info() {
            if type_info.type_path() != PreferencesReflectMap::type_path() {
                return false;
            }
        } else {
            return false;
        }
        PreferencesReflectMap::reflect_partial_eq(self, other).unwrap_or(false)
    }
}

impl FromWorld for PreferencesReflectMap {
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

impl PreferencesReflectMap {
    /// Creates a new empty storage map
    pub fn empty(type_registry_arc: TypeRegistryArc) -> Self {
        Self {
            values: BTreeMap::new(),
            type_registry_arc,
        }
    }

    /// Creates a storage map using the specified dynamic values.
    /// Values are converted into concrete types using the FromReflect implementation.
    pub fn from_dynamic_values(
        values: impl IntoIterator<Item = (String, Box<dyn Reflect>)>,
        type_registry_arc: TypeRegistryArc,
    ) -> Self {
        let values = values.into_iter();

        // This is scope is to make the borrow checker happy
        let values = {
            let type_registry = type_registry_arc.read();

            values
                .map(|(key, value)| {
                    if let Some(type_info) = value.get_represented_type_info() {
                        let registry_data =
                            PreferencesRegistryData::from_type_info(&type_registry, type_info);

                        let new_value = registry_data.apply_from_reflect(value);

                        debug_assert!(!new_value.is_dynamic(), "Dynamic value generated");

                        (key, new_value)
                    } else {
                        // TODO: Should we panic instead?, or at least ignore the value
                        (key, value)
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

    fn effective_type_path_from_dyn<'a>(&self, value: &'a dyn Reflect) -> &'a str {
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
    pub fn set_dyn(&mut self, value: Box<dyn Reflect>) {
        if value.is_dynamic() {
            let type_info = value
                .get_represented_type_info()
                .expect("Provided dynamic value without a a represented type info");

            let key = self.effective_type_path_from_type_info(type_info);

            let type_registry = &self.type_registry_arc.read();
            let registry_data = PreferencesRegistryData::from_type_info(&type_registry, type_info);

            let value = registry_data.apply_from_reflect(value);

            self.values.insert(key.to_owned(), value);
        } else {
            self.values.insert(
                self.effective_type_path_from_dyn(value.as_reflect())
                    .to_owned(),
                value,
            );
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

impl Serialize for PreferencesReflectMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let type_registry = self.type_registry_arc.read();
        let values = &self.values;

        let mut map_serializer = serializer.serialize_map(Some(values.len()))?;

        for (type_path, value) in values.iter() {
            let reflect_serializer = TypedReflectSerializer::new(&**value, &type_registry);
            map_serializer.serialize_entry(type_path, &reflect_serializer)?;
        }

        map_serializer.end()
    }
}

/// [`DeserializeSeed`] used to deserialize a [`PreferencesMap].
/// Is required to deserialize this way in order to have a reference to
/// the [`TypeRegistry`].
///
/// Best way to get a new seed is to call [`PreferencesReflectMap::deserialize_seed`]
pub struct PreferencesReflectMapDeserializeSeed {
    type_registry_arc: TypeRegistryArc,
}

impl PreferencesReflectMapDeserializeSeed {
    pub(crate) fn new(type_registry_arc: TypeRegistryArc) -> Self {
        Self { type_registry_arc }
    }
}

impl PreferencesReflectMap {
    /// Creates an [`PreferencesReflectMapDeserializeSeed`] that allows deserialization of [`PreferencesReflectMap`].
    pub fn deserialize_seed(
        type_registry_arc: TypeRegistryArc,
    ) -> PreferencesReflectMapDeserializeSeed {
        PreferencesReflectMapDeserializeSeed::new(type_registry_arc)
    }
}

impl<'de> DeserializeSeed<'de> for PreferencesReflectMapDeserializeSeed {
    type Value = PreferencesReflectMap;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MapVisitor {
            type_registry_arc: TypeRegistryArc,
        }

        impl<'de> Visitor<'de> for MapVisitor {
            type Value = BTreeMap<String, Box<dyn Reflect>>;

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

        Ok(PreferencesReflectMap::from_dynamic_values(
            values,
            type_registry_arc,
        ))
    }
}

// Implementation of Reflect for PreferencesMap
mod reflect {
    use super::PreferencesReflectMap;
    use bevy::prelude::ReflectFromWorld;
    use bevy::reflect::serde::Serializable;
    use bevy::reflect::*;
    use std::any::Any;

    impl Map for PreferencesReflectMap {
        fn get(&self, key: &dyn Reflect) -> Option<&dyn Reflect> {
            key.downcast_ref::<String>()
                .and_then(|key| self.values.get(key))
                .map(|value| value.as_reflect())
        }

        fn get_mut(&mut self, key: &dyn Reflect) -> Option<&mut dyn Reflect> {
            key.downcast_ref::<String>()
                .and_then(move |key| self.values.get_mut(key))
                .map(|value| value.as_reflect_mut())
        }

        fn get_at(&self, index: usize) -> Option<(&dyn Reflect, &dyn Reflect)> {
            self.values
                .iter()
                .nth(index)
                .map(|(key, value)| (key as &dyn Reflect, value.as_reflect()))
        }

        fn get_at_mut(&mut self, index: usize) -> Option<(&dyn Reflect, &mut dyn Reflect)> {
            self.values
                .iter_mut()
                .nth(index)
                .map(|(key, value)| (key as &dyn Reflect, value.as_reflect_mut()))
        }

        fn len(&self) -> usize {
            self.values.len()
        }

        fn iter(&self) -> MapIter {
            MapIter::new(self)
        }

        fn drain(self: Box<Self>) -> Vec<(Box<dyn Reflect>, Box<dyn Reflect>)> {
            self.values
                .into_iter()
                .map(|(key, value)| (Box::new(key) as Box<dyn Reflect>, value))
                .collect()
        }

        fn clone_dynamic(&self) -> DynamicMap {
            let mut dynamic_map = DynamicMap::default();
            dynamic_map.set_represented_type(self.get_represented_type_info());
            for (k, v) in &self.values {
                let key = k.clone();
                dynamic_map.insert_boxed(Box::new(key), v.clone_value());
            }
            dynamic_map
        }

        fn insert_boxed(
            &mut self,
            key: Box<dyn Reflect>,
            value: Box<dyn Reflect>,
        ) -> Option<Box<dyn Reflect>> {
            let key = String::take_from_reflect(key).unwrap_or_else(|key| {
                panic!(
                    "Attempted to insert invalid key of type {}.",
                    key.reflect_type_path()
                )
            });
            self.values.insert(key, value)
        }

        fn remove(&mut self, key: &dyn Reflect) -> Option<Box<dyn Reflect>> {
            let mut from_reflect = None;
            key.downcast_ref::<String>()
                .or_else(|| {
                    from_reflect = String::from_reflect(key);
                    from_reflect.as_ref()
                })
                .and_then(|key| self.values.remove(key))
        }
    }

    impl Reflect for PreferencesReflectMap {
        fn get_represented_type_info(&self) -> Option<&'static TypeInfo> {
            Some(<Self as Typed>::type_info())
        }

        fn into_any(self: Box<Self>) -> Box<dyn Any> {
            self
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn into_reflect(self: Box<Self>) -> Box<dyn Reflect> {
            self
        }

        fn as_reflect(&self) -> &dyn Reflect {
            self
        }

        fn as_reflect_mut(&mut self) -> &mut dyn Reflect {
            self
        }

        fn apply(&mut self, value: &dyn Reflect) {
            map_apply(self, value)
        }

        fn try_apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
            map_try_apply(self, value)
        }

        fn set(&mut self, value: Box<dyn Reflect>) -> Result<(), Box<dyn Reflect>> {
            *self = value.take()?;
            Ok(())
        }

        fn reflect_kind(&self) -> ReflectKind {
            ReflectKind::Map
        }

        fn reflect_ref(&self) -> ReflectRef {
            ReflectRef::Map(self)
        }

        fn reflect_mut(&mut self) -> ReflectMut {
            ReflectMut::Map(self)
        }

        fn reflect_owned(self: Box<Self>) -> ReflectOwned {
            ReflectOwned::Map(self)
        }

        fn clone_value(&self) -> Box<dyn Reflect> {
            Box::new(Self {
                values: self
                    .values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone_value()))
                    .collect(),
                type_registry_arc: self.type_registry_arc.clone(),
            })
        }

        fn reflect_partial_eq(&self, value: &dyn Reflect) -> Option<bool> {
            map_partial_eq(self, value)
        }

        fn serializable(&self) -> Option<Serializable> {
            Some(Serializable::Borrowed(self))
        }

        fn is_dynamic(&self) -> bool {
            true
        }
    }

    impl Typed for PreferencesReflectMap {
        fn type_info() -> &'static TypeInfo {
            use bevy::reflect::utility::NonGenericTypeInfoCell;

            static CELL: NonGenericTypeInfoCell = NonGenericTypeInfoCell::new();
            CELL.get_or_set(|| TypeInfo::Map(MapInfo::new::<Self, String, DynReflect>()))
        }
    }

    impl GetTypeRegistration for PreferencesReflectMap {
        fn get_type_registration() -> TypeRegistration {
            let mut registration = TypeRegistration::of::<Self>();
            registration.insert::<ReflectFromPtr>(FromType::<Self>::from_type());
            registration.insert::<ReflectFromWorld>(FromType::<Self>::from_type());
            // In bevy 0.15 this should work because FromReflect is not required anymore
            // registration.insert::<ReflectResource>(FromType::<Self>::from_type());
            registration
        }
    }

    // This is required because MapInfo::new does not work with `dyn Reflect` directly.
    #[derive(Reflect)]
    #[reflect(type_path = false)]
    pub(super) struct DynReflect;

    impl TypePath for DynReflect {
        fn type_path() -> &'static str {
            <dyn Reflect as TypePath>::type_path()
        }

        fn short_type_path() -> &'static str {
            <dyn Reflect as TypePath>::short_type_path()
        }

        fn type_ident() -> Option<&'static str> {
            <dyn Reflect as TypePath>::type_ident()
        }

        fn crate_name() -> Option<&'static str> {
            <dyn Reflect as TypePath>::crate_name()
        }

        fn module_path() -> Option<&'static str> {
            <dyn Reflect as TypePath>::module_path()
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::reflect::serde::TypedReflectSerializer;
    use bevy::reflect::{DynamicMap, TypeRegistryArc};
    use std::fmt::Debug;

    use super::{PreferencesReflectMap, PreferencesReflectMapDeserializeSeed};
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

    fn new_map() -> PreferencesReflectMap {
        PreferencesReflectMap::empty(get_registry())
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
            .into_reflect(),
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
    fn test_reflect_partial_eq() {
        let bar = Bar("reflect_partial_eq".into());
        let mut map = new_map();
        map.set(bar.clone());

        let mut dynamic_map = DynamicMap::default();
        dynamic_map.set_represented_type(map.get_represented_type_info());
        dynamic_map.insert("Bar".to_owned(), bar);

        assert!(
            map.reflect_partial_eq(&dynamic_map).unwrap(),
            "{map:#?} != {dynamic_map:#?}"
        );
        assert!(
            dynamic_map.reflect_partial_eq(&map).unwrap(),
            "{dynamic_map:#?} != {map:#?}"
        );
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
    fn test_ser_foo_using_reflect() {
        let mut map = new_map();
        map.set(Foo {
            field: 3,
            option: None,
        });

        let type_registry = map.type_registry_arc.read();
        let serializer = TypedReflectSerializer::new(&map, &type_registry);

        assert_ser_tokens(
            &serializer,
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
                Token::TupleStruct {
                    name: "Bar",
                    len: 1,
                },
                Token::Str("Hello"),
                Token::TupleStructEnd,
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
                Token::Str("bevy_simple_preferences::reflect_map::tests::Bar"),
                Token::TupleStruct {
                    name: "Bar",
                    len: 1,
                },
                Token::Str("Bar"),
                Token::TupleStructEnd,
                Token::Str("bevy_simple_preferences::reflect_map::tests::ambiguous::Bar"),
                Token::TupleStruct {
                    name: "Bar",
                    len: 1,
                },
                Token::Str("ambiguousBar"),
                Token::TupleStructEnd,
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
                Token::TupleStruct {
                    name: "Bar",
                    len: 1,
                },
                Token::Str("Hello"),
                Token::TupleStructEnd,
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

        let deserializer = PreferencesReflectMapDeserializeSeed {
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
