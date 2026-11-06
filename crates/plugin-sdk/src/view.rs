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

/// Drops `node`'s tree a level at a time, without recursion, as a tree
/// too deep for a recursive drop.
pub fn dismantle(node: Node) {
    let mut left = vec![node];
    while let Some(mut node) = left.pop() {
        left.append(&mut node.children);
        if let Kind::List(list) = &mut node.kind {
            left.push(std::mem::replace(&mut *list.row, Node::new(Kind::Spacer)));
        }
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
    /// What accessibility, as VoiceOver, calls it, in place of its own
    /// text, a button's or a toggle's label, or its tooltip: what a button
    /// of an icon alone gives, or a container that is a group of meaning.
    #[serde(default)]
    pub label: Option<String>,
    /// What accessibility takes it for, in place of what its kind is: a
    /// container a group, a toolbar, a status or an alert, a text a
    /// heading, an icon an image.
    #[serde(default)]
    pub role: Option<Role>,
    /// Its children, a container's, a scroll area's, and a view of tabs',
    /// its page, alone.
    #[serde(default)]
    pub children: Vec<Node>,
}

/// What accessibility takes a node for, in place of what its kind is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// A group of nodes of one meaning, of its label, as a find bar's
    /// options.
    Group,
    /// A row of buttons and toggles.
    Toolbar,
    /// What tells a state that changes, as a count of matches, which a
    /// client reads as it changes.
    Status,
    /// What tells of something that needs the user's attention, which a
    /// client reads at once.
    Alert,
    /// A text that heads what follows it.
    Heading,
    /// An icon of a meaning, of its label; without one, an icon is left out
    /// as a decoration.
    Image,
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
    /// A view of several pages, as a settings panel's, of a tab each: a
    /// control of its tabs' labels over the box of its page, its child, the
    /// tab chosen's, as macOS's tab views draw them; a click on a tab, or
    /// an arrow while it has the focus, chooses one, which its plugin
    /// learns of by `select`, and gives in its next render with its page.
    Tabs(Tabs),
}

/// A view of tabs' own properties.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Tabs {
    /// Its tabs, in their order.
    pub tabs: Vec<Tab>,
    /// The tab chosen, by its index, whose page is its child.
    pub chosen: usize,
    /// How its tabs share its control's width.
    pub distribution: Distribution,
}

/// How a view of tabs' tabs share its control's width, as macOS's
/// segmented controls' distributions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Distribution {
    /// Each of its content's width, the control in the middle of the view.
    #[default]
    Fit,
    /// The control of the view's width, each tab of its content's width
    /// and of an equal share of the room left.
    Fill,
    /// The control of the view's width, the tabs of equal widths.
    FillEqually,
    /// The control of the view's width, each tab of its share of it as
    /// its content's of theirs.
    FillProportionally,
}

/// A tab of a view of tabs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Tab {
    /// Its label.
    pub label: String,
    /// Its icon of the host's, by its name, if it has one, before its
    /// label.
    pub icon: String,
    /// Whether it is dimmed, and no click or key chooses it.
    pub disabled: bool,
}

impl Tab {
    /// A tab of `label`.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }
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
    /// The height of every row, in points, a point at least; without it,
    /// each row takes its own, which the host measures as it shows, a point
    /// at least.
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
            Self::Field(_)
            | Self::Button(_)
            | Self::Toggle(_)
            | Self::List(_)
            | Self::Scroll
            | Self::Tabs(_) => true,
        }
    }

    /// Whether a node of the kind takes the keyboard's focus of its own, as
    /// a field, a list, a button, a toggle and a view of tabs do, unless
    /// they are disabled, a view of tabs of every tab, whatever its node
    /// says.
    pub fn takes_focus(&self) -> bool {
        match self {
            Self::Field(_) | Self::List(_) => true,
            Self::Button(button) => !button.disabled,
            Self::Toggle(toggle) => !toggle.disabled,
            Self::Tabs(tabs) => tabs.tabs.iter().any(|tab| !tab.disabled),
            _ => false,
        }
    }

    /// Whether a node of the kind is a control, as macOS has them, a button,
    /// a toggle or a view of tabs: a click gives it no focus, which stays
    /// where it was, and Tab reaches it only as the system's keyboard
    /// navigation reaches every control.
    pub fn is_control(&self) -> bool {
        matches!(self, Self::Button(_) | Self::Toggle(_) | Self::Tabs(_))
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
        matches!(self, Self::Container | Self::Scroll | Self::Tabs(_))
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
    /// changes its opacity, and its hover's, as the pointer goes over it
    /// and away: over a time, of a curve; `None` at once.
    pub transition: Option<Transition>,
    /// What a node of a key takes while the pointer is over it, its
    /// children's boxes too, as the hover's events have it, which the host
    /// applies as the pointer moves, with no render, fading it as its
    /// `transition` says, its colors drawn over the style's own, as the
    /// theme's veils are; `None` for nothing.
    pub hover: Option<Hover>,
    /// What it takes while the pointer is over its nearest node of a key
    /// around it, as a tab's close button shows under the pointer over its
    /// tab, fading as that node's transition says; `None` for nothing.
    pub parent_hover: Option<Hover>,
    /// What a node of a key takes while the primary button that went down
    /// over it is down and the pointer is still over it, as a button's
    /// press shows, over its hover, which the host applies with no render,
    /// fading it as its `transition` says; `None` for nothing.
    pub pressed: Option<Hover>,
}

