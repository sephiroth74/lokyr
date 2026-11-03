//! The display list of a laid-out view tree: each node's box, of its
//! style's background, border, corners and shadow, whose colors the theme's
//! tokens give; its text's or its icon's glyphs, in the interface's face, in
//! its color or the nearest ancestor's; and the clips of the nodes that
//! clip their children; in the tree's order, a node over its parent and
//! over its siblings before it; and the ring of the keyboard's focus around
//! the node that has it, over them all, within its ancestors' clips.

use lokyr_plugin_sdk::view::{self, Kind, Style};
use lokyr_render_core::{Color, Item, PlacedGlyph, Rect, Shadow, Shape};
use lokyr_text_core::FontBackend;

use crate::field::{self, FieldState, Fields};
use crate::font::Font;
use crate::layout::{Frame, Layout, Measure};
use crate::list::Lists;
use crate::theme::Tokens;
use crate::tree::{NodeId, Tree};
use crate::widget;

/// The width of the ring of the keyboard's focus, outside its node's box,
/// in points.
const RING: f32 = 3.0;

/// The top and the height of a clip across alone, a text's: down the whole
/// of any drawable, which the clips around it cut.
const ACROSS: (i32, u32) = (-(1 << 28), 1 << 29);

/// What a tree shows beside its nodes: its appearance, the node with the
/// keyboard's focus, its fields' text, caret and selection, and the nodes
/// that the pointer is over and presses, as its buttons show them.
#[derive(Clone, Copy, Debug)]
pub struct Shown<'a> {
    /// Whether the appearance is dark.
    pub dark: bool,
    /// The node with the keyboard's focus, if the tree has it.
    pub focused: Option<NodeId>,
    /// The tree's fields.
    pub fields: &'a Fields,
    /// The innermost node of a key that the pointer is over, if one.
    pub hovered: Option<NodeId>,
    /// The node that the primary button went down over, while the pointer
    /// is still over it, if one.
    pub pressed: Option<NodeId>,
    /// The tree's lists, their rows that show laid out.
    pub lists: &'a Lists,
}

/// The display list of `tree`, laid out in `layout`, from `origin` in the
/// window's pixels: its colors of `tokens`, its texts and icons of `font`,
/// at the layout's scale, as `shown` has it: of the dark appearance or the
/// light one, the ring of the focus around its node, if it is the tree's,
/// and its fields' text, caret and selection.
pub fn paint<B: FontBackend>(
    tree: &Tree,
    layout: &Layout,
    font: &mut Font<B>,
    tokens: &Tokens,
    origin: (f32, f32),
    shown: Shown<'_>,
) -> Vec<Item> {
    let text = tokens.get("text").unwrap_or(Color::BLACK);
    paint_of(tree, layout, font, tokens, (origin, text), shown)
}

/// [`paint`], of `inherited` for the texts of the root that names no
/// color, as a list's row selected has the selection's.
fn paint_of<B: FontBackend>(
    tree: &Tree,
    layout: &Layout,
    font: &mut Font<B>,
    tokens: &Tokens,
    (origin, inherited): ((f32, f32), Color),
    shown: Shown<'_>,
) -> Vec<Item> {
    let mut painter = Painter {
        tree,
        layout,
        font,
        tokens,
        origin,
        dark: shown.dark,
        focused: shown.focused,
        fields: shown.fields,
        hovered: shown.hovered,
        pressed: shown.pressed,
        lists: shown.lists,
        clip: None,
        ring: None,
        items: Vec::new(),
    };
    if let Some(root) = tree.root() {
        painter.node(root, inherited);
    }
    let mut items = painter.items;
    match painter.ring {
        Some((ring, Some(clip))) => {
            items.extend([
                Item::PushClip(clip),
                Item::Shapes(vec![ring]),
                Item::PopClip,
            ]);
        }
        Some((ring, None)) => match items.last_mut() {
            Some(Item::Shapes(shapes)) => shapes.push(ring),
            _ => items.push(Item::Shapes(vec![ring])),
        },
        None => {}
    }
    items
}

/// The room inside a tooltip's edges, in points: above and below its
/// text, and before and after it.
const TOOLTIP_PADDING: (f32, f32) = (3.0, 7.0);

/// A scroll's indicator, in points: its least height, its width, and its
/// room from its box's right edge.
const INDICATOR: (f32, f32, f32) = (24.0, 4.0, 2.0);

/// How far below the pointer's place a tooltip goes, in points, past the
/// arrow's tip.
const TOOLTIP_BELOW: f32 = 20.0;

/// The room that a tooltip keeps from its window's edges, in points.
const TOOLTIP_MARGIN: f32 = 4.0;

/// The display list of a tooltip of `text`, as the pointer was at `at`, in
/// the window's pixels, of `font` at `scale` and `tokens`' colors: a box of
/// a border, rounded corners and a shadow, below the pointer, or above it
/// where the window's `room`, its width and height in pixels, has none
/// below, and within its edges.
pub fn tooltip<B: FontBackend>(
    font: &mut Font<B>,
    tokens: &Tokens,
    text: &str,
    (at, room): ((f32, f32), (f32, f32)),
    (scale, dark): (f32, bool),
) -> Vec<Item> {
    let (down, across) = TOOLTIP_PADDING;
    let natural = Measure::text(&mut *font, text).0;
    let face = font.interface();
    let (line, ascent) = (face.line(), face.ascent());
    let width = ((natural + 2.0 * across) * scale).ceil();
    let height = ((line + 2.0 * down) * scale).ceil();
    let margin = TOOLTIP_MARGIN * scale;
    let x = at.0.min(room.0 - width - margin).max(margin).round();
    let below = at.1 + TOOLTIP_BELOW * scale;
    let y = if below + height + margin <= room.1 {
        below
    } else {
        at.1 - height - margin
    }
    .max(margin)
    .round();
    let color = |token: &str, fallback| tokens.get(token).unwrap_or(fallback);
    let shape = Shape {
        rect: Rect {
            x: x as i32,
            y: y as i32,
            width: width as u32,
            height: height as u32,
        },
        radius: 4.0 * scale,
        fill: color("tooltip.bg", Color::from_rgb8(255, 255, 255)),
        border: scale.round().max(1.0),
        border_color: color("tooltip.border", Color::CLEAR),
        shadow: tokens.get("shadow").map(|shadow| Shadow {
            x: 0.0,
            y: scale,
            blur: 3.0 * scale,
            color: shadow,
        }),
    };
    let baseline = y + (down * scale).round() + (ascent * scale).round();
    let (glyphs, _) = font.interface_text(
        text,
        (x + across * scale, baseline),
        color("tooltip.fg", Color::BLACK),
        dark,
    );
    let mut items = vec![Item::Shapes(vec![shape])];
    if !glyphs.is_empty() {
        items.push(Item::Glyphs(glyphs));
    }
    items
}

struct Painter<'a, B> {
    tree: &'a Tree,
    layout: &'a Layout,
    font: &'a mut Font<B>,
    tokens: &'a Tokens,
    origin: (f32, f32),
    dark: bool,
    focused: Option<NodeId>,
    fields: &'a Fields,
    hovered: Option<NodeId>,
    pressed: Option<NodeId>,
    lists: &'a Lists,
    /// The clips of the node painted, as one: `None` for none.
    clip: Option<Rect>,
    /// The ring of the focus, and the clips of its node.
    ring: Option<(Shape, Option<Rect>)>,
    items: Vec<Item>,
}

