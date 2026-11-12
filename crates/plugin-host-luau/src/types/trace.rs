//! The SDK's types as serde tells them: a deserializer that records, of
//! each type that it is asked for, its format, a struct's fields and
//! those that it requires, an enum's variants and their values, and gives
//! the type a value, so that the types inside it are asked for too.
//!
//! A type is traced as often as its enums have variants to explore: each
//! run takes, of each enum, a variant that no run took, then each in turn.
//! A struct's required fields are found as it is given none of its fields,
//! then those that it lacked, until it takes them. A type met inside
//! itself, as a node's children, is given its least value: its required
//! fields alone, and an enum's first variant of no value.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::de::{
    self, DeserializeOwned, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};

/// A type of a name and its members, a struct's fields or an enum's
/// variants, as two types of one name are told apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Id {
    pub(crate) name: &'static str,
    pub(crate) members: &'static [&'static str],
}

/// What a value is, as serde tells it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Format {
    Unit,
    Bool,
    Number,
    Text,
    Bytes,
    Option(Box<Format>),
    List(Box<Format>),
    Map(Box<Format>, Box<Format>),
    Tuple(Vec<Format>),
    /// A struct or an enum of the registry.
    Named(Id),
    /// A struct of one value, by its name, as `PaneId`.
    Newtype(&'static str),
    /// A type that a front end of Luau takes as it is: `Data`, a command's
    /// `Value` and `Capability`.
    Plain(&'static str),
    /// Of no value traced, as a list's items given none.
    Unknown,
}

/// A struct's field, or a struct variant's.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Field {
    pub(crate) name: &'static str,
    pub(crate) format: Format,
    pub(crate) required: bool,
}

/// What a variant holds.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Payload {
    Unit,
    Value(Format),
    Tuple(Vec<Format>),
    Fields(Vec<Field>),
}

/// A struct's or an enum's shape.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Shape {
    Struct(Vec<Field>),
    /// Its variants' payloads, each once a run took it.
    Enum(Vec<Option<Payload>>),
}

/// The types that the SDK's types use that a front end of Luau takes as
/// they are.
const PLAIN: [&str; 3] = ["Data", "Value", "Capability"];

/// How many runs a type's tracing takes at most.
const MOST_RUNS: usize = 200_000;

/// What stops a run.
#[derive(Debug)]
pub(crate) enum Stop {
    /// A struct lacked a field that it requires, which the next run gives.
    Again,
    /// A field that its struct requires, as serde tells it.
    Missing(&'static str),
    /// Another failure.
    Other(String),
}

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Again => f.write_str("a field lacked, given at the next run"),
            Self::Missing(field) => write!(f, "missing field `{field}`"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Stop {}

impl de::Error for Stop {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::Other(message.to_string())
    }

    fn missing_field(field: &'static str) -> Self {
        Self::Missing(field)
    }
}

/// The required fields found of a struct, or of a struct variant, whether
/// they are all, and whether a run gave it all its fields.
#[derive(Clone, Debug, Default)]
struct Probe {
    required: BTreeSet<&'static str>,
    done: bool,
    full: bool,
}

/// The registry of the types traced.
#[derive(Default)]
pub(crate) struct Tracer {
    pub(crate) shapes: RefCell<BTreeMap<Id, Shape>>,
    pub(crate) newtypes: RefCell<BTreeMap<&'static str, Format>>,
    /// The variants that the runs took, of each enum, and the next to take
    /// once each was.
    taken: RefCell<BTreeMap<Id, (BTreeSet<usize>, usize)>>,
    probes: RefCell<BTreeMap<(Id, &'static str), Probe>>,
    /// The structs and enums being given a value, one within another.
    active: RefCell<Vec<Id>>,
}

impl Tracer {
    /// Traces `T`, run after run, until each enum met had each of its
    /// variants taken and each struct's required fields are known: the
    /// values that the runs gave, a variant of each of `T`'s, if it is an
    /// enum, among them.
    pub(crate) fn trace<T: DeserializeOwned>(&self) -> Vec<T> {
        let mut values = Vec::new();
        for _ in 0..MOST_RUNS {
            let slot = RefCell::new(Format::Unknown);
            match T::deserialize(Describing {
                tracer: self,
                slot: &slot,
                least: false,
            }) {
                Ok(value) => {
                    values.push(value);
                    if !self.pending() {
                        return values;
                    }
                }
                Err(Stop::Again) => {}
                Err(other) => panic!("{} does not trace: {other}", std::any::type_name::<T>()),
            }
            self.active.borrow_mut().clear();
        }
        panic!(
            "{} does not trace within {MOST_RUNS} runs",
            std::any::type_name::<T>()
        );
    }

