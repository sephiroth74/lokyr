//! The text field: a line of text that the host keeps by its node's key,
//! as a plugin's view names it, as the host keeps every widget's state, so
//! that no key goes through the plugin and back: its text, its caret and
//! its selection, its scroll across, and the edits that it can undo; what
//! the keys, the pointer and the clipboard do to it; and the events that
//! tell its plugin of its text, `input-changed` and `submit`.
//!
//! The caret moves by whole grapheme clusters, UAX #29's
//! (<https://www.unicode.org/reports/tr29/>), so that it never stands
//! inside an emoji of several code points or a letter and its marks; a
//! word is a run of letters and digits. The keys are macOS's text fields'
//! (<https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/EventOverview/TextDefaultsBindings/TextDefaultsBindings.html>):
//! the arrows by clusters, with Option by words and with Command to the
//! line's ends, Shift extending the selection; Backspace and Delete, with
//! Option by words and with Command to the line's ends; Emacs's Control-A,
//! E, B, F, H, D and K; Command-A, C, X, V, Z and Shift-Command-Z.

use std::collections::HashMap;
use std::ops::Range;

use lokyr_plugin_sdk::view::{Key as NodeKey, Kind, UiEventKind};
use lokyr_vt_core::input::{Key, KeyAction, KeyEvent, Keypad};
use lokyr_vt_core::unicode::Segmenter;

use crate::layout::Layout;
use crate::pointer::NodeEvent;
use crate::tree::{NodeId, Tree};

/// The room inside a field's edges, in points: down, above and below its
/// line, and across, before and after its text.
pub const PADDING: (f32, f32) = (4.0, 6.0);

/// A field's natural width, in points, which its layout's width replaces.
pub const WIDTH: f32 = 160.0;

/// The edits that a field can undo, at most.
const UNDOS: usize = 100;

/// Where the caret goes in a line of the interface's text.
pub trait Carets {
    /// The places between the grapheme clusters of `text`, each cluster's
    /// first byte and the caret's place before it, in points from the
    /// text's start, then the text's end and its width.
    fn carets(&mut self, text: &str) -> Vec<(usize, f32)>;
}

/// The clipboard, as a field cuts, copies and pastes.
pub trait Clipboard {
    /// The text on it, if it holds text.
    fn read(&mut self) -> Option<String>;

    /// Puts `text` on it.
    fn write(&mut self, text: &str);
}

/// A field's state, which the host keeps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldState {
    text: String,
    /// The caret, at a cluster's first byte or the text's end.
    caret: usize,
    /// The selection's other end, the caret's own without one.
    anchor: usize,
    /// The points of the text scrolled out at the left.
    scroll: f32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Whether the last edit typed text, which typing more joins, as one
    /// undo takes back a word typed.
    typing: bool,
}

/// What an undo brings back.
#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    text: String,
    caret: usize,
    anchor: usize,
}

impl FieldState {
    /// Its text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The caret, as a byte of the text.
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selection, as bytes of the text: empty without one.
    pub fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    /// The points of the text scrolled out at the left.
    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            caret: self.caret,
            anchor: self.anchor,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.caret = snapshot.caret;
        self.anchor = snapshot.anchor;
    }

    /// Replaces `range` with `with`, the caret after it: whether the text
    /// changed. Typing joins the edit before, if it typed too, which a move
    /// of the caret or a selection ends.
    fn edit(&mut self, range: Range<usize>, with: &str, typing: bool) -> bool {
        if range.is_empty() && with.is_empty() {
            return false;
        }
        if !(typing && self.typing) {
            self.undo.push(self.snapshot());
            if self.undo.len() > UNDOS {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.text.replace_range(range.clone(), with);
        self.caret = range.start + with.len();
        self.anchor = self.caret;
        self.typing = typing;
        true
    }

    /// Moves the caret to `to`, the anchor with it unless `extend`.
    fn go(&mut self, to: usize, extend: bool) {
        self.caret = to;
        if !extend {
            self.anchor = to;
        }
        self.typing = false;
    }

    fn undo(&mut self) -> bool {
        let Some(before) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(before);
        self.typing = false;
        true
    }

    fn redo(&mut self) -> bool {
        let Some(after) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(after);
        self.typing = false;
        true
    }
}

/// The fields of a tree, by their keys, and the one that the pointer's
/// primary button went down over, which its moves select in.
#[derive(Debug, Default)]
pub struct Fields {
    states: HashMap<NodeKey, FieldState>,
    pressed: Option<NodeKey>,
}

/// Where a move of the caret goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Toward {
    Back,
    Forward,
    WordBack,
    WordForward,
    Start,
    End,
}

impl Fields {
    /// No field.
    pub fn new() -> Self {
        Self::default()
    }

