//! The tree of accessibility of a window, as AccessKit takes it
//! (<https://docs.rs/accesskit>): the window, and in it each pane as a
//! terminal, of the text of its rows that show and of its cursor as its
//! selection, in pixels from the content's top left corner, as AccessKit's
//! adapters take them.
//!
//! A row is a run of text, of a character a cell, a wide character's two
//! cells its one; its trailing blanks left out, but those up to the
//! cursor on the cursor's row, and those of a row that wraps into the
//! next, whose text goes on there; a row that does not wrap ends with a
//! line break. A run holds 255 characters at most, as AccessKit counts its
//! words' starts in bytes, so that a longer row is runs of the same line,
//! one after the other: 258 of a row of `u16::MAX` columns and its break.
//!
//! The window's surfaces, its content's, its chrome's and its panels',
//! come after its panes, as [`view`] builds them of their view trees.

pub mod view;

use accesskit::{
    Action, Node, NodeId, Rect, Role, TextPosition, TextSelection, TreeId, TreeInfo, TreeUpdate,
};
use lokyr_vt_core::page::{Content, Width};
use lokyr_vt_core::terminal::{Line, Snapshot};

/// The window's node.
pub const WINDOW: NodeId = NodeId(1);

/// The characters of a run, at most.
const RUN: usize = 255;

/// The runs of a row, at most, as the nodes' ids number them: more than a
/// row of `u16::MAX` columns has.
const RUNS: u64 = 512;

/// The bits of a pane's nodes' ids that number its rows' runs: more than
/// the runs of `u16::MAX` rows take.
const PANE: u32 = 26;

/// A pane, as the window's tree takes it.
#[derive(Clone, Copy, Debug)]
pub struct PaneAccess<'a> {
    /// Its number, unique in its window, which its nodes' ids take.
    pub number: u32,
    /// What a client calls it, as its title.
    pub label: &'a str,
    /// Its screen.
    pub snapshot: &'a Snapshot,
    /// Its grid's top left corner, in pixels from the content's.
    pub grid: (f32, f32),
    /// Its cells' width and height, in pixels.
    pub cell: (f32, f32),
}

/// The node of the pane `number`.
pub fn pane_id(number: u32) -> NodeId {
    NodeId((u64::from(number) + 1) << PANE)
}

/// The pane whose node, or one of whose rows' nodes, `node` is: none of a
/// surface's node, whose id's top bits are past a pane's number.
pub fn pane_of(node: NodeId) -> Option<u32> {
    (node.0 >> PANE)
        .checked_sub(1)
        .and_then(|number| u32::try_from(number).ok())
}

/// The node of the run `run` of the row `row` of the pane `number`.
fn run_id(number: u32, row: u16, run: u64) -> NodeId {
    NodeId(pane_id(number).0 + 1 + u64::from(row) * RUNS + run)
}

