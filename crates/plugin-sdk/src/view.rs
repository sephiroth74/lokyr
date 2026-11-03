//! The plugins' UI as data: a view tree of nodes, which a plugin renders
//! and the host lays out, draws and keeps, plain data that crosses the
//! plugins' boundary alike from the main thread, a worker or a Luau VM.
//!
//! A node is a container of others, a text, an icon, a spacer, a separator,
//! a progress or a widget, a field, a button, a toggle, a list or a scroll
//! area, with the properties of its
//! layout and of its style, whose colors are the theme's tokens, so that a
//! theme restyles every plugin's UI. A node
//! keeps its place from one render to the next by its key, unique in its
//! whole tree, as `tab-3.close`, or, without one, by its order among its
//! siblings of its kind; a node that holds state, as a widget whose state
//! the host keeps, that takes the keyboard's focus, or that has a tooltip,
//! needs its key. A
//! node of a key learns of the pointer over it, as [`UiEventKind`] names
//! what it did, by its key alone, wherever a render puts it.
//! The host refuses a tree that nests past 64 levels, or of a number that
//! is none or infinite, a length, a gap, a padding, a grow, a radius, a
//! border or a blur below 0, or a fraction past 0 to 1, keeping the last.

use serde::{Deserialize, Serialize};

/// A node's key, unique in its whole tree: what keeps the node, and the
/// state that the host keeps of it, from one render to the next, and what
/// its events tell it by.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Key(pub String);

impl From<&str> for Key {
    fn from(key: &str) -> Self {
        Self(key.to_owned())
    }
}

/// A node of a view tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Its key, if it has one.
    pub key: Option<Key>,
    /// What it is.
    pub kind: Kind,
    /// Its place and size among its siblings, and its children's.
    #[serde(default)]
    pub layout: Layout,
    /// How it draws.
    #[serde(default)]
    pub style: Style,
    /// Whether it takes the keyboard's focus, which Tab and Shift-Tab move
    /// among its panel's nodes that take it, in the tree's order: a node of
    /// a key alone, as the host keeps the focus by it.
    #[serde(default)]
    pub focusable: bool,
    /// What a tooltip tells of it, as the pointer rests over it: a node of
    /// a key alone, as the host follows the pointer by it.
    #[serde(default)]
    pub tooltip: Option<String>,
    /// Its children, a container's alone.
    #[serde(default)]
    pub children: Vec<Node>,
}

/// What a node is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A box that lays out its children in a row or a column.
    Container,
    /// A text.
    Text(String),
    /// An icon of the host's, by its name.
    Icon(String),
    /// Room, which grows by default, as between the left and the right of a
    /// bar.
    Spacer,
    /// A field of one line of text, whose text, caret and selection the
    /// host keeps by the node's key, which its plugin learns of through
    /// `input-changed` and `submit`, and sets through a request.
    Field(Field),
    /// A button, of a label, an icon or both, which a click, Space or
    /// Return presses: its plugin learns of it as a `click`.
    Button(Button),
    /// A button that stays on or off, as a find bar's case-sensitive
    /// option, which a click, Space or Return switches: its plugin learns
    /// of the state asked as a `toggle`, and gives it in its next render.
    Toggle(Toggle),
    /// A line across its container, a point thick: upright in a row, flat
    /// in a column.
    Separator,
    /// A list of many rows, of flat items and a row's template, whose rows
    /// the host lays out and draws as they show alone, and whose scroll and
    /// selection it keeps by the node's key: its plugin gives the items
    /// that the host asks for, by `list-range`, and learns of its selection
    /// by `select` and `activate`. It takes the room that its layout gives
    /// it, a height or a grow.
    List(List),
    /// A container that scrolls its children up and down, as a wheel or a
    /// trackpad moves them, whose scroll the host keeps by the node's key.
    /// It takes the room that its layout gives it.
    Scroll,
    /// A progress, a bar or a ring, of a state and how far it is, of the
    /// size that its layout gives it; its track of its style's background
    /// and its fill of its style's color, its state's tokens without them.
    /// An indeterminate one moves without the app's frames, as the window
    /// server moves it.
    Progress(Progress),
}

