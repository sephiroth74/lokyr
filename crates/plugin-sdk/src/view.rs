//! The plugins' UI as data: a view tree of nodes, which a plugin renders
//! and the host lays out, draws and keeps, plain data that crosses the
//! plugins' boundary alike from the main thread, a worker or a Luau VM.
//!
//! A node is a container of others, a text, an icon or a spacer, with the
//! properties of its layout; the widgets come as their own kinds. A node
//! keeps its place from one render to the next by its key, unique among
//! its siblings, or, without one, by its order among its siblings of its
//! kind; a node that holds state, as a widget whose state the host keeps,
//! needs its key.

use serde::{Deserialize, Serialize};

/// A node's key, unique among its siblings: what keeps the node, and the
/// state that the host keeps of it, from one render to the next.
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
    pub layout: Layout,
    /// Its children, a container's alone.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_crosses_the_boundary_as_it_is() {
        let tree = Node::container(vec![
            Node::icon("terminal").key("icon"),
            Node::text("~/Projects"),
            Node::spacer(),
        ])
        .layout(Layout {
            direction: Direction::Row,
            gap: 4.0,
            padding: Edges::all(2.0),
            ..Layout::default()
        });
        let json = serde_json::to_string(&tree).unwrap();
        assert_eq!(serde_json::from_str::<Node>(&json).unwrap(), tree);
        // A layout's properties left out take their defaults.
        let node: Node = serde_json::from_str(
            r#"{"key":null,"kind":"spacer","layout":{"grow":1.0},"children":[]}"#,
        )
        .unwrap();
        assert_eq!(node, Node::spacer());
    }
}
