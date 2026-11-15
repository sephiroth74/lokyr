//! The rules of the interface's tokens: each token's color where a theme
//! gives none, an expression of the theme's terminal colors and of the
//! other tokens, which resolve the same way, so that a token that a theme
//! gives reaches every token whose rule names it.
//!
//! The rules are data, `rules.toml` beside this file, which tells their
//! expressions; the arithmetic is of the colors' encoded sRGB components,
//! as the interface draws them.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::OnceLock;

use super::{TerminalColors, ThemeColor};

/// The rules' file, of a rule a line.
const RULES: &str = include_str!("rules.toml");

/// A color of the rules' arithmetic: its sRGB components, encoded, and its
/// opacity, each from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    /// Red.
    pub red: f32,
    /// Green.
    pub green: f32,
    /// Blue.
    pub blue: f32,
    /// The opacity.
    pub alpha: f32,
}

impl Rgba {
    /// Opaque black.
    pub const BLACK: Self = Self::rgb8(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Self = Self::rgb8(255, 255, 255);
    /// Nothing: black, of no opacity.
    pub const CLEAR: Self = Self::BLACK.with_alpha(0.0);

    /// The opaque color of the components `red`, `green` and `blue`, each
    /// of 255 at most.
    const fn rgb8(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red: red as f32 / 255.0,
            green: green as f32 / 255.0,
            blue: blue as f32 / 255.0,
            alpha: 1.0,
        }
    }

    /// The color of a theme's `color`, its opacity too.
    pub fn of(color: ThemeColor) -> Self {
        Self::rgb8(color.red, color.green, color.blue).with_alpha(f32::from(color.alpha) / 255.0)
    }

    /// The color, at the opacity `alpha`.
    #[must_use]
    pub const fn with_alpha(self, alpha: f32) -> Self {
        Self { alpha, ..self }
    }
}

/// `from` mixed toward `to` by `share`, from 0, `from`, to 1, `to`, of
/// their encoded components, opaque.
pub fn mix(from: Rgba, to: Rgba, share: f32) -> Rgba {
    let lerp = |a: f32, b: f32| a + (b - a) * share;
    Rgba {
        red: lerp(from.red, to.red),
        green: lerp(from.green, to.green),
        blue: lerp(from.blue, to.blue),
        alpha: 1.0,
    }
}

/// The relative luminance of `color`, as WCAG 2 defines it, of its
/// encoded sRGB components.
pub fn luminance(color: Rgba) -> f32 {
    let linear = |value: f32| {
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.red) + 0.7152 * linear(color.green) + 0.0722 * linear(color.blue)
}

/// The contrast ratio of `a` and `b`, as WCAG 2 defines it, from 1 to 21.
pub fn contrast(a: Rgba, b: Rgba) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `text`, of a contrast with `background` of `ratio` at least: as it is
/// where it has it, or moved toward white or black, whichever reads
/// better on the background, as little as gives it, its opacity kept.
pub fn with_contrast(text: Rgba, background: Rgba, ratio: f32) -> Rgba {
    if ratio <= 1.0 || contrast(text, background) >= ratio {
        return text;
    }
    let extreme = contrasting(background);
    // The least share toward the extreme that gives the ratio, or the
    // extreme itself where none does, of a binary search.
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    for _ in 0..12 {
        let middle = (low + high) / 2.0;
        if contrast(mix(text, extreme, middle), background) >= ratio {
            high = middle;
        } else {
            low = middle;
        }
    }
    mix(text, extreme, high).with_alpha(text.alpha)
}

/// White or black, whichever reads better on `background`, white of a tie.
pub fn contrasting(background: Rgba) -> Rgba {
    if contrast(Rgba::WHITE, background) >= contrast(Rgba::BLACK, background) {
        Rgba::WHITE
    } else {
        Rgba::BLACK
    }
}

/// Why an expression does not read: its text, and what is wrong at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprError(String);

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExprError {}

