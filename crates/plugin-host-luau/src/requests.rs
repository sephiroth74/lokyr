//! The SDK's requests as `cx`'s functions: one a kind of [`Request`], of
//! its Rust name, as `cx.line_text`, read from the SDK's own type, so that
//! a new request reaches Luau at once; its arguments its fields in their
//! order, its value, or none; its answer the response's value, `true` of a
//! write, or `nil` and the error.

use std::cell::Cell;
use std::fmt;
use std::sync::OnceLock;

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::message::{Request, Response};
use mlua::{Lua, MultiValue, Table, Value};
use serde::Deserialize;
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};

use crate::shape::{self, MOST_VALUES, ShapeError, Taking};

/// What a request takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Params {
    /// Nothing.
    None,
    /// A value.
    One,
    /// Values, in their order.
    Values(usize),
    /// Its fields, by their names as serde gives them, in their order.
    Fields(&'static [&'static str]),
}

impl Params {
    /// How many arguments it takes.
    fn count(self) -> usize {
        match self {
            Self::None => 0,
            Self::One => 1,
            Self::Values(count) => count,
            Self::Fields(fields) => fields.len(),
        }
    }
}

/// A kind of request, as Luau calls it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Kind {
    /// Its function's name, the Rust name of the request's variant.
    pub(crate) name: String,
    /// Its variant's place among the request's.
    index: u32,
    /// What it takes.
    pub(crate) params: Params,
}

/// The kinds of the SDK's requests, in the order of its type's variants,
/// as serde tells them.
pub(crate) fn kinds() -> &'static [Kind] {
    static KINDS: OnceLock<Vec<Kind>> = OnceLock::new();
    KINDS.get_or_init(|| {
        let Err(Traced::Variants(variants)) = Request::deserialize(Tracing) else {
            return Vec::new();
        };
        (0..)
            .zip(variants)
            .filter_map(|(index, variant)| {
                let Err(Traced::Params(params)) = Request::deserialize(Picking(index)) else {
                    return None;
                };
                Some(Kind {
                    name: variant.replace('-', "_"),
                    index,
                    params,
                })
            })
            .collect()
    })
}

/// What tracing a type tells: its variants, a variant's params, or that it
/// is none of an enum.
#[derive(Debug)]
enum Traced {
    Variants(&'static [&'static str]),
    Params(Params),
    Other,
}

impl fmt::Display for Traced {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Traced {}

impl de::Error for Traced {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self::Other
    }
}

/// What tells an enum's variants.
struct Tracing;

impl<'de> de::Deserializer<'de> for Tracing {
    type Error = Traced;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Traced> {
        Err(Traced::Other)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        variants: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Traced> {
        Err(Traced::Variants(variants))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct identifier ignored_any
    }
}

/// What tells the params of an enum's variant at its place.
struct Picking(u32);

impl<'de> de::Deserializer<'de> for Picking {
    type Error = Traced;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Traced> {
        Err(Traced::Other)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Traced> {
        visitor.visit_enum(self)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct identifier ignored_any
    }
}

impl<'de> EnumAccess<'de> for Picking {
    type Error = Traced;
    type Variant = Self;