/// A progress's own properties.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Progress {
    /// Its state.
    pub state: ProgressState,
    /// How far it is, from 0 to 1, but of an indeterminate one.
    pub value: f32,
    /// Its shape.
    pub shape: ProgressShape,
    /// The radius of a bar's corners, in points: half its height, a pill's,
    /// without it; 0 for square ones.
    pub radius: Option<f32>,
    /// The thickness of a ring's line, in points.
    pub thickness: f32,
    /// The part of an indeterminate one that moves, of a bar's width or of
    /// a ring's turn, from 0 to 1.
    pub segment: f32,
    /// How long an indeterminate one's sweep across, or turn round, takes,
    /// in seconds: 1.5 for a bar, 1 for a ring, without it.
    pub period: Option<f32>,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            state: ProgressState::Normal,
            value: 0.0,
            shape: ProgressShape::Bar,
            radius: None,
            thickness: 2.0,
            segment: 0.3,
            period: None,
        }
    }
}

/// A progress's state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgressState {
    /// Under way, as far as its value, of the accent's color.
    #[default]
    Normal,
    /// Failed, as far as it went, red.
    Error,
    /// Paused, as far as it went, yellow.
    Paused,
    /// Under way, of no known end: a part of it moves.
    Indeterminate,
}

/// A progress's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgressShape {
    /// A bar, filled from the left.
    #[default]
    Bar,
    /// A ring, filled clockwise from the top, as a spinner of macOS's.
    Ring,
}

/// A list's own properties.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct List {
    /// The template of a row: its texts and its icons, in the tree's order,
    /// take an item's texts by their places, the first the first; one
    /// that the item lacks keeps its own. Containers, texts, icons, spacers
    /// and separators alone.
    pub row: Box<Node>,
    /// How many items the list has.
    pub count: usize,
    /// The place of the first of `items` among them all.
    #[serde(default)]
    pub first: usize,
    /// The items from `first`, as many as the host asked for, or more.
    #[serde(default)]
    pub items: Vec<ListItem>,
    /// The height of every row, in points; without it, each row takes its
    /// own, which the host measures as it shows.
    #[serde(default)]
    pub row_height: Option<f32>,
}

/// A list's item: the texts that its row's template shows, by their
/// places in it. Its index among the list's items tells it apart.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ListItem {
    /// Its texts.
    pub texts: Vec<String>,
}

/// A text field's own properties.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Field {
    /// What it shows while it is empty, as `Find`.
    pub placeholder: String,
}

/// A button's own properties.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Button {
    /// Its label, if it has one.
    pub label: String,
    /// Its icon of the host's, by its name, if it has one, before its
    /// label.
    pub icon: String,
    /// Whether it is dimmed, and takes neither a click nor the focus.
    pub disabled: bool,
}

/// A toggle's own properties.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Toggle {
    /// Its label, if it has one.
    pub label: String,
    /// Its icon of the host's, by its name, if it has one, before its
    /// label.
    pub icon: String,
    /// Whether it is on.
    pub on: bool,
    /// Whether it is dimmed, and takes neither a click nor the focus.
    pub disabled: bool,
}

impl Kind {
    /// Whether a node of the kind holds state that the host keeps, which
    /// needs the node's key.
    pub fn holds_state(&self) -> bool {
        match self {
            Self::Container
            | Self::Text(_)
            | Self::Icon(_)
            | Self::Spacer
            | Self::Separator
            | Self::Progress(_) => false,
            Self::Field(_) | Self::Button(_) | Self::Toggle(_) | Self::List(_) | Self::Scroll => {
                true
            }
        }
    }

    /// Whether a node of the kind takes the keyboard's focus of its own, as
    /// a field, a list, a button and a toggle do, unless they are disabled,
    /// whatever its node says.
    pub fn takes_focus(&self) -> bool {
        match self {
            Self::Field(_) | Self::List(_) => true,
            Self::Button(button) => !button.disabled,
            Self::Toggle(toggle) => !toggle.disabled,
            _ => false,
        }
    }

    /// Whether a node of the kind is a control, as macOS has them, a button
    /// or a toggle: a click gives it no focus, which stays where it was,
    /// and Tab reaches it only as the system's keyboard navigation reaches
    /// every control.
    pub fn is_control(&self) -> bool {
        matches!(self, Self::Button(_) | Self::Toggle(_))
    }

    /// Whether a node of the kind is a widget that is disabled, which takes
    /// neither the pointer's presses nor the focus.
    pub fn disabled(&self) -> bool {
        match self {
            Self::Button(button) => button.disabled,
            Self::Toggle(toggle) => toggle.disabled,
            _ => false,
        }
    }

