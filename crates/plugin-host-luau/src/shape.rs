//! The SDK's types as Luau's values, both ways, through serde: a struct is
//! a table of its fields by their Rust names, as `close_escape` and
//! `min_width`, of snake case where serde names them in kebab case; a
//! variant of a value is a table of one field, of its Rust name, as
//! `{ pane_corner = { ... } }`, and one of none a text, as serde names it,
//! as `"page-up"`; a map's keys are as they are, a command's arguments' or
//! a state's; `Option`'s none is `nil`, and a field of `nil` is left out; a
//! list is a sequence. A plain value, the SDK's `Data`, an event's or a
//! state's, and a command's argument are as a Luau table is: `nil`, `true`
//! or `false`, a number, a text, or a table of them, a sequence a list and
//! another table, an empty one too, a map.
//!
//! A plugin's value nests [`MOST_LEVELS`] levels of tables at most, and a
//! plain value [`Data::DEEPEST`], as the host takes it, a table that holds
//! itself refused as one past them; one conversion takes [`MOST_VALUES`]
//! values at most, those that it read out of their tables and holds until
//! it converts them among them, and copies [`MOST_BYTES`] of texts, and it
//! stops as its call's deadline passes, so that a plugin's value can
//! neither overflow the host's stack, nor hold it for long, nor take more
//! of its memory, of a table or a text that its tables hold many times. A
//! number is finite. Its tables are read raw, of no metamethod, as no code
//! of the plugin's runs outside its calls.

use std::borrow::Cow;
use std::cell::Cell;
use std::fmt;

use lokyr_plugin_sdk::message::Data;
use lokyr_plugin_sdk::view::Node;
use mlua::{Lua, Table, Value};
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use serde::ser::{self, Serialize};

use crate::watchdog::Deadline;

/// The most levels of tables that a plugin's value nests: twice those of
/// the host's trees and more, as a tab's layout nests two a split.
pub(crate) const MOST_LEVELS: usize = 2 * Node::DEEPEST + 16;

/// The most values that one conversion of a plugin's value takes.
pub(crate) const MOST_VALUES: usize = 1 << 20;

/// The most bytes of texts and buffers that one conversion copies into the
/// host's memory, as a plugin's VM holds one text however many times its
/// tables hold it: sixteen times a state's.
pub(crate) const MOST_BYTES: usize = 16 << 20;

/// How many values a conversion takes between its looks at its call's
/// deadline.
const LOOK_EVERY: usize = 64;

/// The types whose values are plain, as a Luau table is, rather than
/// variants: the SDK's `Data` and a command's argument's `Value`.
const PLAIN: [&str; 2] = ["Data", "Value"];

/// Why a value did not take its shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShapeError(pub(crate) String);

impl ShapeError {
    /// The error of `what`, within the place of `within`.
    pub(crate) fn within(self, within: impl fmt::Display) -> Self {
        Self(format!("{within}: {}", self.0))
    }
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ShapeError {}

impl de::Error for ShapeError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

impl ser::Error for ShapeError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

impl From<mlua::Error> for ShapeError {
    fn from(error: mlua::Error) -> Self {
        Self(error.to_string())
    }
}

/// The Rust name of `name`, which serde gives: snake case for kebab case.
fn rust_name(name: &str) -> Cow<'_, str> {
    if name.contains('-') {
        Cow::Owned(name.replace('-', "_"))
    } else {
        Cow::Borrowed(name)
    }
}

/// Which of `names`, as serde gives them, `given` is, of its Rust name or
/// of serde's own.
fn named(given: &str, names: &'static [&'static str]) -> Option<&'static str> {
    names
        .iter()
        .find(|name| **name == given || rust_name(name) == given)
        .copied()
}

/// What a Luau value is, as an error tells it.
pub(crate) fn what(value: &Value) -> &'static str {
    match value {
        Value::Nil => "nil",
        Value::Boolean(_) => "a boolean",
        Value::Integer(_) | Value::Number(_) => "a number",
        Value::String(_) => "a text",
        Value::Table(_) => "a table",
        Value::Function(_) => "a function",
        Value::Buffer(_) => "a buffer",
        Value::Vector(_) => "a vector",
        _ => "a value of no data",
    }
}

