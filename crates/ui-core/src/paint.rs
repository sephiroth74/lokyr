//! The display list of a laid-out view tree: each node's box, of its
//! style's background, border, corners and shadow, whose colors the theme's
//! tokens give; its text's or its icon's glyphs, in the interface's face, in
//! its color or the nearest ancestor's; and the clips of the nodes that
//! clip their children; in the tree's order, a node over its parent and
//! over its siblings before it; and the ring of the keyboard's focus around
//! the node that has it, over them all, within its ancestors' clips.

use lokyr_plugin_sdk::view::{Kind, Style};
use lokyr_render_core::{Color, Item, PlacedGlyph, Rect, Shadow, Shape};
use lokyr_text_core::FontBackend;

use crate::font::Font;
use crate::layout::{Frame, Layout};
use crate::theme::Tokens;
use crate::tree::{NodeId, Tree};

/// The width of the ring of the keyboard's focus, outside its node's box,
/// in points.
const RING: f32 = 3.0;

/// The display list of `tree`, laid out in `layout`, from `origin` in the
/// window's pixels: its colors of `tokens`, its texts and icons of `font`,
/// at the layout's scale, of the dark appearance when `dark`; the ring of
/// the focus around `focused`, if it is the tree's.
pub fn paint<B: FontBackend>(
    tree: &Tree,
    layout: &Layout,
    font: &mut Font<B>,
    tokens: &Tokens,
    origin: (f32, f32),
    (dark, focused): (bool, Option<NodeId>),
) -> Vec<Item> {
    let mut painter = Painter {
        tree,
        layout,
        font,
        tokens,
        origin,
        dark,
        focused,
        clip: None,
        ring: None,
        items: Vec::new(),
    };
    if let Some(root) = tree.root() {
        painter.node(root, tokens.get("text").unwrap_or(Color::BLACK));
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

struct Painter<'a, B> {
    tree: &'a Tree,
    layout: &'a Layout,
    font: &'a mut Font<B>,
    tokens: &'a Tokens,
    origin: (f32, f32),
    dark: bool,
    focused: Option<NodeId>,
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
        if self.focused == Some(id) {
            self.ring = self.ring(frame, style).map(|ring| (ring, self.clip));
        }
        if let Some(shape) = self.shape(frame, style) {
            match self.items.last_mut() {
                Some(Item::Shapes(shapes)) => shapes.push(shape),
                _ => self.items.push(Item::Shapes(vec![shape])),
            }
        }
        match kind {
            Kind::Text(text) => self.text(text, frame, color),
            Kind::Icon(name) => self.icon(name, frame, color),
            Kind::Container | Kind::Spacer => {}
        }
        let children = tree.children(id);
        if children.is_empty() {
            return;
        }
        let outer = self.clip;
        if style.clip {
            let inside = inside_border(rect(frame), style, self.layout.scale());
            self.items.push(Item::PushClip(inside));
            self.clip = Some(outer.map_or(inside, |outer| intersection(outer, inside)));
        }
        for &child in children {
            self.node(child, color);
        }
        if style.clip {
            self.items.push(Item::PopClip);
            self.clip = outer;
        }
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
                width: inner.width + 2 * outset,
                height: inner.height + 2 * outset,
            },
            radius: style.radius * scale + width,
            fill: Color::CLEAR,
            border: width,
            border_color: color,
            shadow: None,
        })
    }

    /// The shape of a node's box at `frame`, of `style`: `None` for a box
    /// that draws nothing, or whose tokens the theme lacks.
    fn shape(&self, frame: Frame, style: &Style) -> Option<Shape> {
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
            rect: rect(frame),
            radius: style.radius * scale,
            fill: fill.unwrap_or(Color::CLEAR),
            border: border.map_or(0.0, |_| border_pixels(style, scale) as f32),
            border_color: border.unwrap_or(Color::CLEAR),
            shadow,
        })
    }

    /// The glyphs of `text`, on its line in the middle of `frame`, clipped
    /// to it where they reach past it.
    fn text(&mut self, text: &str, frame: Frame, color: Color) {
        let scale = self.layout.scale();
        let face = self.font.interface();
        let (line, ascent) = (face.line() * scale, face.ascent() * scale);
        let baseline = (frame.y + (frame.height - line) / 2.0 + ascent).round();
        let (glyphs, end) = self
            .font
            .interface_text(text, (frame.x, baseline), color, self.dark);
        if end > frame.x + frame.width {
            self.items.push(Item::PushClip(rect(frame)));
            self.items.push(Item::Glyphs(glyphs));
            self.items.push(Item::PopClip);
        } else {
            self.glyphs(glyphs);
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
        width: rect.width.saturating_sub(2 * border),
        height: rect.height.saturating_sub(2 * border),
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
    use crate::theme::Theme;

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
        let items = paint(&tree, &layout, &mut font, tokens, origin, (true, None));
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
        assert_eq!(
            *text,
            Rect {
                x: 0,
                y: 0,
                width: 40,
                height: 13
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
                (true, Some(field)),
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
            let unfocused = paint(&tree, &layout, &mut font, tokens, (10.0, 0.0), (true, None));
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
                (true, Some(elsewhere)),
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
            (true, Some(field)),
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
            (true, Some(root)),
        );
        let Some(Item::Shapes(last)) = items.last() else {
            panic!("{items:#?}");
        };
        assert_eq!(last.last().unwrap().border_color, token("focus.ring"));
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