    /// Whether a node of the kind has children.
    pub fn has_children(&self) -> bool {
        matches!(self, Self::Container | Self::Scroll)
    }
}

/// How a node lays out its children, and takes its room among its
/// siblings, of flexbox's properties.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Layout {
    /// The direction of its children.
    pub direction: Direction,
    /// The room between its children, in points.
    pub gap: f32,
    /// The room inside its edges, in points.
    pub padding: Edges,
    /// Its children's place across its direction.
    pub align: Align,
    /// Its children's place along its direction.
    pub justify: Justify,
    /// Its width.
    pub width: Length,
    /// Its height.
    pub height: Length,
    /// How much of the room left along its parent's direction it takes, in
    /// proportion to its siblings'.
    pub grow: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            direction: Direction::Row,
            gap: 0.0,
            padding: Edges::default(),
            align: Align::Stretch,
            justify: Justify::Start,
            width: Length::Auto,
            height: Length::Auto,
            grow: 0.0,
        }
    }
}

/// A color of the theme, by its token's name, as `tab.active.bg` or
/// `search.match`, which the theme gives in the light and the dark
/// appearance. A name that the theme lacks takes the color of the name
/// without its last part, `tab.active` for `tab.active.bg`, then `tab`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Token(pub String);

impl From<&str> for Token {
    fn from(token: &str) -> Self {
        Self(token.to_owned())
    }
}

/// How a node draws: its box, of a background, a border, rounded corners
/// and a shadow, and the color of its texts and icons.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Style {
    /// What fills its box; `None` for nothing.
    pub background: Option<Token>,
    /// Its border, inside its edges; `None` for none.
    pub border: Option<Border>,
    /// The radius of its box's corners, in points.
    pub radius: f32,
    /// Its box's shadow; `None` for none.
    pub shadow: Option<Shadow>,
    /// The color of its text or icon, and of its children's that name none;
    /// `None` for its parent's, and at the root the theme's `text`.
    pub color: Option<Token>,
    /// Whether its children draw within its box alone, as a list's rows
    /// within the list, and the pointer finds them there alone.
    pub clip: bool,
    /// How the pointer looks over its box, and over its children's that
    /// name none; `None` for its parent's, and at the root the arrow.
    pub pointer: Option<PointerShape>,
    /// How opaque it draws, with its children, from 0 to 1; `None` for
    /// opaque.
    pub opacity: Option<f32>,
    /// How its changes of place and of opacity go, as a render moves it or
    /// changes its opacity: over a time, of a curve; `None` at once.
    pub transition: Option<Transition>,
}

/// How a node's changes of place and of opacity go.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Transition {
    /// How long they take, in seconds.
    pub duration: f32,
    /// Their curve.
    pub easing: Easing,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            duration: 0.2,
            easing: Easing::EaseOut,
        }
    }
}

/// How a transition goes, from its start to its end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Easing {
    /// At a constant rate.
    Linear,
    /// Slow at its start.
    EaseIn,
    /// Slow at its end.
    #[default]
    EaseOut,
    /// Slow at its start and at its end.
    EaseInOut,
}

/// How the pointer looks over a node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PointerShape {
    /// The arrow.
    #[default]
    Arrow,
    /// The text's I-beam, as over text to select or to edit.
    Text,
    /// The pointing hand, as over a link or a button.
    PointingHand,
}