/// The whole number that `number` is, if it is one of an `i64`.
fn whole(number: f64) -> Option<i64> {
    // 2^63, past which a double is no `i64`.
    const TOP: f64 = 9_223_372_036_854_775_808.0;
    #[allow(clippy::cast_possible_truncation)]
    (number.fract() == 0.0 && (-TOP..TOP).contains(&number)).then_some(number as i64)
}

/// The value of `T` of `value`, a plugin's: why not, of a value of
/// another shape, too deep, or of too many values.
#[cfg(any(test, feature = "testing"))]
pub(crate) fn from_lua<T: de::DeserializeOwned>(value: Value) -> Result<T, ShapeError> {
    let budget = Budget::default();
    T::deserialize(Taking::new(value, &budget))
}

/// What a conversion takes: the values that it converted, and those that
/// it read out of their tables and holds until it converts them, as a
/// table's items wait for those before them, so that the host holds no
/// more of a plugin's values than it would convert, of tables that hold
/// one table many times too; the bytes of the texts and buffers that it
/// copied; and the time of its call, whose deadline stops it, as Luau's
/// interrupt stops the plugin's code alone.
pub(crate) struct Budget {
    /// The values left to convert.
    left: Cell<usize>,
    /// The values read, and not converted yet.
    held: Cell<usize>,
    /// The bytes left to copy.
    bytes: Cell<usize>,
    /// The deadline of the call of the conversion, if it has one.
    deadline: Option<Deadline>,
}

impl Default for Budget {
    /// [`MOST_VALUES`] values and [`MOST_BYTES`] bytes, of no deadline.
    fn default() -> Self {
        Self::of(MOST_VALUES)
    }
}

impl Budget {
    /// A budget of `values`, of no deadline.
    pub(crate) fn of(values: usize) -> Self {
        Self {
            left: Cell::new(values),
            held: Cell::new(0),
            bytes: Cell::new(MOST_BYTES),
            deadline: None,
        }
    }

    /// The default budget, within `deadline`, of the call of the
    /// conversion.
    pub(crate) fn within(deadline: Deadline) -> Self {
        Self {
            deadline: Some(deadline),
            ..Self::default()
        }
    }

    /// Counts a value against it, and looks at its call's deadline now and
    /// then.
    pub(crate) fn count(&self) -> Result<(), ShapeError> {
        let left = self.left.get();
        if left == 0 {
            return Err(too_many());
        }
        self.left.set(left - 1);
        if left.is_multiple_of(LOOK_EVERY) && self.passed() {
            return Err(past());
        }
        Ok(())
    }

    /// Counts `bytes` of a text or a buffer that it copies against it.
    fn copy(&self, bytes: usize) -> Result<(), ShapeError> {
        let left = self.bytes.get();
        if bytes > left {
            return Err(ShapeError(format!(
                "a plugin's value holds {} MiB of texts at most",
                MOST_BYTES >> 20
            )));
        }
        self.bytes.set(left - bytes);
        Ok(())
    }

    /// Whether the deadline of its call passed.
    pub(crate) fn passed(&self) -> bool {
        self.deadline.as_ref().is_some_and(Deadline::passed)
    }

    /// How many values a conversion can read out of a table still.
    pub(crate) fn room(&self) -> usize {
        self.left.get().saturating_sub(self.held.get())
    }

    /// Holds `values` read out of a table.
    pub(crate) fn hold(&self, values: usize) -> Result<(), ShapeError> {
        if values > self.room() {
            return Err(too_many());
        }
        self.held.set(self.held.get() + values);
        Ok(())
    }

    /// Lets go of a value held, as it converts.
    pub(crate) fn release(&self) {
        self.held.set(self.held.get().saturating_sub(1));
    }
}

/// A plugin's value as the SDK's types take it.
pub(crate) struct Taking<'b> {
    value: Value,
    level: usize,
    /// How many tables of a plain value hold it, which nests
    /// [`Data::DEEPEST`] levels at most, as the host takes it.
    data: usize,
    budget: &'b Budget,
}