impl<B: FontBackend> Painter<'_, B> {
    /// Paints `id` and its children, of `inherited` for their text where
    /// they name no color.
    fn node(&mut self, id: NodeId, inherited: Color) {
        let tree = self.tree;
        let (Some(frame), Some(kind), Some(style)) =
            (self.layout.frame(tree, id), tree.kind(id), tree.style(id))
        else {
            return;
        };
        let frame = Frame {
            x: frame.x + self.origin.0,
            y: frame.y + self.origin.1,
            ..frame
        };
        let color = style
            .color
            .as_ref()
            .and_then(|token| self.tokens.get(&token.0))
            .unwrap_or(inherited);
        // A field, a button, a toggle and a separator of no box of their
        // own take theirs, a button's and a toggle's of their state.
        let boxed;
        let own = style.background.is_none() && style.border.is_none();
        let style = match kind {
            Kind::Field(_) if own => {
                boxed = field_box(style);
                &boxed
            }
            Kind::Button(_) | Kind::Toggle(_) if own => {
                let state = (self.hovered == Some(id), self.pressed == Some(id));
                boxed = widget_box(kind, style, state);
                &boxed
            }
            Kind::Separator if own => {
                boxed = Style {
                    background: Some(view::Token::from("separator")),
                    ..style.clone()
                };
                &boxed
            }
            _ => style,
        };
        if self.focused == Some(id) {
            self.ring = self.ring(frame, style).map(|ring| (ring, self.clip));
        }
        if let Some(shape) = self.shape(frame, style) {
            self.push_shape(shape);
        }
        match kind {
            Kind::Text(text) => {
                let natural = self.layout.measured(id).map(|(width, _)| width);
                self.text(text, frame, natural, color);
            }
            Kind::Icon(name) => self.icon(name, frame, color),
            Kind::Field(field) => self.field(id, field, (frame, style), color),
            Kind::Button(view::Button { label, icon, .. })
            | Kind::Toggle(view::Toggle { label, icon, .. }) => {
                let color = match kind {
                    _ if kind.disabled() => self.tokens.get("text.disabled").unwrap_or(color),
                    Kind::Toggle(toggle) if toggle.on => {
                        self.tokens.get("toggle.on.fg").unwrap_or(color)
                    }
                    _ => color,
                };
                self.widget((label, icon), frame, color);
            }
            Kind::List(list) => self.list(id, list, (frame, style), color),
            Kind::Container | Kind::Spacer | Kind::Separator | Kind::Scroll => {}
        }
        let children = tree.children(id);
        if children.is_empty() {
            return;
        }
        // A scroll area clips its children, which its scroll moves.
        let clips = style.clip || matches!(kind, Kind::Scroll);
        let outer = self.clip;
        if clips {
            let inside = inside_border(rect(frame), style, self.layout.scale());
            self.items.push(Item::PushClip(inside));
            self.clip = Some(outer.map_or(inside, |outer| intersection(outer, inside)));
        }
        for &child in children {
            self.node(child, color);
        }
        if matches!(kind, Kind::Scroll) {
            let scale = self.layout.scale();
            let room = self.layout.scroll_room(tree, id) / scale;
            self.indicator(frame, (self.layout.scrolled(id) / scale, room));
        }
        if clips {
            self.items.push(Item::PopClip);
            self.clip = outer;
        }
    }

    /// The list `id`'s rows that show, of `list`, within its box at
    /// `frame`, of `style`, inside its border: each of the selection's
    /// background where it is selected, of the accent's while the list has
    /// the keyboard's focus, its texts in `color`, or the selection's; and
    /// its scroll's indicator.
    fn list(
        &mut self,
        id: NodeId,
        list: &view::List,
        (frame, style): (Frame, &Style),
        color: Color,
    ) {
        let Some(state) = self.tree.key(id).and_then(|key| self.lists.state(key)) else {
            return;
        };
        let scale = self.layout.scale();
        let inside = inside_border(rect(frame), style, scale);
        self.items.push(Item::PushClip(inside));
        let focused = self.focused == Some(id);
        let (selection, selected_color) = if focused {
            (
                self.tokens.get("list.selection.bg"),
                self.tokens.get("list.selection.fg").unwrap_or(color),
            )
        } else {
            (self.tokens.get("list.selection.inactive.bg"), color)
        };
        let fields = Fields::new();
        let lists = Lists::new();
        for (index, top) in state.visible(list) {
            let y = frame.y + top * scale;
            let selected = state.selected() == Some(index);
            if selected && let Some(selection) = selection {
                let height = state.height_of(list, index) * scale;
                let band = Rect {
                    x: frame.x.round() as i32,
                    y: y.round() as i32,
                    width: frame.width.round() as u32,
                    height: ((y + height).round() - y.round()).max(0.0) as u32,
                };
                self.push_shape(Shape::filled(band, selection));
            }
            let Some(row) = state.row(index) else {
                continue;
            };
            let inherited = if selected { selected_color } else { color };
            let items = paint_of(
                row.tree(),
                row.layout(),
                &mut *self.font,
                self.tokens,
                ((frame.x, y), inherited),
                Shown {
                    dark: self.dark,
                    focused: None,
                    fields: &fields,
                    hovered: None,
                    pressed: None,
                    lists: &lists,
                },
            );
            self.items.extend(items);
        }
        self.indicator(frame, state.extent(list));
        self.items.push(Item::PopClip);
    }

    /// The indicator of a scroll within `frame`, scrolled down `scrolled`
    /// points of `room`, at its right edge, as macOS's overlay scroll bars
    /// show it: none of a content that fits.
    fn indicator(&mut self, frame: Frame, (scrolled, room): (f32, f32)) {
        let Some(color) = self.tokens.get("scrollbar") else {
            return;
        };
        if room <= 0.0 {
            return;
        }
        let scale = self.layout.scale();
        let view = frame.height;
        let total = view + room * scale;
        let height = (view * view / total).max(INDICATOR.0 * scale).min(view);
        let top = frame.y + (view - height) * (scrolled / room).clamp(0.0, 1.0);
        let (width, inset) = (INDICATOR.1 * scale, INDICATOR.2 * scale);
        self.push_shape(Shape {
            radius: width / 2.0,
            ..Shape::filled(
                Rect {
                    x: (frame.x + frame.width - width - inset).round() as i32,
                    y: top.round() as i32,
                    width: width.round() as u32,
                    height: height.round() as u32,
                },
                color,
            )
        });
    }

    /// The ring of the focus around a node's box at `frame`, of `style`:
    /// outside its edges, its corners round of the box's radius and its
    /// own width; `None` if the theme lacks its color.
    fn ring(&self, frame: Frame, style: &Style) -> Option<Shape> {
        let color = self.tokens.get("focus.ring")?;
        let scale = self.layout.scale();
        let width = (RING * scale).round().max(1.0);
        let inner = rect(frame);
        let outset = width as u32;
        Some(Shape {
            rect: Rect {
                x: inner.x.saturating_sub_unsigned(outset),
                y: inner.y.saturating_sub_unsigned(outset),
                width: inner.width.saturating_add(outset.saturating_mul(2)),
                height: inner.height.saturating_add(outset.saturating_mul(2)),
            },
            radius: style.radius * scale + width,
            fill: Color::CLEAR,
            border: width,
            border_color: color,
            shadow: None,
        })
    }

    /// The shape of a node's box at `frame`, of `style`: `None` for a box
    /// that draws nothing, of no width or height, or whose tokens the theme
    /// lacks.
    fn shape(&self, frame: Frame, style: &Style) -> Option<Shape> {
        let pixels = rect(frame);
        if pixels.width == 0 || pixels.height == 0 {
            return None;
        }
        let scale = self.layout.scale();
        let color = |token: &lokyr_plugin_sdk::view::Token| self.tokens.get(&token.0);
        let fill = style.background.as_ref().and_then(color);
        let border = style
            .border
            .as_ref()
            .and_then(|border| color(&border.color));
        let shadow = style.shadow.as_ref().and_then(|shadow| {
            Some(Shadow {
                x: shadow.x * scale,
                y: shadow.y * scale,
                blur: shadow.blur * scale,
                color: color(&shadow.color)?,
            })
        });
        if fill.is_none() && border.is_none() && shadow.is_none() {
            return None;
        }
        Some(Shape {
            rect: pixels,
            radius: style.radius * scale,
            fill: fill.unwrap_or(Color::CLEAR),
            border: border.map_or(0.0, |_| border_pixels(style, scale) as f32),
            border_color: border.unwrap_or(Color::CLEAR),
            shadow,
        })
    }

    /// Adds `shape` to the shapes of the item before, if they are shapes.
    fn push_shape(&mut self, shape: Shape) {
        match self.items.last_mut() {
            Some(Item::Shapes(shapes)) => shapes.push(shape),
            _ => self.items.push(Item::Shapes(vec![shape])),
        }
    }

    /// The field `id` within its box at `frame`, of `style`, inside its
    /// border: its selection, behind its text, its text in `color` or its
    /// placeholder in the secondary text's, scrolled across as its state
    /// has it, and its caret, while it has the keyboard's focus.
    fn field(
        &mut self,
        id: NodeId,
        field: &view::Field,
        (frame, style): (Frame, &Style),
        color: Color,
    ) {
        let scale = self.layout.scale();
        let state = self.tree.key(id).and_then(|key| self.fields.state(key));
        let showing = state.map(FieldState::showing);
        let text = showing.as_ref().map_or("", |showing| showing.text.as_ref());
        let focused = self.focused == Some(id);
        let face = self.font.interface();
        let (line, ascent) = (face.line() * scale, face.ascent() * scale);
        let top = (frame.y + (frame.height - line) / 2.0).round();
        let baseline = top + ascent.round();
        let scroll = state.map_or(0.0, FieldState::scroll);
        let start = frame.x + field::PADDING.1 * scale - scroll * scale;
        self.items
            .push(Item::PushClip(inside_border(rect(frame), style, scale)));
        let places = self.font.interface_carets(text);
        let x = |byte: usize| {
            let points = places
                .iter()
                .find(|(at, _)| *at >= byte)
                .map_or(0.0, |&(_, x)| x);
            (start + points * scale).round()
        };
        let span = |left: f32, right: f32, (y, height): (f32, f32)| Rect {
            x: left as i32,
            y: y as i32,
            width: (right - left).max(0.0) as u32,
            height: height as u32,
        };
        let on_line = (top, line.round());
        let selection = showing
            .as_ref()
            .map(|showing| showing.selection.clone())
            .unwrap_or_default();
        if focused
            && !selection.is_empty()
            && let Some(selected) = self.tokens.get("field.selection")
        {
            let rect = span(x(selection.start), x(selection.end), on_line);
            self.push_shape(Shape::filled(rect, selected));
        }
        if text.is_empty() {
            let placeholder = self.tokens.get("text.secondary").unwrap_or(color);
            let (glyphs, _) = self.font.interface_text(
                &field.placeholder,
                (start, baseline),
                placeholder,
                self.dark,
            );
            self.glyphs(glyphs);
        } else {
            let (glyphs, _) = self
                .font
                .interface_text(text, (start, baseline), color, self.dark);
            self.glyphs(glyphs);
        }
        // The composition underlined, as macOS marks it, its selected part
        // of a line twice as thick.
        let thin = scale.round().max(1.0);
        if let Some((composition, selected)) = showing.as_ref().and_then(|s| s.composition.clone())
        {
            let under = baseline + thin;
            self.push_shape(Shape::filled(
                span(x(composition.start), x(composition.end), (under, thin)),
                color,
            ));
            if !selected.is_empty() {
                self.push_shape(Shape::filled(
                    span(x(selected.start), x(selected.end), (under, 2.0 * thin)),
                    color,
                ));
            }
        }
        if focused
            && selection.is_empty()
            && let Some(caret) = showing.as_ref().map_or(Some(0), |showing| showing.caret)
        {
            let caret = x(caret);
            self.push_shape(Shape::filled(span(caret, caret + thin, on_line), color));
        }
        self.items.push(Item::PopClip);
    }

    /// The glyphs of `text`, on its line in the middle of `frame`, clipped
    /// across to it where `natural`, its width as the layout measured it, in
    /// points, passes it, as a title too long for its room: not down, where
    /// an accent above the line or a descent below it keeps its ink.
    fn text(&mut self, text: &str, frame: Frame, natural: Option<f32>, color: Color) {
        let scale = self.layout.scale();
        let face = self.font.interface();
        let (line, ascent) = (face.line() * scale, face.ascent() * scale);
        let baseline = (frame.y + (frame.height - line) / 2.0 + ascent).round();
        let (glyphs, _) = self
            .font
            .interface_text(text, (frame.x, baseline), color, self.dark);
        // A pixel past the frame, as the layout's rounding of edges takes
        // up to one from a text's width.
        let past = natural.is_some_and(|width| width * scale > frame.width + 1.0);
        if past {
            self.items.push(Item::PushClip(Rect {
                y: ACROSS.0,
                height: ACROSS.1,
                ..rect(frame)
            }));
            self.items.push(Item::Glyphs(glyphs));
            self.items.push(Item::PopClip);
        } else {
            self.glyphs(glyphs);
        }
    }

    /// A button's or a toggle's content, of `label` and `icon`, either
    /// empty, side by side in the middle of `frame`, in `color`: its label
    /// clipped across where its frame is too narrow for it.
    fn widget(&mut self, (label, icon): (&str, &str), frame: Frame, color: Color) {
        let scale = self.layout.scale();
        let side = if icon.is_empty() {
            0.0
        } else {
            self.font.interface().line() * scale
        };
        let natural = if label.is_empty() {
            0.0
        } else {
            Measure::text(&mut *self.font, label).0
        };
        let gap = if label.is_empty() || icon.is_empty() {
            0.0
        } else {
            widget::GAP * scale
        };
        let (_, across) = widget::padding(label);
        let room = (frame.width - 2.0 * across * scale - side - gap).max(0.0);
        let width = (natural * scale).min(room);
        let left = (frame.x + (frame.width - (side + gap + width)) / 2.0).round();
        if !icon.is_empty() {
            let at = Frame {
                x: left,
                y: (frame.y + (frame.height - side) / 2.0).round(),
                width: side,
                height: side,
            };
            self.icon(icon, at, color);
        }
        if !label.is_empty() {
            let at = Frame {
                x: left + side + gap,
                width,
                ..frame
            };
            self.text(label, at, Some(natural), color);
        }
    }

    /// The glyph of the icon `name`, in the middle of `frame`.
    fn icon(&mut self, name: &str, frame: Frame, color: Color) {
        let side = frame.width.min(frame.height);
        let corner = (
            frame.x + (frame.width - side) / 2.0,
            frame.y + (frame.height - side) / 2.0,
        );
        if let Some(glyph) = self
            .font
            .interface_icon(name, corner, side, color, self.dark)
        {
            self.glyphs(vec![glyph]);
        }
    }

    /// Adds `glyphs` to the glyphs of the item before, if they are glyphs.
    fn glyphs(&mut self, glyphs: Vec<PlacedGlyph>) {
        if glyphs.is_empty() {
            return;
        }
        match self.items.last_mut() {
            Some(Item::Glyphs(before)) => before.extend(glyphs),
            _ => self.items.push(Item::Glyphs(glyphs)),
        }
    }
}

