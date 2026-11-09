//! Whether two Luau values are the same, their tables of the same entries,
//! as the tests compare a value that the host gives with the one that they
//! expect.

use mlua::Value;

use crate::shape::MOST_LEVELS;

/// Whether `a` and `b` are the same value, at `level` of tables.
pub(crate) fn value(a: &Value, b: &Value, level: usize) -> bool {
    if level > MOST_LEVELS {
        return false;
    }
    let number = |value: &Value| match value {
        #[allow(clippy::cast_precision_loss)]
        Value::Integer(integer) => Some(*integer as f64),
        Value::Number(number) => Some(*number),
        _ => None,
    };
    match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::String(a), Value::String(b)) => a.as_bytes() == b.as_bytes(),
        (Value::Table(a), Value::Table(b)) => {
            let entries = |table: &mlua::Table| {
                table
                    .pairs::<Value, Value>()
                    .collect::<mlua::Result<Vec<_>>>()
                    .unwrap_or_default()
            };
            let (from_a, from_b) = (entries(a), entries(b));
            from_a.len() == from_b.len()
                && from_a.iter().all(|(key, from_a)| {
                    b.raw_get::<Value>(key.clone())
                        .is_ok_and(|from_b| value(from_a, &from_b, level + 1))
                })
        }
        (a, b) => number(a).is_some_and(|a| number(b) == Some(a)),
    }
}
