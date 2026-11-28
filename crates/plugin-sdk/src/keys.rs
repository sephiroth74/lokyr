//! The grammars of a binding's keys and of a context, as plugins and the
//! user's config write them, which the host checks and the SDK's fake host
//! checks alike.
//!
//! A binding's keys are a combination, as `cmd+shift+]`, or a sequence of
//! them, as `ctrl+a>c`. A combination names its modifiers, `cmd`, `ctrl`,
//! `alt` or `opt`, and `shift`, and its key: the character that the key
//! types without any modifier, Shift neither, as `]` in `cmd+shift+]`; a
//! named key, as `enter` or `f5`; or a step of the wheel, `wheel-up`, or the
//! left button's click, `click`, alone. A binding whose keys start with
//! `global:`, as `global:ctrl+\``, takes its key from every app, by its
//! place on the keyboard, as the US layout has it.
//!
//! A context, a binding's or a command's `when`, is an expression of the
//! names of the host's contexts and of the plugins' panels, `!`, `&&`, `||`
//! and parentheses.

use std::fmt;

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
    /// The left button's click, whose command takes the cell's `column`
    /// and `row`.
    Click,
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

/// The left button's click, as bindings write it.
const CLICK: &str = "click";

/// The steps of the wheel, as bindings write them.
const WHEELS: [(&str, Direction); 4] = [
    ("wheel-up", Direction::Up),
    ("wheel-down", Direction::Down),
    ("wheel-left", Direction::Left),
    ("wheel-right", Direction::Right),
];

impl Combo {
    /// This combination as the US layout's keys write it: of a character
    /// that the US layout types with Shift, and no `shift`, its key with
    /// Shift, so that `cmd+}` is `cmd+shift+]`; any other as it is.
    pub fn canonical(self) -> Self {
        match self.press {
            Press::Char(character) if !self.mods.shift => match us_unshifted(character) {
                Some(key) => Self {
                    mods: Mods {
                        shift: true,
                        ..self.mods
                    },
                    press: Press::Char(key),
                },
                None => self,
            },
            _ => self,
        }
    }

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
    if lower == CLICK {
        return Some(Press::Click);
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
            Press::Click => f.write_str(CLICK),
        }
    }
}

/// The bindings that macOS's own app menu has, its Quit's and its
/// Settings', by their keys and their commands, which the settings neither
/// change nor take away.
pub const SYSTEM: [(&str, &str); 2] = [("cmd+q", "app.quit"), ("cmd+,", "app.settings")];

/// Whether `keys` are those of one of [`SYSTEM`]'s bindings: of its
/// command where `command` is given, else of any.
pub fn is_system(keys: &[Combo], command: Option<&str>) -> bool {
    SYSTEM.iter().any(|(own, name)| {
        parse_keys(own).is_ok_and(|own| own == keys)
            && command.is_none_or(|command| command == *name)
    })
}

/// The error of a write of the settings at `keys`, [`SYSTEM`]'s, whose
/// binding is macOS's own.
pub fn system_error(keys: &str) -> String {
    format!(
        "`{keys}` are macOS's own keys, of its app menu, which the settings neither change nor take away"
    )
}

/// The text of `keys`, as a config writes them and [`parse_keys`] reads
/// them: each combination as `cmd+shift+n`, a sequence's parted by `>`.
pub fn keys_text(keys: &[Combo]) -> String {
    keys.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(">")
}

/// The keys of a binding: a combination, or a sequence of them, each
/// pressed after the one before; a step of the wheel and a click only
/// alone.
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
    let click = combos.iter().any(|combo| combo.press == Press::Click);
    if click && combos.len() > 1 {
        return Err(format!("a click takes no sequence, in `{text}`"));
    }
    Ok(combos)
}

/// The keys of a binding, `text`, as macOS writes them in its menus: each
/// combination of its modifiers' symbols, Control, Option, Shift then
/// Command, and its key's, as `⇧⌘N`, a sequence's apart, as `⌃A N`; a
/// global binding's of its keys alone. `None` of keys that do not read.
pub fn macos_text(text: &str) -> Option<String> {
    let combos = match text.strip_prefix(GLOBAL) {
        Some(global) => vec![parse_global(global).ok()?],
        None => parse_keys(text).ok()?,
    };
    let written: Vec<String> = combos.iter().map(macos_combo).collect();
    Some(written.join(" "))
}