/// What happened to a node of a key, as the host tells the node's plugin:
/// what the pointer did over it, its keyboard's focus, and what
/// accessibility asked of it. It crosses the boundary as a flat table, of
/// its `kind` and its values, as `{ kind = "drag-move", x = 12, y = 0 }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UiEventKind {
    /// The primary button pressed and released over it, or accessibility's
    /// press, as a button's.
    Click,
    /// The second click of a double click, which comes after its click.
    DoubleClick,
    /// The middle button pressed and released over it, as a tab closes.
    MiddleClick,
    /// The secondary button pressed over it, or the primary one with
    /// Control, which opens a context menu.
    ContextMenu,
    /// The pointer came over its box, or over one of its children's.
    HoverEnter,
    /// The pointer left its box and its children's.
    HoverLeave,
    /// The primary button went down over it and the pointer moved past the
    /// threshold of a drag: its moves come to it alone until the button
    /// goes up, wherever the pointer goes, and no click.
    DragStart,
    /// The pointer moved in a drag, `x` rightwards and `y` downwards of
    /// where the drag started, in points.
    DragMove {
        /// Rightwards, in points.
        x: f32,
        /// Downwards, in points.
        y: f32,
    },
    /// The button went up, and the drag ended there, `x` and `y` of where
    /// it started, in points.
    DragEnd {
        /// Rightwards, in points.
        x: f32,
        /// Downwards, in points.
        y: f32,
    },
    /// The drag ended without its end: the window lost the keyboard's
    /// focus, Tab moved it, or the app took the pointer back.
    DragCancel,
    /// A wheel or a trackpad scrolled over it, `x` and `y` in points, a
    /// positive `y` towards the content above, as the system's natural
    /// scrolling has it.
    Scroll {
        /// Across, in points.
        x: f32,
        /// Up and down, in points.
        y: f32,
    },
    /// It took the keyboard's focus.
    Focus,
    /// It lost the keyboard's focus.
    Blur,
    /// Accessibility set its value, as VoiceOver does of a field.
    SetValue {
        /// The value.
        value: String,
    },
    /// A field's text changed, as its user edited it.
    InputChanged {
        /// Its text now.
        text: String,
    },
    /// Return pressed in a field.
    Submit {
        /// Its text.
        text: String,
    },
    /// A toggle clicked, or pressed by Space or Return: the state asked of
    /// it, the other than its render's.
    Toggle {
        /// Whether it is to be on.
        on: bool,
    },
    /// A list's selection moved to an item, by a click or the keys.
    Select {
        /// The item's index.
        item: usize,
    },
    /// A list's item chosen, by a double click or Return.
    Activate {
        /// The item's index.
        item: usize,
    },
    /// The host asks for a list's items from `first`, `count` of them, or
    /// as many as the list has, about the rows that show: the next render
    /// gives them.
    ListRange {
        /// The first item's index.
        first: usize,
        /// How many.
        count: usize,
    },
}

/// A node's border.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Border {
    /// Its color.
    pub color: Token,
    /// Its width, in points.
    pub width: f32,
}

/// A node's shadow: its box moved, blurred and filled with its color.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Shadow {
    /// Its color, of an opacity that makes it lighter.
    pub color: Token,
    /// How far right it is moved, in points.
    pub x: f32,
    /// How far down it is moved, in points.
    pub y: f32,
    /// How far its edge blurs, in points.
    pub blur: f32,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            color: Token::from("shadow"),
            x: 0.0,
            y: 2.0,
            blur: 8.0,
        }
    }
}

/// The direction of a container's children.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    /// Left to right.
    #[default]
    Row,
    /// Top to bottom.
    Column,
}

/// The room inside a node's four edges, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Edges {
    /// At the top.
    pub top: f32,
    /// At the right.
    pub right: f32,
    /// At the bottom.
    pub bottom: f32,
    /// At the left.
    pub left: f32,
}

impl Edges {
    /// The same room at every edge.
    pub fn all(points: f32) -> Self {
        Self {
            top: points,
            right: points,
            bottom: points,
            left: points,
        }
    }
}

/// A container's children's place across its direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Align {
    /// At the start, the top of a row, the left of a column.
    Start,
    /// In the middle.
    Center,
    /// At the end.
    End,
    /// Across the whole container.
    #[default]
    Stretch,
}

/// A container's children's place along its direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Justify {
    /// From the start.
    #[default]
    Start,
    /// In the middle.
    Center,
    /// To the end.
    End,
    /// The room left between them.
    SpaceBetween,
}

/// A node's width or height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Length {
    /// Of its content.
    #[default]
    Auto,
    /// Of points.
    Points(f32),
    /// A fraction of its parent's, from 0 to 1.
    Fraction(f32),
}

impl Node {
    /// A node of `kind`, without a key, of the default layout, and with no
    /// children.
    pub fn new(kind: Kind) -> Self {
        Self {
            key: None,
            kind,
            layout: Layout::default(),
            style: Style::default(),
            focusable: false,
            tooltip: None,
            children: Vec::new(),
        }
    }

    /// A container of `children`.
    pub fn container(children: Vec<Node>) -> Self {
        Self {
            children,
            ..Self::new(Kind::Container)
        }
    }

    /// A text.
    pub fn text(text: impl Into<String>) -> Self {
        Self::new(Kind::Text(text.into()))
    }

    /// An icon of the host's, by its name.
    pub fn icon(name: impl Into<String>) -> Self {
        Self::new(Kind::Icon(name.into()))
    }