    /// Whether an enum has a variant that no run took, a struct a field that
    /// no run gave, or a struct a probe not done.
    fn pending(&self) -> bool {
        let shapes = self.shapes.borrow();
        let unexplored = shapes.iter().any(|(id, shape)| match shape {
            Shape::Enum(variants) => variants.iter().any(Option::is_none),
            Shape::Struct(fields) => fields.len() < id.members.len(),
        });
        unexplored
            || self
                .probes
                .borrow()
                .values()
                .any(|probe| !probe.done || !probe.full)
    }

    /// The variant of `id` that the run takes: one that no run took, else
    /// each in turn; its first of no value, or its first, as a value of
    /// `id` inside itself, which `least` tells.
    fn variant(&self, id: Id, least: bool) -> usize {
        if least {
            if let Some(Shape::Enum(variants)) = self.shapes.borrow().get(&id)
                && let Some(at) = variants
                    .iter()
                    .position(|payload| matches!(payload, Some(Payload::Unit)))
            {
                return at;
            }
            return 0;
        }
        let mut taken = self.taken.borrow_mut();
        let (seen, next) = taken.entry(id).or_default();
        let count = id.members.len();
        let at = (0..count).find(|at| !seen.contains(at)).unwrap_or_else(|| {
            let at = *next % count;
            *next += 1;
            at
        });
        seen.insert(at);
        at
    }

    /// Records `payload` of the variant at `at` of `id`.
    fn record_variant(&self, id: Id, at: usize, payload: Payload) {
        let mut shapes = self.shapes.borrow_mut();
        let shape = shapes
            .entry(id)
            .or_insert_with(|| Shape::Enum(vec![None; id.members.len()]));
        if let Shape::Enum(variants) = shape {
            let merged = match (variants[at].take(), payload) {
                (Some(Payload::Fields(old)), Payload::Fields(new)) => {
                    Payload::Fields(merge(old, new))
                }
                (Some(Payload::Value(old)), Payload::Value(new)) => {
                    Payload::Value(better(old, new))
                }
                (Some(Payload::Tuple(old)), Payload::Tuple(new)) => Payload::Tuple(
                    old.into_iter()
                        .zip(new)
                        .map(|(a, b)| better(a, b))
                        .collect(),
                ),
                (_, payload) => payload,
            };
            variants[at] = Some(merged);
        }
    }

    /// Records `fields` of the struct `id`.
    fn record_struct(&self, id: Id, fields: Vec<Field>) {
        let mut shapes = self.shapes.borrow_mut();
        let shape = shapes
            .entry(id)
            .or_insert_with(|| Shape::Struct(Vec::new()));
        if let Shape::Struct(old) = shape {
            *old = merge(std::mem::take(old), fields);
        }
    }

    /// The probe of the fields of `id`, of the variant `variant` or of the
    /// struct, as `""` tells.
    fn probe(&self, id: Id, variant: &'static str) -> Probe {
        self.probes
            .borrow_mut()
            .entry((id, variant))
            .or_default()
            .clone()
    }