/// `combo` as macOS writes it in its menus.
fn macos_combo(combo: &Combo) -> String {
    let mods = combo.mods;
    let mut written = String::new();
    for (on, symbol) in [
        (mods.ctrl, '⌃'),
        (mods.alt, '⌥'),
        (mods.shift, '⇧'),
        (mods.cmd, '⌘'),
    ] {
        if on {
            written.push(symbol);
        }
    }
    let key = match combo.press {
        Press::Char(character) => character.to_uppercase().collect(),
        Press::Named(Named::F(number)) => format!("F{number}"),
        Press::Named(named) => match named {
            Named::Enter => "↩",
            Named::Tab => "⇥",
            Named::Backspace => "⌫",
            Named::Escape => "⎋",
            Named::Space => "Space",
            Named::Delete => "⌦",
            Named::Insert => "Help",
            Named::Home => "↖",
            Named::End => "↘",
            Named::PageUp => "⇞",
            Named::PageDown => "⇟",
            Named::Up => "↑",
            Named::Down => "↓",
            Named::Left => "←",
            Named::Right => "→",
            Named::F(_) => "",
        }
        .to_owned(),
        Press::Wheel(direction) => match direction {
            Direction::Up => "Scroll ↑",
            Direction::Down => "Scroll ↓",
            Direction::Left => "Scroll ←",
            Direction::Right => "Scroll →",
        }
        .to_owned(),
        Press::Click => "Click".to_owned(),
    };
    // A word after the modifiers, apart from them; a symbol or a key next
    // to them, as macOS writes it.
    if !written.is_empty() && key.chars().count() > 1 && !key.starts_with('F') {
        written.push(' ');
    }
    written.push_str(&key);
    written
}

/// The prefix of the keys of a binding that takes its key from every app.
pub const GLOBAL: &str = "global:";

/// The combination of `keys` that a key sends a pane, as `ctrl+c`: one
/// combination of a key, not a click nor a wheel's step; why not
/// otherwise.
pub fn typed_key(keys: &str) -> Result<Combo, String> {
    let combo = Combo::parse(keys.trim())?;
    match combo.press {
        Press::Char(_) | Press::Named(_) => Ok(combo),
        Press::Wheel(_) | Press::Click => Err(format!(
            "`{keys}` is a click or a wheel's step, not a key that a pane takes"
        )),
    }
}

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

/// The character that the US layout types without Shift at the key that
/// types `shifted` with it, as `]` of `}`; none of a character that it
/// types without Shift.
pub fn us_unshifted(shifted: char) -> Option<char> {
    US_SHIFTED
        .iter()
        .find(|&&(character, _)| character == shifted)
        .map(|&(_, key)| key)
}

/// Whether `keys` and `other` are the same keys, of their combinations as
/// [`Combo::canonical`] writes them: `cmd+}` and `cmd+shift+]` are.
pub fn same_keys(keys: &[Combo], other: &[Combo]) -> bool {
    keys.len() == other.len()
        && keys
            .iter()
            .zip(other)
            .all(|(one, two)| one.canonical() == two.canonical())
}

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

/// The names of the host's contexts that a `when` can use, beside those of
/// the plugins' panels, which [`is_panel_context`] tells.
pub const CONTEXTS: [&str; 3] = ["pane.focused", "selection", "alternate-screen"];

/// Whether `name` is a context of a plugin's: `namespace.open`, while one
/// of its panels shows, `namespace.focused`, while one of them has the
/// keyboard's focus, or `namespace.selected`, while it selects a block of
/// the pane that has the keyboard, of the plugin's namespace, as
/// `search.open`; or `namespace.panel.open` and `namespace.panel.focused`
/// of one of its panels alone, by the panel's id, as
/// `search.find-all.focused`; not of the first part of a host's context, as
/// `pane` or `selection`, whose names are those of [`CONTEXTS`] alone.
pub fn is_panel_context(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    let (namespace, states): (&str, &[&str]) = match parts[..] {
        [namespace, _] => (namespace, &["open", "focused", "selected"]),
        [namespace, panel, _] if crate::command::is_part(panel) => {
            (namespace, &["open", "focused"])
        }
        _ => return false,
    };
    parts.last().is_some_and(|state| states.contains(state))
        && crate::command::is_part(namespace)
        && !CONTEXTS
            .iter()
            .any(|host| host.split('.').next() == Some(namespace))
}

