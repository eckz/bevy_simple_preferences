use bevy::prelude::*;
use bevy::reflect::serde::{TypedReflectDeserializer, TypedReflectSerializer};
use bevy::reflect::{TypePathTable, TypeRegistry, TypeRegistryArc};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

use crate::PreferencesError;
use serde::de::{DeserializeSeed, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserializer, Serialize, Serializer};

#[derive(Resource, TypePath)]
pub struct PreferencesMap {
    values: BTreeMap<String, Box<dyn Reflect>>,
    type_registry_arc: TypeRegistryArc,
}

impl Debug for PreferencesMap {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Reflect::debug(self, f)
    }
}

impl PartialEq for PreferencesMap {
    fn eq(&self, other: &Self) -> bool {
        if let Some(type_info) = other.get_represented_type_info() {
            if type_info.type_path() != PreferencesMap::type_path() {
                return false;
            }
        } else {
            return false;
        }
        PreferencesMap::reflect_partial_eq(self, other).unwrap_or(false)
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

impl PreferencesMap {
    pub(crate) fn new(type_registry_arc: TypeRegistryArc) -> Self {
        Self {
            values: BTreeMap::new(),
            type_registry_arc,
        }
    }

    pub(crate) fn set_if_missing(
        &mut self,
        type_path_table: &TypePathTable,
        f: impl FnOnce() -> Box<dyn Reflect>,
    ) {
        let type_registry = self.type_registry_arc.read();
        let type_path = effective_type_path(
            type_path_table.path(),
            type_path_table.short_path(),
            &type_registry,
        );
        self.values.entry(type_path.to_owned()).or_insert_with(f);
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

    /// Set preferences entry of type `P`, potentially overwriting an existing entry.
    pub fn set<P: Reflect>(&mut self, value: P) {
        self.values.insert(
            self.effective_type_path_from_dyn(&value).to_owned(),
            Box::new(value),
        );
    }

    /// Set preferences entry from a boxed trait object of unknown type.
    pub fn set_dyn(&mut self, value: Box<dyn Reflect>) {
        self.values.insert(
            self.effective_type_path_from_dyn(value.as_reflect())
                .to_owned(),
            value,
        );
    }

    /// Get preferences entry of type `P`.
    #[track_caller]
    pub fn get<P: Reflect + TypePath>(&self) -> &P {
        self.get_fallible().unwrap_or_else(|| {
            panic!(
                "Type {} not registered using register_preferences",
                P::type_path()
            )
        })
    }

    /// Get preferences entry of type `T`.
    pub fn get_fallible<T: Reflect + TypePath>(&self) -> Option<&T> {
        self.values
            .get(self.effective_type_path_from_type::<T>())
            .and_then(|val| val.downcast_ref())
    }

    /// Get a mutable reference to a preferences entry of type `P`.
    #[track_caller]
    pub fn get_mut<T: Reflect + TypePath>(&mut self) -> &mut T {
        self.get_mut_fallible().unwrap_or_else(|| {
            panic!(
                "Type {} not registered using register_preferences",
                T::type_path()
            )
        })
    }

    /// Get a mutable reference to a preferences entry of type `P`.
    #[track_caller]
    pub fn get_mut_or_default<T: Reflect + TypePath + Default>(&mut self) -> &mut T {
        self.values
            .entry(self.effective_type_path_from_type::<T>().to_owned())
            .or_insert_with(|| Box::<T>::default())
            .downcast_mut()
            .expect("Could not downcast")
    }

    /// Get a mutable reference to a preferences entry of type `T`.
    pub fn get_mut_fallible<T: Reflect + TypePath>(&mut self) -> Option<&mut T> {
        let type_path = self.effective_type_path_from_type::<T>();
        self.values
            .get_mut(type_path)
            .and_then(|val| val.downcast_mut())
    }

    /// Iterator over all preference entries as [`Reflect`] trait objects.
    pub fn iter_values(&self) -> impl Iterator<Item = &dyn Reflect> {
        self.values.values().map(|v| &**v)
    }

    pub fn iter_entries(&mut self) -> impl Iterator<Item = (&str, &dyn Reflect)> {
        self.values.iter_mut().map(|(k, v)| (k.as_str(), &**v))
    }

    /// Remove and return an entry from preferences, if it exists.
    pub fn remove<T: Reflect + TypePath>(&mut self) -> Option<Box<T>> {
        let type_path = self.effective_type_path_from_type::<T>();

        self.values
            .remove(type_path)
            .and_then(|val| val.downcast().ok())
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }
}

impl Serialize for PreferencesMap {
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

pub struct PreferencesMapDeserializeSeed {
    type_registry_arc: TypeRegistryArc,
}

impl PreferencesMapDeserializeSeed {
    pub fn new(type_registry_arc: TypeRegistryArc) -> Self {
        Self { type_registry_arc }
    }
}

impl PreferencesMap {
    pub fn deserialize_seed(type_registry_arc: TypeRegistryArc) -> PreferencesMapDeserializeSeed {
        PreferencesMapDeserializeSeed::new(type_registry_arc)
    }
}

impl<'de> DeserializeSeed<'de> for PreferencesMapDeserializeSeed {
    type Value = PreferencesMap;

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

        Ok(PreferencesMap {
            values,
            type_registry_arc,
        })
    }
}

// Implementation of Reflect for PreferencesMap
mod reflect {
    use super::PreferencesMap;
    use bevy::reflect::serde::Serializable;
    use bevy::reflect::*;
    use std::any::Any;

    impl Map for PreferencesMap {
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

    impl Reflect for PreferencesMap {
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

    impl Typed for PreferencesMap {
        fn type_info() -> &'static TypeInfo {
            use bevy::reflect::utility::GenericTypeInfoCell;

            static CELL: GenericTypeInfoCell = GenericTypeInfoCell::new();
            CELL.get_or_insert::<Self, _>(|| {
                TypeInfo::Map(MapInfo::new::<Self, String, DynReflect>())
            })
        }
    }

    impl GetTypeRegistration for PreferencesMap {
        fn get_type_registration() -> TypeRegistration {
            let mut registration = TypeRegistration::of::<Self>();
            registration.insert::<ReflectFromPtr>(FromType::<Self>::from_type());
            registration
        }
    }

    // We define DynReflect that implements Reflect is Sized
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

    use super::{PreferencesMap, PreferencesMapDeserializeSeed};
    use serde_test::{assert_ser_tokens, Token};

    #[derive(Reflect, Debug)]
    struct Foo {
        field: u32,
        option: Option<u32>,
    }

    #[derive(Reflect, Clone, Default, Debug)]
    struct Bar(String);

    mod ambiguous {
        use bevy::prelude::Reflect;

        #[derive(Reflect, Clone, PartialEq, Default, Debug)]
        pub(super) struct Bar(pub(super) String);
    }

    fn get_registry() -> TypeRegistryArc {
        let type_registry_arc = TypeRegistryArc::default();

        {
            let mut type_registry = type_registry_arc.write();
            type_registry.register::<Foo>();
            type_registry.register::<Bar>();
            type_registry.register::<Option<u32>>();
        }

        type_registry_arc
    }

    fn new_map() -> PreferencesMap {
        PreferencesMap::new(get_registry())
    }

    #[test]
    fn test_sets_and_gets() {
        let mut map = new_map();
        map.set(Foo {
            field: 4,
            option: Some(2),
        });

        let value: &Foo = map.get();

        assert_eq!(value.field, 4);
        assert_eq!(value.option, Some(2));
    }

    #[test]
    fn test_sets_and_gets_with_ambiguous() {
        let mut map = new_map();
        map.type_registry_arc.write().register::<ambiguous::Bar>();

        map.set(Bar("Bar".into()));
        map.set(ambiguous::Bar("ambiguousBar".into()));

        println!("{map:#?}");

        let bar: &Bar = map.get();
        let ambiguous_bar: &ambiguous::Bar = map.get();

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

        let value: &Foo = map.get();

        assert_eq!(value.field, 4);
        assert_eq!(value.option, Some(2));
    }

    #[test]
    fn test_get_mut_or_default() {
        let mut map = new_map();

        {
            let value: &mut Bar = map.get_mut_or_default();
            value.0 = "Other String".into();
        }

        let value: &Bar = map.get();
        assert_eq!(value.0, "Other String");
    }

    #[test]
    fn test_remove() {
        let mut map = new_map();
        map.set(Bar("H".into()));

        map.remove::<Bar>();
        assert!(map.get_fallible::<Bar>().is_none());
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
    fn test_servebar_with_ambiguous() {
        let mut map = new_map();
        map.type_registry_arc.write().register::<ambiguous::Bar>();

        map.set(Bar("Bar".to_string()));
        map.set(ambiguous::Bar("ambiguousBar".to_string()));

        assert_ser_tokens(
            &map,
            &[
                Token::Map { len: Some(2) },
                Token::Str("bevy_simple_preferences::map::tests::Bar"),
                Token::TupleStruct {
                    name: "Bar",
                    len: 1,
                },
                Token::Str("Bar"),
                Token::TupleStructEnd,
                Token::Str("bevy_simple_preferences::map::tests::ambiguous::Bar"),
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
        let mut de = serde_assert::Deserializer::builder(tokens.clone()).build();
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

        let deserializer = PreferencesMapDeserializeSeed {
            type_registry_arc: map.type_registry_arc.clone(),
        };

        assert_de_seed_tokens(
            &map,
            deserializer,
            [
                Token::Map { len: Some(1) },
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