    /// Takes the result of a visit of the fields of `id`'s `variant`: a
    /// field lacked, found required, stops the run for the next.
    fn probed<T>(
        &self,
        id: Id,
        variant: &'static str,
        visited: Result<T, Stop>,
    ) -> Result<T, Stop> {
        let mut probes = self.probes.borrow_mut();
        let probe = probes.entry((id, variant)).or_default();
        match visited {
            Err(Stop::Missing(field)) => {
                probe.required.insert(field);
                Err(Stop::Again)
            }
            Ok(value) => {
                probe.done = true;
                Ok(value)
            }
            Err(other) => Err(other),
        }
    }
}

/// `new`, of what `old` knew that `new` does not: a format traced over one
/// unknown.
fn better(old: Format, new: Format) -> Format {
    match (old, new) {
        (old, Format::Unknown) => old,
        (Format::Option(old), Format::Option(new)) => Format::Option(Box::new(better(*old, *new))),
        (Format::List(old), Format::List(new)) => Format::List(Box::new(better(*old, *new))),
        (Format::Map(old_key, old), Format::Map(key, new)) => Format::Map(
            Box::new(better(*old_key, *key)),
            Box::new(better(*old, *new)),
        ),
        (_, new) => new,
    }
}

/// The fields of `new`, of the formats that `old` knew better, and of those
/// that `old` had and `new` did not give.
fn merge(old: Vec<Field>, new: Vec<Field>) -> Vec<Field> {
    let mut merged: Vec<Field> = Vec::new();
    for field in new {
        let format = match old.iter().find(|known| known.name == field.name) {
            Some(known) => better(known.format.clone(), field.format),
            None => field.format,
        };
        merged.push(Field { format, ..field });
    }
    for field in old {
        if !merged.iter().any(|known| known.name == field.name) {
            merged.push(field);
        }
    }
    merged
}

/// A value asked for, whose format goes to `slot`; of its type's least
/// value, as `least` tells.
struct Describing<'t> {
    tracer: &'t Tracer,
    slot: &'t RefCell<Format>,
    least: bool,
}

/// An integer that every number's type of the SDK takes, an error's code
/// among them.
const NUMBER: u8 = 101;

/// A text that every text's type of the SDK takes, a theme's color among
/// them.
const TEXT: &str = "#000000";

impl Describing<'_> {
    fn set(&self, format: Format) {
        *self.slot.borrow_mut() = format;
    }

    /// The fields of a struct `id`, or of its variant `variant`, of
    /// `fields`, that a run gives it: its required fields alone, as its
    /// probe goes or it is inside itself.
    fn fields<'de, V: Visitor<'de>>(
        &self,
        id: Id,
        variant: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<(V::Value, Vec<Field>), Stop> {
        let probe = self.tracer.probe(id, variant);
        let given: Vec<&'static str> = if probe.done && !self.least {
            fields.to_vec()
        } else {
            fields
                .iter()
                .copied()
                .filter(|field| probe.required.contains(field))
                .collect()
        };
        let slots: Vec<RefCell<Format>> = given
            .iter()
            .map(|_| RefCell::new(Format::Unknown))
            .collect();
        let visited = visitor.visit_map(Fields {
            tracer: self.tracer,
            given: &given,
            slots: &slots,
            at: 0,
        });
        let value = self.tracer.probed(id, variant, visited)?;
        if given.len() == fields.len()
            && let Some(probe) = self.tracer.probes.borrow_mut().get_mut(&(id, variant))
        {
            probe.full = true;
        }
        let probe = self.tracer.probe(id, variant);
        let traced = given
            .iter()
            .zip(slots)
            .map(|(name, slot)| Field {
                name,
                format: slot.into_inner(),
                required: probe.required.contains(name),
            })
            .collect();
        Ok((value, traced))
    }
}

