//! The keymap: bindings of keys, and of the wheel, to commands, in the
//! contexts that their `when` names, and the resolution of the keys that the
//! user presses, sequences of them included, before they reach the encoder.
//!
//! A binding's keys are a combination, as `cmd+shift+]`, or a sequence of
//! them, as `ctrl+a>c`. A combination names its modifiers, `cmd`, `ctrl`,
//! `alt` or `opt`, and `shift`, and its key: the character that the key
//! types without any modifier, Shift neither, as `]` in `cmd+shift+]`; a
//! named key, as `enter` or `f5`; or a step of the wheel, `wheel-up`, alone.
//! A key matches a binding of its layout's character, or else of the US
//! layout's character at its place, so that the bindings reach the keys of
//! any layout.

use std::collections::BTreeSet;
use std::fmt;

use lokyr_vt_core::input::{Key, Wheel};

use crate::commands::{Args, Value};
use crate::config::{ArgValue, KeyBinding, KeyValue, KeysConfig};

/// The modifiers of a combination.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mods {
    /// Command, or Super.
    pub cmd: bool,
    /// Control.
    pub ctrl: bool,
    /// Alt, or Option, either.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
}

/// What a combination presses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Press {
    /// A key that types a character, by the character, lowercase.
    Char(char),
    /// A named key.
    Named(Named),
    /// A step of the wheel.
    Wheel(Direction),
}

/// A key that types no character, or whose character is a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Named {
    /// Return.
    Enter,
    /// Tab.
    Tab,
    /// Backspace, which macOS calls Delete.
    Backspace,
    /// Escape.
    Escape,
    /// The space bar.
    Space,
    /// Forward Delete.
    Delete,
    /// Insert, or Help.
    Insert,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
    /// The left arrow.
    Left,
    /// The right arrow.
    Right,
    /// A function key, from F1 to F35.
    F(u8),
}

/// Which way a step of the wheel goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    /// Toward the content's top.
    Up,
    /// Toward its bottom.
    Down,
    /// Toward its left edge.
    Left,
    /// Toward its right edge.
    Right,
}

/// A combination: modifiers and what they press.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Combo {
    /// The modifiers.
    pub mods: Mods,
    /// What they press.
    pub press: Press,
}

/// The names of the named keys, as bindings write them.
const NAMES: [(&str, Named); 16] = [
    ("enter", Named::Enter),
    ("tab", Named::Tab),
    ("backspace", Named::Backspace),
    ("escape", Named::Escape),
    ("space", Named::Space),
    ("delete", Named::Delete),
    ("insert", Named::Insert),
    ("home", Named::Home),
    ("end", Named::End),
    ("pageup", Named::PageUp),
    ("pagedown", Named::PageDown),
    ("up", Named::Up),
    ("down", Named::Down),
    ("left", Named::Left),
    ("right", Named::Right),
    ("esc", Named::Escape),
];

/// The steps of the wheel, as bindings write them.
const WHEELS: [(&str, Direction); 4] = [
    ("wheel-up", Direction::Up),
    ("wheel-down", Direction::Down),
    ("wheel-left", Direction::Left),
    ("wheel-right", Direction::Right),
];

impl Combo {
    /// The combination that `text` writes, as `cmd+shift+]` or `ctrl++`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let (mods, key) = if text == "+" {
            ("", "+")
        } else if let Some(mods) = text.strip_suffix("++") {
            (mods, "+")
        } else {
            match text.rsplit_once('+') {
                Some((mods, key)) => (mods, key),
                None => ("", text),
            }
        };
        let mut combo = Mods::default();
        for name in mods
            .split('+')
            .filter(|name| !mods.is_empty() || !name.is_empty())
        {
            let flag = match name {
                "cmd" | "command" | "super" => &mut combo.cmd,
                "ctrl" | "control" => &mut combo.ctrl,
                "alt" | "opt" | "option" => &mut combo.alt,
                "shift" => &mut combo.shift,
                _ => return Err(format!("`{name}` is not a modifier, in `{text}`")),
            };
            if *flag {
                return Err(format!("`{name}` comes twice, in `{text}`"));
            }
            *flag = true;
        }
        Ok(Self {
            mods: combo,
            press: press(key).ok_or_else(|| format!("`{key}` is not a key, in `{text}`"))?,
        })
    }
}