/// `style` of the box of a button or a toggle, `kind`, of no box of its
/// own, of rounded corners, as its state has it, the pointer over it and
/// its primary button down over it: a button's background, lighter or
/// darker of the pointer, as a toggle's of a label when it is off; a
/// toggle of an icon alone shows none without the pointer, as a find
/// bar's; a toggle's tint when it is on; a widget disabled shows no
/// pointer.
fn widget_box(kind: &Kind, style: &Style, (hovered, pressed): (bool, bool)) -> Style {
    let disabled = kind.disabled();
    let on = matches!(kind, Kind::Toggle(toggle) if toggle.on);
    let background = match kind {
        _ if pressed && !disabled => Some("button.pressed.bg"),
        _ if on => Some("toggle.on.bg"),
        _ if hovered && !disabled => Some("button.hover.bg"),
        Kind::Button(_) => Some("button.bg"),
        Kind::Toggle(toggle) if !toggle.label.is_empty() => Some("button.bg"),
        _ => None,
    };
    Style {
        background: background.map(view::Token::from),
        radius: if style.radius > 0.0 {
            style.radius
        } else {
            widget::RADIUS
        },
        ..style.clone()
    }
}

/// `style` of a field's box, as a field of no box of its own takes: the
/// field's background, and its border, of rounded corners.
fn field_box(style: &Style) -> Style {
    Style {
        background: Some(view::Token::from("field.bg")),
        border: Some(view::Border {
            color: view::Token::from("field.border"),
            width: 1.0,
        }),
        radius: if style.radius > 0.0 {
            style.radius
        } else {
            4.0
        },
        ..style.clone()
    }
}

/// The width of the border of `style`, in whole pixels at `scale`, a
/// hairline one at least; 0 without a border.
fn border_pixels(style: &Style, scale: f32) -> u32 {
    style
        .border
        .as_ref()
        .map_or(0, |border| (border.width * scale).round().max(1.0) as u32)
}

/// The pixels of `rect`, a node's box of `style`, inside its border at
/// `scale`, which its children keep within when it clips them.
pub(crate) fn inside_border(rect: Rect, style: &Style, scale: f32) -> Rect {
    let border = border_pixels(style, scale);
    Rect {
        x: rect.x.saturating_add_unsigned(border),
        y: rect.y.saturating_add_unsigned(border),
        width: rect.width.saturating_sub(border.saturating_mul(2)),
        height: rect.height.saturating_sub(border.saturating_mul(2)),
    }
}

/// Where `a` and `b` overlap: an empty rectangle where they do not.
pub(crate) fn intersection(a: Rect, b: Rect) -> Rect {
    let (left, top) = (a.x.max(b.x), a.y.max(b.y));
    let right = right(a).min(right(b));
    let bottom = bottom(a).min(bottom(b));
    Rect {
        x: left,
        y: top,
        width: u32::try_from(right - i64::from(left)).unwrap_or(0),
        height: u32::try_from(bottom - i64::from(top)).unwrap_or(0),
    }
}

/// The column past `rect`'s right edge.
pub(crate) fn right(rect: Rect) -> i64 {
    i64::from(rect.x) + i64::from(rect.width)
}

/// The row past `rect`'s bottom edge.
pub(crate) fn bottom(rect: Rect) -> i64 {
    i64::from(rect.y) + i64::from(rect.height)
}

