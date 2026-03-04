use std::{
    collections::HashMap,
    fmt::{self, Display},
    hash::Hash,
    marker::PhantomData,
    str::FromStr,
};

use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeSeed, MapAccess, Visitor},
};

/// This fixes the issue that number keys in a hashmap are serialized with string, but they're tried to be deserialized as numbers
pub fn de_int_key<'de, D, K, V>(deserializer: D) -> Result<HashMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Eq + Hash + FromStr,
    K::Err: Display,
    V: Deserialize<'de>,
{
    struct KeySeed<K> {
        k: PhantomData<K>,
    }

    impl<'de, K> DeserializeSeed<'de> for KeySeed<K>
    where
        K: FromStr,
        K::Err: Display,
    {
        type Value = K;

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_str(self)
        }
    }

    impl<'de, K> Visitor<'de> for KeySeed<K>
    where
        K: FromStr,
        K::Err: Display,
    {
        type Value = K;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string")
        }

        fn visit_str<E>(self, string: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            K::from_str(string).map_err(de::Error::custom)
        }
    }

    struct MapVisitor<K, V> {
        k: PhantomData<K>,
        v: PhantomData<V>,
    }

    impl<'de, K, V> Visitor<'de> for MapVisitor<K, V>
    where
        K: Eq + Hash + FromStr,
        K::Err: Display,
        V: Deserialize<'de>,
    {
        type Value = HashMap<K, V>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<A>(self, mut input: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut map = HashMap::new();
            while let Some((k, v)) =
                input.next_entry_seed(KeySeed { k: PhantomData }, PhantomData)?
            {
                map.insert(k, v);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_map(MapVisitor {
        k: PhantomData,
        v: PhantomData,
    })
}

pub mod hex_array {
    use serde::{Deserialize, Deserializer, Serializer, de};

    pub fn serialize<S, const N: usize>(bytes: &[u8; N], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = hex::encode(bytes);
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let decoded = hex::decode(&s).map_err(de::Error::custom)?;
        decoded
            .try_into()
            .map_err(|_| de::Error::custom(format!("invalid length: expected {N} bytes")))
    }
}