/// What `key`, a combination's last part, presses.
fn press(key: &str) -> Option<Press> {
    let mut chars = key.chars();
    if let (Some(character), None) = (chars.next(), chars.next()) {
        return (!character.is_whitespace() && !character.is_control())
            .then(|| Press::Char(character.to_lowercase().next().unwrap_or(character)));
    }
    let lower = key.to_ascii_lowercase();
    if let Some(&(_, named)) = NAMES.iter().find(|(name, _)| *name == lower) {
        return Some(Press::Named(named));
    }
    if let Some(&(_, direction)) = WHEELS.iter().find(|(name, _)| *name == lower) {
        return Some(Press::Wheel(direction));
    }
    let number: u8 = lower.strip_prefix('f')?.parse().ok()?;
    (1..=35)
        .contains(&number)
        .then_some(Press::Named(Named::F(number)))
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mods = self.mods;
        for (on, name) in [
            (mods.cmd, "cmd"),
            (mods.ctrl, "ctrl"),
            (mods.alt, "alt"),
            (mods.shift, "shift"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        match self.press {
            Press::Char(character) => write!(f, "{character}"),
            Press::Named(Named::F(number)) => write!(f, "f{number}"),
            Press::Named(named) => {
                let name = NAMES
                    .iter()
                    .find(|(_, other)| *other == named)
                    .map_or("", |(name, _)| *name);
                f.write_str(name)
            }
            Press::Wheel(direction) => {
                let name = WHEELS
                    .iter()
                    .find(|(_, other)| *other == direction)
                    .map_or("", |(name, _)| *name);
                f.write_str(name)
            }
        }
    }
}

/// The keys of a binding: a combination, or a sequence of them, each
/// pressed after the one before; a step of the wheel only alone.
pub fn parse_keys(text: &str) -> Result<Vec<Combo>, String> {
    // A `>` key, as in `shift+>`, ends a part rather than parting two.
    let mut parts = Vec::new();
    let mut start = 0;
    for (at, character) in text.char_indices() {
        let ends_a_key = character == '>' && at > start && !text[..at].ends_with('+');
        if ends_a_key {
            parts.push(&text[start..at]);
            start = at + 1;
        }
    }
    parts.push(&text[start..]);
    let combos = parts
        .into_iter()
        .map(Combo::parse)
        .collect::<Result<Vec<_>, _>>()?;
    let wheel = combos
        .iter()
        .any(|combo| matches!(combo.press, Press::Wheel(_)));
    if wheel && combos.len() > 1 {
        return Err(format!(
            "a step of the wheel takes no sequence, in `{text}`"
        ));
    }
    Ok(combos)
}

/// A context, as a binding's `when` names it: whether the names that hold
/// make the expression true.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum When {
    /// A name of the context, which holds or not.
    Name(String),
    /// Its operand does not hold.
    Not(Box<When>),
    /// Both hold.
    And(Box<When>, Box<When>),
    /// Either holds.
    Or(Box<When>, Box<When>),
}

/// The names that a `when` can use.
pub const CONTEXTS: [&str; 3] = ["pane.focused", "selection", "alternate-screen"];

/// The levels that a `when` nests at most: of `!`, of parentheses, and of
/// the operands of `&&` and `||`, so that its parsing and its evaluation,
/// which recurse, never take more stack than that.
const DEEPEST: usize = 32;

impl When {
    /// The expression of `text`: names, `!`, `&&`, `||` and parentheses,
    /// `&&` binding tighter than `||`, nested 32 levels deep at most.
    pub fn parse(text: &str) -> Result<Self, String> {
        let tokens = tokens(text)?;
        let mut at = 0;
        let when = or(&tokens, &mut at, 0)?;
        if at < tokens.len() {
            return Err(format!("`{}` is out of place, in `{text}`", tokens[at]));
        }
        Ok(when)
    }