impl<'b> Taking<'b> {
    /// The value `value`, at the top, of the values left of `budget`.
    pub(crate) fn new(value: Value, budget: &'b Budget) -> Self {
        Self {
            value,
            level: 0,
            data: 0,
            budget,
        }
    }

    /// A value inside this one's table, a level down.
    fn inner(&self, value: Value) -> Self {
        Self {
            value,
            level: self.level + 1,
            data: self.data,
            budget: self.budget,
        }
    }

    /// Whether its table is within the levels that a value nests.
    fn enter(&self) -> Result<(), ShapeError> {
        if self.level >= MOST_LEVELS {
            return Err(ShapeError(format!(
                "a plugin's value nests {MOST_LEVELS} levels of tables at most, \
                 and holds no table that holds itself"
            )));
        }
        Ok(())
    }

    /// Counts the value against the conversion's.
    fn count(&self) -> Result<(), ShapeError> {
        self.budget.count()
    }

    fn expected(&self, expected: &str) -> ShapeError {
        ShapeError(format!("expected {expected}, not {}", what(&self.value)))
    }

    /// The whole number that it is.
    fn integer(&self) -> Result<i64, ShapeError> {
        match &self.value {
            Value::Integer(integer) => Ok(*integer),
            Value::Number(number) => whole(*number)
                .ok_or_else(|| ShapeError(format!("expected a whole number, not {number}"))),
            _ => Err(self.expected("a whole number")),
        }
    }

    /// The whole number that it is, of `T`'s range.
    fn integer_of<T: TryFrom<i64>>(&self, range: &str) -> Result<T, ShapeError> {
        let integer = self.integer()?;
        T::try_from(integer)
            .map_err(|_| ShapeError(format!("expected a whole number {range}, not {integer}")))
    }

    /// The number that it is, finite, as JSON holds the host's numbers.
    fn number(&self) -> Result<f64, ShapeError> {
        match &self.value {
            #[allow(clippy::cast_precision_loss)]
            Value::Integer(integer) => Ok(*integer as f64),
            Value::Number(number) if number.is_finite() => Ok(*number),
            Value::Number(number) => Err(ShapeError(format!(
                "expected a finite number, not {number}"
            ))),
            _ => Err(self.expected("a number")),
        }
    }

    /// The text that it is, its bytes counted.
    fn text(&self) -> Result<String, ShapeError> {
        match &self.value {
            Value::String(text) => {
                let text = text
                    .to_str()
                    .map_err(|_| ShapeError("expected a text of UTF-8".to_owned()))?;
                self.budget.copy(text.len())?;
                Ok(text.to_owned())
            }
            _ => Err(self.expected("a text")),
        }
    }

    /// The table that it is.
    fn table(&self, expected: &str) -> Result<Table, ShapeError> {
        match &self.value {
            Value::Table(table) => {
                self.enter()?;
                Ok(table.clone())
            }
            _ => Err(self.expected(expected)),
        }
    }
}

/// The error of a value of more values than a conversion takes.
fn too_many() -> ShapeError {
    ShapeError(format!(
        "a plugin's value holds {MOST_VALUES} values at most"
    ))
}

/// The error of a conversion whose call's deadline passed.
fn past() -> ShapeError {
    ShapeError("the call's deadline passed as its value converted".to_owned())
}

/// The items of `table`, if it is a sequence, of keys from 1 to its length
/// alone, none an empty table's; refused past the values left to `budget`,
/// as no more are read, or as its call's deadline passes as they are read.
pub(crate) fn sequence(table: &Table, budget: &Budget) -> Result<Option<Vec<Value>>, ShapeError> {
    let length = table.raw_len();
    if length > budget.room() {
        return Err(too_many());
    }
    // In one walk of its entries, each in its place.
    let mut items = vec![Value::Nil; length];
    let mut count: usize = 0;
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let at = match key {
            Value::Integer(at) => Some(at),
            Value::Number(at) => whole(at),
            _ => None,
        };
        let Some(place) = at
            .and_then(|at| usize::try_from(at).ok())
            .filter(|at| (1..=length).contains(at))
        else {
            return Ok(None);
        };
        items[place - 1] = value;
        count += 1;
        if count.is_multiple_of(LOOK_EVERY) && budget.passed() {
            return Err(past());
        }
    }
    Ok((count == length).then_some(items))
}

