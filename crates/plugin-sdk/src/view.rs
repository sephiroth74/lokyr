//! The plugins' UI as data: a view tree of nodes, which a plugin renders
//! and the host lays out, draws and keeps, plain data that crosses the
//! plugins' boundary alike from the main thread, a worker or a Luau VM.
//!
//! A node is a container of others, a text, an icon or a spacer, with the
//! properties of its layout and of its style, whose colors are the theme's
//! tokens, so that a theme restyles every plugin's UI; the widgets come as
//! their own kinds. A node
//! keeps its place from one render to the next by its key, unique in its
//! whole tree, as `tab-3.close`, or, without one, by its order among its
//! siblings of its kind; a node that holds state, as a widget whose state
//! the host keeps, or that takes the keyboard's focus, needs its key. A
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
}

impl Kind {
    /// Whether a node of the kind holds state that the host keeps, which
    /// needs the node's key.
    pub fn holds_state(&self) -> bool {
        match self {
            Self::Container | Self::Text(_) | Self::Icon(_) | Self::Spacer => false,
        }
    }

    /// Whether a node of the kind has children.
    pub fn has_children(&self) -> bool {
        matches!(self, Self::Container)
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
        });
        let json = serde_json::to_string(&tree).unwrap();
        assert_eq!(serde_json::from_str::<Node>(&json).unwrap(), tree);
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
        let events = [
            UiEventKind::DoubleClick,
            UiEventKind::DragMove { x: 12.0, y: -1.5 },
            UiEventKind::SetValue {
                value: "main".to_owned(),
            },
        ];
        let json = serde_json::to_string(&events).unwrap();
        assert_eq!(
            json,
            r#"[{"kind":"double-click"},{"kind":"drag-move","x":12.0,"y":-1.5},{"kind":"set-value","value":"main"}]"#
        );
        assert_eq!(
            serde_json::from_str::<Vec<UiEventKind>>(&json).unwrap(),
            events
        );
    }
}