    /// Whether the expression holds when the names of `context` hold.
    pub fn holds(&self, context: &Context) -> bool {
        match self {
            Self::Name(name) => context.holds(name),
            Self::Not(when) => !when.holds(context),
            Self::And(a, b) => a.holds(context) && b.holds(context),
            Self::Or(a, b) => a.holds(context) || b.holds(context),
        }
    }
}

/// The tokens of a `when`.
fn tokens(text: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&character) = chars.peek() {
        match character {
            ' ' => {
                chars.next();
            }
            '!' | '(' | ')' => {
                tokens.push(character.to_string());
                chars.next();
            }
            '&' | '|' => {
                chars.next();
                if chars.next() != Some(character) {
                    return Err(format!("`{character}` comes alone, in `{text}`"));
                }
                tokens.push(format!("{character}{character}"));
            }
            _ if character.is_ascii_lowercase() => {
                let mut name = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_lowercase()
                        || next.is_ascii_digit()
                        || next == '.'
                        || next == '-'
                    {
                        name.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !CONTEXTS.contains(&name.as_str()) {
                    return Err(format!("`{name}` is no context, in `{text}`"));
                }
                tokens.push(name);
            }
            _ => return Err(format!("`{character}` is out of place, in `{text}`")),
        }
    }
    Ok(tokens)
}

/// Fails at a level deeper than [`DEEPEST`].
fn deeper(depth: usize) -> Result<usize, String> {
    if depth >= DEEPEST {
        return Err(format!("the expression nests deeper than {DEEPEST} levels"));
    }
    Ok(depth + 1)
}

fn or(tokens: &[String], at: &mut usize, mut depth: usize) -> Result<When, String> {
    let mut when = and(tokens, at, depth)?;
    while tokens.get(*at).is_some_and(|token| token == "||") {
        *at += 1;
        depth = deeper(depth)?;
        when = When::Or(Box::new(when), Box::new(and(tokens, at, depth)?));
    }
    Ok(when)
}

fn and(tokens: &[String], at: &mut usize, mut depth: usize) -> Result<When, String> {
    let mut when = not(tokens, at, depth)?;
    while tokens.get(*at).is_some_and(|token| token == "&&") {
        *at += 1;
        depth = deeper(depth)?;
        when = When::And(Box::new(when), Box::new(not(tokens, at, depth)?));
    }
    Ok(when)
}

fn not(tokens: &[String], at: &mut usize, depth: usize) -> Result<When, String> {
    match tokens.get(*at).map(String::as_str) {
        Some("!") => {
            *at += 1;
            Ok(When::Not(Box::new(not(tokens, at, deeper(depth)?)?)))
        }
        Some("(") => {
            *at += 1;
            let when = or(tokens, at, deeper(depth)?)?;
            if tokens.get(*at).map(String::as_str) != Some(")") {
                return Err("a `(` is never closed".to_owned());
            }
            *at += 1;
            Ok(when)
        }
        Some(name) if !["&&", "||", ")"].contains(&name) => {
            *at += 1;
            Ok(When::Name(name.to_owned()))
        }
        Some(token) => Err(format!("`{token}` is out of place")),
        None => Err("the expression ends early".to_owned()),
    }
}

/// The names of the context that hold, where a key is pressed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    holding: BTreeSet<String>,
}

impl Context {
    /// The context in which the names of `holding` hold.
    pub fn new<'a>(holding: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            holding: holding.into_iter().map(str::to_owned).collect(),
        }
    }

    /// Whether `name` holds.
    pub fn holds(&self, name: &str) -> bool {
        self.holding.contains(name)
    }
}

/// A command and its arguments, which a binding runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Invocation {
    /// The command's name.
    pub command: String,
    /// Its arguments.
    pub args: Args,
}

/// A binding: its keys, the command that it runs, and the context in which
/// it holds, every context without a `when`.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    /// Its keys.
    pub keys: Vec<Combo>,
    /// The command that it runs.
    pub invocation: Invocation,
    /// The context in which it holds.
    pub when: Option<When>,
}