impl<'de> de::Deserializer<'de> for Describing<'_> {
    type Error = Stop;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Stop> {
        Err(Stop::Other(
            "a type of no shape of its own, as one tagged inside".to_owned(),
        ))
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Bool);
        visitor.visit_bool(false)
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Number);
        visitor.visit_i8(NUMBER.cast_signed())
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Number);
        visitor.visit_i16(i16::from(NUMBER))
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Number);
        visitor.visit_i32(i32::from(NUMBER))
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Number);
        visitor.visit_i64(i64::from(NUMBER))
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Number);
        visitor.visit_u8(NUMBER)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Number);
        visitor.visit_u16(u16::from(NUMBER))
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Number);
        visitor.visit_u32(u32::from(NUMBER))
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Number);
        visitor.visit_u64(u64::from(NUMBER))
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Number);
        visitor.visit_f32(f32::from(NUMBER))
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Number);
        visitor.visit_f64(f64::from(NUMBER))
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Text);
        visitor.visit_char('x')
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Text);
        visitor.visit_str(TEXT)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Bytes);
        visitor.visit_bytes(&[])
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        if self.least {
            self.set(Format::Option(Box::new(Format::Unknown)));
            return visitor.visit_none();
        }
        let inner = RefCell::new(Format::Unknown);
        let value = visitor.visit_some(Describing {
            tracer: self.tracer,
            slot: &inner,
            least: false,
        });
        self.set(Format::Option(Box::new(inner.into_inner())));
        value
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.set(Format::Unit);
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Stop> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Stop> {
        self.set(Format::Newtype(name));
        let inner = RefCell::new(Format::Unknown);
        let value = visitor.visit_newtype_struct(Describing {
            tracer: self.tracer,
            slot: &inner,
            least: self.least,
        });
        let inner = inner.into_inner();
        let mut newtypes = self.tracer.newtypes.borrow_mut();
        let known = newtypes.remove(name).unwrap_or(Format::Unknown);
        newtypes.insert(name, better(known, inner));
        value
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        let item = RefCell::new(Format::Unknown);
        let value = visitor.visit_seq(Items {
            tracer: self.tracer,
            slots: std::slice::from_ref(&item),
            given: usize::from(!self.least),
            at: 0,
        });
        self.set(Format::List(Box::new(item.into_inner())));
        value
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, Stop> {
        let slots: Vec<RefCell<Format>> =
            (0..length).map(|_| RefCell::new(Format::Unknown)).collect();
        let value = visitor.visit_seq(Items {
            tracer: self.tracer,
            slots: &slots,
            given: length,
            at: 0,
        });
        self.set(Format::Tuple(
            slots.into_iter().map(RefCell::into_inner).collect(),
        ));
        value
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, Stop> {
        self.deserialize_tuple(length, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        let (key, item) = (RefCell::new(Format::Unknown), RefCell::new(Format::Unknown));
        let value = visitor.visit_map(Entries {
            tracer: self.tracer,
            key: &key,
            item: &item,
            given: !self.least,
        });
        self.set(Format::Map(
            Box::new(key.into_inner()),
            Box::new(item.into_inner()),
        ));
        value
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Stop> {
        let id = Id {
            name,
            members: fields,
        };
        self.set(Format::Named(id));
        let inside = self.tracer.active.borrow().contains(&id);
        self.tracer.active.borrow_mut().push(id);
        let described = Describing {
            least: self.least || inside,
            ..self
        };
        let given = described.fields(id, "", fields, visitor);
        self.tracer.active.borrow_mut().pop();
        let (value, traced) = given?;
        self.tracer.record_struct(id, traced);
        Ok(value)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Stop> {
        let id = Id {
            name,
            members: variants,
        };
        if PLAIN.contains(&name) {
            self.set(Format::Plain(name));
            // A value of no payload, or of a boolean's.
            let at = usize::from(name == "Value") * 2;
            return visitor.visit_enum(Variant {
                describing: Describing {
                    least: true,
                    ..self
                },
                id,
                at: variants.len().min(at),
                record: false,
            });
        }
        self.set(Format::Named(id));
        let inside = self.tracer.active.borrow().contains(&id);
        let least = self.least || inside;
        let at = self.tracer.variant(id, least);
        self.tracer.active.borrow_mut().push(id);
        let value = visitor.visit_enum(Variant {
            describing: Describing { least, ..self },
            id,
            at,
            record: true,
        });
        self.tracer.active.borrow_mut().pop();
        value
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        visitor.visit_unit()
    }
}

/// A struct's fields, as a run gives them.
struct Fields<'t> {
    tracer: &'t Tracer,
    given: &'t [&'static str],
    slots: &'t [RefCell<Format>],
    at: usize,
}

impl<'de> MapAccess<'de> for Fields<'_> {
    type Error = Stop;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Stop> {
        let Some(name) = self.given.get(self.at) else {
            return Ok(None);
        };
        let key: de::value::StrDeserializer<'_, Stop> = name.into_deserializer();
        seed.deserialize(key).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Stop> {
        let slot = &self.slots[self.at];
        self.at += 1;
        seed.deserialize(Describing {
            tracer: self.tracer,
            slot,
            least: false,
        })
    }
}

/// A list's items, or a tuple's, as a run gives them.
struct Items<'t> {
    tracer: &'t Tracer,
    slots: &'t [RefCell<Format>],
    given: usize,
    at: usize,
}

impl<'de> SeqAccess<'de> for Items<'_> {
    type Error = Stop;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Stop> {
        if self.at >= self.given {
            return Ok(None);
        }
        let slot = &self.slots[self.at.min(self.slots.len() - 1)];
        self.at += 1;
        seed.deserialize(Describing {
            tracer: self.tracer,
            slot,
            least: false,
        })
        .map(Some)
    }
}