    /// The state of the field of `key`, if it has one yet.
    pub fn state(&self, key: &NodeKey) -> Option<&FieldState> {
        self.states.get(key)
    }

    /// Sets the text of the field of `key`, as its plugin asks: the caret
    /// at its end, without a selection, nothing to undo, and no event.
    pub fn set_text(&mut self, key: &NodeKey, text: &str) {
        let state = self.states.entry(key.clone()).or_default();
        *state = FieldState {
            text: single_line(text),
            ..FieldState::default()
        };
        state.caret = state.text.len();
        state.anchor = state.caret;
    }

    /// Forgets the fields whose keys `tree` no longer has as fields.
    pub fn updated(&mut self, tree: &Tree) {
        self.states.retain(|key, _| {
            tree.by_key(key)
                .is_some_and(|node| matches!(tree.kind(node), Some(Kind::Field(_))))
        });
        if self
            .pressed
            .as_ref()
            .is_some_and(|key| !self.states.contains_key(key))
        {
            self.pressed = None;
        }
    }

    /// Does `event` to the field `node` of `tree`, whose text `clipboard`
    /// cuts, copies and pastes: the events of its text, or `None` for a key
    /// that a field does not take, as Command-W, which the app's binding
    /// may take. A release takes nothing, and tells nothing.
    pub fn key(
        &mut self,
        tree: &Tree,
        node: NodeId,
        event: &KeyEvent<'_>,
        clipboard: &mut dyn Clipboard,
    ) -> Option<Vec<NodeEvent>> {
        let key = field_key(tree, node)?;
        if event.action == KeyAction::Release {
            return Some(Vec::new());
        }
        let state = self.states.entry(key.clone()).or_default();
        let mods = event.mods;
        let (command, control, shift) = (mods.super_key, mods.control, mods.shift);
        let option = mods.left_alt || mods.right_alt;
        let moving = |toward| Some((toward, shift));
        // A move, and whether it extends the selection.
        let toward = match event.key {
            Key::Left if command => moving(Toward::Start),
            Key::Left if option => moving(Toward::WordBack),
            Key::Left => moving(Toward::Back),
            Key::Right if command => moving(Toward::End),
            Key::Right if option => moving(Toward::WordForward),
            Key::Right => moving(Toward::Forward),
            Key::Home | Key::Up => moving(Toward::Start),
            Key::End | Key::Down => moving(Toward::End),
            Key::Char('a') if control && !command => moving(Toward::Start),
            Key::Char('e') if control && !command => moving(Toward::End),
            Key::Char('b') if control && !command => moving(Toward::Back),
            Key::Char('f') if control && !command => moving(Toward::Forward),
            _ => None,
        };
        if let Some((toward, extend)) = toward {
            let selection = state.selection();
            // An arrow over a selection, without Shift, goes to its end.
            let to = match toward {
                Toward::Back if !extend && !selection.is_empty() => selection.start,
                Toward::Forward if !extend && !selection.is_empty() => selection.end,
                toward => step(&state.text, state.caret, toward),
            };
            state.go(to, extend);
            return Some(Vec::new());
        }
        // A deletion, of the selection, or of what it reaches from the
        // caret.
        let deleting = match event.key {
            Key::Backspace if command => Some(Toward::Start),
            Key::Backspace if option => Some(Toward::WordBack),
            Key::Backspace => Some(Toward::Back),
            Key::Delete if command => Some(Toward::End),
            Key::Delete if option => Some(Toward::WordForward),
            Key::Delete => Some(Toward::Forward),
            Key::Char('h') if control && !command => Some(Toward::Back),
            Key::Char('d') if control && !command => Some(Toward::Forward),
            Key::Char('k') if control && !command => Some(Toward::End),
            _ => None,
        };
        if let Some(toward) = deleting {
            let selection = state.selection();
            let range = if selection.is_empty() {
                let to = step(&state.text, state.caret, toward);
                state.caret.min(to)..state.caret.max(to)
            } else {
                selection
            };
            let changed = state.edit(range, "", false);
            return Some(changed_events(node, key, state, changed));
        }
        let enter = matches!(event.key, Key::Enter | Key::Keypad(Keypad::Enter));
        if enter && !command && !control {
            return Some(vec![NodeEvent {
                node,
                key: key.clone(),
                kind: UiEventKind::Submit {
                    text: state.text.clone(),
                },
            }]);
        }
        if command && !control {
            let changed = match event.key {
                Key::Char('a') => {
                    state.anchor = 0;
                    state.caret = state.text.len();
                    state.typing = false;
                    false
                }
                Key::Char('c') => {
                    let selection = state.selection();
                    if !selection.is_empty() {
                        clipboard.write(&state.text[selection]);
                    }
                    false
                }
                Key::Char('x') => {
                    let selection = state.selection();
                    if !selection.is_empty() {
                        clipboard.write(&state.text[selection.clone()]);
                    }
                    state.edit(selection, "", false)
                }
                Key::Char('v') => match clipboard.read() {
                    Some(pasted) => {
                        let selection = state.selection();
                        state.edit(selection, &single_line(&pasted), false)
                    }
                    None => false,
                },
                Key::Char('z') if shift => state.redo(),
                Key::Char('z') => state.undo(),
                _ => return None,
            };
            return Some(changed_events(node, key, state, changed));
        }
        // Text that a key of a character types, Option's characters too:
        // a named key's, as the private use characters that macOS gives
        // the function keys, types nothing.
        if control || !matches!(event.key, Key::Char(_) | Key::Keypad(_)) {
            return None;
        }
        self.insert(tree, node, event.text)
    }