impl Binding {
    fn holds(&self, context: &Context) -> bool {
        self.when.as_ref().is_none_or(|when| when.holds(context))
    }
}

/// The bindings, the defaults first and then the user's, of which the last
/// that holds wins.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Keymap {
    /// A keymap of `defaults`.
    pub fn new(defaults: Vec<Binding>) -> Self {
        Self { bindings: defaults }
    }

    /// The keymap of `defaults`, with the bindings of the config's `[keys]`
    /// over them: an entry of `false` takes the defaults of its keys away,
    /// in every context, and the others bind theirs after the defaults, so
    /// that they win where they hold. The config's reading checked them.
    pub fn with_config(defaults: Vec<Binding>, config: &KeysConfig) -> Self {
        let mut keymap = Self::new(defaults);
        for (keys, value) in &config.bindings {
            let Ok(keys) = parse_keys(keys) else {
                continue;
            };
            let bindings = match value {
                KeyValue::Command(command) => vec![Binding {
                    keys: keys.clone(),
                    invocation: Invocation {
                        command: command.clone(),
                        args: Args::new(),
                    },
                    when: None,
                }],
                KeyValue::Unbound(_) => {
                    keymap.unbind(&keys);
                    Vec::new()
                }
                KeyValue::Binding(binding) => vec![from_config(&keys, binding)],
                KeyValue::Bindings(bindings) => bindings
                    .iter()
                    .map(|binding| from_config(&keys, binding))
                    .collect(),
            };
            for binding in bindings {
                keymap.bind(binding);
            }
        }
        keymap
    }

    /// Takes away the bindings of `keys`, in every context.
    pub fn unbind(&mut self, keys: &[Combo]) {
        self.bindings.retain(|binding| binding.keys != keys);
    }

    /// Adds `binding`, which wins over those before it where both hold.
    pub fn bind(&mut self, binding: Binding) {
        self.bindings.push(binding);
    }

    /// The bindings, in their order.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The binding of `keys` that holds in `context`, the last of them.
    fn exact(&self, keys: &[Combo], context: &Context) -> Option<&Binding> {
        self.bindings
            .iter()
            .rev()
            .find(|binding| binding.keys == keys && binding.holds(context))
    }

    /// Whether a binding that holds in `context` starts with `keys`, and
    /// goes on.
    fn continues(&self, keys: &[Combo], context: &Context) -> bool {
        self.bindings.iter().any(|binding| {
            binding.keys.len() > keys.len()
                && binding.keys.starts_with(keys)
                && binding.holds(context)
        })
    }
}

/// The binding of `keys` that the config's `binding` writes.
fn from_config(keys: &[Combo], binding: &KeyBinding) -> Binding {
    let args = binding
        .args
        .iter()
        .map(|(name, value)| {
            let value = match value {
                ArgValue::Boolean(value) => Value::Boolean(*value),
                ArgValue::Integer(value) => Value::Integer(*value),
                ArgValue::Number(value) => Value::Number(*value),
                ArgValue::String(value) => Value::String(value.clone()),
            };
            (name.clone(), value)
        })
        .collect();
    Binding {
        keys: keys.to_vec(),
        invocation: Invocation {
            command: binding.command.clone(),
            args,
        },
        when: binding
            .when
            .as_deref()
            .and_then(|when| When::parse(when).ok()),
    }
}

/// A key as it is pressed: its modifiers, what it presses as its layout
/// has it, and what the US layout's key at its place would.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pressed {
    /// The modifiers held.
    pub mods: Mods,
    /// What it presses, as its layout has it.
    pub press: Press,
    /// What the US layout's key at its place presses, if another.
    pub us: Option<Press>,
}