/// A map's entry, one, as a run gives it.
struct Entries<'t> {
    tracer: &'t Tracer,
    key: &'t RefCell<Format>,
    item: &'t RefCell<Format>,
    given: bool,
}

impl<'de> MapAccess<'de> for Entries<'_> {
    type Error = Stop;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Stop> {
        if !self.given {
            return Ok(None);
        }
        self.given = false;
        seed.deserialize(Describing {
            tracer: self.tracer,
            slot: self.key,
            least: false,
        })
        .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Stop> {
        seed.deserialize(Describing {
            tracer: self.tracer,
            slot: self.item,
            least: false,
        })
    }
}

/// The variant at `at` of the enum `id`, as a run takes it, whose payload
/// it records, unless of a plain type.
struct Variant<'t> {
    describing: Describing<'t>,
    id: Id,
    at: usize,
    record: bool,
}

impl<'de, 't> EnumAccess<'de> for Variant<'t> {
    type Error = Stop;
    type Variant = Self;

    fn variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<(T::Value, Self), Stop> {
        let name = self
            .id
            .members
            .get(self.at)
            .copied()
            .ok_or_else(|| Stop::Other(format!("{} has no variant", self.id.name)))?;
        let key: de::value::StrDeserializer<'_, Stop> = name.into_deserializer();
        let variant = seed.deserialize(key)?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for Variant<'_> {
    type Error = Stop;

    fn unit_variant(self) -> Result<(), Stop> {
        if self.record {
            self.describing
                .tracer
                .record_variant(self.id, self.at, Payload::Unit);
        }
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Stop> {
        let slot = RefCell::new(Format::Unknown);
        let value = seed.deserialize(Describing {
            slot: &slot,
            ..self.describing
        })?;
        if self.record {
            self.describing.tracer.record_variant(
                self.id,
                self.at,
                Payload::Value(slot.into_inner()),
            );
        }
        Ok(value)
    }

    fn tuple_variant<V: Visitor<'de>>(self, length: usize, visitor: V) -> Result<V::Value, Stop> {
        let slot = RefCell::new(Format::Unknown);
        let value = de::Deserializer::deserialize_tuple(
            Describing {
                slot: &slot,
                ..self.describing
            },
            length,
            visitor,
        )?;
        if self.record
            && let Format::Tuple(items) = slot.into_inner()
        {
            self.describing
                .tracer
                .record_variant(self.id, self.at, Payload::Tuple(items));
        }
        Ok(value)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Stop> {
        let variant = self.id.members[self.at];
        let (value, traced) = self.describing.fields(self.id, variant, fields, visitor)?;
        if self.record {
            self.describing
                .tracer
                .record_variant(self.id, self.at, Payload::Fields(traced));
        }
        Ok(value)
    }
}