/// The entries of `table`, in no order; refused past the values left to
/// `budget`, as no more are read, or as its call's deadline passes as they
/// are read.
pub(crate) fn entries(table: &Table, budget: &Budget) -> Result<Vec<(Value, Value)>, ShapeError> {
    let most = budget.room();
    let mut entries = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        if entries.len() == most {
            return Err(too_many());
        }
        entries.push(pair?);
        if entries.len().is_multiple_of(LOOK_EVERY) && budget.passed() {
            return Err(past());
        }
    }
    Ok(entries)
}

impl<'de> de::Deserializer<'de> for Taking<'_> {
    type Error = ShapeError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        match &self.value {
            Value::Nil => visitor.visit_unit(),
            Value::Boolean(boolean) => visitor.visit_bool(*boolean),
            Value::Integer(integer) => visitor.visit_i64(*integer),
            Value::Number(number) => match whole(*number) {
                Some(integer) => visitor.visit_i64(integer),
                None => visitor.visit_f64(self.number()?),
            },
            Value::String(_) => visitor.visit_string(self.text()?),
            Value::Table(table) => {
                self.enter()?;
                match sequence(table, self.budget)? {
                    Some(items) => visitor.visit_seq(Items::new(&self, items)?),
                    None => {
                        let entries = entries(table, self.budget)?;
                        visitor.visit_map(Entries::new(&self, entries, None)?)
                    }
                }
            }
            _ => Err(self.expected("data")),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        match self.value {
            Value::Boolean(boolean) => visitor.visit_bool(boolean),
            _ => Err(self.expected("a boolean")),
        }
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_i8(self.integer_of("from -128 to 127")?)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_i16(self.integer_of("from -32768 to 32767")?)
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_i32(self.integer_of("of 32 bits")?)
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_i64(self.integer()?)
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_u8(self.integer_of("from 0 to 255")?)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_u16(self.integer_of("from 0 to 65535")?)
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_u32(self.integer_of("from 0, of 32 bits")?)
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_u64(self.integer_of("from 0")?)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        #[allow(clippy::cast_possible_truncation)]
        visitor.visit_f32(self.number()? as f32)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_f64(self.number()?)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        let text = self.text()?;
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(char), None) => visitor.visit_char(char),
            _ => Err(ShapeError(format!("expected one character, not {text:?}"))),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_string(self.text()?)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        match &self.value {
            Value::String(text) => {
                self.budget.copy(text.as_bytes().len())?;
                visitor.visit_byte_buf(text.as_bytes().to_vec())
            }
            Value::Buffer(buffer) => {
                self.budget.copy(buffer.len())?;
                visitor.visit_byte_buf(buffer.to_vec())
            }
            _ => Err(self.expected("a text or a buffer")),
        }
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        match self.value {
            Value::Nil => {
                self.count()?;
                visitor.visit_none()
            }
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        match self.value {
            Value::Nil => visitor.visit_unit(),
            _ => Err(self.expected("nil")),
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, ShapeError> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, ShapeError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        let table = self.table("a list")?;
        let items = sequence(&table, self.budget)?.ok_or_else(|| {
            ShapeError("expected a list, a table of keys from 1 to its length alone".to_owned())
        })?;
        visitor.visit_seq(Items::new(&self, items)?)
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, ShapeError> {
        self.count()?;
        let table = self.table("a list")?;
        match sequence(&table, self.budget)? {
            Some(items) if items.len() == length => visitor.visit_seq(Items::new(&self, items)?),
            _ => Err(ShapeError(format!("expected a list of {length}"))),
        }
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, ShapeError> {
        self.deserialize_tuple(length, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        let table = self.table("a table")?;
        let entries = entries(&table, self.budget)?;
        visitor.visit_map(Entries::new(&self, entries, None)?)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, ShapeError> {
        self.count()?;
        let table = self.table("a table")?;
        let entries = entries(&table, self.budget)?;
        visitor.visit_map(Entries::new(&self, entries, Some(fields))?)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, ShapeError> {
        self.count()?;
        if PLAIN.contains(&name) {
            return visitor.visit_enum(Plain::of(name, self)?);
        }
        let unknown = |given: &str| {
            let names: Vec<_> = variants.iter().map(|name| rust_name(name)).collect();
            ShapeError(format!(
                "`{given}` is no variant of {name}, of {}",
                names.join(", ")
            ))
        };
        match &self.value {
            Value::String(_) => {
                let given = self.text()?;
                let variant = named(&given, variants).ok_or_else(|| unknown(&given))?;
                visitor.visit_enum(Variant {
                    variant,
                    value: None,
                })
            }
            Value::Table(table) => {
                self.enter()?;
                let mut entries = entries(table, self.budget)?;
                let (Some((Value::String(key), value)), None) = (entries.pop(), entries.pop())
                else {
                    return Err(ShapeError(format!(
                        "expected a variant of {name}, a text or a table of one field"
                    )));
                };
                let given = key
                    .to_str()
                    .map(|key| key.to_owned())
                    .map_err(|_| unknown("?"))?;
                let variant = named(&given, variants).ok_or_else(|| unknown(&given))?;
                visitor.visit_enum(Variant {
                    variant,
                    value: Some(self.inner(value)),
                })
            }
            _ => Err(self.expected(&format!("a variant of {name}"))),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ShapeError> {
        self.count()?;
        visitor.visit_unit()
    }
}

/// The items of a sequence.
struct Items<'b> {
    items: std::vec::IntoIter<Value>,
    of: Taking<'b>,
}

impl<'b> Items<'b> {
    fn new(of: &Taking<'b>, items: Vec<Value>) -> Result<Self, ShapeError> {
        of.budget.hold(items.len())?;
        Ok(Self {
            items: items.into_iter(),
            of: of.inner(Value::Nil),
        })
    }
}

impl<'b> Taking<'b> {
    /// A value of the same place as this one, as the items of a table each
    /// take its place.
    fn at(&self, value: Value) -> Self {
        Self {
            value,
            level: self.level,
            data: self.data,
            budget: self.budget,
        }
    }
}

impl<'de> SeqAccess<'de> for Items<'_> {
    type Error = ShapeError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, ShapeError> {
        let Some(item) = self.items.next() else {
            return Ok(None);
        };
        self.of.budget.release();
        seed.deserialize(self.of.at(item)).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }
}

/// The entries of a table, a map's, or a struct's of `fields`, its keys of
/// their Rust names or serde's.
struct Entries<'b> {
    entries: std::vec::IntoIter<(Value, Value)>,
    fields: Option<&'static [&'static str]>,
    value: Option<Value>,
    of: Taking<'b>,
}

impl<'b> Entries<'b> {
    fn new(
        of: &Taking<'b>,
        entries: Vec<(Value, Value)>,
        fields: Option<&'static [&'static str]>,
    ) -> Result<Self, ShapeError> {
        of.budget.hold(entries.len())?;
        Ok(Self {
            entries: entries.into_iter(),
            fields,
            value: None,
            of: of.inner(Value::Nil),
        })
    }
}

impl<'de> MapAccess<'de> for Entries<'_> {
    type Error = ShapeError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, ShapeError> {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        self.of.budget.release();
        self.value = Some(value);
        let Some(fields) = self.fields else {
            return seed
                .deserialize(self.of.at(key))
                .map(Some)
                .map_err(|error| error.within("a map's key"));
        };
        let Value::String(key) = key else {
            return Err(ShapeError(format!(
                "a field's name is a text, not {}",
                what(&key)
            )));
        };
        let given = key
            .to_str()
            .map(|key| key.to_owned())
            .map_err(|_| ShapeError("a field's name is a text of UTF-8".to_owned()))?;
        let Some(field) = named(&given, fields) else {
            let names: Vec<_> = fields.iter().map(|name| rust_name(name)).collect();
            return Err(ShapeError(format!(
                "no field `{given}`, of {}",
                names.join(", ")
            )));
        };
        seed.deserialize(field.into_deserializer()).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, ShapeError> {
        let value = self.value.take().unwrap_or(Value::Nil);
        seed.deserialize(self.of.at(value))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

/// A variant given by its name, of its value, if it has one.
struct Variant<'b> {
    variant: &'static str,
    value: Option<Taking<'b>>,
}

/// A variant's value, if it has one.
struct Inside<'b>(Option<Taking<'b>>);

impl<'de, 'b> EnumAccess<'de> for Variant<'b> {
    type Error = ShapeError;
    type Variant = Inside<'b>;

    fn variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<(T::Value, Self::Variant), ShapeError> {
        let name: de::value::StrDeserializer<'_, ShapeError> = self.variant.into_deserializer();
        let variant = seed.deserialize(name)?;
        Ok((variant, Inside(self.value)))
    }
}

impl<'de> VariantAccess<'de> for Inside<'_> {
    type Error = ShapeError;

    fn unit_variant(self) -> Result<(), ShapeError> {
        match self.0 {
            None => Ok(()),
            Some(_) => Err(ShapeError(
                "a variant of no value is its name alone, a text".to_owned(),
            )),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, ShapeError> {
        match self.0 {
            Some(value) => seed.deserialize(value),
            None => Err(ShapeError(
                "a variant of a value is a table of one field, its name's".to_owned(),
            )),
        }
    }

    fn tuple_variant<V: Visitor<'de>>(
        self,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, ShapeError> {
        match self.0 {
            Some(value) => de::Deserializer::deserialize_tuple(value, length, visitor),
            None => Err(ShapeError(
                "a variant of values is a table of one field, its name's".to_owned(),
            )),
        }
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, ShapeError> {
        match self.0 {
            Some(value) => de::Deserializer::deserialize_struct(value, "", fields, visitor),
            None => Err(ShapeError(
                "a variant of fields is a table of one field, its name's".to_owned(),
            )),
        }
    }
}

/// A plain value as the variant of `Data` or of a command's `Value` that
/// its kind is.
struct Plain<'b> {
    variant: &'static str,
    value: Taking<'b>,
}

impl<'b> Plain<'b> {
    /// The variant of `name`, `Data` or `Value`, of `value`.
    fn of(name: &'static str, mut value: Taking<'b>) -> Result<Self, ShapeError> {
        let data = name == "Data";
        let variant = match &value.value {
            Value::Nil if data => "nil",
            Value::Boolean(_) => "boolean",
            Value::Integer(_) => "integer",
            Value::Number(number) if whole(*number).is_some() => "integer",
            Value::Number(_) => "number",
            Value::String(_) if data => "text",
            Value::String(_) => "string",
            // An empty table a map, as Lua's JSON libraries take it.
            Value::Table(table) if data => {
                value.enter()?;
                if value.data >= Data::DEEPEST {
                    return Err(ShapeError(format!(
                        "a plain value nests {} levels of tables at most",
                        Data::DEEPEST
                    )));
                }
                value.data += 1;
                if sequence(table, value.budget)?.is_some_and(|items| !items.is_empty()) {
                    "list"
                } else {
                    "map"
                }
            }
            _ if data => {
                return Err(
                    value.expected("data: nil, a boolean, a number, a text, or a table of them")
                );
            }
            _ => return Err(value.expected("a text, a number or a boolean")),
        };
        Ok(Self { variant, value })
    }
}

impl<'de, 'b> EnumAccess<'de> for Plain<'b> {
    type Error = ShapeError;
    type Variant = Self;

    fn variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<(T::Value, Self), ShapeError> {
        let name: de::value::StrDeserializer<'_, ShapeError> = self.variant.into_deserializer();
        let variant = seed.deserialize(name)?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for Plain<'_> {
    type Error = ShapeError;

    fn unit_variant(self) -> Result<(), ShapeError> {
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, ShapeError> {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V: Visitor<'de>>(self, _: usize, _: V) -> Result<V::Value, ShapeError> {
        Err(ShapeError("a plain value has no values".to_owned()))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, ShapeError> {
        Err(ShapeError("a plain value has no fields".to_owned()))
    }
}

/// The Luau value of `value`, of the host's.
pub(crate) fn to_lua<T: Serialize + ?Sized>(lua: &Lua, value: &T) -> Result<Value, ShapeError> {
    value.serialize(Giving { lua, level: 0 })
}

/// A value of the host's as a Luau value.
#[derive(Clone, Copy)]
struct Giving<'l> {
    lua: &'l Lua,
    level: usize,
}

impl Giving<'_> {
    fn inner(self) -> Result<Self, ShapeError> {
        if self.level + 1 > MOST_LEVELS {
            return Err(ShapeError(format!(
                "a value for a plugin nests {MOST_LEVELS} levels at most"
            )));
        }
        Ok(Self {
            lua: self.lua,
            level: self.level + 1,
        })
    }

    fn table(self) -> Result<Table, ShapeError> {
        self.lua.create_table().map_err(ShapeError::from)
    }

    fn text(self, text: &str) -> Result<Value, ShapeError> {
        Ok(Value::String(self.lua.create_string(text)?))
    }

    /// A table of one field, `variant`'s Rust name, of `value`.
    fn variant(self, variant: &str, value: Value) -> Result<Value, ShapeError> {
        let table = self.table()?;
        table.raw_set(rust_name(variant), value)?;
        Ok(Value::Table(table))
    }
}

/// A sequence that a value of the host's makes.
struct GivingItems<'l> {
    giving: Giving<'l>,
    table: Table,
    next: usize,
    variant: Option<&'static str>,
}

impl ser::SerializeSeq for GivingItems<'_> {
    type Ok = Value;
    type Error = ShapeError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        self.next += 1;
        let value = value.serialize(self.giving.inner()?)?;
        self.table.raw_set(self.next, value)?;
        Ok(())
    }

    fn end(self) -> Result<Value, ShapeError> {
        match self.variant {
            Some(variant) => self.giving.variant(variant, Value::Table(self.table)),
            None => Ok(Value::Table(self.table)),
        }
    }
}

impl ser::SerializeTuple for GivingItems<'_> {
    type Ok = Value;
    type Error = ShapeError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Value, ShapeError> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for GivingItems<'_> {
    type Ok = Value;
    type Error = ShapeError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Value, ShapeError> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleVariant for GivingItems<'_> {
    type Ok = Value;
    type Error = ShapeError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Value, ShapeError> {
        ser::SerializeSeq::end(self)
    }
}