impl Pressed {
    /// The key `key`, as the encoder names it, of `mods`, whose place on
    /// the US layout types `us`: `None` for a modifier or a lock, which no
    /// binding takes.
    ///
    /// The US layout's character stands in for a character past ASCII, as
    /// the Italian layout's `è` for `[` and the Russian one's `й` for `q`,
    /// and for ASCII's punctuation, as the Italian `+` for `]`, unless it
    /// is a letter or a digit; never for an ASCII letter or digit. A key
    /// that types one of them is the user's, wherever the layout puts it,
    /// so that AZERTY's `a`, at the US layout's `q`, and Dvorak's `'`, are
    /// no `q`.
    pub fn key(key: Key, mods: Mods, us: Option<char>) -> Option<Self> {
        let press = match key {
            Key::Char(' ') => Press::Named(Named::Space),
            Key::Char(character) => {
                Press::Char(character.to_lowercase().next().unwrap_or(character))
            }
            Key::Enter => Press::Named(Named::Enter),
            Key::Tab => Press::Named(Named::Tab),
            Key::Backspace => Press::Named(Named::Backspace),
            Key::Escape => Press::Named(Named::Escape),
            Key::Delete => Press::Named(Named::Delete),
            Key::Insert => Press::Named(Named::Insert),
            Key::Home => Press::Named(Named::Home),
            Key::End => Press::Named(Named::End),
            Key::PageUp => Press::Named(Named::PageUp),
            Key::PageDown => Press::Named(Named::PageDown),
            Key::Up => Press::Named(Named::Up),
            Key::Down => Press::Named(Named::Down),
            Key::Left => Press::Named(Named::Left),
            Key::Right => Press::Named(Named::Right),
            Key::F(number) => Press::Named(Named::F(number)),
            _ => return None,
        };
        let us = us
            .filter(|us| match press {
                Press::Char(own) if own.is_ascii_alphanumeric() => false,
                Press::Char(own) if own.is_ascii() => !us.is_ascii_alphanumeric(),
                _ => true,
            })
            .map(|character| Press::Char(character.to_lowercase().next().unwrap_or(character)))
            .filter(|&us| us != press);
        Some(Self { mods, press, us })
    }

    /// A step of the wheel `wheel`, with `mods`.
    pub fn wheel(wheel: Wheel, mods: Mods) -> Self {
        let direction = match wheel {
            Wheel::Up => Direction::Up,
            Wheel::Down => Direction::Down,
            Wheel::Left => Direction::Left,
            Wheel::Right => Direction::Right,
        };
        Self {
            mods,
            press: Press::Wheel(direction),
            us: None,
        }
    }

    /// The combinations that it makes, its layout's first.
    fn combos(self) -> impl Iterator<Item = Combo> {
        let combo = |press| Combo {
            mods: self.mods,
            press,
        };
        std::iter::once(combo(self.press)).chain(self.us.map(combo))
    }
}

/// What a key does, as the keymap resolves it.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolved {
    /// It runs a command.
    Run(Invocation),
    /// It starts or goes on with a sequence, which waits for the next key.
    Pending,
    /// It goes to the program, after the keys of a sequence that it did
    /// not go on with, which go first, as many as this tells.
    Program {
        /// The keys of the sequence before it, which go first.
        before: usize,
    },
}

/// The keys of a sequence that wait for the next, in a window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolver {
    pending: Vec<Combo>,
}

impl Resolver {
    /// A resolver without a sequence under way.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a sequence waits for its next key.
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Resolves `pressed`, in `context`, after the keys of the sequence
    /// that waits: a binding of the keys, with either of its combinations,
    /// runs; a sequence that goes on waits; otherwise the key goes to the
    /// program, after those that waited.
    pub fn press(&mut self, keymap: &Keymap, pressed: Pressed, context: &Context) -> Resolved {
        for combo in pressed.combos() {
            let mut keys = self.pending.clone();
            keys.push(combo);
            if let Some(binding) = keymap.exact(&keys, context) {
                self.pending.clear();
                return Resolved::Run(binding.invocation.clone());
            }
            if keymap.continues(&keys, context) {
                self.pending = keys;
                return Resolved::Pending;
            }
        }
        let before = self.pending.len();
        self.pending.clear();
        Resolved::Program { before }
    }
}

#[cfg(test)]
mod tests;