/// One of the terminal's colors, as an expression names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminal {
    /// The default background.
    Background,
    /// The default text.
    Foreground,
    /// The cursor's, the text's where the theme gives none.
    Cursor,
    /// The selection's background, the token `field.selection` where the
    /// theme gives none.
    Selection,
    /// One of the 16 colors, from 0, black, to 15, bright white.
    Palette(u8),
}

/// The names of the 16 colors, as an expression writes them.
const PALETTE: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright-black",
    "bright-red",
    "bright-green",
    "bright-yellow",
    "bright-blue",
    "bright-magenta",
    "bright-cyan",
    "bright-white",
];

/// An expression of a color.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A color as it is written, `transparent` among them.
    Color(Rgba),
    /// One of the terminal's colors.
    Terminal(Terminal),
    /// A token, by its name.
    Token(String),
    /// A color at an opacity, from 0 to 1.
    Opacity(Box<Expr>, f32),
    /// A color mixed toward another by a share, from 0 to 1.
    Mix(Box<Expr>, Box<Expr>, f32),
    /// A color of the theme's least contrast on another.
    Readable(Box<Expr>, Box<Expr>),
    /// White or black, whichever reads better on a color.
    Contrasting(Box<Expr>),
    /// The first of the colors of a contrast at least on another, else the
    /// last.
    First(Vec<Expr>, Box<Expr>, f32),
}

/// A part of an expression's text.
#[derive(Clone, Debug, PartialEq)]
enum Lexeme {
    Name(String),
    Hex(String),
    Number(f32, bool),
    Open,
    Close,
    Comma,
    Slash,
    Colon,
}

/// The parts of `text`, or why one does not read.
fn lexemes(text: &str) -> Result<Vec<Lexeme>, ExprError> {
    let bytes = text.as_bytes();
    // The end of the run from `from` of the bytes that `keep` takes.
    let run = |from: usize, keep: fn(u8) -> bool| {
        let mut end = from;
        while end < bytes.len() && keep(bytes[end]) {
            end += 1;
        }
        end
    };
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b' ' | b'\t' => at += 1,
            c @ (b'(' | b')' | b',' | b'/' | b':') => {
                out.push(match c {
                    b'(' => Lexeme::Open,
                    b')' => Lexeme::Close,
                    b',' => Lexeme::Comma,
                    b'/' => Lexeme::Slash,
                    _ => Lexeme::Colon,
                });
                at += 1;
            }
            b'#' => {
                let end = run(at + 1, |c| c.is_ascii_hexdigit());
                out.push(Lexeme::Hex(text[at..end].to_owned()));
                at = end;
            }
            b'0'..=b'9' => {
                let end = run(at, |c| c.is_ascii_digit() || c == b'.');
                let number = &text[at..end];
                let value = number
                    .parse::<f32>()
                    .map_err(|_| ExprError(format!("`{number}` is no number")))?;
                at = end;
                let percent = bytes.get(at) == Some(&b'%');
                if percent {
                    at += 1;
                }
                out.push(Lexeme::Number(value, percent));
            }
            b'a'..=b'z' => {
                let end = run(at, |c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'.'
                });
                out.push(Lexeme::Name(text[at..end].to_owned()));
                at = end;
            }
            _ => {
                let c = text[at..].chars().next().unwrap_or_default();
                return Err(ExprError(format!(
                    "`{c}` has no place in a color's expression"
                )));
            }
        }
    }
    Ok(out)
}

/// An argument of a function, before its function takes it.
enum Arg {
    Expr(Expr),
    Number(f32, bool),
    Named(String, Box<Arg>),
}

/// The reader of an expression's parts.
struct Reader {
    lexemes: Vec<Lexeme>,
    at: usize,
}

impl Reader {
    fn peek(&self) -> Option<&Lexeme> {
        self.lexemes.get(self.at)
    }

    fn next(&mut self) -> Option<Lexeme> {
        let lexeme = self.lexemes.get(self.at).cloned();
        self.at += 1;
        lexeme
    }

    fn eat(&mut self, lexeme: &Lexeme) -> bool {
        let found = self.peek() == Some(lexeme);
        if found {
            self.at += 1;
        }
        found
    }