/// The window's tree, whole: titled `title`, of `panes`, the pane
/// `focused` of the keyboard's focus, or the window of none.
pub fn window(title: &str, panes: &[PaneAccess<'_>], focused: Option<u32>) -> TreeUpdate {
    let mut tree = WindowTree::new();
    for pane in panes {
        tree.pane(pane);
    }
    tree.finish(title, focused.map(pane_id))
}

/// A window's tree as it is built: its panes and its surfaces, in the
/// order that they draw, each over those before it.
#[derive(Debug, Default)]
pub struct WindowTree {
    nodes: Vec<(NodeId, Node)>,
    children: Vec<NodeId>,
}

impl WindowTree {
    /// A window of nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `pane`.
    pub fn pane(&mut self, pane: &PaneAccess<'_>) {
        self.children.push(pane_id(pane.number));
        self.nodes.extend(pane_nodes(pane));
    }

    /// The window's tree whole, titled `title`, the keyboard's focus on the
    /// node `focus`, or on the window's where it is none of the tree's.
    pub fn finish(self, title: &str, focus: Option<NodeId>) -> TreeUpdate {
        let focus = focus
            .filter(|focus| self.nodes.iter().any(|(id, _)| id == focus))
            .unwrap_or(WINDOW);
        let mut window = Node::new(Role::Window);
        window.set_label(title);
        window.set_children(self.children);
        let mut nodes = Vec::with_capacity(self.nodes.len() + 1);
        nodes.push((WINDOW, window));
        nodes.extend(self.nodes);
        let mut tree = TreeInfo::new(WINDOW);
        tree.toolkit_name = Some("Lokyr".to_owned());
        TreeUpdate {
            nodes,
            tree: Some(tree),
            tree_id: TreeId::ROOT,
            focus,
        }
    }
}

/// The nodes of `pane`: its own, a terminal of its rows' runs, of its
/// cursor as its selection, which a client may give the focus; and its
/// runs'.
fn pane_nodes(pane: &PaneAccess<'_>) -> Vec<(NodeId, Node)> {
    let snapshot = pane.snapshot;
    let (width, height) = pane.cell;
    let (left, top) = pane.grid;
    let cursor = snapshot.cursor().position;
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    let mut selection = None;
    for y in 0..snapshot.rows() {
        let reach = (y == cursor.y).then_some(cursor.x);
        let row = row(snapshot.line(y), reach);
        let count = row.len().div_ceil(RUN);
        for (run, characters) in row.chunks(RUN).enumerate() {
            let id = run_id(pane.number, y, run as u64);
            let first = characters.first().map_or(0, |character| character.column);
            let end = characters.last().map_or(first, |character| {
                character.column + u16::from(character.columns)
            });
            let mut node = Node::new(Role::TextRun);
            let x0 = left + f32::from(first) * width;
            let y0 = top + f32::from(y) * height;
            node.set_bounds(Rect {
                x0: f64::from(x0),
                y0: f64::from(y0),
                x1: f64::from(left + f32::from(end) * width),
                y1: f64::from(y0 + height),
            });
            let text: String = characters
                .iter()
                .map(|character| character.text.as_str())
                .collect();
            node.set_value(text);
            // A cluster holds 32 characters at most, of 128 bytes.
            node.set_character_lengths(
                characters
                    .iter()
                    .map(|character| u8::try_from(character.text.len()).unwrap_or(u8::MAX))
                    .collect::<Vec<_>>(),
            );
            node.set_character_positions(
                characters
                    .iter()
                    .map(|character| f32::from(character.column - first) * width)
                    .collect::<Vec<_>>(),
            );
            node.set_character_widths(
                characters
                    .iter()
                    .map(|character| f32::from(character.columns) * width)
                    .collect::<Vec<_>>(),
            );
            node.set_word_starts(word_starts(characters));
            if run > 0 {
                node.set_previous_on_line(run_id(pane.number, y, run as u64 - 1));
            }
            if run + 1 < count {
                node.set_next_on_line(run_id(pane.number, y, run as u64 + 1));
            }
            // The cursor at its column's character, or at the line's end.
            if let Some(x) = reach {
                let at = characters.iter().position(|character| {
                    u32::from(x) < u32::from(character.column) + u32::from(character.columns.max(1))
                });
                if let Some(index) = at.filter(|_| selection.is_none()) {
                    selection = Some(TextPosition {
                        node: id,
                        character_index: index,
                    });
                }
            }
            children.push(id);
            nodes.push((id, node));
        }
    }
    let mut node = Node::new(Role::Terminal);
    node.set_label(pane.label);
    node.set_bounds(Rect {
        x0: f64::from(left),
        y0: f64::from(top),
        x1: f64::from(left + f32::from(snapshot.cols()) * width),
        y1: f64::from(top + f32::from(snapshot.rows()) * height),
    });
    node.set_children(children);
    node.add_action(Action::Focus);
    if let Some(position) = selection {
        node.set_text_selection(TextSelection {
            anchor: position,
            focus: position,
        });
    }
    let mut all = vec![(pane_id(pane.number), node)];
    all.extend(nodes);
    all
}

/// A character of a row: its text, its first column, and its columns, 0 of
/// a line break.
#[derive(Clone, Debug, PartialEq)]
struct Character {
    text: String,
    column: u16,
    columns: u8,
}

/// The characters of `line`, its trailing blanks left out but those up to
/// `reach`, the cursor's column, of a row that does not wrap, which ends
/// with a line break.
fn row(line: &Line, reach: Option<u16>) -> Vec<Character> {
    let mut characters = Vec::new();
    for (x, cell) in line.cells().iter().enumerate() {
        // A row has `u16::MAX` cells at most.
        let column = x as u16;
        let (text, columns) = match (cell.width(), cell.content()) {
            (Width::SpacerTail, _) => continue,
            (Width::SpacerHead, _) => (" ".to_owned(), 1),
            (width, Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_)) => {
                (" ".to_owned(), if width == Width::Wide { 2 } else { 1 })
            }
            (width, Content::Char(character)) => (
                character.to_string(),
                if width == Width::Wide { 2 } else { 1 },
            ),
            (width, Content::Grapheme(character)) => {
                let mut text = character.to_string();
                text.extend(line.grapheme(column));
                (text, if width == Width::Wide { 2 } else { 1 })
            }
        };
        characters.push(Character {
            text,
            column,
            columns,
        });
    }
    if line.soft_wrapped() {
        return characters;
    }
    let kept = characters
        .iter()
        .rposition(|character| character.text != " ")
        .map_or(0, |last| last + 1);
    let reached = reach.map_or(0, |x| {
        characters
            .iter()
            .take_while(|character| character.column + u16::from(character.columns) <= x)
            .count()
    });
    characters.truncate(kept.max(reached));
    let end = characters.last().map_or(0, |character| {
        character.column + u16::from(character.columns)
    });
    characters.push(Character {
        text: "\n".to_owned(),
        column: end,
        columns: 0,
    });
    characters
}