/// A table that a map or a struct of the host's makes.
struct GivingFields<'l> {
    giving: Giving<'l>,
    table: Table,
    key: Option<Value>,
    variant: Option<&'static str>,
}

impl ser::SerializeMap for GivingFields<'_> {
    type Ok = Value;
    type Error = ShapeError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), ShapeError> {
        self.key = Some(key.serialize(self.giving.inner()?)?);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ShapeError> {
        let key = self.key.take().unwrap_or(Value::Nil);
        if key.is_nil() {
            return Err(ShapeError("a map's key for a plugin is not nil".to_owned()));
        }
        let value = value.serialize(self.giving.inner()?)?;
        self.table.raw_set(key, value)?;
        Ok(())
    }

    fn end(self) -> Result<Value, ShapeError> {
        Ok(Value::Table(self.table))
    }
}

impl ser::SerializeStruct for GivingFields<'_> {
    type Ok = Value;
    type Error = ShapeError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), ShapeError> {
        let value = value.serialize(self.giving.inner()?)?;
        if !value.is_nil() {
            self.table.raw_set(rust_name(key), value)?;
        }
        Ok(())
    }

    fn end(self) -> Result<Value, ShapeError> {
        match self.variant {
            Some(variant) => self.giving.variant(variant, Value::Table(self.table)),
            None => Ok(Value::Table(self.table)),
        }
    }
}