/// The levels that a `when` nests at most: of `!`, of parentheses, and of
/// the operands of `&&` and `||`, so that its parsing and its evaluation,
/// which recurse, never take more stack than that.
const DEEPEST: usize = 32;

/// The expression as a config writes it, which [`When::parse`] reads
/// back: `&&` and `||` spaced, and parentheses where an operand binds
/// looser than its operator.
impl fmt::Display for When {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(name) => f.write_str(name),
            Self::Not(when) => match **when {
                Self::Name(_) | Self::Not(_) => write!(f, "!{when}"),
                _ => write!(f, "!({when})"),
            },
            Self::And(a, b) => {
                for (at, side) in [a, b].into_iter().enumerate() {
                    if at > 0 {
                        f.write_str(" && ")?;
                    }
                    match **side {
                        Self::Or(..) => write!(f, "({side})")?,
                        _ => write!(f, "{side}")?,
                    }
                }
                Ok(())
            }
            Self::Or(a, b) => write!(f, "{a} || {b}"),
        }
    }
}

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

    /// Whether the expression holds where `holds` tells which names hold.
    pub fn holds(&self, holds: &impl Fn(&str) -> bool) -> bool {
        match self {
            Self::Name(name) => holds(name),
            Self::Not(when) => !when.holds(holds),
            Self::And(a, b) => a.holds(holds) && b.holds(holds),
            Self::Or(a, b) => a.holds(holds) || b.holds(holds),
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
                if !CONTEXTS.contains(&name.as_str()) && !is_panel_context(&name) {
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

#[cfg(test)]
mod macos_tests {
    use super::*;

    #[test]
    fn a_context_and_keys_are_written_as_they_read_back() {
        for (text, written) in [
            ("selection", "selection"),
            ("!selection", "!selection"),
            (
                "selection&&pane.focused||search.open",
                "selection && pane.focused || search.open",
            ),
            (
                "selection && (pane.focused || search.open)",
                "selection && (pane.focused || search.open)",
            ),
            (
                "!(selection && pane.focused)",
                "!(selection && pane.focused)",
            ),
            ("!!selection", "!!selection"),
            (
                "(selection || alternate-screen) && !search.focused",
                "(selection || alternate-screen) && !search.focused",
            ),
        ] {
            let when = When::parse(text).unwrap();
            assert_eq!(when.to_string(), written, "{text}");
            assert_eq!(When::parse(&when.to_string()).unwrap(), when, "{text}");
        }
        for (keys, written) in [
            ("cmd+shift+n", "cmd+shift+n"),
            ("shift+cmd+N", "cmd+shift+n"),
            ("ctrl+a>c", "ctrl+a>c"),
        ] {
            let combos = parse_keys(keys).unwrap();
            assert_eq!(keys_text(&combos), written, "{keys}");
            assert_eq!(parse_keys(&keys_text(&combos)).unwrap(), combos);
        }
    }

    #[test]
    fn keys_are_written_as_macos_menus_write_them() {
        for (keys, written) in [
            ("cmd+shift+n", "⇧⌘N"),
            ("ctrl+alt+shift+cmd+k", "⌃⌥⇧⌘K"),
            ("cmd+]", "⌘]"),
            ("cmd+enter", "⌘↩"),
            ("cmd+up", "⌘↑"),
            ("shift+f5", "⇧F5"),
            ("ctrl+a>n", "⌃A N"),
            ("alt+space", "⌥ Space"),
            ("cmd+click", "⌘ Click"),
            ("wheel-up", "Scroll ↑"),
            ("global:ctrl+`", "⌃`"),
        ] {
            assert_eq!(macos_text(keys).as_deref(), Some(written), "{keys}");
        }
        assert_eq!(macos_text("cmd+nothing"), None);
    }
}
