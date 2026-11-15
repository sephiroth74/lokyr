//! The rules of the interface's tokens: each token's color where a theme
//! gives none, an expression of the theme's terminal colors and of the
//! other tokens, which resolve the same way, so that a token that a theme
//! gives reaches every token whose rule names it.
//!
//! The rules are data, `rules.toml` beside this file, which tells their
//! expressions; the arithmetic is of the colors' encoded sRGB components,
//! as the interface draws them.

use std::collections::BTreeMap;
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
pub const PALETTE: [&str; 16] = [
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

/// How many bytes an expression's text has at most.
pub const MOST_EXPR_BYTES: usize = 512;

/// How deep an expression's functions and opacities nest at most.
pub const MOST_EXPR_DEPTH: usize = 16;

/// How deep tokens that name each other go at most, as a token's color
/// takes another's that takes another's.
pub const MOST_CHAIN: usize = 32;

/// An argument of a function, before its function takes it.
enum Arg {
    Expr(Expr),
    Number(f32, bool),
    Named(String, Box<Arg>),
}

/// The reader of an expression's parts, and how deep its functions nest
/// where it reads.
struct Reader {
    lexemes: Vec<Lexeme>,
    at: usize,
    depth: usize,
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

    /// The call of the function `name`, after its `(`, of its arguments
    /// one level deeper, [`MOST_EXPR_DEPTH`] at most.
    fn call(&mut self, name: &str) -> Result<Expr, ExprError> {
        if self.depth >= MOST_EXPR_DEPTH {
            return Err(ExprError(format!(
                "an expression's functions nest {MOST_EXPR_DEPTH} deep at most"
            )));
        }
        self.depth += 1;
        let args = self.args();
        self.depth -= 1;
        let args = args?;
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
        // The arguments taken whole, of no copy of their expressions.
        match name {
            "mix" => match (<[Arg; 3]>::try_from(positional), named.is_empty()) {
                (Ok([Arg::Expr(from), Arg::Expr(to), Arg::Number(value, true)]), true) => {
                    Ok(Expr::Mix(Box::new(from), Box::new(to), share(value)?))
                }
                _ => wrong("mix(X, Y, N%)"),
            },
            "readable" => {
                let on = take(&mut named, "on");
                match (<[Arg; 1]>::try_from(positional), on, named.is_empty()) {
                    (Ok([Arg::Expr(color)]), Some(Arg::Expr(on)), true) => {
                        Ok(Expr::Readable(Box::new(color), Box::new(on)))
                    }
                    _ => wrong("readable(X, on: Y)"),
                }
            }
            "contrasting" => match (<[Arg; 1]>::try_from(positional), named.is_empty()) {
                (Ok([Arg::Expr(color)]), true) => Ok(Expr::Contrasting(Box::new(color))),
                _ => wrong("contrasting(X)"),
            },
            "first" => {
                let (on, at) = (take(&mut named, "on"), take(&mut named, "at"));
                let colors: Option<Vec<Expr>> = positional
                    .into_iter()
                    .map(|arg| match arg {
                        Arg::Expr(expr) => Some(expr),
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

    /// The expression that `text` writes, whole, of
    /// [`MOST_EXPR_BYTES`] at most.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.len() > MOST_EXPR_BYTES {
            return Err(ExprError(format!(
                "an expression is {MOST_EXPR_BYTES} bytes at most, not {}",
                text.len()
            )));
        }
        let mut reader = Reader {
            lexemes: lexemes(text)?,
            at: 0,
            depth: 0,
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

/// A token's rule: what its token colors, its text and its expression of
/// each variant.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    /// What its token colors, of the comment right above it, without its
    /// `# `; empty of none.
    pub description: String,
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
    // The comment's lines right above the line under way.
    let mut comment: Vec<&str> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            comment.clear();
            continue;
        }
        if let Some(text) = line.strip_prefix('#') {
            comment.push(text.trim());
            continue;
        }
        let description = std::mem::take(&mut comment).join(" ");
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
                description,
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

/// Why a token that a theme gives does not resolve, which keeps its rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unresolved {
    /// A name of no color: of no rule, not given, nor of the terminal's.
    Unknown(String),
    /// Tokens that name each other, of their chain, as `a → b → a`.
    Cycle(String),
    /// An expression that does not read, of why.
    Unreadable(String),
    /// Tokens that name each other deeper than [`MOST_CHAIN`], of the
    /// token where they went past it.
    Deep(String),
}

impl fmt::Display for Unresolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(name) => write!(
                f,
                "`{name}` names no color: no token, nor one of the terminal's"
            ),
            Self::Cycle(chain) => write!(f, "tokens name each other: {chain}"),
            Self::Unreadable(why) => f.write_str(why),
            Self::Deep(name) => write!(
                f,
                "tokens name each other past {MOST_CHAIN} deep, at `{name}`"
            ),
        }
    }
}

/// A token's value that a theme gives, read.
#[derive(Clone, Debug)]
enum Given {
    Color(Rgba),
    Expr(Expr),
}

/// The resolution of the tokens of a theme, each resolved once.
struct Resolver<'a> {
    rules: &'a [(&'static str, Rule)],
    terminal: &'a TerminalColors,
    dark: bool,
    floor: f32,
    given: &'a BTreeMap<String, Given>,
    done: BTreeMap<String, Rgba>,
    /// The tokens under way, in their chain's order.
    busy: Vec<String>,
    /// The innermost given token whose expression did not resolve, of the
    /// last failure.
    blamed: Option<String>,
    /// The tokens that failed of a given token to blame, each of why and
    /// of that token, which fail again at once.
    failed: BTreeMap<String, (Unresolved, Option<String>)>,
}