    /// A text field, of its placeholder; its key, which it needs, follows.
    pub fn field(placeholder: impl Into<String>) -> Self {
        Self::new(Kind::Field(Field {
            placeholder: placeholder.into(),
        }))
    }

    /// A button of `label`; its key, which it needs, follows.
    pub fn button(label: impl Into<String>) -> Self {
        Self::new(Kind::Button(Button {
            label: label.into(),
            ..Button::default()
        }))
    }

    /// A button of the icon `name` alone, which its tooltip names; its
    /// key, which it needs, follows.
    pub fn icon_button(name: impl Into<String>) -> Self {
        Self::new(Kind::Button(Button {
            icon: name.into(),
            ..Button::default()
        }))
    }

    /// A toggle of the icon `name`, on or off; its key, which it needs,
    /// follows.
    pub fn toggle(name: impl Into<String>, on: bool) -> Self {
        Self::new(Kind::Toggle(Toggle {
            icon: name.into(),
            on,
            ..Toggle::default()
        }))
    }

    /// A line across its container.
    pub fn separator() -> Self {
        Self::new(Kind::Separator)
    }

    /// A list of `count` items, of rows of `row`'s template, which gives
    /// `items` from `first`; its key, which it needs, follows.
    pub fn list(row: Node, count: usize, (first, items): (usize, Vec<ListItem>)) -> Self {
        Self::new(Kind::List(List {
            row: Box::new(row),
            count,
            first,
            items,
            row_height: None,
        }))
    }

    /// A progress of `state`, as far as `value`, from 0 to 1, in `shape`.
    pub fn progress(shape: ProgressShape, state: ProgressState, value: f32) -> Self {
        Self::new(Kind::Progress(Progress {
            state,
            value,
            shape,
            ..Progress::default()
        }))
    }

    /// A container of `children` that scrolls them; its key, which it
    /// needs, follows.
    pub fn scroll(children: Vec<Node>) -> Self {
        Self {
            children,
            ..Self::new(Kind::Scroll)
        }
    }

    /// Room that grows.
    pub fn spacer() -> Self {
        let mut spacer = Self::new(Kind::Spacer);
        spacer.layout.grow = 1.0;
        spacer
    }

