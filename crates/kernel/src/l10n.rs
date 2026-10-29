//! Localization: the catalogs of the texts that Lokyr shows, in [Project
//! Fluent](https://projectfluent.org)'s format, a set for each language,
//! and the language that the user reads them in.
//!
//! English is the source, which every other catalog follows, and whose
//! texts stand in for those that another lacks.

use std::collections::{BTreeMap, BTreeSet};

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use fluent_syntax::ast;
use unic_langid::LanguageIdentifier;

/// The language of the source catalogs.
pub const SOURCE: &str = "en";

/// The kernel's catalogs, by language.
pub const KERNEL: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/kernel.ftl")),
    ("it", include_str!("../locales/it/kernel.ftl")),
];

/// The value of a variable of a message.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg<'a> {
    /// Text.
    Text(&'a str),
    /// A number, which plural rules can select by.
    Number(i64),
}

/// The texts of a language, with the source's in place of those it lacks.
pub struct Texts {
    language: String,
    bundle: FluentBundle<FluentResource>,
    source: FluentBundle<FluentResource>,
}

impl std::fmt::Debug for Texts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Texts")
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}

/// A bundle of the catalogs of `catalogs` that are `language`'s.
fn bundle(language: &str, catalogs: &[(&str, &str)]) -> FluentBundle<FluentResource> {
    let id: LanguageIdentifier = language.parse().unwrap_or_default();
    let mut bundle = FluentBundle::new(vec![id]);
    // A terminal's line shows the marks that would isolate the variables.
    bundle.set_use_isolating(false);
    for (_, source) in catalogs.iter().filter(|(of, _)| *of == language) {
        let resource = match FluentResource::try_new((*source).to_owned()) {
            Ok(resource) => resource,
            // The entries that parse, which the catalogs' test keeps whole.
            Err((resource, _)) => resource,
        };
        // A message that two catalogs define keeps the first.
        let _ = bundle.add_resource(resource);
    }
    bundle
}

impl Texts {
    /// The texts of `catalogs`, Fluent's sources by their languages, in
    /// `language`.
    pub fn new(language: &str, catalogs: &[(&str, &str)]) -> Self {
        Self {
            language: language.to_owned(),
            bundle: bundle(language, catalogs),
            source: bundle(SOURCE, catalogs),
        }
    }

    /// The language that the texts are in.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// The message `id`.
    pub fn text(&self, id: &str) -> String {
        self.text_with(id, &[])
    }

    /// The message `id`, the language's or the source's, if either has it.
    pub fn get(&self, id: &str) -> Option<String> {
        [&self.bundle, &self.source].into_iter().find_map(|bundle| {
            let pattern = bundle.get_message(id)?.value()?;
            let mut errors = Vec::new();
            Some(
                bundle
                    .format_pattern(pattern, None, &mut errors)
                    .into_owned(),
            )
        })
    }

    /// The message `id`, with the values of its variables, `args`: the
    /// language's, or the source's where the language has none, or the id
    /// itself where neither has it.
    pub fn text_with(&self, id: &str, args: &[(&str, Arg<'_>)]) -> String {
        let mut values = FluentArgs::new();
        for (name, arg) in args {
            let value = match arg {
                Arg::Text(text) => FluentValue::from(*text),
                Arg::Number(number) => FluentValue::from(*number),
            };
            values.set(*name, value);
        }
        for bundle in [&self.bundle, &self.source] {
            if let Some(pattern) = bundle.get_message(id).and_then(|message| message.value()) {
                let mut errors = Vec::new();
                return bundle
                    .format_pattern(pattern, Some(&values), &mut errors)
                    .into_owned();
            }
        }
        id.to_owned()
    }
}

/// The languages of `catalogs`, each once, the source first.
pub fn languages<'a>(catalogs: &[(&'a str, &str)]) -> Vec<&'a str> {
    let mut languages = vec![SOURCE];
    for (language, _) in catalogs {
        if !languages.contains(language) {
            languages.push(language);
        }
    }
    languages
}