    /// Types `text` into the field `node` of `tree`, over its selection, as
    /// a key types it or an input method commits it, its controls left out:
    /// the events of its text, or `None` if `node` is no field or nothing
    /// is left to type.
    pub fn insert(&mut self, tree: &Tree, node: NodeId, text: &str) -> Option<Vec<NodeEvent>> {
        let key = field_key(tree, node)?;
        let typed: String = text.chars().filter(|c| !c.is_control()).collect();
        if typed.is_empty() {
            return None;
        }
        let state = self.states.entry(key.clone()).or_default();
        let selection = state.selection();
        let changed = state.edit(selection, &typed, true);
        Some(changed_events(node, key, state, changed))
    }

    /// The pointer's primary button went down at `at`, in pixels from the
    /// tree's corner, over the field `node`, of `clicks` in a row: the caret
    /// at the nearest place between clusters, the selection extended to it
    /// when `extend`; a double click's word selected, a triple click's
    /// whole text; and its moves select until the button goes up.
    pub fn press(
        &mut self,
        (tree, layout): (&Tree, &Layout),
        node: NodeId,
        (at, clicks, extend): ((f32, f32), u32, bool),
        carets: &mut dyn Carets,
    ) {
        let Some(key) = field_key(tree, node) else {
            return;
        };
        let state = self.states.entry(key.clone()).or_default();
        let Some(to) = caret_at(state, layout, tree, node, at.0, carets) else {
            return;
        };
        match clicks {
            0 | 1 => state.go(to, extend),
            2 => {
                let word = word_at(&state.text, to);
                state.anchor = word.start;
                state.caret = word.end;
                state.typing = false;
            }
            _ => {
                state.anchor = 0;
                state.caret = state.text.len();
                state.typing = false;
            }
        }
        self.pressed = Some(key.clone());
    }

    /// The pointer moved to `at` with the primary button down since it went
    /// down over a field: its selection to the place nearest it.
    pub fn drag(
        &mut self,
        (tree, layout): (&Tree, &Layout),
        at: (f32, f32),
        carets: &mut dyn Carets,
    ) {
        let Some(key) = &self.pressed else {
            return;
        };
        let Some(node) = tree.by_key(key) else {
            return;
        };
        let Some(state) = self.states.get_mut(key) else {
            return;
        };
        if let Some(to) = caret_at(state, layout, tree, node, at.0, carets) {
            state.go(to, true);
        }
    }

    /// The primary button went up: its moves select no more.
    pub fn release(&mut self) {
        self.pressed = None;
    }

    /// Whether the primary button went down over a field, whose moves
    /// select.
    pub fn selecting(&self) -> bool {
        self.pressed.is_some()
    }

    /// Selects the whole text of the field `node` of `tree`, as macOS does
    /// as Tab gives a field the focus.
    pub fn select_all(&mut self, tree: &Tree, node: NodeId) {
        let Some(key) = field_key(tree, node) else {
            return;
        };
        let state = self.states.entry(key.clone()).or_default();
        state.anchor = 0;
        state.caret = state.text.len();
        state.typing = false;
    }

    /// Scrolls the field `focused` of `tree` across so that its caret shows
    /// within its room, as `layout` lays it out, and none of its room is
    /// left empty at the right while its text scrolls; the others back to
    /// their text's start, as a field shows it without the focus.
    pub fn follow(
        &mut self,
        tree: &Tree,
        layout: &Layout,
        focused: Option<NodeId>,
        carets: &mut dyn Carets,
    ) {
        let scale = layout.scale();
        for (key, state) in &mut self.states {
            let Some(node) = tree.by_key(key) else {
                continue;
            };
            if focused != Some(node) {
                state.scroll = 0.0;
                continue;
            }
            let Some(frame) = layout.frame(tree, node) else {
                continue;
            };
            let room = (frame.width / scale - 2.0 * PADDING.1).max(0.0);
            let places = carets.carets(&state.text);
            let x = |byte: usize| {
                places
                    .iter()
                    .find(|(at, _)| *at >= byte)
                    .map_or(0.0, |&(_, x)| x)
            };
            let (caret, width) = (x(state.caret), x(state.text.len()));
            let mut scroll = state.scroll;
            if caret - scroll > room {
                scroll = caret - room;
            }
            if caret < scroll {
                scroll = caret;
            }
            state.scroll = scroll.min((width - room).max(0.0)).max(0.0);
        }
    }
}