    /// An expression: a term, each `/ N%` after it an opacity.
    fn expr(&mut self) -> Result<Expr, ExprError> {
        let mut expr = self.term()?;
        while self.eat(&Lexeme::Slash) {
            match self.next() {
                Some(Lexeme::Number(value, true)) => {
                    expr = Expr::Opacity(Box::new(expr), share(value)?);
                }
                _ => return Err(ExprError("`/` takes an opacity, as `/ 50%`".to_owned())),
            }
        }
        Ok(expr)
    }

    /// A color, a name, or a function's call.
    fn term(&mut self) -> Result<Expr, ExprError> {
        match self.next() {
            Some(Lexeme::Hex(hex)) => hex
                .parse::<ThemeColor>()
                .map(|color| Expr::Color(Rgba::of(color)))
                .map_err(|error| ExprError(error.to_string())),
            Some(Lexeme::Name(name)) if self.eat(&Lexeme::Open) => self.call(&name),
            Some(Lexeme::Name(name)) => Ok(named(&name)),
            Some(Lexeme::Number(..)) => Err(ExprError(
                "a number is no color: a color is `#rrggbb`, a name, or a function's".to_owned(),
            )),
            Some(_) => Err(ExprError("a color is missing before it".to_owned())),
            None => Err(ExprError("a color is missing".to_owned())),
        }
    }

    /// The arguments of a call, after its `(`, to its `)`.
    fn args(&mut self) -> Result<Vec<Arg>, ExprError> {
        let mut args = Vec::new();
        if self.eat(&Lexeme::Close) {
            return Ok(args);
        }
        loop {
            let named = match (self.peek(), self.lexemes.get(self.at + 1)) {
                (Some(Lexeme::Name(name)), Some(Lexeme::Colon)) => {
                    let name = name.clone();
                    self.at += 2;
                    Some(name)
                }
                _ => None,
            };
            let arg = match self.peek() {
                Some(&Lexeme::Number(value, percent)) => {
                    self.at += 1;
                    Arg::Number(value, percent)
                }
                _ => Arg::Expr(self.expr()?),
            };
            args.push(match named {
                Some(name) => Arg::Named(name, Box::new(arg)),
                None => arg,
            });
            if self.eat(&Lexeme::Close) {
                return Ok(args);
            }
            if !self.eat(&Lexeme::Comma) {
                return Err(ExprError("`,` or `)` is missing".to_owned()));
            }
        }
    }

    /// The call of the function `name`, after its `(`.
    fn call(&mut self, name: &str) -> Result<Expr, ExprError> {
        let args = self.args()?;
        let wrong = |usage: &str| Err(ExprError(format!("`{name}` is written `{usage}`")));
        let mut positional = Vec::new();
        let mut named = BTreeMap::new();
        for arg in args {
            match arg {
                Arg::Named(key, value) => {
                    named.insert(key, *value);
                }
                other => positional.push(other),
            }
        }
        let take = |named: &mut BTreeMap<String, Arg>, key: &str| named.remove(key);
        match name {
            "mix" => match (positional.as_slice(), named.is_empty()) {
                ([Arg::Expr(from), Arg::Expr(to), Arg::Number(value, true)], true) => Ok(
                    Expr::Mix(Box::new(from.clone()), Box::new(to.clone()), share(*value)?),
                ),
                _ => wrong("mix(X, Y, N%)"),
            },
            "readable" => {
                let on = take(&mut named, "on");
                match (positional.as_slice(), on, named.is_empty()) {
                    ([Arg::Expr(color)], Some(Arg::Expr(on)), true) => {
                        Ok(Expr::Readable(Box::new(color.clone()), Box::new(on)))
                    }
                    _ => wrong("readable(X, on: Y)"),
                }
            }
            "contrasting" => match (positional.as_slice(), named.is_empty()) {
                ([Arg::Expr(color)], true) => Ok(Expr::Contrasting(Box::new(color.clone()))),
                _ => wrong("contrasting(X)"),
            },
            "first" => {
                let (on, at) = (take(&mut named, "on"), take(&mut named, "at"));
                let colors: Option<Vec<Expr>> = positional
                    .iter()
                    .map(|arg| match arg {
                        Arg::Expr(expr) => Some(expr.clone()),
                        _ => None,
                    })
                    .collect();
                match (colors, on, at, named.is_empty()) {
                    (Some(colors), Some(Arg::Expr(on)), Some(Arg::Number(ratio, false)), true)
                        if !colors.is_empty() && (1.0..=21.0).contains(&ratio) =>
                    {
                        Ok(Expr::First(colors, Box::new(on), ratio))
                    }
                    _ => wrong("first(X, Y, …, on: Z, at: R), R from 1 to 21"),
                }
            }
            _ => Err(ExprError(format!(
                "`{name}` is no function: they are `mix`, `readable`, `contrasting` and `first`"
            ))),
        }
    }
}