impl Resolver<'_> {
    /// The color of the token `name`: the theme's, else its rule's.
    fn token(&mut self, name: &str) -> Result<Rgba, Unresolved> {
        if let Some(&color) = self.done.get(name) {
            return Ok(color);
        }
        // Failed already: again, of the same token to blame.
        if let Some((why, blamed)) = self.failed.get(name) {
            if self.blamed.is_none() {
                self.blamed.clone_from(blamed);
            }
            return Err(why.clone());
        }
        let given = self.given.get(name);
        if let Some(Given::Color(color)) = given {
            self.done.insert(name.to_owned(), *color);
            return Ok(*color);
        }
        let expr = match given {
            Some(Given::Expr(expr)) => expr,
            _ => self
                .rules
                .iter()
                .find(|(token, _)| *token == name)
                .map(|(_, rule)| rule.expr(self.dark))
                .ok_or_else(|| Unresolved::Unknown(name.to_owned()))?,
        };
        if let Some(at) = self.busy.iter().position(|busy| busy == name) {
            let chain: Vec<String> = self.busy[at..]
                .iter()
                .map(String::as_str)
                .chain([name])
                .map(|name| format!("`{name}`"))
                .collect();
            return Err(Unresolved::Cycle(chain.join(" → ")));
        }
        // Of no deeper chain, whose stack the evaluation's recursion holds.
        if self.busy.len() >= MOST_CHAIN {
            return Err(Unresolved::Deep(name.to_owned()));
        }
        self.busy.push(name.to_owned());
        let color = self.eval(expr);
        self.busy.pop();
        if color.is_err() && given.is_some() && self.blamed.is_none() {
            self.blamed = Some(name.to_owned());
        }
        match color {
            Ok(color) => {
                self.done.insert(name.to_owned(), color);
                Ok(color)
            }
            Err(why) => {
                // Kept of a given token to blame alone: a rule's of a cycle
                // through it resolves once the given token that closes the
                // cycle goes.
                if self.blamed.is_some() {
                    self.failed
                        .insert(name.to_owned(), (why.clone(), self.blamed.clone()));
                }
                Err(why)
            }
        }
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
/// color, or its expression's, the others of their rules, of the variant
/// that `terminal` makes; the given tokens that Lokyr has no rule of, as a
/// plugin's, kept. A given token that does not resolve, as
/// [`problems`] tells, takes its rule, or none of no rule.
pub fn resolve(
    terminal: &TerminalColors,
    given: &BTreeMap<String, super::TokenValue>,
) -> BTreeMap<String, Rgba> {
    resolution(rules(), terminal, given).0
}

/// The given tokens of a theme of `terminal` and `given` that do not
/// resolve, each of why, which [`resolve`] gives their rules: an
/// expression that does not read, a name of no color, a cycle.
pub fn problems(
    terminal: &TerminalColors,
    given: &BTreeMap<String, super::TokenValue>,
) -> Vec<(String, Unresolved)> {
    resolution(rules(), terminal, given).1
}

/// The resolution of a theme of `terminal` and `given` by `rules`, and the
/// given tokens left out, each of why: in passes, each leaving out the
/// innermost given token of every failure, until every token resolves.
fn resolution(
    rules: &[(&'static str, Rule)],
    terminal: &TerminalColors,
    given: &BTreeMap<String, super::TokenValue>,
) -> (BTreeMap<String, Rgba>, Vec<(String, Unresolved)>) {
    let mut problems = Vec::new();
    let mut kept = BTreeMap::new();
    for (name, value) in given {
        match value {
            super::TokenValue::Color(color) => {
                kept.insert(name.clone(), Given::Color(Rgba::of(*color)));
            }
            super::TokenValue::Expr(text) => match text.parse::<Expr>() {
                Ok(expr) => {
                    kept.insert(name.clone(), Given::Expr(expr));
                }
                Err(error) => problems.push((name.clone(), Unresolved::Unreadable(error.0))),
            },
        }
    }
    loop {
        let mut resolver = Resolver::new(rules, terminal, &kept);
        let mut names: Vec<String> = kept.keys().cloned().collect();
        names.extend(rules.iter().map(|(name, _)| (*name).to_owned()));
        // Every failure of the pass, of its given token to blame, the
        // innermost, or the one that failed; once each.
        let mut failures: Vec<(String, Unresolved)> = Vec::new();
        let mut stray = false;
        for name in &names {
            if let Err(why) = resolver.token(name) {
                let blamed = resolver.blamed.take().unwrap_or_else(|| name.clone());
                if !kept.contains_key(&blamed) {
                    // A rule's failure of no given token below it: of a
                    // cycle that a given token closes, or a rule's own.
                    stray = true;
                } else if !failures.iter().any(|(each, _)| *each == blamed) {
                    failures.push((blamed, why));
                }
            }
        }
        let mut done = resolver.done;
        if failures.is_empty() {
            if stray {
                // A rule that fails of no token given, which the tests
                // hold the rules of none: clear, as the rest.
                for name in names {
                    done.entry(name).or_insert(Rgba::CLEAR);
                }
            }
            return (done, problems);
        }
        for (blamed, why) in failures {
            kept.remove(&blamed);
            problems.push((blamed, why));
        }
    }
}

impl<'a> Resolver<'a> {
    /// The resolution of a theme of `terminal` and `given` by `rules`.
    fn new(
        rules: &'a [(&'static str, Rule)],
        terminal: &'a TerminalColors,
        given: &'a BTreeMap<String, Given>,
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
            busy: Vec::new(),
            blamed: None,
            failed: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests;