/// The pixels of `frame`, whose edges the layout rounded.
pub(crate) fn rect(frame: Frame) -> Rect {
    Rect {
        x: frame.x.round() as i32,
        y: frame.y.round() as i32,
        width: frame.width.round().max(0.0) as u32,
        height: frame.height.round().max(0.0) as u32,
    }
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::{self, Border, Edges, Node, Token};
    use lokyr_render_core::Texture;
    use lokyr_text_core::{
        Bitmap, BitmapFormat, FaceId, FaceInfo, FaceMetrics, Features, FontConfig, GlyphId,
        Rendering, ShapedGlyph, Synthesis,
    };

    use super::*;

    use crate::field::Clipboard;
    use crate::theme::Theme;
    use lokyr_vt_core::input::{Key, KeyAction, KeyEvent, Mods};

    /// What a tree of no fields shows, of the dark appearance when `dark`,
    /// of the focus on `focused`.
    fn shown(dark: bool, focused: Option<NodeId>) -> Shown<'static> {
        static NONE: std::sync::OnceLock<Fields> = std::sync::OnceLock::new();
        Shown {
            dark,
            focused,
            fields: NONE.get_or_init(Fields::new),
            hovered: None,
            pressed: None,
            // A tree of no lists: nothing to keep, as the test ends.
            lists: Box::leak(Box::new(Lists::new())),
        }
    }

    const SYMBOLS: FaceId = FaceId(1);

    /// A font of blocks: a face whose glyph is a block, its advance 0.6 of
    /// the size, as wide as it, and its ascent 0.8, as tall; a line of the
    /// size; and a face of symbols, an em wide, whose glyphs are squares of
    /// half the size.
    struct Blocks;

    impl FontBackend for Blocks {
        fn default_family(&mut self) -> String {
            "Blocks".to_owned()
        }

        fn family(&mut self, name: &str) -> Option<Vec<FaceInfo>> {
            (name == "Blocks").then(|| {
                vec![FaceInfo {
                    id: FaceId(0),
                    style: "Regular".to_owned(),
                    weight: 400.0,
                    italic: false,
                    oblique: false,
                    width: 0.0,
                    in_group: false,
                    preferred: false,
                }]
            })
        }

        fn symbols(&mut self) -> Option<FaceId> {
            Some(SYMBOLS)
        }

        fn metrics(&mut self, face: FaceId, size: f32) -> FaceMetrics {
            FaceMetrics {
                ascent: size * 0.8,
                descent: size * 0.2,
                advance: if face == SYMBOLS { size } else { size * 0.6 },
                ..FaceMetrics::default()
            }
        }

        fn glyph(&mut self, _: FaceId, character: char) -> Option<GlyphId> {
            u16::try_from(u32::from(character)).ok().map(GlyphId)
        }

        fn fallback(&mut self, _: FaceId, _: &str) -> Option<FaceId> {
            None
        }

        fn shape(
            &mut self,
            face: FaceId,
            size: f32,
            text: &str,
            _: Features,
            glyphs: &mut Vec<ShapedGlyph>,
        ) {
            for (index, (at, character)) in text.char_indices().enumerate() {
                glyphs.push(ShapedGlyph {
                    face,
                    glyph: GlyphId(u16::try_from(u32::from(character)).unwrap_or(0)),
                    cluster: at as u32,
                    x: index as f32 * size * 0.6,
                    y: 0.0,
                });
            }
        }

        fn rasterize(
            &mut self,
            face: FaceId,
            _: GlyphId,
            size: f32,
            _: Synthesis,
            _: Rendering,
        ) -> Bitmap {
            let (width, height) = if face == SYMBOLS {
                ((size / 2.0) as u32, (size / 2.0) as u32)
            } else {
                ((size * 0.6) as u32, (size * 0.8) as u32)
            };
            Bitmap {
                format: BitmapFormat::Gray,
                width,
                height,
                left: 0,
                top: height as i32,
                data: vec![255; (width * height) as usize],
            }
        }
    }

    /// `root` laid out at `scale` in `width` points, and painted from
    /// `origin` in the dark variant's tokens.
    fn painted(
        root: &Node,
        scale: f32,
        width: f32,
        origin: (f32, f32),
    ) -> (Tree, Layout, Vec<Item>) {
        let mut font = Font::new(Blocks, FontConfig::default(), scale).unwrap();
        let mut tree = Tree::new();
        let changes = tree.update(root).unwrap();
        let mut layout = Layout::new(scale);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(width * scale), None, &mut font);
        let tokens = &Theme::builtin().dark.tokens;
        let items = paint(&tree, &layout, &mut font, tokens, origin, shown(true, None));
        (tree, layout, items)
    }

    fn token(name: &str) -> Color {
        Theme::builtin().dark.tokens.get(name).unwrap()
    }

    /// A panel of a border, rounded corners and a shadow, of a title and a
    /// button of an icon, the title in the secondary text's color.
    fn panel(title: &str) -> Node {
        Node::container(vec![
            Node::text(title).style(view::Style {
                color: Some(Token::from("text.secondary")),
                ..view::Style::default()
            }),
            Node::container(vec![Node::icon("close")]).style(view::Style {
                background: Some(Token::from("button.bg")),
                radius: 4.0,
                ..view::Style::default()
            }),
        ])
        .layout(view::Layout {
            padding: Edges::all(8.0),
            gap: 6.0,
            ..view::Layout::default()
        })
        .style(view::Style {
            background: Some(Token::from("panel.bg")),
            border: Some(Border {
                color: Token::from("panel.border"),
                width: 1.0,
            }),
            radius: 8.0,
            shadow: Some(view::Shadow::default()),
            ..view::Style::default()
        })
    }

    #[test]
    fn a_panel_paints_its_boxes_then_its_glyphs_at_their_places() {
        // Points of 13, a line of 13: a title of 3 characters 23.4 wide.
        let (_, _, items) = painted(&panel("abc"), 1.0, 200.0, (10.0, 20.0));
        let [
            Item::Shapes(boxes),
            Item::Glyphs(title),
            Item::Shapes(button),
            Item::Glyphs(icon),
        ] = &items[..]
        else {
            panic!("{items:#?}");
        };
        let [panel] = &boxes[..] else { panic!() };
        // 200 wide, the line and the padding high, from the origin.
        assert_eq!(
            panel.rect,
            Rect {
                x: 10,
                y: 20,
                width: 200,
                height: 29
            }
        );
        assert_eq!((panel.radius, panel.border), (8.0, 1.0));
        assert_eq!(panel.fill, token("panel.bg"));
        assert_eq!(panel.border_color, token("panel.border"));
        let shadow = panel.shadow.unwrap();
        assert_eq!((shadow.x, shadow.y, shadow.blur), (0.0, 2.0, 8.0));
        assert_eq!(shadow.color, token("shadow"));
        // Three blocks, from the padding, 7.8 apart, at their whole pixels
        // and the thirds past them, on the baseline 0.8 of the line down,
        // in the secondary text's color.
        assert_eq!(title.len(), 3);
        assert_eq!(
            title
                .iter()
                .map(|glyph| (glyph.x, glyph.y))
                .collect::<Vec<_>>(),
            [(18, 28), (25, 28), (33, 28)]
        );
        assert!(
            title
                .iter()
                .all(|glyph| glyph.color == token("text.secondary"))
        );
        assert!(
            title
                .iter()
                .all(|glyph| glyph.region.texture == Texture::Coverage)
        );
        // The button after the title and the gap, 47.4 rounded, a line
        // square; its icon in its middle, in the text's color, of the
        // parent's.
        let [button] = &button[..] else { panic!() };
        assert_eq!(
            button.rect,
            Rect {
                x: 47,
                y: 28,
                width: 13,
                height: 13
            }
        );
        assert_eq!((button.radius, button.shadow), (4.0, None));
        let [icon] = &icon[..] else { panic!() };
        // A square of half the size of the em that is the icon's ink, three
        // quarters of 13 pixels.
        assert_eq!((icon.region.width, icon.region.height), (4, 4));
        assert_eq!((icon.x, icon.y), (52, 33));
        assert_eq!(icon.color, token("text"));
    }

    #[test]
    fn twice_the_scale_paints_twice_the_pixels() {
        let (_, _, ones) = painted(&panel("abc"), 1.0, 200.0, (0.0, 0.0));
        let (_, _, twos) = painted(&panel("abc"), 2.0, 200.0, (0.0, 0.0));
        let shapes = |items: &[Item]| -> Vec<Shape> {
            items
                .iter()
                .filter_map(|item| match item {
                    Item::Shapes(shapes) => Some(shapes.clone()),
                    _ => None,
                })
                .flatten()
                .collect()
        };
        // Twice, within the pixel that a place of a fraction rounds to: the
        // button after a title of 37.4 points, 74.8 pixels.
        for (one, two) in shapes(&ones).iter().zip(shapes(&twos)) {
            let (a, b) = (one.rect, two.rect);
            assert!(
                b.x.abs_diff(a.x * 2) <= 1 && b.y.abs_diff(a.y * 2) <= 1,
                "{a:?} {b:?}"
            );
            assert!(b.width.abs_diff(a.width * 2) <= 1, "{a:?} {b:?}");
            assert!(b.height.abs_diff(a.height * 2) <= 1, "{a:?} {b:?}");
            assert_eq!(two.radius, one.radius * 2.0);
            assert_eq!(two.border, one.border * 2.0);
        }
        let [_, Item::Glyphs(one), ..] = &ones[..] else {
            panic!()
        };
        let [_, Item::Glyphs(two), ..] = &twos[..] else {
            panic!()
        };
        // The blocks of 26 pixels, 15.6 apart: their whole pixels, the
        // thirds past them drawn into their glyphs.
        assert_eq!(
            two.iter().map(|glyph| glyph.x).collect::<Vec<_>>(),
            [16, 31, 47]
        );
        assert_eq!(two[0].region.width, 2 * one[0].region.width + 1);
    }

    #[test]
    fn a_text_past_its_box_and_a_box_that_clips_keep_within_it() {
        let clipped = Node::container(vec![
            Node::text("a title too long").layout(view::Layout {
                width: view::Length::Points(40.0),
                ..view::Layout::default()
            }),
            Node::container(vec![Node::text("x")]).style(view::Style {
                border: Some(Border {
                    color: Token::from("separator"),
                    width: 1.0,
                }),
                clip: true,
                ..view::Style::default()
            }),
        ]);
        let (_, _, items) = painted(&clipped, 1.0, 200.0, (0.0, 0.0));
        let [
            Item::PushClip(text),
            Item::Glyphs(glyphs),
            Item::PopClip,
            Item::Shapes(_),
            Item::PushClip(box_clip),
            Item::Glyphs(_),
            Item::PopClip,
        ] = &items[..]
        else {
            panic!("{items:#?}");
        };
        // Across alone, as an accent above the line keeps its ink.
        assert_eq!(
            *text,
            Rect {
                x: 0,
                y: ACROSS.0,
                width: 40,
                height: ACROSS.1
            }
        );
        assert_eq!(glyphs.len(), 16);
        // The box of the text of 7.8, 8 rounded, inside its border.
        assert_eq!(
            *box_clip,
            Rect {
                x: 41,
                y: 1,
                width: 6,
                height: 11
            }
        );
    }

    #[test]
    fn the_focus_s_ring_goes_round_its_node_over_the_others_within_its_clips() {
        // A panel that clips, inside its border of 1, a field of a radius
        // of 4 at 8 points from its corner, of 40 by 20, and a box after it
        // that covers its ring's right.
        let field = Node::container(vec![])
            .key("field")
            .focusable()
            .layout(view::Layout {
                width: view::Length::Points(40.0),
                height: view::Length::Points(20.0),
                ..view::Layout::default()
            })
            .style(view::Style {
                radius: 4.0,
                ..view::Style::default()
            });
        let after = Node::container(vec![])
            .layout(view::Layout {
                width: view::Length::Points(20.0),
                height: view::Length::Points(20.0),
                ..view::Layout::default()
            })
            .style(view::Style {
                background: Some(Token::from("button.bg")),
                ..view::Style::default()
            });
        let root = Node::container(vec![field, after])
            .layout(view::Layout {
                padding: Edges::all(8.0),
                ..view::Layout::default()
            })
            .style(view::Style {
                border: Some(Border {
                    color: Token::from("panel.border"),
                    width: 1.0,
                }),
                clip: true,
                ..view::Style::default()
            });
        for scale in [1.0, 2.0] {
            let mut font = Font::new(Blocks, FontConfig::default(), scale).unwrap();
            let mut tree = Tree::new();
            let changes = tree.update(&root).unwrap();
            let mut layout = Layout::new(scale);
            layout.update(&tree, &changes);
            layout.compute(&tree, Some(100.0 * scale), None, &mut font);
            let field = tree.children(tree.root().unwrap())[0];
            let tokens = &Theme::builtin().dark.tokens;
            let s = scale as i32;
            let focused = paint(
                &tree,
                &layout,
                &mut font,
                tokens,
                (10.0, 0.0),
                shown(true, Some(field)),
            );
            let [
                Item::Shapes(_),
                Item::PushClip(_),
                Item::Shapes(_),
                Item::PopClip,
                Item::PushClip(clip),
                Item::Shapes(ring),
                Item::PopClip,
            ] = &focused[..]
            else {
                panic!("{focused:#?}");
            };
            // The panel's clip, inside its border.
            assert_eq!(
                *clip,
                Rect {
                    x: 10 + s,
                    y: s,
                    width: (100 * s - 2 * s) as u32,
                    height: (36 * s - 2 * s) as u32,
                }
            );
            assert_eq!(
                ring[..],
                [Shape {
                    rect: Rect {
                        x: 10 + 5 * s,
                        y: 5 * s,
                        width: (46 * s) as u32,
                        height: (26 * s) as u32,
                    },
                    radius: 7.0 * scale,
                    fill: Color::CLEAR,
                    border: 3.0 * scale,
                    border_color: token("focus.ring"),
                    shadow: None,
                }]
            );
            // Without the focus, or of a node that the tree lacks, no ring.
            let unfocused = paint(
                &tree,
                &layout,
                &mut font,
                tokens,
                (10.0, 0.0),
                shown(true, None),
            );
            assert_eq!(unfocused[..], focused[..4]);
            let mut other = Tree::new();
            other
                .update(&Node::container(vec![Node::spacer(); 4]))
                .unwrap();
            let elsewhere = *other.children(other.root().unwrap()).last().unwrap();
            let wrong = paint(
                &tree,
                &layout,
                &mut font,
                tokens,
                (10.0, 0.0),
                shown(true, Some(elsewhere)),
            );
            assert_eq!(wrong[..], focused[..4]);
        }
        // A ring of a node after one that clips, out of its clip.
        let clipper = Node::container(vec![Node::text("x")]).style(view::Style {
            clip: true,
            ..view::Style::default()
        });
        let field = Node::container(vec![]).key("field").focusable();
        let mut font = Font::new(Blocks, FontConfig::default(), 1.0).unwrap();
        let mut tree = Tree::new();
        let changes = tree.update(&Node::container(vec![clipper, field])).unwrap();
        let mut layout = Layout::new(1.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(100.0), None, &mut font);
        let field = tree.children(tree.root().unwrap())[1];
        let tokens = &Theme::builtin().dark.tokens;
        let items = paint(
            &tree,
            &layout,
            &mut font,
            tokens,
            (0.0, 0.0),
            shown(true, Some(field)),
        );
        let [.., Item::PopClip, Item::Shapes(ring)] = &items[..] else {
            panic!("{items:#?}");
        };
        assert_eq!(ring[0].border_color, token("focus.ring"));
        // A ring without a clip around it follows the shapes before it.
        let (tree, layout, _) = painted(&panel("abc"), 1.0, 200.0, (0.0, 0.0));
        let mut font = Font::new(Blocks, FontConfig::default(), 1.0).unwrap();
        let tokens = &Theme::builtin().dark.tokens;
        let root = tree.root().unwrap();
        let items = paint(
            &tree,
            &layout,
            &mut font,
            tokens,
            (0.0, 0.0),
            shown(true, Some(root)),
        );
        let Some(Item::Shapes(last)) = items.last() else {
            panic!("{items:#?}");
        };
        assert_eq!(last.last().unwrap().border_color, token("focus.ring"));
    }

    #[test]
    fn a_field_paints_its_text_its_selection_and_its_caret_inside_its_border() {
        struct Nothing;
        impl Clipboard for Nothing {
            fn read(&mut self) -> Option<String> {
                None
            }
            fn write(&mut self, _: &str) {}
        }
        // At a scale of 2, a field of 160 by 21 points, its text of 13
        // points, 7.8 a character, from its padding of 6, 12 pixels in, on
        // its line 26 high, 8 down.
        let scale = 2.0;
        let mut font = Font::new(Blocks, FontConfig::default(), scale).unwrap();
        let mut tree = Tree::new();
        let root = Node::container(vec![Node::field("Find").key("find")]);
        let changes = tree.update(&root).unwrap();
        let mut layout = Layout::new(scale);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0 * scale), None, &mut font);
        let field = tree.children(tree.root().unwrap())[0];
        assert_eq!(layout.frame(&tree, field).unwrap().height, 42.0);
        let tokens = &Theme::builtin().dark.tokens;
        let mut fields = Fields::new();
        let painted = |font: &mut Font<Blocks>, fields: &Fields, focused: Option<NodeId>| {
            paint(
                &tree,
                &layout,
                font,
                tokens,
                (0.0, 0.0),
                Shown {
                    dark: true,
                    focused,
                    fields,
                    hovered: None,
                    pressed: None,
                    lists: &Lists::new(),
                },
            )
        };
        let inside = Rect {
            x: 2,
            y: 2,
            width: 316,
            height: 38,
        };
        let line = |left: i32, right: i32| Rect {
            x: left,
            y: 8,
            width: (right - left) as u32,
            height: 26,
        };
        // Empty: its placeholder, in the secondary text's color.
        let items = painted(&mut font, &fields, None);
        let [
            Item::Shapes(field_box),
            Item::PushClip(clip),
            Item::Glyphs(placeholder),
            Item::PopClip,
        ] = &items[..]
        else {
            panic!("{items:#?}");
        };
        assert_eq!(field_box[0].fill, token("field.bg"));
        assert_eq!(field_box[0].border_color, token("field.border"));
        assert_eq!(*clip, inside);
        assert_eq!(placeholder.len(), 4);
        assert_eq!(placeholder[0].x, 12);
        assert!(
            placeholder
                .iter()
                .all(|glyph| glyph.color == token("text.secondary"))
        );
        // Focused, its caret after its text, of its color, a pixel a point
        // wide; without the focus, no caret.
        let key = view::Key::from("find");
        fields.set_text(&key, "abc");
        let items = painted(&mut font, &fields, Some(field));
        let [
            Item::Shapes(_),
            Item::PushClip(_),
            Item::Glyphs(text),
            Item::Shapes(caret),
            Item::PopClip,
            Item::Shapes(_),
        ] = &items[..]
        else {
            panic!("{items:#?}");
        };
        assert_eq!(text.len(), 3);
        assert_eq!(caret[..], [Shape::filled(line(59, 61), text[0].color)]);
        let items = painted(&mut font, &fields, None);
        assert!(matches!(
            &items[..],
            [
                Item::Shapes(_),
                Item::PushClip(_),
                Item::Glyphs(_),
                Item::PopClip
            ]
        ));
        // A selection of `bc`, behind the text, and no caret; none shown
        // without the focus.
        let shift_left = KeyEvent {
            key: Key::Left,
            action: KeyAction::Press,
            mods: Mods {
                shift: true,
                ..Mods::NONE
            },
            text: "",
            unmodified: "",
            base_layout: None,
        };
        for _ in 0..2 {
            fields.key(&tree, field, &shift_left, &mut Nothing);
        }
        let items = painted(&mut font, &fields, Some(field));
        let [
            Item::Shapes(_),
            Item::PushClip(_),
            Item::Shapes(selection),
            Item::Glyphs(_),
            Item::PopClip,
            Item::Shapes(_),
        ] = &items[..]
        else {
            panic!("{items:#?}");
        };
        assert_eq!(
            selection[..],
            [Shape::filled(line(28, 59), token("field.selection"))]
        );
        let items = painted(&mut font, &fields, None);
        assert!(matches!(
            &items[..],
            [
                Item::Shapes(_),
                Item::PushClip(_),
                Item::Glyphs(_),
                Item::PopClip
            ]
        ));
        // A text longer than its room, of 30 characters, 234 points, scrolled
        // to its caret at its end: the caret at the room's right, 148 points
        // from its left, and its first glyph 172 pixels left of its place.
        fields.set_text(&key, &"x".repeat(30));
        fields.follow(&tree, &layout, Some(field), &mut font);
        let items = painted(&mut font, &fields, Some(field));
        let [
            Item::Shapes(_),
            Item::PushClip(_),
            Item::Glyphs(text),
            Item::Shapes(caret),
            Item::PopClip,
            Item::Shapes(_),
        ] = &items[..]
        else {
            panic!("{items:#?}");
        };
        assert_eq!(text[0].x, 12 - 172);
        assert_eq!(caret[0].rect, line(308, 310));
    }

    #[test]
    fn a_composition_paints_underlined_and_its_selected_part_twice_as_thick() {
        // At a scale of 2, as the field's test: 7.8 points a character, the
        // text from 12 pixels in, its baseline at 29, a line 2 pixels thick.
        let scale = 2.0;
        let mut font = Font::new(Blocks, FontConfig::default(), scale).unwrap();
        let mut tree = Tree::new();
        let root = Node::container(vec![Node::field("Find").key("find")]);
        let changes = tree.update(&root).unwrap();
        let mut layout = Layout::new(scale);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0 * scale), None, &mut font);
        let field = tree.children(tree.root().unwrap())[0];
        let tokens = &Theme::builtin().dark.tokens;
        let mut fields = Fields::new();
        fields.set_text(&view::Key::from("find"), "ab");
        let mut painted = |fields: &Fields| {
            paint(
                &tree,
                &layout,
                &mut font,
                tokens,
                (0.0, 0.0),
                Shown {
                    dark: true,
                    focused: Some(field),
                    fields,
                    hovered: None,
                    pressed: None,
                    lists: &Lists::new(),
                },
            )
        };
        // `にほ` after `ab`, the input method's cursor after `に`: its
        // line from 43 to 74, under the baseline, and the caret at 59.
        fields.compose(&tree, field, "にほ", 3..3);
        let items = painted(&fields);
        let [
            Item::Shapes(_),
            Item::PushClip(_),
            Item::Glyphs(text),
            Item::Shapes(marks),
            Item::PopClip,
            Item::Shapes(_),
        ] = &items[..]
        else {
            panic!("{items:#?}");
        };
        let color = text[0].color;
        assert_eq!(text.len(), 4);
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        assert_eq!(
            marks[..],
            [
                Shape::filled(rect(43, 31, 31, 2), color),
                Shape::filled(rect(59, 8, 2, 26), color)
            ]
        );
        // `に` selected: its part twice as thick, and no caret.
        fields.compose(&tree, field, "にほ", 0..3);
        let items = painted(&fields);
        let [_, _, _, Item::Shapes(marks), Item::PopClip, _] = &items[..] else {
            panic!("{items:#?}");
        };
        assert_eq!(
            marks[..],
            [
                Shape::filled(rect(43, 31, 31, 2), color),
                Shape::filled(rect(43, 31, 16, 4), color)
            ]
        );
    }

    #[test]
    fn buttons_and_toggles_paint_of_their_state() {
        use lokyr_plugin_sdk::view::Kind;
        let disabled = Node::new(Kind::Button(view::Button {
            label: "All".to_owned(),
            disabled: true,
            ..view::Button::default()
        }))
        .key("all");
        let labelled = Node::new(Kind::Toggle(view::Toggle {
            label: "Wrap".to_owned(),
            ..view::Toggle::default()
        }))
        .key("wrap");
        let root = Node::container(vec![
            Node::button("Go").key("go"),
            Node::toggle("regex", false).key("off"),
            Node::toggle("regex", true).key("on"),
            disabled,
            Node::separator(),
            labelled,
        ])
        .layout(view::Layout {
            gap: 4.0,
            padding: Edges::all(4.0),
            ..view::Layout::default()
        });
        let mut font = Font::new(Blocks, FontConfig::default(), 1.0).unwrap();
        let mut tree = Tree::new();
        let changes = tree.update(&root).unwrap();
        let mut layout = Layout::new(1.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(400.0), None, &mut font);
        let node = |index: usize| tree.children(tree.root().unwrap())[index];
        let [go, off, on, all, separator, wrap] = [0, 1, 2, 3, 4, 5].map(node);
        let tokens = &Theme::builtin().dark.tokens;
        let mut painted = |hovered: Option<NodeId>, pressed: Option<NodeId>| {
            let fields = Fields::new();
            let items = paint(
                &tree,
                &layout,
                &mut font,
                tokens,
                (0.0, 0.0),
                Shown {
                    dark: true,
                    focused: None,
                    fields: &fields,
                    hovered,
                    pressed,
                    lists: &Lists::new(),
                },
            );
            let mut fills = Vec::new();
            let mut glyphs = Vec::new();
            for item in items {
                match item {
                    Item::Shapes(shapes) => {
                        fills.extend(shapes.into_iter().map(|shape| (shape.rect, shape.fill)));
                    }
                    Item::Glyphs(placed) => glyphs.extend(placed),
                    _ => {}
                }
            }
            (fills, glyphs)
        };
        let box_of = |fills: &[(Rect, Color)], id: NodeId| {
            let at = rect(layout.frame(&tree, id).unwrap());
            fills
                .iter()
                .find(|(rect, _)| *rect == at)
                .map(|&(_, fill)| fill)
        };
        let ink_of = |glyphs: &[PlacedGlyph], id: NodeId| {
            let at = rect(layout.frame(&tree, id).unwrap());
            let colors: Vec<Color> = glyphs
                .iter()
                .filter(|glyph| glyph.x >= at.x && glyph.x < at.x + at.width as i32)
                .map(|glyph| glyph.color)
                .collect();
            assert!(!colors.is_empty());
            assert!(colors.iter().all(|&color| color == colors[0]));
            colors[0]
        };
        // At rest: a button's background, none of a toggle of an icon that
        // is off, a tint of one that is on, a button's of one of a label;
        // a disabled button's background, of its dimmed label; the
        // separator's line.
        let (fills, glyphs) = painted(None, None);
        assert_eq!(box_of(&fills, go), Some(token("button.bg")));
        assert_eq!(box_of(&fills, off), None);
        assert_eq!(box_of(&fills, on), Some(token("toggle.on.bg")));
        assert_eq!(box_of(&fills, all), Some(token("button.bg")));
        assert_eq!(box_of(&fills, separator), Some(token("separator")));
        assert_eq!(box_of(&fills, wrap), Some(token("button.bg")));
        assert_eq!(ink_of(&glyphs, go), token("text"));
        assert_eq!(ink_of(&glyphs, off), token("text"));
        assert_eq!(ink_of(&glyphs, on), token("toggle.on.fg"));
        assert_eq!(ink_of(&glyphs, all), token("text.disabled"));
        // The pointer over them, and pressing them.
        let (fills, _) = painted(Some(off), None);
        assert_eq!(box_of(&fills, off), Some(token("button.hover.bg")));
        let (fills, _) = painted(Some(go), Some(go));
        assert_eq!(box_of(&fills, go), Some(token("button.pressed.bg")));
        let (fills, _) = painted(Some(on), Some(on));
        assert_eq!(box_of(&fills, on), Some(token("button.pressed.bg")));
        let (fills, _) = painted(Some(on), None);
        assert_eq!(box_of(&fills, on), Some(token("toggle.on.bg")));
        // A widget disabled shows neither.
        let (fills, _) = painted(Some(all), Some(all));
        assert_eq!(box_of(&fills, all), Some(token("button.bg")));
        // A label too long for its button's room is clipped across, within
        // its padding.
        let narrow = Node::container(vec![Node::button("Replace All").key("all").layout(
            view::Layout {
                width: view::Length::Points(40.0),
                ..view::Layout::default()
            },
        )]);
        let (tree, layout, items) = painted_tree(
            &narrow,
            &mut Font::new(Blocks, FontConfig::default(), 1.0).unwrap(),
        );
        let all = rect(
            layout
                .frame(&tree, tree.children(tree.root().unwrap())[0])
                .unwrap(),
        );
        let clipped = items.iter().find_map(|item| match item {
            Item::PushClip(clip) => Some(*clip),
            _ => None,
        });
        assert_eq!(
            clipped.map(|clip| (clip.x, clip.width)),
            Some((all.x + 8, all.width - 16))
        );
        // Of a box of its own, the plugin's style draws it, whatever its
        // state.
        let own = Node::container(vec![Node::button("Go").key("go").style(view::Style {
            background: Some(Token::from("accent")),
            ..view::Style::default()
        })]);
        let (_, _, items) = painted_tree(
            &own,
            &mut Font::new(Blocks, FontConfig::default(), 1.0).unwrap(),
        );
        assert!(items.iter().any(|item| matches!(
            item,
            Item::Shapes(shapes) if shapes.iter().any(|shape| shape.fill == token("accent") && shape.radius == 0.0)
        )));
    }

    /// `root` laid out at a scale of 1 in 400 points, and painted of the
    /// pointer over its first child and pressing it.
    fn painted_tree(root: &Node, font: &mut Font<Blocks>) -> (Tree, Layout, Vec<Item>) {
        let mut tree = Tree::new();
        let changes = tree.update(root).unwrap();
        let mut layout = Layout::new(1.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(400.0), None, font);
        let first = tree.children(tree.root().unwrap())[0];
        let fields = Fields::new();
        let items = paint(
            &tree,
            &layout,
            font,
            &Theme::builtin().dark.tokens,
            (0.0, 0.0),
            Shown {
                dark: true,
                focused: None,
                fields: &fields,
                hovered: Some(first),
                pressed: Some(first),
                lists: &Lists::new(),
            },
        );
        (tree, layout, items)
    }

    #[test]
    fn a_tooltip_goes_below_the_pointer_within_its_window() {
        let tokens = &Theme::builtin().dark.tokens;
        // `New Tab`, 7 characters of 7.8 points, and 7 at each side; a
        // line of 13, and 3 above and below.
        let placed = |scale: f32, at: (f32, f32), room: (f32, f32)| {
            let mut font = Font::new(Blocks, FontConfig::default(), scale).unwrap();
            let items = tooltip(&mut font, tokens, "New Tab", (at, room), (scale, true));
            let [Item::Shapes(shapes), Item::Glyphs(glyphs)] = &items[..] else {
                panic!("{items:#?}");
            };
            assert_eq!(glyphs.len(), 7);
            assert!(
                glyphs
                    .iter()
                    .all(|glyph| glyph.color == token("tooltip.fg"))
            );
            let shape = &shapes[0];
            assert_eq!(shape.fill, token("tooltip.bg"));
            assert_eq!(shape.border_color, token("tooltip.border"));
            assert_eq!(shape.border, scale);
            assert!(shape.shadow.is_some());
            (shape.rect, glyphs[0].x - shape.rect.x)
        };
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        // 20 points below the pointer, its text 7 in.
        assert_eq!(
            placed(1.0, (100.0, 50.0), (400.0, 300.0)),
            (rect(100, 70, 69, 19), 7)
        );
        // Kept 4 points from the right edge, and above the pointer where
        // the bottom has no room.
        assert_eq!(
            placed(1.0, (380.0, 50.0), (400.0, 300.0)).0,
            rect(327, 70, 69, 19)
        );
        assert_eq!(
            placed(1.0, (100.0, 290.0), (400.0, 300.0)).0,
            rect(100, 267, 69, 19)
        );
        // Within the window's left and top too.
        assert_eq!(
            placed(1.0, (-10.0, -50.0), (400.0, 300.0)).0,
            rect(4, 4, 69, 19)
        );
        // Twice the scale, twice the pixels.
        assert_eq!(
            placed(2.0, (200.0, 100.0), (800.0, 600.0)),
            (rect(200, 140, 138, 38), 14)
        );
    }

    #[test]
    fn a_list_paints_its_rows_that_show_its_selection_and_its_scroll() {
        use lokyr_plugin_sdk::view::ListItem;
        // 100 rows of 20 points in 100, at a scale of 1: 5 show.
        let items = (0..100)
            .map(|index| ListItem {
                texts: vec![format!("{index:03}")],
            })
            .collect();
        let mut node = Node::list(Node::text(""), 100, (0, items))
            .key("rows")
            .layout(view::Layout {
                height: view::Length::Points(100.0),
                ..view::Layout::default()
            });
        if let lokyr_plugin_sdk::view::Kind::List(list) = &mut node.kind {
            list.row_height = Some(20.0);
        }
        let root = Node::container(vec![node]).layout(view::Layout {
            direction: view::Direction::Column,
            padding: Edges::all(10.0),
            ..view::Layout::default()
        });
        let mut font = Font::new(Blocks, FontConfig::default(), 1.0).unwrap();
        let mut tree = Tree::new();
        let changes = tree.update(&root).unwrap();
        let mut layout = Layout::new(1.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        let list = tree.children(tree.root().unwrap())[0];
        let mut lists = Lists::new();
        lists.lay_out(&tree, &layout, &mut font);
        lists.select(&tree, list, 2);
        let tokens = &Theme::builtin().dark.tokens;
        let painted = |font: &mut Font<Blocks>, focused: Option<NodeId>, lists: &Lists| {
            let fields = Fields::new();
            paint(
                &tree,
                &layout,
                font,
                tokens,
                (0.0, 0.0),
                Shown {
                    dark: true,
                    focused,
                    fields: &fields,
                    hovered: None,
                    pressed: None,
                    lists,
                },
            )
        };
        let split = |items: &[Item]| {
            let mut shapes = Vec::new();
            let mut glyphs = Vec::new();
            for item in items {
                match item {
                    Item::Shapes(found) => shapes.extend(found.iter().copied()),
                    Item::Glyphs(found) => glyphs.extend(found.iter().copied()),
                    _ => {}
                }
            }
            (shapes, glyphs)
        };
        // Focused: the selection of the accent, its row's text of the
        // selection's; 3 glyphs a row, 5 rows; the list clipped within its
        // box; the indicator at the top, 24 points of 100.
        let items = painted(&mut font, Some(list), &lists);
        assert!(matches!(
            items.first(),
            Some(Item::PushClip(Rect {
                x: 10,
                y: 10,
                width: 180,
                height: 100
            }))
        ));
        let (shapes, glyphs) = split(&items);
        let band = Rect {
            x: 10,
            y: 50,
            width: 180,
            height: 20,
        };
        assert!(shapes.contains(&Shape::filled(band, token("list.selection.bg"))));
        assert_eq!(glyphs.len(), 15);
        let in_band = |glyph: &PlacedGlyph| glyph.y >= 50 && glyph.y < 70;
        assert!(
            glyphs
                .iter()
                .filter(|glyph| in_band(glyph))
                .all(|glyph| glyph.color == token("list.selection.fg"))
        );
        assert!(
            glyphs
                .iter()
                .filter(|glyph| !in_band(glyph))
                .all(|glyph| glyph.color == token("text"))
        );
        let indicator = shapes
            .iter()
            .find(|shape| shape.fill == token("scrollbar"))
            .unwrap();
        assert_eq!(
            indicator.rect,
            Rect {
                x: 10 + 180 - 4 - 2,
                y: 10,
                width: 4,
                height: 24
            }
        );
        // Without the focus, the selection of a gray, its text as the
        // others'.
        let (shapes, glyphs) = split(&painted(&mut font, None, &lists));
        assert!(shapes.contains(&Shape::filled(band, token("list.selection.inactive.bg"))));
        assert!(glyphs.iter().all(|glyph| glyph.color == token("text")));
        // Scrolled to the end, the indicator at the bottom.
        lists.scroll(&tree, list, 1e9);
        lists.lay_out(&tree, &layout, &mut font);
        let (shapes, _) = split(&painted(&mut font, None, &lists));
        let indicator = shapes
            .iter()
            .find(|shape| shape.fill == token("scrollbar"))
            .unwrap();
        assert_eq!(indicator.rect.y, 10 + 100 - 24);
        // A list of no state yet paints none of its rows.
        let (_, glyphs) = split(&painted(&mut font, None, &Lists::new()));
        assert!(glyphs.is_empty());
    }

    #[test]
    fn a_list_whose_rows_fit_shows_no_indicator() {
        use lokyr_plugin_sdk::view::ListItem;
        let items = (0..3)
            .map(|index| ListItem {
                texts: vec![format!("{index}")],
            })
            .collect();
        let mut node = Node::list(Node::text(""), 3, (0, items))
            .key("rows")
            .layout(view::Layout {
                height: view::Length::Points(100.0),
                ..view::Layout::default()
            });
        if let lokyr_plugin_sdk::view::Kind::List(list) = &mut node.kind {
            list.row_height = Some(20.0);
        }
        let mut font = Font::new(Blocks, FontConfig::default(), 1.0).unwrap();
        let mut tree = Tree::new();
        let changes = tree.update(&Node::container(vec![node])).unwrap();
        let mut layout = Layout::new(1.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        let mut lists = Lists::new();
        lists.lay_out(&tree, &layout, &mut font);
        let fields = Fields::new();
        let items = paint(
            &tree,
            &layout,
            &mut font,
            &Theme::builtin().dark.tokens,
            (0.0, 0.0),
            Shown {
                dark: true,
                focused: None,
                fields: &fields,
                hovered: None,
                pressed: None,
                lists: &lists,
            },
        );
        let glyphs: usize = items
            .iter()
            .map(|item| match item {
                Item::Glyphs(glyphs) => glyphs.len(),
                _ => 0,
            })
            .sum();
        assert_eq!(glyphs, 3);
        assert!(!items.iter().any(|item| matches!(
            item,
            Item::Shapes(shapes) if shapes.iter().any(|shape| shape.fill == token("scrollbar"))
        )));
    }

    #[test]
    fn a_scroll_area_clips_its_children_and_shows_its_scroll() {
        let boxes: Vec<Node> = (0..10)
            .map(|index| {
                Node::container(vec![])
                    .key(format!("box-{index}").as_str())
                    .layout(view::Layout {
                        height: view::Length::Points(20.0),
                        ..view::Layout::default()
                    })
                    .style(view::Style {
                        background: Some(Token::from("accent")),
                        ..view::Style::default()
                    })
            })
            .collect();
        let area = Node::scroll(boxes).key("area").layout(view::Layout {
            direction: view::Direction::Column,
            height: view::Length::Points(50.0),
            ..view::Layout::default()
        });
        let mut font = Font::new(Blocks, FontConfig::default(), 1.0).unwrap();
        let mut tree = Tree::new();
        let root = Node::container(vec![area]).layout(view::Layout {
            direction: view::Direction::Column,
            ..view::Layout::default()
        });
        let changes = tree.update(&root).unwrap();
        let mut layout = Layout::new(1.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(100.0), None, &mut font);
        let node = tree.children(tree.root().unwrap())[0];
        layout.scroll_by(&tree, node, 75.0);
        let items = paint(
            &tree,
            &layout,
            &mut font,
            &Theme::builtin().dark.tokens,
            (0.0, 0.0),
            shown(true, None),
        );
        let [Item::PushClip(clip), Item::Shapes(shapes), Item::PopClip] = &items[..] else {
            panic!("{items:#?}");
        };
        assert_eq!(
            *clip,
            Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 50
            }
        );
        // The boxes moved up 75 points: the fourth from -15.
        assert_eq!(shapes[3].rect.y, 60 - 75);
        // 200 points in 50: 150 to scroll, half of it, the indicator 24
        // points high, halfway down the 26 left.
        let indicator = shapes.last().unwrap();
        assert_eq!(indicator.fill, token("scrollbar"));
        assert_eq!((indicator.rect.y, indicator.rect.height), (13, 24));
    }

    #[test]
    fn huge_numbers_paint_without_overflowing() {
        // A border and a box past any display, of numbers that a tree takes,
        // the box focused and clipping its child inside its border: no
        // arithmetic overflows, as a debug build would panic of it.
        let huge = Node::container(vec![Node::text("x")])
            .key("huge")
            .focusable()
            .layout(view::Layout {
                width: view::Length::Points(5e9),
                height: view::Length::Points(10.0),
                ..view::Layout::default()
            })
            .style(view::Style {
                background: Some(Token::from("panel.bg")),
                border: Some(Border {
                    color: Token::from("panel.border"),
                    width: 1e30,
                }),
                clip: true,
                ..view::Style::default()
            });
        let root = Node::container(vec![huge.clone(), huge.key("other")]);
        let mut font = Font::new(Blocks, FontConfig::default(), 2.0).unwrap();
        let mut tree = Tree::new();
        let changes = tree.update(&root).unwrap();
        let mut layout = Layout::new(2.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, None, None, &mut font);
        let first = tree.children(tree.root().unwrap())[0];
        let tokens = &Theme::builtin().dark.tokens;
        let items = paint(
            &tree,
            &layout,
            &mut font,
            tokens,
            (0.0, 0.0),
            shown(true, Some(first)),
        );
        assert!(!items.is_empty());
    }

    #[test]
    fn an_icon_of_no_width_paints_nothing() {
        let row = Node::container(vec![Node::icon("close").layout(view::Layout {
            width: view::Length::Points(0.0),
            ..view::Layout::default()
        })]);
        let (_, _, items) = painted(&row, 1.0, 100.0, (0.0, 0.0));
        assert!(items.is_empty(), "{items:#?}");
    }

    /// Texts of the system's font at their natural width, of CoreText's
    /// glyphs, whose ink reaches a pixel past their advance: no clip.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_text_of_its_natural_width_takes_no_clip() {
        use lokyr_font_coretext::CoreText;
        let texts = ["~/lokyr", "let", "Hello, world", "Ångström", "fn main() {}"];
        for scale in [1.0, 2.0] {
            let mut font = Font::new(CoreText::new(), FontConfig::default(), scale).unwrap();
            let row = Node::container(texts.iter().map(|&text| Node::text(text)).collect()).layout(
                view::Layout {
                    gap: 8.0,
                    align: view::Align::Start,
                    ..view::Layout::default()
                },
            );
            let mut tree = Tree::new();
            let changes = tree.update(&row).unwrap();
            let mut layout = Layout::new(scale);
            layout.update(&tree, &changes);
            layout.compute(&tree, None, None, &mut font);
            let tokens = &Theme::builtin().light.tokens;
            let items = paint(
                &tree,
                &layout,
                &mut font,
                tokens,
                (0.0, 0.0),
                shown(false, None),
            );
            assert!(
                !items.iter().any(|item| matches!(item, Item::PushClip(_))),
                "{scale}: {items:#?}"
            );
        }
    }

    #[test]
    fn a_box_of_no_width_paints_nothing() {
        let empty = Node::container(vec![])
            .layout(view::Layout {
                width: view::Length::Points(0.0),
                height: view::Length::Points(10.0),
                ..view::Layout::default()
            })
            .style(view::Style {
                background: Some(Token::from("panel.bg")),
                shadow: Some(view::Shadow::default()),
                ..view::Style::default()
            });
        let row = Node::container(vec![empty]).layout(view::Layout {
            align: view::Align::Start,
            ..view::Layout::default()
        });
        let (_, _, items) = painted(&row, 1.0, 100.0, (0.0, 0.0));
        assert!(items.is_empty(), "{items:#?}");
    }

    #[test]
    fn a_token_that_the_theme_lacks_paints_nothing_and_an_icon_unknown_neither() {
        let node = Node::container(vec![Node::icon("no-such-icon")]).style(view::Style {
            background: Some(Token::from("nothing.here")),
            border: Some(Border {
                color: Token::from("nor.here"),
                width: 1.0,
            }),
            ..view::Style::default()
        });
        let (_, _, items) = painted(&node, 1.0, 100.0, (0.0, 0.0));
        assert!(items.is_empty(), "{items:#?}");
        // A text's color of a token unknown takes its parent's.
        let node = Node::container(vec![Node::text("a").style(view::Style {
            color: Some(Token::from("nothing.here")),
            ..view::Style::default()
        })])
        .style(view::Style {
            color: Some(Token::from("accent")),
            ..view::Style::default()
        });
        let (_, _, items) = painted(&node, 1.0, 100.0, (0.0, 0.0));
        let [Item::Glyphs(glyphs)] = &items[..] else {
            panic!("{items:#?}")
        };
        assert_eq!(glyphs[0].color, token("accent"));
    }
}