impl ser::SerializeStructVariant for GivingFields<'_> {
    type Ok = Value;
    type Error = ShapeError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), ShapeError> {
        ser::SerializeStruct::serialize_field(self, key, value)
    }

    fn end(self) -> Result<Value, ShapeError> {
        ser::SerializeStruct::end(self)
    }
}

impl<'l> ser::Serializer for Giving<'l> {
    type Ok = Value;
    type Error = ShapeError;
    type SerializeSeq = GivingItems<'l>;
    type SerializeTuple = GivingItems<'l>;
    type SerializeTupleStruct = GivingItems<'l>;
    type SerializeTupleVariant = GivingItems<'l>;
    type SerializeMap = GivingFields<'l>;
    type SerializeStruct = GivingFields<'l>;
    type SerializeStructVariant = GivingFields<'l>;

    fn serialize_bool(self, value: bool) -> Result<Value, ShapeError> {
        Ok(Value::Boolean(value))
    }

    fn serialize_i8(self, value: i8) -> Result<Value, ShapeError> {
        Ok(Value::Integer(value.into()))
    }

    fn serialize_i16(self, value: i16) -> Result<Value, ShapeError> {
        Ok(Value::Integer(value.into()))
    }

    fn serialize_i32(self, value: i32) -> Result<Value, ShapeError> {
        Ok(Value::Integer(value.into()))
    }