/// The share of a percentage, `value` from 0 to 100.
fn share(value: f32) -> Result<f32, ExprError> {
    if (0.0..=100.0).contains(&value) {
        Ok(value / 100.0)
    } else {
        Err(ExprError(format!("{value}% is not from 0% to 100%")))
    }
}

/// The expression of the name `name`: `transparent`, one of the terminal's
/// colors, or a token.
fn named(name: &str) -> Expr {
    match name {
        "transparent" => Expr::Color(Rgba::CLEAR),
        "background" => Expr::Terminal(Terminal::Background),
        "foreground" => Expr::Terminal(Terminal::Foreground),
        "cursor" => Expr::Terminal(Terminal::Cursor),
        "selection" => Expr::Terminal(Terminal::Selection),
        _ => match PALETTE.iter().position(|&color| color == name) {
            Some(at) => Expr::Terminal(Terminal::Palette(at as u8)),
            None => Expr::Token(name.to_owned()),
        },
    }
}

impl std::str::FromStr for Expr {
    type Err = ExprError;

    /// The expression that `text` writes, whole.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut reader = Reader {
            lexemes: lexemes(text)?,
            at: 0,
        };
        let expr = reader.expr()?;
        match reader.peek() {
            None => Ok(expr),
            Some(_) => Err(ExprError(format!(
                "`{text}` goes on past its color: one color an expression"
            ))),
        }
    }
}

impl Expr {
    /// The tokens that the expression names, in their order.
    pub fn tokens(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.visit(&mut |expr| {
            if let Expr::Token(name) = expr {
                out.push(name.as_str());
            }
        });
        out
    }

    /// Calls `seen` of the expression and of each of its parts.
    fn visit<'a>(&'a self, seen: &mut impl FnMut(&'a Expr)) {
        seen(self);
        match self {
            Expr::Color(_) | Expr::Terminal(_) | Expr::Token(_) => {}
            Expr::Opacity(expr, _) | Expr::Contrasting(expr) => expr.visit(seen),
            Expr::Mix(a, b, _) | Expr::Readable(a, b) => {
                a.visit(seen);
                b.visit(seen);
            }
            Expr::First(colors, on, _) => {
                for color in colors {
                    color.visit(seen);
                }
                on.visit(seen);
            }
        }
    }
}

/// A token's rule: its text and its expression of each variant.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    /// The light variant's text.
    pub light: &'static str,
    /// The dark variant's text.
    pub dark: &'static str,
    /// The light variant's expression.
    light_expr: Expr,
    /// The dark variant's expression.
    dark_expr: Expr,
}

impl Rule {
    /// The text of the variant, dark or light.
    pub fn text(&self, dark: bool) -> &'static str {
        if dark { self.dark } else { self.light }
    }

    /// The expression of the variant, dark or light.
    pub fn expr(&self, dark: bool) -> &Expr {
        if dark {
            &self.dark_expr
        } else {
            &self.light_expr
        }
    }
}