/// The starts of `characters`' words, past the first: each character that
/// is not a blank after one that is.
fn word_starts(characters: &[Character]) -> Vec<u8> {
    let blank = |character: &Character| character.text.chars().all(char::is_whitespace);
    characters
        .windows(2)
        .enumerate()
        .filter(|(_, pair)| blank(&pair[0]) && !blank(&pair[1]))
        .filter_map(|(at, _)| u8::try_from(at + 1).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use lokyr_vt_core::parser::Parser;
    use lokyr_vt_core::terminal::{Options, Terminal};

    use super::*;

    /// The snapshot of a screen of `cols` by `rows` after `text`.
    fn snapshot(cols: u16, rows: u16, text: &str) -> Snapshot {
        let mut terminal = Terminal::new(cols, rows, Options::default());
        Parser::new().advance(&mut terminal, text.as_bytes());
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
    }

    fn pane(snapshot: &Snapshot) -> PaneAccess<'_> {
        PaneAccess {
            number: 0,
            label: "zsh",
            snapshot,
            grid: (10.0, 20.0),
            cell: (8.0, 16.0),
        }
    }

    fn node(update: &TreeUpdate, id: NodeId) -> &Node {
        &update.nodes.iter().find(|(at, _)| *at == id).unwrap().1
    }

    #[test]
    fn a_window_s_tree_is_its_panes_of_their_rows_and_their_cursors() {
        // `ls` and its output, then a prompt, the cursor after it.
        let screen = snapshot(20, 4, "$ ls\r\nCargo.toml  src\r\n$ ");
        let update = window("Lokyr", &[pane(&screen)], Some(0));
        assert_eq!(update.tree.as_ref().map(|tree| tree.root), Some(WINDOW));
        assert_eq!(update.focus, pane_id(0));
        let root = node(&update, WINDOW);
        assert_eq!((root.role(), root.label()), (Role::Window, Some("Lokyr")));
        assert_eq!(root.children(), [pane_id(0)]);
        let terminal = node(&update, pane_id(0));
        assert_eq!(terminal.role(), Role::Terminal);
        assert_eq!(terminal.label(), Some("zsh"));
        assert!(terminal.supports_action(Action::Focus));
        let bounds = terminal.bounds().unwrap();
        assert_eq!(
            (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
            (10.0, 20.0, 170.0, 84.0)
        );
        // A run a row, of its text and a line break.
        let runs: Vec<&str> = terminal
            .children()
            .iter()
            .map(|&id| node(&update, id).value().unwrap())
            .collect();
        assert_eq!(runs, ["$ ls\n", "Cargo.toml  src\n", "$ \n", "\n"]);
        let second = node(&update, terminal.children()[1]);
        assert_eq!(second.word_starts(), [12]);
        assert_eq!(second.character_lengths().len(), 16);
        assert_eq!(second.character_positions().unwrap()[12], 96.0);
        let bounds = second.bounds().unwrap();
        assert_eq!(
            (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
            (10.0, 36.0, 130.0, 52.0)
        );
        // The cursor after the prompt's blank, at the third row's break.
        let selection = terminal.text_selection().unwrap();
        assert_eq!(selection.focus.node, terminal.children()[2]);
        assert_eq!(selection.focus.character_index, 2);
        assert_eq!(selection.anchor, selection.focus);
        assert_eq!(pane_of(selection.focus.node), Some(0));
        assert_eq!(pane_of(WINDOW), None);
    }

    #[test]
    fn a_row_keeps_the_blanks_to_the_cursor_and_a_wrapped_one_its_own() {
        // The cursor four columns past the text: its blanks kept.
        let screen = snapshot(10, 2, "ab\x1b[1;7H");
        let update = window("", &[pane(&screen)], None);
        assert_eq!(update.focus, WINDOW);
        let terminal = node(&update, pane_id(0));
        let first = node(&update, terminal.children()[0]);
        assert_eq!(first.value(), Some("ab    \n"));
        assert_eq!(terminal.text_selection().unwrap().focus.character_index, 6);
        // A row that wraps keeps its blanks, and has no break.
        let screen = snapshot(4, 3, "ab  cdef");
        let update = window("", &[pane(&screen)], None);
        let terminal = node(&update, pane_id(0));
        let runs: Vec<&str> = terminal
            .children()
            .iter()
            .map(|&id| node(&update, id).value().unwrap())
            .collect();
        assert_eq!(runs[0], "ab  ");
        assert_eq!(runs[1], "cdef\n");
    }

    #[test]
    fn a_wide_character_is_one_character_of_two_cells() {
        let screen = snapshot(10, 1, "日本 x");
        let update = window("", &[pane(&screen)], None);
        let terminal = node(&update, pane_id(0));
        let run = node(&update, terminal.children()[0]);
        assert_eq!(run.value(), Some("日本 x\n"));
        assert_eq!(run.character_lengths(), [3, 3, 1, 1, 1]);
        assert_eq!(
            run.character_positions().unwrap(),
            [0.0, 16.0, 32.0, 40.0, 48.0]
        );
        assert_eq!(run.character_widths().unwrap(), [16.0, 16.0, 8.0, 8.0, 0.0]);
        assert_eq!(run.word_starts(), [3]);
    }

    #[test]
    fn a_cluster_is_one_character_and_a_wide_one_of_two_cells() {
        // An accent that combines, and a thumb of a skin's tone.
        let screen = snapshot(10, 1, "e\u{301}\u{1F44D}\u{1F3FD}x");
        let update = window("", &[pane(&screen)], None);
        let terminal = node(&update, pane_id(0));
        let run = node(&update, terminal.children()[0]);
        assert_eq!(run.value(), Some("e\u{301}\u{1F44D}\u{1F3FD}x\n"));
        assert_eq!(run.character_lengths(), [3, 8, 1, 1]);
        assert_eq!(run.character_widths().unwrap(), [8.0, 16.0, 8.0, 0.0]);
    }

    #[test]
    fn a_wide_character_that_wraps_leaves_a_blank_at_the_row_s_end() {
        let screen = snapshot(4, 2, "abc日");
        let update = window("", &[pane(&screen)], None);
        let terminal = node(&update, pane_id(0));
        let runs: Vec<&str> = terminal
            .children()
            .iter()
            .map(|&id| node(&update, id).value().unwrap())
            .collect();
        assert_eq!(runs, ["abc ", "日\n"]);
    }

    #[test]
    fn the_focus_of_a_pane_that_is_not_there_is_the_window_s() {
        let screen = snapshot(10, 1, "");
        let update = window("", &[pane(&screen)], Some(3));
        assert_eq!(update.focus, WINDOW);
        assert_eq!(
            update.tree.and_then(|tree| tree.toolkit_name).as_deref(),
            Some("Lokyr")
        );
    }

    #[test]
    fn a_long_row_is_runs_of_the_same_line() {
        let text = "x".repeat(300);
        let screen = snapshot(300, 1, &text);
        let update = window("", &[pane(&screen)], None);
        let terminal = node(&update, pane_id(0));
        assert_eq!(terminal.children().len(), 2);
        let (first, second) = (
            node(&update, terminal.children()[0]),
            node(&update, terminal.children()[1]),
        );
        assert_eq!(first.value().unwrap().len(), RUN);
        assert_eq!(
            second.value().unwrap(),
            format!("{}\n", "x".repeat(300 - RUN))
        );
        assert_eq!(first.next_on_line(), Some(terminal.children()[1]));
        assert_eq!(second.previous_on_line(), Some(terminal.children()[0]));
        assert_eq!(first.previous_on_line(), None);
        assert_eq!(second.next_on_line(), None);
        assert_eq!(second.bounds().unwrap().x0, 10.0 + RUN as f64 * 8.0);
        assert_eq!(second.character_positions().unwrap()[..2], [0.0, 8.0]);
        // The cursor at the last column, as it waits to wrap.
        let selection = terminal.text_selection().unwrap();
        assert_eq!(selection.focus.node, terminal.children()[1]);
        assert_eq!(selection.focus.character_index, 300 - RUN - 1);
    }

    #[test]
    fn the_cursor_of_a_long_row_s_first_run_is_there() {
        let text = format!("{}\r", "x".repeat(300));
        let screen = snapshot(300, 2, &text);
        let update = window("", &[pane(&screen)], None);
        let terminal = node(&update, pane_id(0));
        let selection = terminal.text_selection().unwrap();
        assert_eq!(selection.focus.node, terminal.children()[0]);
        assert_eq!(selection.focus.character_index, 0);
    }

    #[test]
    fn the_widest_rows_keep_their_text_and_their_runs_ids_their_own() {
        let screen = snapshot(u16::MAX, 2, &"x".repeat(usize::from(u16::MAX)));
        let update = window("", &[pane(&screen)], None);
        let terminal = node(&update, pane_id(0));
        // 255 characters a run of the first row, then its break in a run
        // of its own, and the second row's break.
        assert_eq!(terminal.children().len(), 258 + 1);
        let text: usize = terminal.children()[..257]
            .iter()
            .map(|&id| node(&update, id).value().unwrap().len())
            .sum();
        assert_eq!(text, usize::from(u16::MAX));
        let mut ids = terminal.children().to_vec();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 259);
        assert!(ids.iter().all(|&id| pane_of(id) == Some(0)));
        assert_eq!(pane_of(pane_id(u32::MAX)), Some(u32::MAX));
    }
}