/// The events of an edit of `state`, the field `node` of `key`'s: its
/// text's change, if it changed.
fn changed_events(
    node: NodeId,
    key: &NodeKey,
    state: &FieldState,
    changed: bool,
) -> Vec<NodeEvent> {
    if !changed {
        return Vec::new();
    }
    vec![NodeEvent {
        node,
        key: key.clone(),
        kind: UiEventKind::InputChanged {
            text: state.text.clone(),
        },
    }]
}

/// The key of `node` of `tree`, if it is a field's.
fn field_key(tree: &Tree, node: NodeId) -> Option<&NodeKey> {
    matches!(tree.kind(node), Some(Kind::Field(_)))
        .then(|| tree.key(node))
        .flatten()
}

/// The place between clusters of `state`'s text nearest `x`, in pixels from
/// the tree's corner, in the field `node`.
fn caret_at(
    state: &FieldState,
    layout: &Layout,
    tree: &Tree,
    node: NodeId,
    x: f32,
    carets: &mut dyn Carets,
) -> Option<usize> {
    let frame = layout.frame(tree, node)?;
    let scale = layout.scale();
    let points = (x - frame.x) / scale - PADDING.1 + state.scroll;
    carets
        .carets(&state.text)
        .into_iter()
        .min_by(|(_, a), (_, b)| (a - points).abs().total_cmp(&(b - points).abs()))
        .map(|(byte, _)| byte)
}