/// What a node takes under the pointer, hovered or pressed, in place of
/// its style's: a part left `None` stays the style's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Hover {
    /// What fills its box.
    pub background: Option<Token>,
    /// The color of its text or icon, and of its children's that name
    /// none.
    pub color: Option<Token>,
    /// Its border, inside its edges.
    pub border: Option<Border>,
    /// How opaque it draws, with its children, from 0 to 1.
    pub opacity: Option<f32>,
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
        /// The text's revision, one more at each of its user's edits,
        /// which a request that sets the field's text of this one gives
        /// back, so that the host sets it over no keys typed since.
        #[serde(default)]
        revision: u64,
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
    /// A list's selection moved to an item, by a click or the keys; or a
    /// view of tabs' tab chosen, by a click or the arrows, which its next
    /// render gives.
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
    /// The height of the window's titlebar over its content's top, which a
    /// slot's view that runs under it leaves to the window's buttons, as
    /// the host gives it; 0 without one, as in full screen, and in a
    /// panel.
    Titlebar,
    /// The width of the room at the titlebar's left that the window's
    /// buttons take, as the host gives it; 0 without them, as in full
    /// screen, and in a panel.
    Buttons,
}

impl Node {
    /// The most levels that a view tree nests, its root the first and a
    /// list's row its own, which the host takes: a deeper one it refuses,
    /// as it walks the trees recursively, on the main thread.
    pub const DEEPEST: usize = 64;

    /// Whether the tree nests [`Node::DEEPEST`] levels at most, which it
    /// tells without recursion, of a tree of any depth.
    pub fn nests_within_deepest(&self) -> bool {
        let mut left = vec![(self, 1)];
        while let Some((node, depth)) = left.pop() {
            if depth > Self::DEEPEST {
                return false;
            }
            left.extend(node.children.iter().map(|child| (child, depth + 1)));
            if let Kind::List(list) = &node.kind {
                left.push((&list.row, depth + 1));
            }
        }
        true
    }

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
            label: None,
            role: None,
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

    /// A view of `tabs`, of the tab `chosen`, whose page is `page`; its key,
    /// which it needs, follows.
    pub fn tabs(tabs: Vec<Tab>, chosen: usize, page: Node) -> Self {
        Self {
            children: vec![page],
            ..Self::new(Kind::Tabs(Tabs {
                tabs,
                chosen,
                distribution: Distribution::Fit,
            }))
        }
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

    /// The node of what accessibility calls it.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The node of what accessibility takes it for.
    #[must_use]
    pub fn role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chain of containers `depth` levels deep, a text at its end, made
    /// without recursion.
    fn chain(depth: usize) -> Node {
        (1..depth).fold(Node::text("leaf"), |inner, _| Node::container(vec![inner]))
    }

    #[test]
    fn a_tree_s_depth_is_told_and_a_deep_one_dropped_without_recursion() {
        assert!(chain(Node::DEEPEST).nests_within_deepest());
        assert!(!chain(Node::DEEPEST + 1).nests_within_deepest());
        // A list's row is a level of its own.
        let list = |row: Node| Node::list(row, 0, (0, Vec::new()));
        assert!(!list(chain(Node::DEEPEST)).nests_within_deepest());
        assert!(list(chain(Node::DEEPEST - 1)).nests_within_deepest());
        // A million levels, told and dropped of no recursion.
        let deep = chain(1_000_000);
        assert!(!deep.nests_within_deepest());
        dismantle(deep);
    }

    #[test]
    fn a_tree_crosses_the_boundary_as_it_is() {
        let tree = Node::container(vec![
            Node::icon("terminal")
                .key("icon")
                .focusable()
                .label("Terminal")
                .role(Role::Image),
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
            hover: Some(Hover {
                background: Some(Token::from("tab.hover.bg")),
                opacity: Some(1.0),
                ..Hover::default()
            }),
            parent_hover: Some(Hover {
                border: Some(Border {
                    color: Token::from("accent"),
                    width: 2.0,
                }),
                ..Hover::default()
            }),
            pressed: Some(Hover {
                color: Some(Token::from("tab.active.fg")),
                ..Hover::default()
            }),
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
        // A view of tabs, of its page.
        let tabs: Node = serde_json::from_str(concat!(
            r#"{"key":"settings","kind":{"tabs":{"tabs":[{"label":"General"},"#,
            r#"{"label":"Keys","icon":"terminal","disabled":true}],"chosen":0}},"#,
            r#""children":[{"key":null,"kind":{"text":"page"}}]}"#
        ))
        .unwrap();
        let expected = Node::tabs(
            vec![
                Tab::new("General"),
                Tab {
                    icon: "terminal".to_owned(),
                    disabled: true,
                    ..Tab::new("Keys")
                },
            ],
            0,
            Node::text("page"),
        )
        .key("settings");
        assert_eq!(tabs, expected);
        assert!(tabs.kind.holds_state() && tabs.kind.is_control() && tabs.kind.has_children());
        assert!(tabs.kind.takes_focus());
        let all_disabled = Kind::Tabs(Tabs {
            tabs: vec![Tab {
                disabled: true,
                ..Tab::new("Off")
            }],
            chosen: 0,
            distribution: Distribution::Fit,
        });
        let filled: Tabs =
            serde_json::from_str(r#"{"tabs":[],"distribution":"fill-proportionally"}"#).unwrap();
        assert_eq!(filled.distribution, Distribution::FillProportionally);
        assert!(!all_disabled.takes_focus());
        let events = [
            UiEventKind::DoubleClick,
            UiEventKind::DragMove { x: 12.0, y: -1.5 },
            UiEventKind::SetValue {
                value: "main".to_owned(),
            },
            UiEventKind::InputChanged {
                text: "ma".to_owned(),
                revision: 2,
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
                r#"{"kind":"set-value","value":"main"},{"kind":"input-changed","text":"ma","revision":2},"#,
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