    fn serialize_i64(self, value: i64) -> Result<Value, ShapeError> {
        Ok(Value::Integer(value))
    }

    fn serialize_u8(self, value: u8) -> Result<Value, ShapeError> {
        Ok(Value::Integer(value.into()))
    }

    fn serialize_u16(self, value: u16) -> Result<Value, ShapeError> {
        Ok(Value::Integer(value.into()))
    }

    fn serialize_u32(self, value: u32) -> Result<Value, ShapeError> {
        Ok(Value::Integer(value.into()))
    }

    fn serialize_u64(self, value: u64) -> Result<Value, ShapeError> {
        #[allow(clippy::cast_precision_loss)]
        Ok(i64::try_from(value).map_or(Value::Number(value as f64), Value::Integer))
    }

    fn serialize_f32(self, value: f32) -> Result<Value, ShapeError> {
        Ok(Value::Number(value.into()))
    }

    fn serialize_f64(self, value: f64) -> Result<Value, ShapeError> {
        Ok(Value::Number(value))
    }

    fn serialize_char(self, value: char) -> Result<Value, ShapeError> {
        self.text(value.encode_utf8(&mut [0; 4]))
    }

    fn serialize_str(self, value: &str) -> Result<Value, ShapeError> {
        self.text(value)
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Value, ShapeError> {
        Ok(Value::String(self.lua.create_string(value)?))
    }

    fn serialize_none(self) -> Result<Value, ShapeError> {
        Ok(Value::Nil)
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Value, ShapeError> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Value, ShapeError> {
        Ok(Value::Nil)
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<Value, ShapeError> {
        Ok(Value::Nil)
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Value, ShapeError> {
        // `Data`'s nil.
        if PLAIN.contains(&name) {
            return Ok(Value::Nil);
        }
        self.text(variant)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<Value, ShapeError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value, ShapeError> {
        if PLAIN.contains(&name) {
            return value.serialize(self);
        }
        let value = value.serialize(self.inner()?)?;
        self.variant(variant, value)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<GivingItems<'l>, ShapeError> {
        Ok(GivingItems {
            giving: self,
            table: self.table()?,
            next: 0,
            variant: None,
        })
    }

    fn serialize_tuple(self, length: usize) -> Result<GivingItems<'l>, ShapeError> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        length: usize,
    ) -> Result<GivingItems<'l>, ShapeError> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<GivingItems<'l>, ShapeError> {
        Ok(GivingItems {
            giving: self,
            table: self.table()?,
            next: 0,
            variant: Some(variant),
        })
    }

    fn serialize_map(self, _: Option<usize>) -> Result<GivingFields<'l>, ShapeError> {
        Ok(GivingFields {
            giving: self,
            table: self.table()?,
            key: None,
            variant: None,
        })
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<GivingFields<'l>, ShapeError> {
        self.serialize_map(None)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<GivingFields<'l>, ShapeError> {
        Ok(GivingFields {
            giving: self,
            table: self.table()?,
            key: None,
            variant: Some(variant),
        })
    }
}

#[cfg(test)]
mod tests;