/// The rules of the file, in its order, each of its token's name.
fn rules() -> &'static [(&'static str, Rule)] {
    static RULES_READ: OnceLock<Vec<(&'static str, Rule)>> = OnceLock::new();
    RULES_READ.get_or_init(|| read_rules(RULES).unwrap_or_else(|error| panic!("{error}")))
}

/// The rules of `text`, a rule a line, or why a line does not read.
fn read_rules(text: &'static str) -> Result<Vec<(&'static str, Rule)>, String> {
    let mut out: Vec<(&'static str, Rule)> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let wrong = |why: &str| format!("rules.toml:{}: {why}", number + 1);
        let quoted = |text: &'static str| -> Option<(&'static str, &'static str)> {
            let text = text.strip_prefix('"')?;
            let end = text.find('"')?;
            Some((&text[..end], &text[end + 1..]))
        };
        let (name, rest) = quoted(line).ok_or_else(|| wrong("a rule is `\"token\" = …`"))?;
        let rest = rest
            .trim_start()
            .strip_prefix('=')
            .ok_or_else(|| wrong("`=` is missing"))?
            .trim();
        let (light, dark) = if let Some(table) = rest.strip_prefix('{') {
            let table = table
                .strip_suffix('}')
                .ok_or_else(|| wrong("`}` is missing"))?
                .trim();
            let value = |table: &'static str, key: &str| -> Option<(&'static str, &'static str)> {
                let rest = table.strip_prefix(key)?.trim_start().strip_prefix('=')?;
                quoted(rest.trim_start())
            };
            let (light, rest) =
                value(table, "light").ok_or_else(|| wrong("`light = \"…\"` comes first"))?;
            let rest = rest
                .trim_start()
                .strip_prefix(',')
                .ok_or_else(|| wrong("`,` is missing"))?
                .trim_start();
            let (dark, rest) =
                value(rest, "dark").ok_or_else(|| wrong("`dark = \"…\"` is missing"))?;
            if !rest.trim().is_empty() {
                return Err(wrong("a table holds `light` and `dark` alone"));
            }
            (light, dark)
        } else {
            let (both, rest) =
                quoted(rest).ok_or_else(|| wrong("a rule is a quoted expression"))?;
            if !rest.trim().is_empty() {
                return Err(wrong("one expression a rule"));
            }
            (both, both)
        };
        let parse = |text: &str| {
            text.parse::<Expr>()
                .map_err(|error| wrong(&error.to_string()))
        };
        if out.iter().any(|(known, _)| *known == name) {
            return Err(wrong(&format!("`{name}` has a rule already")));
        }
        out.push((
            name,
            Rule {
                light,
                dark,
                light_expr: parse(light)?,
                dark_expr: parse(dark)?,
            },
        ));
    }
    Ok(out)
}

/// The rule of the token `name`, if Lokyr has one.
pub fn rule(name: &str) -> Option<&'static Rule> {
    rules()
        .iter()
        .find(|(token, _)| *token == name)
        .map(|(_, rule)| rule)
}

/// The tokens of the rules, in the file's order.
pub fn tokens() -> impl Iterator<Item = &'static str> {
    rules().iter().map(|(name, _)| *name)
}

/// Whether the terminal's colors of `terminal` make a dark variant: its
/// background darker than its text.
pub fn is_dark(terminal: &TerminalColors) -> bool {
    luminance(Rgba::of(terminal.background)) < luminance(Rgba::of(terminal.foreground))
}

/// Why a token does not resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unresolved {
    /// A name of no color, of no rule and not given.
    Unknown(String),
    /// A token that names itself, through the tokens of its chain.
    Cycle(String),
}

/// The resolution of the tokens of a theme, each resolved once.
struct Resolver<'a> {
    rules: &'a [(&'static str, Rule)],
    terminal: &'a TerminalColors,
    dark: bool,
    floor: f32,
    given: &'a BTreeMap<String, ThemeColor>,
    done: BTreeMap<String, Rgba>,
    busy: BTreeSet<String>,
}