    /// The node with its key.
    #[must_use]
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// The node with its layout.
    #[must_use]
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// The node with its style.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// The node, taking the keyboard's focus.
    #[must_use]
    pub fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }

    /// The node with its tooltip.
    #[must_use]
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_crosses_the_boundary_as_it_is() {
        let tree = Node::container(vec![
            Node::icon("terminal").key("icon").focusable(),
            Node::text("~/Projects"),
            Node::spacer(),
        ])
        .layout(Layout {
            direction: Direction::Row,
            gap: 4.0,
            padding: Edges::all(2.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("panel.bg")),
            border: Some(Border {
                color: Token::from("panel.border"),
                width: 1.0,
            }),
            radius: 6.0,
            shadow: Some(Shadow::default()),
            color: Some(Token::from("text.secondary")),
            clip: true,
            pointer: Some(PointerShape::PointingHand),
            opacity: Some(0.5),
            transition: Some(Transition::default()),
        });
        let json = serde_json::to_string(&tree).unwrap();
        assert_eq!(serde_json::from_str::<Node>(&json).unwrap(), tree);
        // A transition of its curve alone, of the default time.
        let style: Style =
            serde_json::from_str(r#"{"opacity":0.25,"transition":{"easing":"ease-in-out"}}"#)
                .unwrap();
        assert_eq!(style.opacity, Some(0.25));
        assert_eq!(
            style.transition,
            Some(Transition {
                duration: 0.2,
                easing: Easing::EaseInOut
            })
        );
        // A layout's properties left out take their defaults, and a style
        // left out the default style.
        let node: Node = serde_json::from_str(
            r#"{"key":null,"kind":"spacer","layout":{"grow":1.0},"children":[]}"#,
        )
        .unwrap();
        assert_eq!(node, Node::spacer());
        // A leaf without its layout and its children, as Luau's tables can
        // leave them out.
        let leaf: Node = serde_json::from_str(r#"{"key":null,"kind":{"text":"a"}}"#).unwrap();
        assert_eq!(leaf, Node::text("a"));
        // A field, of its placeholder or without it.
        let field: Node =
            serde_json::from_str(r#"{"key":"find","kind":{"field":{"placeholder":"Find"}}}"#)
                .unwrap();
        assert_eq!(field, Node::field("Find").key("find"));
        let field: Node = serde_json::from_str(r#"{"key":"find","kind":{"field":{}}}"#).unwrap();
        assert_eq!(field, Node::field("").key("find"));
        // A button, a toggle and a separator, of their properties left out
        // as their defaults; a tooltip.
        let button: Node = serde_json::from_str(
            r#"{"key":"add","kind":{"button":{"icon":"add"}},"tooltip":"New Tab"}"#,
        )
        .unwrap();
        assert_eq!(
            button,
            Node::icon_button("add").key("add").tooltip("New Tab")
        );
        let toggle: Node =
            serde_json::from_str(r#"{"key":"regex","kind":{"toggle":{"icon":"regex","on":true}}}"#)
                .unwrap();
        assert_eq!(toggle, Node::toggle("regex", true).key("regex"));
        let separator: Node = serde_json::from_str(r#"{"key":null,"kind":"separator"}"#).unwrap();
        assert_eq!(separator, Node::separator());
        // A list, of its window of items and its row's template; a scroll
        // area.
        let list: Node = serde_json::from_str(concat!(
            r#"{"key":"rows","kind":{"list":{"row":{"key":null,"kind":{"text":""}},"#,
            r#""count":100000,"first":20,"items":[{"texts":["row 20"]}],"row-height":22.0}}}"#
        ))
        .unwrap();
        let mut expected = Node::list(
            Node::text(""),
            100_000,
            (
                20,
                vec![ListItem {
                    texts: vec!["row 20".to_owned()],
                }],
            ),
        )
        .key("rows");
        if let Kind::List(list) = &mut expected.kind {
            list.row_height = Some(22.0);
        }
        assert_eq!(list, expected);
        let scroll: Node =
            serde_json::from_str(r#"{"key":"area","kind":"scroll","children":[]}"#).unwrap();
        assert_eq!(scroll, Node::scroll(vec![]).key("area"));
        // A progress, of its properties left out as their defaults.
        let ring: Node = serde_json::from_str(
            r#"{"key":null,"kind":{"progress":{"state":"indeterminate","shape":"ring","period":0.7}}}"#,
        )
        .unwrap();
        let mut expected = Node::progress(ProgressShape::Ring, ProgressState::Indeterminate, 0.0);
        if let Kind::Progress(progress) = &mut expected.kind {
            progress.period = Some(0.7);
        }
        assert_eq!(ring, expected);
        let bar: Node =
            serde_json::from_str(r#"{"key":null,"kind":{"progress":{"value":0.4,"radius":0.0}}}"#)
                .unwrap();
        let Kind::Progress(bar) = bar.kind else {
            panic!("no progress");
        };
        assert_eq!(
            (
                bar.state,
                bar.shape,
                bar.value,
                bar.radius,
                bar.thickness,
                bar.segment
            ),
            (
                ProgressState::Normal,
                ProgressShape::Bar,
                0.4,
                Some(0.0),
                2.0,
                0.3
            )
        );
        let events = [
            UiEventKind::DoubleClick,
            UiEventKind::DragMove { x: 12.0, y: -1.5 },
            UiEventKind::SetValue {
                value: "main".to_owned(),
            },
            UiEventKind::InputChanged {
                text: "ma".to_owned(),
            },
            UiEventKind::Submit {
                text: "main".to_owned(),
            },
            UiEventKind::Toggle { on: true },
            UiEventKind::Select { item: 3 },
            UiEventKind::Activate { item: 3 },
            UiEventKind::ListRange {
                first: 10,
                count: 105,
            },
        ];
        let json = serde_json::to_string(&events).unwrap();
        assert_eq!(
            json,
            concat!(
                r#"[{"kind":"double-click"},{"kind":"drag-move","x":12.0,"y":-1.5},"#,
                r#"{"kind":"set-value","value":"main"},{"kind":"input-changed","text":"ma"},"#,
                r#"{"kind":"submit","text":"main"},{"kind":"toggle","on":true},"#,
                r#"{"kind":"select","item":3},{"kind":"activate","item":3},"#,
                r#"{"kind":"list-range","first":10,"count":105}]"#
            )
        );
        assert_eq!(
            serde_json::from_str::<Vec<UiEventKind>>(&json).unwrap(),
            events
        );
    }
}
