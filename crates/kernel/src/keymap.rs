//! The keymap: bindings of keys, and of the wheel, to commands, in the
//! contexts that their `when` names, and the resolution of the keys that the
//! user presses, sequences of them included, before they reach the encoder.
//!
//! A binding's keys are a combination, as `cmd+shift+]`, or a sequence of
//! them, as `ctrl+a>c`. A combination names its modifiers, `cmd`, `ctrl`,
//! `alt` or `opt`, and `shift`, and its key: the character that the key
//! types without any modifier, Shift neither, as `]` in `cmd+shift+]`; a
//! named key, as `enter` or `f5`; or a step of the wheel, `wheel-up`, or the
//! left button's click, `click`, alone.
//! A key matches a binding of its layout's character, or else of the US
//! layout's character at its place, so that the bindings reach the keys of
//! any layout.
//!
//! A binding whose keys start with `global:`, as `global:ctrl+\``, takes
//! its key from every app, as the system gives it, by its place on the
//! keyboard, as the US layout has it; the keymap of the windows leaves it
//! out.

use std::collections::BTreeSet;

use lokyr_vt_core::input::{Key, Wheel};

pub use lokyr_plugin_sdk::keys::{
    CONTEXTS, Combo, Direction, Mods, Named, Press, When, is_panel_context, parse_keys,
};

use crate::commands::{Args, Value};
use crate::config::{ArgValue, KeyBinding, KeyValue, KeysConfig};

/// The prefix of the keys of a binding that takes its key from every app.
pub const GLOBAL: &str = "global:";

/// The characters that the US layout types without a modifier, by which
/// a global key goes, as the system takes it by its place.
const US_KEYS: &str = "abcdefghijklmnopqrstuvwxyz0123456789`-=[]\\;',./";

/// The characters that the US layout types with Shift, each with the one
/// of its key without it.
const US_SHIFTED: [(char, char); 21] = [
    ('~', '`'),
    ('!', '1'),
    ('@', '2'),
    ('#', '3'),
    ('$', '4'),
    ('%', '5'),
    ('^', '6'),
    ('&', '7'),
    ('*', '8'),
    ('(', '9'),
    (')', '0'),
    ('_', '-'),
    ('+', '='),
    ('{', '['),
    ('}', ']'),
    ('|', '\\'),
    (':', ';'),
    ('"', '\''),
    ('<', ','),
    ('>', '.'),
    ('?', '/'),
];

/// The combination of the keys of a global binding, `text` after
/// [`GLOBAL`]: one key, which no sequence follows, with Command, Control
/// or Alt, or a function key, alone or not; neither the wheel nor a
/// click, which reach no other app. A character is a key of the US
/// layout's, which types it without Shift, as the system takes the key by
/// its place: `~` is `shift+\``.
pub fn parse_global(text: &str) -> Result<Combo, String> {
    let combo = Combo::parse(text)?;
    match combo.press {
        Press::Wheel(_) | Press::Click => {
            return Err(format!(
                "a global key is a key, not a step of the wheel nor a click, in `{text}`"
            ));
        }
        Press::Char(character) if !US_KEYS.contains(character) => {
            let message = match US_SHIFTED.iter().find(|(shifted, _)| *shifted == character) {
                Some(&(_, key)) => {
                    let mods = Mods {
                        shift: true,
                        ..combo.mods
                    };
                    let written = Combo {
                        mods,
                        press: Press::Char(key),
                    };
                    format!(
                        "`{character}` takes Shift on the US layout, by whose keys a global key goes: write `{written}`, in `{text}`"
                    )
                }
                None => format!(
                    "`{character}` is no key of the US layout, by whose keys a global key goes, in `{text}`"
                ),
            };
            return Err(message);
        }
        _ => {}
    }
    match combo.press {
        Press::Named(Named::F(_)) => Ok(combo),
        _ if combo.mods.cmd || combo.mods.ctrl || combo.mods.alt => Ok(combo),
        _ => Err(format!(
            "a global key takes `cmd`, `ctrl` or `alt`, but a function key, as it takes the key from every app, in `{text}`"
        )),
    }
}

/// The global bindings of the config's `[keys]`, of the keys that
/// [`GLOBAL`] starts, each with the command that it runs, in the order of
/// their keys; those that do not read, which the config's reading
/// checked, are left out.
pub fn global_bindings(config: &KeysConfig) -> Vec<(Combo, Invocation)> {
    config
        .bindings
        .iter()
        .filter_map(|(keys, value)| {
            let combo = parse_global(keys.strip_prefix(GLOBAL)?).ok()?;
            let invocation = match value {
                KeyValue::Command(command) => Invocation {
                    command: command.clone(),
                    args: Args::new(),
                },
                KeyValue::Binding(binding) => from_config(&[combo], binding).invocation,
                KeyValue::Unbound(_) | KeyValue::Bindings(_) => return None,
            };
            Some((combo, invocation))
        })
        .collect()
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

    /// Has `name` hold too.
    pub fn hold(&mut self, name: impl Into<String>) {
        self.holding.insert(name.into());
    }

    /// Whether `name` holds.
    pub fn holds(&self, name: &str) -> bool {
        self.holding.contains(name)
    }

    /// Whether `when` holds in the context.
    pub fn holds_when(&self, when: &When) -> bool {
        when.holds(&|name| self.holds(name))
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
        self.when
            .as_ref()
            .is_none_or(|when| context.holds_when(when))
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
    /// The global bindings are not the windows'.
    pub fn with_config(defaults: Vec<Binding>, config: &KeysConfig) -> Self {
        let mut keymap = Self::new(defaults);
        for (keys, value) in &config.bindings {
            if keys.starts_with(GLOBAL) {
                continue;
            }
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

    /// The keys that a menu's item shows for `command`, run without
    /// arguments: of its bindings of one key, in every context, the first
    /// whose key no later binding takes, alone or as the first of a
    /// sequence, as a menu takes its item's key before the bindings see it.
    pub fn shortcut(&self, command: &str) -> Option<Combo> {
        self.shortcut_with(command, &Args::new())
    }

    /// The keys that run `command` with `args`, as [`Keymap::shortcut`]
    /// finds them for a command without arguments.
    pub fn shortcut_with(&self, command: &str, args: &Args) -> Option<Combo> {
        self.bindings.iter().enumerate().find_map(|(at, binding)| {
            let [combo] = binding.keys[..] else {
                return None;
            };
            let ours = binding.invocation.command == command
                && binding.invocation.args == *args
                && binding.when.is_none()
                && matches!(combo.press, Press::Char(_) | Press::Named(_));
            let taken = self.bindings[at + 1..]
                .iter()
                .any(|later| later.keys.first() == Some(&combo));
            (ours && !taken).then_some(combo)
        })
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

    /// The left button's click with `mods`.
    pub fn click(mods: Mods) -> Self {
        Self {
            mods,
            press: Press::Click,
            us: None,
        }
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