/// The language of a locale's name, as `it` of `it_CH` or `it-CH`,
/// lowercase.
fn language_of(locale: &str) -> String {
    locale
        .split(['_', '-'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// The language to show: `chosen`, as the config names it, when it is one
/// of `available`, as `it` or `it-IT`; with `"system"`, or a language that
/// none has, the first of the user's `preferred`, as `it_CH`, that is one
/// of them; otherwise the source. The second value tells whether `chosen`
/// named a language that none has.
pub fn choose(chosen: &str, preferred: &[String], available: &[&str]) -> (String, bool) {
    let has = |language: &str| available.contains(&language);
    let unknown = if chosen == "system" {
        false
    } else {
        let language = language_of(chosen);
        if has(&language) {
            return (language, false);
        }
        true
    };
    let language = preferred
        .iter()
        .map(|locale| language_of(locale))
        .find(|language| has(language))
        .unwrap_or_else(|| SOURCE.to_owned());
    (language, unknown)
}

/// The messages of a catalog, by id, each with the variables that its
/// value and its attributes use.
fn messages(source: &str) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let resource = fluent_syntax::parser::parse(source).map_err(|(_, errors)| {
        let error = &errors[0];
        format!(
            "an entry does not read, from byte {}: {:?}",
            error.pos.start, error.kind
        )
    })?;
    let mut messages = BTreeMap::new();
    for entry in resource.body {
        if let ast::Entry::Message(message) = entry {
            let mut variables = BTreeSet::new();
            if let Some(value) = &message.value {
                pattern_variables(value, &mut variables);
            }
            for attribute in &message.attributes {
                pattern_variables(&attribute.value, &mut variables);
            }
            messages.insert(message.id.name.to_owned(), variables);
        }
    }
    Ok(messages)
}

fn pattern_variables(pattern: &ast::Pattern<&str>, variables: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let ast::PatternElement::Placeable { expression } = element {
            expression_variables(expression, variables);
        }
    }
}

fn expression_variables(expression: &ast::Expression<&str>, variables: &mut BTreeSet<String>) {
    match expression {
        ast::Expression::Select { selector, variants } => {
            inline_variables(selector, variables);
            for variant in variants {
                pattern_variables(&variant.value, variables);
            }
        }
        ast::Expression::Inline(inline) => inline_variables(inline, variables),
    }
}

fn inline_variables(inline: &ast::InlineExpression<&str>, variables: &mut BTreeSet<String>) {
    match inline {
        ast::InlineExpression::VariableReference { id } => {
            variables.insert(id.name.to_owned());
        }
        ast::InlineExpression::FunctionReference { arguments, .. } => {
            for argument in &arguments.positional {
                inline_variables(argument, variables);
            }
            for argument in &arguments.named {
                inline_variables(&argument.value, variables);
            }
        }
        ast::InlineExpression::Placeable { expression } => {
            expression_variables(expression, variables);
        }
        _ => {}
    }
}

/// Checks that the catalog `other` has the messages of `source`, and no
/// other, each with the same variables.
pub fn check(source: &str, other: &str) -> Result<(), String> {
    let (source, other) = (messages(source)?, messages(other)?);
    for (id, variables) in &source {
        match other.get(id) {
            None => return Err(format!("`{id}` is missing")),
            Some(theirs) if theirs != variables => {
                return Err(format!(
                    "`{id}` takes {variables:?} in the source, and {theirs:?} here"
                ));
            }
            Some(_) => {}
        }
    }
    if let Some(id) = other.keys().find(|id| !source.contains_key(*id)) {
        return Err(format!("`{id}` is not in the source"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_takes_its_language_s_message_or_the_source_s() {
        let catalogs = [
            ("en", "hello = Hello, { $name }\nonly = only in English\n"),
            ("it", "hello = Ciao, { $name }\n"),
        ];
        let texts = Texts::new("it", &catalogs);
        assert_eq!(texts.language(), "it");
        assert_eq!(
            texts.text_with("hello", &[("name", Arg::Text("Ada"))]),
            "Ciao, Ada"
        );
        assert_eq!(texts.text("only"), "only in English");
        assert_eq!(texts.text("nothing"), "nothing");
        let english = Texts::new("en", &catalogs);
        assert_eq!(
            english.text_with("hello", &[("name", Arg::Text("Ada"))]),
            "Hello, Ada"
        );
    }

    #[test]
    fn plurals_select_by_the_language_s_rules() {
        let catalogs = [(
            "it",
            "rows = { $count ->\n    [one] { $count } riga\n   *[other] { $count } righe\n}\n",
        )];
        let texts = Texts::new("it", &catalogs);
        assert_eq!(
            texts.text_with("rows", &[("count", Arg::Number(1))]),
            "1 riga"
        );
        assert_eq!(
            texts.text_with("rows", &[("count", Arg::Number(3))]),
            "3 righe"
        );
    }

    #[test]
    fn the_language_is_the_config_s_or_the_first_preferred_that_is_there() {
        let available = ["en", "it"];
        let preferred = |locales: &[&str]| -> Vec<String> {
            locales.iter().map(|&locale| locale.to_owned()).collect()
        };
        let system = preferred(&["de_CH", "it_CH", "en_US"]);
        assert_eq!(
            choose("system", &system, &available),
            ("it".to_owned(), false)
        );
        assert_eq!(choose("en", &system, &available), ("en".to_owned(), false));
        assert_eq!(choose("it-IT", &[], &available), ("it".to_owned(), false));
        assert_eq!(choose("IT", &[], &available), ("it".to_owned(), false));
        // A language that the catalogs lack falls back to the system's.
        assert_eq!(choose("fr", &system, &available), ("it".to_owned(), true));
        assert_eq!(
            choose("system", &preferred(&["ja_JP"]), &available),
            ("en".to_owned(), false)
        );
        assert_eq!(
            languages(&[("it", ""), ("en", ""), ("it", "")]),
            ["en", "it"]
        );
    }

    #[test]
    fn catalogs_check_their_messages_against_the_source() {
        let source = "a = { $x } and { $y }\nb = plain\n    .tooltip = { $z }\n";
        assert_eq!(
            check(
                source,
                "b = altro\n    .tooltip = { $z }\na = { $y } e { $x }\n"
            ),
            Ok(())
        );
        assert!(
            check(source, "a = { $x } e { $y }\n")
                .unwrap_err()
                .contains("`b` is missing")
        );
        assert!(
            check(source, "a = { $x }\nb = altro\n    .tooltip = { $z }\n")
                .unwrap_err()
                .contains("`a`")
        );
        let extra = "a = { $x } e { $y }\nb = altro\n    .tooltip = { $z }\nc = in più\n";
        assert!(
            check(source, extra)
                .unwrap_err()
                .contains("`c` is not in the source")
        );
        assert!(check(source, "a = {").is_err());
    }

    #[test]
    fn the_kernel_s_catalogs_match_their_source() {
        let source = KERNEL[0].1;
        for (language, catalog) in &KERNEL[1..] {
            assert_eq!(check(source, catalog), Ok(()), "{language}");
        }
        let texts = Texts::new("it", &KERNEL);
        assert_eq!(
            texts.text_with("program-exited", &[("code", Arg::Number(3))]),
            "il programma è uscito con codice 3; un tasto chiude la finestra"
        );
    }
}
