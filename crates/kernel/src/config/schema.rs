//! The config's schema: a JSON Schema of its sections and their keys, each
//! of its type, its values, its range, its default and its description, of
//! the plugins' schemas' subset and of Lokyr's annotations of `x-`, each
//! key's platforms, its control where its type does not tell it, the
//! guide's text and the example's value, of which the configuration's
//! guide and the settings' page are made. Its tests hold it to the parser.

use std::sync::OnceLock;

use toml_edit::TableLike;

/// The schema, as the kernel holds it, JSON.
pub const SCHEMA: &str = include_str!("../../config.schema.json");

/// A text key's bound: its section, its name, the most characters of its
/// text, or of each of its list's texts.
struct Longest {
    section: String,
    key: String,
    most: usize,
}

/// The text keys that the schema bounds by `maxLength`, of a text or of a
/// list's texts, read once.
fn bounds() -> &'static [Longest] {
    static BOUNDS: OnceLock<Vec<Longest>> = OnceLock::new();
    BOUNDS.get_or_init(|| {
        let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
        let mut bounds = Vec::new();
        let sections = schema["properties"].as_object().into_iter().flatten();
        for (section, node) in sections {
            for (key, node) in node["properties"].as_object().into_iter().flatten() {
                let most = node["maxLength"]
                    .as_u64()
                    .or_else(|| node["items"]["maxLength"].as_u64());
                if let Some(most) = most.and_then(|most| usize::try_from(most).ok()) {
                    bounds.push(Longest {
                        section: section.clone(),
                        key: key.clone(),
                        most,
                    });
                }
            }
        }
        bounds
    })
}

/// Whether the texts of the config's `root` are within the schema's
/// bounds, each of its characters, a list's each of its own; the first
/// one past them, of its place and why, otherwise.
pub(super) fn check_lengths(
    root: &dyn TableLike,
) -> Result<(), (Option<std::ops::Range<usize>>, String)> {
    for bound in bounds() {
        let Some(value) = root
            .get(&bound.section)
            .and_then(toml_edit::Item::as_table_like)
            .and_then(|section| section.get(&bound.key))
            .and_then(toml_edit::Item::as_value)
        else {
            continue;
        };
        let texts: Vec<&toml_edit::Value> = match value {
            toml_edit::Value::Array(items) => items.iter().collect(),
            value => vec![value],
        };
        for text in texts {
            if text
                .as_str()
                .is_some_and(|text| text.chars().count() > bound.most)
            {
                return Err((
                    text.span(),
                    format!(
                        "`{}` in `[{}]` is a text of {} characters at most",
                        bound.key, bound.section, bound.most
                    ),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