impl Resolver<'_> {
    /// The color of the token `name`: the theme's, else its rule's.
    fn token(&mut self, name: &str) -> Result<Rgba, Unresolved> {
        if let Some(&color) = self.done.get(name) {
            return Ok(color);
        }
        if let Some(&color) = self.given.get(name) {
            let color = Rgba::of(color);
            self.done.insert(name.to_owned(), color);
            return Ok(color);
        }
        let rule = self
            .rules
            .iter()
            .find(|(token, _)| *token == name)
            .map(|(_, rule)| rule)
            .ok_or_else(|| Unresolved::Unknown(name.to_owned()))?;
        if !self.busy.insert(name.to_owned()) {
            return Err(Unresolved::Cycle(name.to_owned()));
        }
        let color = self.eval(rule.expr(self.dark));
        self.busy.remove(name);
        let color = color?;
        self.done.insert(name.to_owned(), color);
        Ok(color)
    }

    /// The color of `expr`.
    fn eval(&mut self, expr: &Expr) -> Result<Rgba, Unresolved> {
        let terminal = self.terminal;
        Ok(match expr {
            Expr::Color(color) => *color,
            Expr::Terminal(Terminal::Background) => Rgba::of(terminal.background),
            Expr::Terminal(Terminal::Foreground) => Rgba::of(terminal.foreground),
            Expr::Terminal(Terminal::Cursor) => {
                Rgba::of(terminal.cursor.unwrap_or(terminal.foreground))
            }
            Expr::Terminal(Terminal::Selection) => match terminal.selection {
                Some(selection) => Rgba::of(selection),
                None => self.token("field.selection")?,
            },
            Expr::Terminal(Terminal::Palette(at)) => Rgba::of(terminal.palette[usize::from(*at)]),
            Expr::Token(name) => self.token(name)?,
            Expr::Opacity(expr, alpha) => self.eval(expr)?.with_alpha(*alpha),
            Expr::Mix(from, to, share) => mix(self.eval(from)?, self.eval(to)?, *share),
            Expr::Readable(color, on) => {
                with_contrast(self.eval(color)?, self.eval(on)?, self.floor)
            }
            Expr::Contrasting(on) => contrasting(self.eval(on)?),
            Expr::First(colors, on, ratio) => {
                let on = self.eval(on)?;
                let mut chosen = Rgba::CLEAR;
                for color in colors {
                    chosen = self.eval(color)?;
                    if contrast(chosen, on) >= *ratio {
                        break;
                    }
                }
                chosen
            }
        })
    }
}

/// The colors of every token of a theme of the terminal's colors
/// `terminal` and of the tokens that it gives, `given`: each given its
/// color, the others of their rules, of the variant that `terminal` makes;
/// the given tokens that Lokyr has no rule of, as a plugin's, kept.
pub fn resolve(
    terminal: &TerminalColors,
    given: &BTreeMap<String, ThemeColor>,
) -> BTreeMap<String, Rgba> {
    let mut resolver = Resolver::new(rules(), terminal, given);
    let mut names: Vec<&str> = given.keys().map(String::as_str).collect();
    for name in tokens() {
        names.push(name);
    }
    for name in names {
        // The rules name no color that is not there, nor a cycle, which
        // the tests hold.
        if resolver.token(name).is_err() {
            resolver.done.insert(name.to_owned(), Rgba::CLEAR);
        }
    }
    resolver.done
}

impl<'a> Resolver<'a> {
    /// The resolution of a theme of `terminal` and `given` by `rules`.
    fn new(
        rules: &'a [(&'static str, Rule)],
        terminal: &'a TerminalColors,
        given: &'a BTreeMap<String, ThemeColor>,
    ) -> Self {
        let (background, foreground) =
            (Rgba::of(terminal.background), Rgba::of(terminal.foreground));
        Self {
            rules,
            terminal,
            dark: is_dark(terminal),
            // Three quarters of the theme's own contrast, 4.5 at most, so that
            // a theme of a low contrast keeps it readable.
            floor: (contrast(foreground, background) * 0.75).min(4.5),
            given,
            done: BTreeMap::new(),
            busy: BTreeSet::new(),
        }
    }
}

#[cfg(test)]
mod tests;
