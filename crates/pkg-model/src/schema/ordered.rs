//! A JSON value whose objects keep their keys in their text's order, which
//! `serde_json`'s maps sort: the order of a schema's keys, which a guide and
//! a page of settings follow.

use std::fmt;

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// A JSON value whose objects keep their keys in their file's order.
#[derive(Clone, Debug, PartialEq)]
pub enum Ordered {
    /// A value of no keys nor items.
    Scalar(serde_json::Value),
    /// A list.
    Array(Vec<Ordered>),
    /// An object, its keys in their order.
    Object(Vec<(String, Ordered)>),
}

impl Ordered {
    /// The value of `key`, of an object that has it.
    pub fn get(&self, key: &str) -> Option<&Ordered> {
        match self {
            Self::Object(entries) => entries
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The text, of a text.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Scalar(serde_json::Value::String(text)) => Some(text),
            _ => None,
        }
    }

    /// The items, of a list; none of another.
    pub fn items(&self) -> &[Ordered] {
        match self {
            Self::Array(items) => items,
            _ => &[],
        }
    }

    /// The entries of the object of `properties`, in their order.
    pub fn properties(&self) -> Option<&[(String, Ordered)]> {
        match self.get("properties") {
            Some(Self::Object(entries)) => Some(entries),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(OrderedVisitor)
    }
}

/// The visitor of [`Ordered`], which takes an object's entries in their
/// order.
struct OrderedVisitor;

impl<'de> Visitor<'de> for OrderedVisitor {
    type Value = Ordered;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Ordered, E> {
        Ok(Ordered::Scalar(value.into()))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Ordered, E> {
        Ok(Ordered::Scalar(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Ordered, E> {
        Ok(Ordered::Scalar(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Ordered, E> {
        Ok(Ordered::Scalar(value.into()))
    }

    fn visit_str<E>(self, value: &str) -> Result<Ordered, E> {
        Ok(Ordered::Scalar(value.into()))
    }

    fn visit_string<E>(self, value: String) -> Result<Ordered, E> {
        Ok(Ordered::Scalar(value.into()))
    }

    fn visit_unit<E>(self) -> Result<Ordered, E> {
        Ok(Ordered::Scalar(serde_json::Value::Null))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Ordered, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Ordered::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
        let mut entries = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, Ordered>()? {
            entries.push((key, value));
        }
        Ok(Ordered::Object(entries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_object_keeps_its_keys_in_their_file_s_order() {
        let read: Ordered =
            serde_json::from_str(r#"{"b": 1, "a": [true, "x"], "c": {"z": null, "y": 2.5}}"#)
                .unwrap();
        let Ordered::Object(entries) = &read else {
            panic!("an object");
        };
        let keys: Vec<&str> = entries.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["b", "a", "c"]);
        assert_eq!(read.get("a").unwrap().items()[1].text(), Some("x"));
        let Some(Ordered::Object(inner)) = read.get("c") else {
            panic!("an object");
        };
        assert_eq!(inner[0].0, "z");
    }
}