/// `text` on one line: its line breaks and tabs as spaces, its other
/// controls left out, as a paste into a field of one line takes it.
fn single_line(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// Where each grapheme cluster of `text` starts, and its end: the places
/// that the caret takes.
pub fn boundaries(text: &str) -> Vec<usize> {
    let mut segmenter = Segmenter::new();
    let mut places: Vec<usize> = text
        .char_indices()
        .filter(|&(_, c)| segmenter.starts_cluster(c))
        .map(|(at, _)| at)
        .collect();
    places.push(text.len());
    places
}

/// The word of `text` at `at`, or just before it, as a double click selects
/// it: its run of letters and digits; else the cluster at `at`.
fn word_at(text: &str, at: usize) -> Range<usize> {
    let places = boundaries(text);
    let after = |at: usize| {
        places
            .iter()
            .find(|&&place| place > at)
            .copied()
            .unwrap_or(text.len())
    };
    let before = |at: usize| {
        places
            .iter()
            .rev()
            .find(|&&place| place < at)
            .copied()
            .unwrap_or(0)
    };
    let word = |at: usize| text[at..].chars().next().is_some_and(char::is_alphanumeric);
    let inside = if at < text.len() && word(at) {
        Some(at)
    } else if at > 0 && word(before(at)) {
        Some(before(at))
    } else {
        None
    };
    let Some(inside) = inside else {
        return at..after(at);
    };
    let (mut start, mut end) = (inside, inside);
    while start > 0 && word(before(start)) {
        start = before(start);
    }
    while end < text.len() && word(end) {
        end = after(end);
    }
    start..end
}

/// Where the caret at `from` in `text` goes `toward`.
fn step(text: &str, from: usize, toward: Toward) -> usize {
    let places = boundaries(text);
    let before = |at: usize| {
        places
            .iter()
            .rev()
            .find(|&&place| place < at)
            .copied()
            .unwrap_or(0)
    };
    let after = |at: usize| {
        places
            .iter()
            .find(|&&place| place > at)
            .copied()
            .unwrap_or(text.len())
    };
    let word = |at: usize| text[at..].chars().next().is_some_and(char::is_alphanumeric);
    match toward {
        Toward::Back => before(from),
        Toward::Forward => after(from),
        Toward::Start => 0,
        Toward::End => text.len(),
        Toward::WordBack => {
            let mut at = from;
            while at > 0 && !word(before(at)) {
                at = before(at);
            }
            while at > 0 && word(before(at)) {
                at = before(at);
            }
            at
        }
        Toward::WordForward => {
            let mut at = from;
            while at < text.len() && !word(at) {
                at = after(at);
            }
            while at < text.len() && word(at) {
                at = after(at);
            }
            at
        }
    }
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::Node;
    use lokyr_vt_core::input::Mods;

    use super::*;
    use crate::layout::Measure;

    /// A font of the tests': a cluster of ASCII 10 points wide, any other
    /// 20; a line 14 high.
    struct Fixed;

    impl Carets for Fixed {
        fn carets(&mut self, text: &str) -> Vec<(usize, f32)> {
            let places = boundaries(text);
            let mut x = 0.0;
            let mut carets = Vec::new();
            for pair in places.windows(2) {
                carets.push((pair[0], x));
                x += if text[pair[0]..pair[1]].is_ascii() {
                    10.0
                } else {
                    20.0
                };
            }
            carets.push((text.len(), x));
            carets
        }
    }

    impl Measure for Fixed {
        fn text(&mut self, text: &str) -> (f32, f32) {
            (self.carets(text).last().map_or(0.0, |&(_, x)| x), 14.0)
        }

        fn icon(&mut self, _: &str) -> (f32, f32) {
            (16.0, 16.0)
        }
    }

    #[derive(Default)]
    struct Board(Option<String>);

    impl Clipboard for Board {
        fn read(&mut self) -> Option<String> {
            self.0.clone()
        }

        fn write(&mut self, text: &str) {
            self.0 = Some(text.to_owned());
        }
    }

    const NONE: Mods = Mods {
        shift: false,
        control: false,
        left_alt: false,
        right_alt: false,
        super_key: false,
        hyper: false,
        meta: false,
        caps_lock: false,
        num_lock: false,
    };

    fn mods(shift: bool, control: bool, option: bool, command: bool) -> Mods {
        Mods {
            shift,
            control,
            left_alt: option,
            super_key: command,
            ..NONE
        }
    }

    /// A field `find` alone in its tree, laid out at `scale`.
    fn field_tree(scale: f32) -> (Tree, Layout, NodeId) {
        let mut tree = Tree::new();
        let changes = tree
            .update(&Node::container(vec![Node::field("Find").key("find")]))
            .unwrap();
        let mut layout = Layout::new(scale);
        layout.update(&tree, &changes);
        layout.compute(&tree, None, None, &mut Fixed);
        let field = tree.children(tree.root().unwrap())[0];
        (tree, layout, field)
    }

    /// The field, its tree and node, and a way to give it keys.
    struct Typing {
        fields: Fields,
        tree: Tree,
        node: NodeId,
        board: Board,
    }

    impl Typing {
        fn new() -> Self {
            let (tree, _, node) = field_tree(1.0);
            Self {
                fields: Fields::new(),
                tree,
                node,
                board: Board::default(),
            }
        }

        /// The key `key` pressed of `mods`, typing `text`: its events, as
        /// `kind: text` strings.
        fn press(&mut self, key: Key, mods: Mods, text: &str) -> Option<Vec<String>> {
            let event = KeyEvent {
                key,
                action: KeyAction::Press,
                mods,
                text,
                unmodified: text,
                base_layout: None,
            };
            let events = self
                .fields
                .key(&self.tree, self.node, &event, &mut self.board)?;
            Some(
                events
                    .into_iter()
                    .map(|event| match event.kind {
                        UiEventKind::InputChanged { text } => format!("input {text}"),
                        UiEventKind::Submit { text } => format!("submit {text}"),
                        other => format!("{other:?}"),
                    })
                    .collect(),
            )
        }

        fn kind(&mut self, text: &str) {
            for c in text.chars() {
                let typed = c.to_string();
                self.press(Key::Char(c), NONE, &typed).unwrap();
            }
        }

        fn state(&self) -> &FieldState {
            self.fields.state(&NodeKey::from("find")).unwrap()
        }

        /// The text, `|` at the caret and `[` `]` around the selection.
        fn shown(&self) -> String {
            let state = self.state();
            let selection = state.selection();
            let mut shown = String::new();
            for (at, c) in state.text.char_indices().chain([(state.text.len(), '\0')]) {
                if !selection.is_empty() && at == selection.start {
                    shown.push('[');
                }
                if !selection.is_empty() && at == selection.end {
                    shown.push(']');
                }
                if selection.is_empty() && at == state.caret {
                    shown.push('|');
                }
                if c != '\0' {
                    shown.push(c);
                }
            }
            shown
        }
    }

    const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    const ITALY: &str = "\u{1F1EE}\u{1F1F9}";
    const ACUTE: &str = "e\u{301}";

    #[test]
    fn typing_moves_and_deletes_by_whole_clusters() {
        let mut field = Typing::new();
        // Each character typed a key at a time, the family and the flag of
        // several code points as their keys' texts.
        field.kind("ab中");
        for text in [FAMILY, ITALY, ACUTE] {
            let events = field.press(Key::Char('x'), NONE, text).unwrap();
            assert_eq!(events.len(), 1);
        }
        let all = format!("ab中{FAMILY}{ITALY}{ACUTE}");
        assert_eq!(field.state().text(), all);
        assert_eq!(field.shown(), format!("{all}|"));
        // Left, a cluster at a time, never inside one.
        let mut stops = Vec::new();
        for _ in 0..7 {
            field.press(Key::Left, NONE, "");
            stops.push(field.state().caret());
        }
        let places = boundaries(&all);
        let mut expected: Vec<usize> = places[..places.len() - 1].iter().rev().copied().collect();
        expected.push(0);
        assert_eq!(stops, expected);
        // Backspace and Delete take a whole cluster.
        field.press(Key::End, NONE, "");
        field.press(Key::Left, NONE, "");
        field.press(Key::Left, NONE, "");
        assert_eq!(
            field.press(Key::Backspace, NONE, ""),
            Some(vec![format!("input ab中{ITALY}{ACUTE}")])
        );
        assert_eq!(
            field.press(Key::Delete, NONE, ""),
            Some(vec![format!("input ab中{ACUTE}")])
        );
        assert_eq!(field.shown(), format!("ab中|{ACUTE}"));
        // At an end, a deletion past it changes nothing and tells nothing.
        field.press(Key::Home, NONE, "");
        assert_eq!(field.press(Key::Backspace, NONE, ""), Some(vec![]));
    }

    #[test]
    fn words_and_the_line_s_ends_of_macos_s_keys() {
        let mut field = Typing::new();
        field.kind("hello, wide world");
        let option = mods(false, false, true, false);
        let shift_option = mods(true, false, true, false);
        field.press(Key::Left, option, "");
        assert_eq!(field.shown(), "hello, wide |world");
        field.press(Key::Left, option, "");
        field.press(Key::Left, option, "");
        assert_eq!(field.shown(), "|hello, wide world");
        field.press(Key::Right, option, "");
        assert_eq!(field.shown(), "hello|, wide world");
        field.press(Key::Right, shift_option, "");
        assert_eq!(field.shown(), "hello[, wide] world");
        // An arrow over a selection goes to its end.
        field.press(Key::Left, NONE, "");
        assert_eq!(field.shown(), "hello|, wide world");
        field.press(Key::Right, shift_option, "");
        field.press(Key::Right, NONE, "");
        assert_eq!(field.shown(), "hello, wide| world");
        field.press(Key::Right, mods(true, false, false, true), "");
        assert_eq!(field.shown(), "hello, wide[ world]");
        field.press(Key::Char('a'), mods(false, true, false, false), "");
        assert_eq!(field.shown(), "|hello, wide world");
        field.press(Key::Down, NONE, "");
        assert_eq!(field.shown(), "hello, wide world|");
        field.press(Key::Up, NONE, "");
        assert_eq!(field.shown(), "|hello, wide world");
        field.press(Key::Char('e'), mods(false, true, false, false), "");
        field.press(Key::Char('h'), mods(false, true, false, false), "");
        assert_eq!(field.shown(), "hello, wide worl|");
        field.kind("d");
        // Option-Backspace takes a word, Command-Backspace the line's start.
        field.press(Key::Backspace, option, "");
        assert_eq!(field.shown(), "hello, wide |");
        field.press(Key::Left, option, "");
        field.press(Key::Char('k'), mods(false, true, false, false), "");
        assert_eq!(field.shown(), "hello, |");
        field.press(Key::Backspace, mods(false, false, false, true), "");
        assert_eq!(field.shown(), "|");
    }

    #[test]
    fn a_selection_cut_copied_and_pasted_of_any_script() {
        let mut field = Typing::new();
        let command = mods(false, false, false, true);
        field.kind("ab");
        field.press(Key::Char('x'), NONE, "中");
        field.press(Key::Char('x'), NONE, FAMILY);
        // Select all, copy: the clipboard holds it; cut: it goes.
        field.press(Key::Char('a'), command, "");
        assert_eq!(field.press(Key::Char('c'), command, ""), Some(vec![]));
        assert_eq!(
            field.board.0.as_deref(),
            Some(format!("ab中{FAMILY}").as_str())
        );
        assert_eq!(
            field.press(Key::Char('x'), command, ""),
            Some(vec!["input ".to_owned()])
        );
        assert_eq!(field.shown(), "|");
        // Pasted twice; a paste of lines on one line.
        field.press(Key::Char('v'), command, "");
        field.press(Key::Char('v'), command, "");
        assert_eq!(field.shown(), format!("ab中{FAMILY}ab中{FAMILY}|"));
        field.board.0 = Some("one\ntwo\tthree\u{7}".to_owned());
        field.press(Key::Char('a'), command, "");
        assert_eq!(
            field.press(Key::Char('v'), command, ""),
            Some(vec!["input one two three".to_owned()])
        );
        // Typing over a selection replaces it.
        field.press(Key::Left, mods(true, false, true, false), "");
        field.kind("4");
        assert_eq!(field.shown(), "one two 4|");
    }

    #[test]
    fn typing_undoes_at_once_and_an_undo_redoes() {
        let mut field = Typing::new();
        let command = mods(false, false, false, true);
        field.kind("abc");
        field.press(Key::Left, NONE, "");
        field.kind("x");
        assert_eq!(field.shown(), "abx|c");
        field.press(Key::Backspace, NONE, "");
        // The deletion, then the typing after the move, then the typing
        // before it, at once.
        assert_eq!(
            field.press(Key::Char('z'), command, ""),
            Some(vec!["input abxc".to_owned()])
        );
        assert_eq!(
            field.press(Key::Char('z'), command, ""),
            Some(vec!["input abc".to_owned()])
        );
        assert_eq!(
            field.press(Key::Char('z'), command, ""),
            Some(vec!["input ".to_owned()])
        );
        assert_eq!(field.press(Key::Char('z'), command, ""), Some(vec![]));
        let redo = mods(true, false, false, true);
        assert_eq!(
            field.press(Key::Char('z'), redo, ""),
            Some(vec!["input abc".to_owned()])
        );
        assert_eq!(
            field.press(Key::Char('z'), redo, ""),
            Some(vec!["input abxc".to_owned()])
        );
        // A new edit forgets what was undone.
        field.press(Key::Char('z'), command, "");
        field.kind("!");
        assert_eq!(field.press(Key::Char('z'), redo, ""), Some(vec![]));
    }

    #[test]
    fn return_submits_and_the_app_s_keys_are_left() {
        let mut field = Typing::new();
        field.kind("main");
        assert_eq!(
            field.press(Key::Enter, NONE, "\r"),
            Some(vec!["submit main".to_owned()])
        );
        assert_eq!(
            field.press(Key::Keypad(Keypad::Enter), NONE, "\u{3}"),
            Some(vec!["submit main".to_owned()])
        );
        assert_eq!(
            field.press(Key::Char('w'), mods(false, false, false, true), ""),
            None
        );
        assert_eq!(
            field.press(Key::Char('l'), mods(false, true, false, false), ""),
            None
        );
        // A release takes nothing; a node that is no field, nothing.
        let release = KeyEvent {
            key: Key::Char('m'),
            action: KeyAction::Release,
            mods: NONE,
            text: "",
            unmodified: "",
            base_layout: None,
        };
        assert_eq!(
            field
                .fields
                .key(&field.tree, field.node, &release, &mut field.board),
            Some(vec![])
        );
        let root = field.tree.root().unwrap();
        assert_eq!(
            field
                .fields
                .key(&field.tree, root, &release, &mut field.board),
            None
        );
        assert_eq!(field.state().text(), "main");
    }

    #[test]
    fn a_named_key_types_nothing_and_an_input_method_s_text_goes_in() {
        let mut field = Typing::new();
        field.kind("ab");
        // macOS gives the function keys characters of the private use area.
        assert_eq!(field.press(Key::PageUp, NONE, "\u{F72C}"), None);
        assert_eq!(field.press(Key::F(1), NONE, "\u{F704}"), None);
        assert_eq!(field.press(Key::Insert, NONE, "\u{F727}"), None);
        // Option's characters, as Option-Shift-K's of the private use area
        // too, go in.
        let option = mods(false, false, true, false);
        assert_eq!(
            field.press(Key::Char('k'), mods(true, false, true, false), "\u{F8FF}"),
            Some(vec!["input ab\u{F8FF}".to_owned()])
        );
        field.press(Key::Char('e'), option, "\u{B4}");
        assert_eq!(field.shown(), "ab\u{F8FF}\u{B4}|");
        // A commit of an input method, at the caret, over the selection.
        field.press(Key::Left, mods(true, false, false, false), "");
        let events = field.fields.insert(&field.tree, field.node, "中文");
        assert_eq!(events.map(|events| events.len()), Some(1));
        assert_eq!(field.shown(), "ab\u{F8FF}中文|");
        assert_eq!(field.fields.insert(&field.tree, field.node, "\n"), None);
        let root = field.tree.root().unwrap();
        assert_eq!(field.fields.insert(&field.tree, root, "x"), None);
    }

    #[test]
    fn the_pointer_places_the_caret_and_selects() {
        let (tree, layout, node) = field_tree(2.0);
        let frame = layout.frame(&tree, node).unwrap();
        let mut fields = Fields::new();
        fields.set_text(&NodeKey::from("find"), "one two");
        // At a scale of 2: the text from the padding, 12 pixels in, of 20 a
        // character.
        let at = |characters: f32| (frame.x + 12.0 + characters * 20.0, frame.y + 5.0);
        let shown = |fields: &Fields| {
            let state = fields.state(&NodeKey::from("find")).unwrap();
            (state.caret(), state.selection())
        };
        fields.press((&tree, &layout), node, (at(2.4), 1, false), &mut Fixed);
        assert_eq!(shown(&fields), (2, 2..2));
        fields.press((&tree, &layout), node, (at(1.6), 1, false), &mut Fixed);
        assert_eq!(shown(&fields), (2, 2..2));
        // Moves with the button down select, until it goes up.
        assert!(fields.selecting());
        fields.drag((&tree, &layout), at(5.2), &mut Fixed);
        assert_eq!(shown(&fields), (5, 2..5));
        fields.release();
        fields.drag((&tree, &layout), at(7.0), &mut Fixed);
        assert_eq!(shown(&fields).1, 2..5);
        // Shift extends; a double click selects a word; a triple, all.
        fields.press((&tree, &layout), node, (at(7.0), 1, true), &mut Fixed);
        assert_eq!(shown(&fields).1, 2..7);
        fields.press((&tree, &layout), node, (at(5.0), 2, false), &mut Fixed);
        assert_eq!(shown(&fields).1, 4..7);
        fields.press((&tree, &layout), node, (at(0.5), 3, false), &mut Fixed);
        assert_eq!(shown(&fields).1, 0..7);
        // Past the text's end, its end.
        fields.press((&tree, &layout), node, (at(40.0), 1, false), &mut Fixed);
        assert_eq!(shown(&fields), (7, 7..7));
        // As Tab gives it the focus, all of it.
        fields.select_all(&tree, node);
        assert_eq!(shown(&fields), (7, 0..7));
    }

    #[test]
    fn the_caret_scrolls_into_the_field_s_room() {
        let (tree, layout, node) = field_tree(1.0);
        let frame = layout.frame(&tree, node).unwrap();
        let room = frame.width - 2.0 * PADDING.1;
        let key = NodeKey::from("find");
        let mut fields = Fields::new();
        let long = "x".repeat(30);
        fields.set_text(&key, &long);
        fields.follow(&tree, &layout, Some(node), &mut Fixed);
        // The caret at the end of 300 points, in a room of 148; without the
        // focus, the text's start.
        assert_eq!(fields.state(&key).unwrap().scroll(), 300.0 - room);
        fields.follow(&tree, &layout, None, &mut Fixed);
        assert_eq!(fields.state(&key).unwrap().scroll(), 0.0);
        fields.follow(&tree, &layout, Some(node), &mut Fixed);
        let mut board = Board::default();
        let pressed = |key| KeyEvent {
            key,
            action: KeyAction::Press,
            mods: NONE,
            text: "",
            unmodified: "",
            base_layout: None,
        };
        let (home, end, backspace) = (
            pressed(Key::Home),
            pressed(Key::End),
            pressed(Key::Backspace),
        );
        // A click 2 points into the text's room, of a scroll of 152, at
        // 154: the caret between the 15th and the 16th character.
        fields.press(
            (&tree, &layout),
            node,
            ((frame.x + PADDING.1 + 2.0, frame.y + 5.0), 1, false),
            &mut Fixed,
        );
        assert_eq!(fields.state(&key).unwrap().caret(), 15);
        fields.release();
        // A text shortened leaves no room empty at its right: 20
        // characters, 200 points, of a room of 148.
        fields.key(&tree, node, &end, &mut board);
        for _ in 0..10 {
            fields.key(&tree, node, &backspace, &mut board);
        }
        fields.follow(&tree, &layout, Some(node), &mut Fixed);
        assert_eq!(fields.state(&key).unwrap().scroll(), 200.0 - room);
        fields.key(&tree, node, &home, &mut board);
        fields.follow(&tree, &layout, Some(node), &mut Fixed);
        assert_eq!(fields.state(&key).unwrap().scroll(), 0.0);
        // A text that fits scrolls not at all.
        fields.set_text(&key, "short");
        fields.follow(&tree, &layout, Some(node), &mut Fixed);
        assert_eq!(fields.state(&key).unwrap().scroll(), 0.0);
    }

    #[test]
    fn a_plugin_sets_the_text_and_a_field_gone_is_forgotten() {
        let mut field = Typing::new();
        field.kind("typed");
        let key = NodeKey::from("find");
        field.fields.set_text(&key, "set\nby its plugin");
        assert_eq!(field.shown(), "set by its plugin|");
        // Nothing to undo of the plugin's text.
        assert_eq!(
            field.press(Key::Char('z'), mods(false, false, false, true), ""),
            Some(vec![])
        );
        field.fields.updated(&field.tree);
        assert!(field.fields.state(&key).is_some());
        field.tree.update(&Node::container(vec![])).unwrap();
        field.fields.updated(&field.tree);
        assert!(field.fields.state(&key).is_none());
    }
}