    fn variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<(T::Value, Self), Traced> {
        let index: de::value::U32Deserializer<Traced> = self.0.into_deserializer();
        let variant = seed.deserialize(index)?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for Picking {
    type Error = Traced;

    fn unit_variant(self) -> Result<(), Traced> {
        Err(Traced::Params(Params::None))
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, _: T) -> Result<T::Value, Traced> {
        Err(Traced::Params(Params::One))
    }

    fn tuple_variant<V: Visitor<'de>>(self, count: usize, _: V) -> Result<V::Value, Traced> {
        Err(Traced::Params(Params::Values(count)))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Traced> {
        Err(Traced::Params(Params::Fields(fields)))
    }
}

/// The request of `kind` of a plugin's `args`, its call's: why not, of an
/// argument of another shape or of more arguments than it takes, a `nil`
/// of an optional one leaving it out.
pub(crate) fn request(kind: &Kind, args: &[Value]) -> Result<Request, ShapeError> {
    let given = args
        .iter()
        .rposition(|arg| !arg.is_nil())
        .map_or(0, |at| at + 1);
    let takes = kind.params.count();
    if given > takes {
        return Err(ShapeError(format!(
            "it takes {takes} argument{}, not {given}",
            if takes == 1 { "" } else { "s" }
        )));
    }
    // A field of `nil` is left out, its default or its none, unless the
    // request lacks it then, as a plain value's, whose `nil` is its value.
    let mut kept = vec![false; args.len()];
    loop {
        let budget = Cell::new(MOST_VALUES);
        let asked = Request::deserialize(Asking {
            kind,
            args: args.to_vec(),
            kept: &kept,
            budget: &budget,
        });
        let Err(error) = &asked else {
            return asked;
        };
        let Params::Fields(fields) = kind.params else {
            return asked;
        };
        let lacked = fields
            .iter()
            .position(|field| error.0 == format!("missing field `{field}`"));
        match lacked {
            Some(at) if at < args.len() && !kept[at] => kept[at] = true,
            _ => return asked,
        }
    }
}

/// A call's arguments as the request of a kind: those of `nil` left out
/// but where `kept` says.
struct Asking<'k, 'b> {
    kind: &'k Kind,
    args: Vec<Value>,
    kept: &'b [bool],
    budget: &'b Cell<usize>,
}

impl Asking<'_, '_> {
    /// The argument at `at`, of `nil` past those given.
    fn arg(&mut self, at: usize) -> Value {
        self.args.get_mut(at).map_or(Value::Nil, std::mem::take)
    }
}

impl<'de> de::Deserializer<'de> for Asking<'_, '_> {
    type Error = ShapeError;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, ShapeError> {
        Err(ShapeError("a request is a variant".to_owned()))
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, ShapeError> {
        visitor.visit_enum(self)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct identifier ignored_any
    }
}

impl<'de> EnumAccess<'de> for Asking<'_, '_> {
    type Error = ShapeError;
    type Variant = Self;

    fn variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<(T::Value, Self), ShapeError> {
        let index: de::value::U32Deserializer<ShapeError> = self.kind.index.into_deserializer();
        let variant = seed.deserialize(index)?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for Asking<'_, '_> {
    type Error = ShapeError;

    fn unit_variant(self) -> Result<(), ShapeError> {
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        mut self,
        seed: T,
    ) -> Result<T::Value, ShapeError> {
        let arg = self.arg(0);
        seed.deserialize(Taking::new(arg, self.budget))
            .map_err(|error| error.within("argument 1"))
    }

    fn tuple_variant<V: Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value, ShapeError> {
        visitor.visit_seq(Positional {
            asking: self,
            fields: None,
            next: 0,
        })
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, ShapeError> {
        visitor.visit_map(Positional {
            asking: self,
            fields: Some(fields),
            next: 0,
        })
    }
}

/// A call's arguments as a request's values, or its fields, each by its
/// place.
struct Positional<'k, 'b> {
    asking: Asking<'k, 'b>,
    fields: Option<&'static [&'static str]>,
    next: usize,
}

impl Positional<'_, '_> {
    /// The place of the argument at hand, of its field's Rust name.
    fn place(&self) -> String {
        let at = self.next;
        match self.fields.and_then(|fields| fields.get(at - 1)) {
            Some(field) => format!("argument {at} (`{}`)", field.replace('-', "_")),
            None => format!("argument {at}"),
        }
    }
}

impl<'de> SeqAccess<'de> for Positional<'_, '_> {
    type Error = ShapeError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, ShapeError> {
        let arg = self.asking.arg(self.next);
        self.next += 1;
        seed.deserialize(Taking::new(arg, self.asking.budget))
            .map(Some)
            .map_err(|error| error.within(self.place()))
    }
}

impl<'de> MapAccess<'de> for Positional<'_, '_> {
    type Error = ShapeError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, ShapeError> {
        let fields = self.fields.unwrap_or_default();
        // A field of `nil` left out, as its default or its none.
        while self.asking.args.get(self.next).is_some_and(Value::is_nil)
            && !self.asking.kept.get(self.next).copied().unwrap_or(false)
        {
            self.next += 1;
        }
        let Some(field) = fields.get(self.next) else {
            return Ok(None);
        };
        if self.next >= self.asking.args.len() {
            return Ok(None);
        }
        seed.deserialize((*field).into_deserializer()).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, ShapeError> {
        let arg = self.asking.arg(self.next);
        self.next += 1;
        seed.deserialize(Taking::new(arg, self.asking.budget))
            .map_err(|error| error.within(self.place()))
    }
}

/// The answer of `response` to a request, as its Luau function returns
/// it: `true` of a write done, a declaration taken or a request of the
/// app's taken; `nil` and the error, a table of its `code`, its `kind` and
/// its `message`, of `errors`' metatable, of a failure; or the response's
/// value, `nil` of none.
pub(crate) fn answer(
    lua: &Lua,
    response: &Response,
    errors: &Table,
) -> Result<MultiValue, ShapeError> {
    let values = match response {
        Response::Done => vec![Value::Boolean(true)],
        Response::Failed(error) => vec![Value::Nil, Value::Table(error_of(lua, error, errors)?)],
        other => {
            // A variant of a value: a table of its one field, or of none as
            // its value is none.
            let Value::Table(variant) = shape::to_lua(lua, other)? else {
                return Err(ShapeError(
                    "a response of a value is a variant of one".to_owned(),
                ));
            };
            let value = variant
                .pairs::<Value, Value>()
                .next()
                .transpose()?
                .map_or(Value::Nil, |(_, value)| value);
            vec![value]
        }
    };
    Ok(MultiValue::from_vec(values))
}

/// The table of `error`, of `errors`' metatable, which gives its message as
/// a text, as an error raised of it tells.
pub(crate) fn error_of(lua: &Lua, error: &Error, errors: &Table) -> Result<Table, ShapeError> {
    let table = lua.create_table()?;
    table.raw_set("code", u16::from(error.code))?;
    table.raw_set("kind", error.code.kind())?;
    table.raw_set("message", error.message.as_str())?;
    table.set_metatable(Some(errors.clone()))?;
    Ok(table)
}

/// The metatable of the errors' tables: their message as their text, and
/// none for the plugin to change.
pub(crate) fn errors(lua: &Lua) -> mlua::Result<Table> {
    let metatable = lua.create_table()?;
    metatable.raw_set(
        "__tostring",
        lua.create_function(|_, error: Table| error.raw_get::<Value>("message"))?,
    )?;
    metatable.raw_set("__metatable", false)?;
    metatable.set_readonly(true);
    Ok(metatable)
}

/// The function's name and the arguments of `request`'s call, as a plugin
/// makes it, which the tests check against the request that it makes.
#[cfg(any(test, feature = "testing"))]
pub(crate) fn call_of(lua: &Lua, request: &Request) -> Result<(String, Vec<Value>), ShapeError> {
    // Its variant's name, as its debug's text starts with it, which a
    // table of a value of none, as `PreviewTheme(None)`'s, lacks.
    let debug = format!("{request:?}");
    let mut name = String::new();
    for char in debug.chars().take_while(char::is_ascii_alphanumeric) {
        if char.is_ascii_uppercase() && !name.is_empty() {
            name.push('_');
        }
        name.push(char.to_ascii_lowercase());
    }
    let value = match shape::to_lua(lua, request)? {
        Value::Table(table) => table
            .pairs::<String, Value>()
            .next()
            .transpose()?
            .map_or(Value::Nil, |(_, value)| value),
        _ => Value::Nil,
    };
    let kind = kinds()
        .iter()
        .find(|kind| kind.name == name)
        .ok_or_else(|| ShapeError(format!("no kind of request `{name}`")))?;
    let args = match (kind.params, value) {
        (Params::None, _) => Vec::new(),
        (Params::One, value) => vec![value],
        (Params::Values(_), Value::Table(values)) => values
            .sequence_values::<Value>()
            .collect::<mlua::Result<_>>()?,
        (Params::Fields(fields), Value::Table(table)) => fields
            .iter()
            .map(|field| table.raw_get::<Value>(field.replace('-', "_")))
            .collect::<mlua::Result<_>>()?,
        (_, other) => return Err(ShapeError(format!("`{name}`'s values, not {other:?}"))),
    };
    Ok((name, args))
}

/// Whether `answer` is the answer of `response`, as it came back from a
/// plugin's call: `true` of a write; `nil` and the error, its fields and
/// its text, of a failure; or the response's value, which takes the
/// response back; why not.
#[cfg(any(test, feature = "testing"))]
pub(crate) fn answered(lua: &Lua, response: &Response, answer: &MultiValue) -> Result<(), String> {
    let first = answer.front().cloned().unwrap_or(Value::Nil);
    let second = answer.get(1).cloned().unwrap_or(Value::Nil);
    match response {
        Response::Done => matches!(first, Value::Boolean(true)) && second.is_nil(),
        Response::Failed(error) => {
            let Value::Table(table) = second else {
                return Err(format!("{response:?} answered {answer:?}, of no error"));
            };
            let text = lua
                .globals()
                .get::<mlua::Function>("tostring")
                .and_then(|tostring| tostring.call::<String>(&table))
                .map_err(|error| error.to_string())?;
            let field = |name: &str| table.raw_get::<Value>(name).unwrap_or(Value::Nil);
            first.is_nil()
                && field("code").as_u32() == Some(u32::from(u16::from(error.code)))
                && field("kind").to_string().ok().as_deref() == Some(error.code.kind())
                && field("message").to_string().ok() == Some(error.message.clone())
                && text == error.message
        }
        other => {
            let Value::Table(variant) =
                shape::to_lua(lua, other).map_err(|error| error.to_string())?
            else {
                return Err(format!("{other:?} is a variant of a value"));
            };
            match variant.pairs::<String, Value>().next().transpose() {
                // A value of none: `nil`.
                Ok(None) => first.is_nil() && second.is_nil(),
                Ok(Some((name, _))) => {
                    let back = lua
                        .create_table()
                        .and_then(|table| table.raw_set(name, first).map(|()| table))
                        .map_err(|error| error.to_string())?;
                    second.is_nil()
                        && shape::from_lua::<Response>(Value::Table(back)).as_ref() == Ok(other)
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    .then_some(())
    .ok_or_else(|| format!("{response:?} answered {answer:?}"))
}

#[cfg(test)]
mod tests;
