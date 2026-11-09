//! The events as Luau's tables, both ways, of a flat shape: an event is a
//! table of its fields and its `kind`, its name, as `{ kind =
//! "tab-created", tab = 3, window = 1, index = 0 }`, the workspace's as the
//! others, an event of no fields its `kind` alone, as `{ kind =
//! "keys-changed" }`, and a panel's closing of its panel's `id`. A view's
//! event alike: its `view`, its node's `key`, and its `kind` and its
//! fields, as `{ view = { panel = "find" }, key = "query", kind = "submit",
//! text = "x", shift = false }`. Their fields are as the SDK's other types
//! are, a custom event's value plain.

use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::ui::UiEvent;
use mlua::{Lua, Table, Value};

use crate::shape::{self, ShapeError};

/// The variant of the workspace's events, which an event's table names by
/// their own names.
const WORKSPACE: &str = "workspace";

/// The field of the value of an event's variant of a value that is no
/// table, by the event's name, as a panel's closing names its panel.
const VALUES: [(&str, &str); 1] = [("panel-closed", "id")];

/// The field of the value of the event `kind`'s variant of a value that is
/// no table.
fn value_field(kind: &str) -> &'static str {
    VALUES
        .iter()
        .find(|(name, _)| *name == kind)
        .map_or("value", |(_, field)| field)
}

/// The table of `event`, of its flat shape.
pub(crate) fn to_lua(lua: &Lua, event: &Event) -> Result<Table, ShapeError> {
    flat(lua, shape::to_lua(lua, event)?, true)
}

/// The list of `events`' tables.
pub(crate) fn list_to_lua(lua: &Lua, events: &[Event]) -> Result<Table, ShapeError> {
    let list = lua.create_table_with_capacity(events.len(), 0)?;
    for event in events {
        list.raw_push(to_lua(lua, event)?)?;
    }
    Ok(list)
}

/// The flat table of `given`, an event's of its variant's shape: a text of
/// a variant of none, or a table of one field, the variant's, of its
/// fields or its value, the workspace's variant's of a variant of its own,
/// which `outer` lets it take.
fn flat(lua: &Lua, given: Value, outer: bool) -> Result<Table, ShapeError> {
    match given {
        Value::String(name) => {
            let table = lua.create_table()?;
            table.raw_set("kind", name)?;
            Ok(table)
        }
        Value::Table(variant) => {
            let mut entries = variant.pairs::<Value, Value>();
            let (Some(Ok((Value::String(name), value))), None) = (entries.next(), entries.next())
            else {
                return Err(ShapeError("an event is a variant of one".to_owned()));
            };
            let kind = name.to_str()?.replace('_', "-");
            if outer && kind == WORKSPACE {
                return flat(lua, value, false);
            }
            let fields = match value {
                Value::Table(fields) => fields,
                value => {
                    let fields = lua.create_table()?;
                    fields.raw_set(value_field(&kind), value)?;
                    fields
                }
            };
            fields.raw_set("kind", kind)?;
            Ok(fields)
        }
        other => Err(ShapeError(format!(
            "an event is a variant, not {}",
            shape::what(&other)
        ))),
    }
}

/// The table of `event`, a view's, of its flat shape.
pub(crate) fn ui_to_lua(lua: &Lua, event: &UiEvent) -> Result<Table, ShapeError> {
    let UiEvent { view, key, kind } = event;
    let Value::Table(table) = shape::to_lua(lua, kind)? else {
        return Err(ShapeError(
            "a view's event is a table of its kind".to_owned(),
        ));
    };
    table.raw_set("view", shape::to_lua(lua, view)?)?;
    table.raw_set("key", key.0.as_str())?;
    Ok(table)
}

#[cfg(any(test, feature = "testing"))]
pub(crate) use taking::{from_lua, ui_from_lua};

/// The events of Luau's tables, which the tests take back.
#[cfg(any(test, feature = "testing"))]
mod taking {
    use lokyr_plugin_sdk::event::WorkspaceEvent;
    use lokyr_plugin_sdk::message::Event;
    use lokyr_plugin_sdk::ui::UiEvent;
    use lokyr_plugin_sdk::view::{Key, UiEventKind};
    use mlua::{Lua, Table, Value};

    use super::{VALUES, WORKSPACE, value_field};
    use crate::shape::{self, MOST_VALUES, ShapeError};

    /// The fields of `table` but its `kind`, which it gives: why not, of a
    /// table of no `kind` of a text, or of too many values.
    fn split(lua: &Lua, value: Value, what: &str) -> Result<(String, Table), ShapeError> {
        let Value::Table(table) = value else {
            return Err(ShapeError(format!(
                "expected {what}, a table, not {}",
                shape::what(&value)
            )));
        };
        let rest = lua.create_table()?;
        let mut kind = None;
        for (key, value) in shape::entries(&table, MOST_VALUES)? {
            match (&key, value) {
                (Value::String(name), Value::String(given)) if name.as_bytes() == b"kind" => {
                    kind = Some(given.to_str()?.to_owned());
                }
                (Value::String(name), other) if name.as_bytes() == b"kind" => {
                    return Err(ShapeError(format!(
                        "`kind`: expected a text, not {}",
                        shape::what(&other)
                    )));
                }
                (_, value) => rest.raw_set(key, value)?,
            }
        }
        let kind = kind.ok_or_else(|| ShapeError(format!("{what} takes its `kind`")))?;
        Ok((kind, rest))
    }

    /// The event of `value`, a table of its flat shape: why not.
    pub(crate) fn from_lua(lua: &Lua, value: Value) -> Result<Event, ShapeError> {
        let (kind, rest) = split(lua, value, "an event")?;
        let variant = |name: &str, value: Value| -> Result<Value, ShapeError> {
            let table = lua.create_table()?;
            table.raw_set(name, value)?;
            Ok(Value::Table(table))
        };
        let field = value_field(&kind);
        let given = if WorkspaceEvent::NAMES.contains(&kind.as_str()) {
            variant(WORKSPACE, variant(&kind, Value::Table(rest))?)?
        } else if VALUES.iter().any(|(name, _)| *name == kind) {
            let value = rest.raw_get::<Value>(field)?;
            rest.raw_set(field, Value::Nil)?;
            if let Some(entry) = rest.pairs::<Value, Value>().next() {
                let (key, _) = entry?;
                return Err(ShapeError(format!(
                    "no field `{}` of a {kind} event, of {field}",
                    key.to_string().unwrap_or_default()
                )));
            }
            variant(&kind, value)?
        } else if rest.pairs::<Value, Value>().next().is_none() {
            Value::String(lua.create_string(&kind)?)
        } else {
            variant(&kind, Value::Table(rest))?
        };
        shape::from_lua(given)
    }

    /// The view's event of `value`, a table of its flat shape: why not.
    pub(crate) fn ui_from_lua(lua: &Lua, value: Value) -> Result<UiEvent, ShapeError> {
        let (kind, rest) = split(lua, value, "a view's event")?;
        let view = rest.raw_get::<Value>("view")?;
        let key = rest.raw_get::<Value>("key")?;
        rest.raw_set("view", Value::Nil)?;
        rest.raw_set("key", Value::Nil)?;
        rest.raw_set("kind", kind)?;
        Ok(UiEvent {
            view: shape::from_lua(view).map_err(|error| error.within("`view`"))?,
            key: Key(shape::from_lua(key).map_err(|error| error.within("`key`"))?),
            kind: shape::from_lua::<UiEventKind>(Value::Table(rest))?,
        })
    }
}

#[cfg(test)]
mod tests;
