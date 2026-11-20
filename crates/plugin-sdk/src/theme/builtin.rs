//! Lokyr's built-in themes, Lokyr Light and Lokyr Dark: a palette of their
//! own, whose colors read on their variant's background, its text,
//! background and selection, and every token of the interface their own.

use super::{TerminalColors, Theme, ThemeColor, ThemeVariant, TokenValue};

/// The name of the built-in theme of the light appearance.
pub const LIGHT_NAME: &str = "Lokyr Light";

/// The name of the built-in theme of the dark appearance.
pub const DARK_NAME: &str = "Lokyr Dark";

/// A token's color: `#rrggbb` and its opacity.
pub type TokenColor = (&'static str, u32, f32);

/// The interface's colors in the light variant: the system's grays, the
/// text the terminal's.
pub const LIGHT_TOKENS: &[TokenColor] = &[
    ("text", 0x1e1e20, 1.0),
    ("text.secondary", 0x6c6c72, 1.0),
    ("text.disabled", 0xa8a8ae, 1.0),
    ("text.success", 0x248a3d, 1.0),
    ("text.warning", 0x9a6700, 1.0),
    ("accent", 0x0a64d8, 1.0),
    ("separator", 0x000000, 0.1),
    ("pane.divider", 0x000000, 0.16),
    ("pane.drop", 0x0a64d8, 0.25),
    ("shadow", 0x000000, 0.25),
    ("panel.bg", 0xf6f6f6, 1.0),
    ("panel.border", 0x000000, 0.14),
    ("field.bg", 0xffffff, 1.0),
    ("field.border", 0x000000, 0.18),
    ("field.focus", 0x0a64d8, 0.5),
    ("field.selection", 0xb3d7ff, 1.0),
    ("focus.ring", 0x0a64d8, 0.5),
    ("button.bg", 0x000000, 0.06),
    ("button.hover.bg", 0x000000, 0.08),
    ("button.pressed.bg", 0x000000, 0.16),
    ("button.default.bg", 0x0a64d8, 1.0),
    ("button.default.pressed.bg", 0x0955b8, 1.0),
    ("button.default.fg", 0xffffff, 1.0),
    ("button.destructive.fg", 0xc4262e, 1.0),
    ("toggle.on.bg", 0x0a64d8, 0.12),
    ("toggle.on.fg", 0x0a5cc6, 1.0),
    ("switch.off.bg", 0x1e1e20, 0.1),
    ("switch.on.bg", 0x0a64d8, 1.0),
    ("switch.knob.bg", 0xffffff, 1.0),
    ("switch.knob.shadow", 0x000000, 0.12),
    ("check.off.bg", 0x1e1e20, 0.1),
    ("check.off.pressed.bg", 0x1e1e20, 0.19),
    ("check.on.bg", 0x0a64d8, 1.0),
    ("check.on.pressed.bg", 0x095ac2, 1.0),
    ("check.mark", 0xffffff, 1.0),
    ("group.bg", 0x1e1e20, 0.03),
    ("group.separator", 0x1e1e20, 0.05),
    ("keycap.bg", 0x1e1e20, 0.08),
    ("keycap.fg", 0x6c6c72, 1.0),
    ("callout.info.bg", 0x0a64d8, 0.12),
    ("callout.info.fg", 0x0a64d8, 1.0),
    ("callout.success.bg", 0x248a3d, 0.12),
    ("callout.success.fg", 0x248a3d, 1.0),
    ("callout.warning.bg", 0x9a6700, 0.12),
    ("callout.warning.fg", 0x9a6700, 1.0),
    ("callout.error.bg", 0xd63b3b, 0.12),
    ("callout.error.fg", 0xd63b3b, 1.0),
    ("tooltip.bg", 0xfbfbfb, 1.0),
    ("tooltip.fg", 0x1e1e20, 1.0),
    ("tooltip.border", 0x000000, 0.15),
    ("list.selection.bg", 0x0a64d8, 1.0),
    ("list.selection.fg", 0xffffff, 1.0),
    ("list.selection.inactive.bg", 0x000000, 0.1),
    ("scrollbar", 0x000000, 0.3),
    ("scrollbar.hover", 0x000000, 0.5),
    ("scrollbar.track", 0x000000, 0.05),
    ("scrollbar.mark", 0xd9a300, 1.0),
    ("scrollbar.mark.current", 0xff7a00, 1.0),
    ("progress.track", 0x000000, 0.1),
    ("progress.normal", 0x0a64d8, 1.0),
    ("progress.error", 0xd63b3b, 1.0),
    ("progress.paused", 0xd99a00, 1.0),
    ("tabs.bar.bg", 0xf4f4f6, 1.0),
    ("tabs.track.bg", 0x000000, 0.07),
    ("tabs.active.bg", 0xffffff, 1.0),
    ("tabs.active.fg", 0x1e1e20, 1.0),
    ("tabs.active.border", 0x000000, 0.1),
    ("tabs.active.shadow", 0x000000, 0.12),
    ("tabs.inactive.bg", 0x000000, 0.0),
    ("tabs.inactive.fg", 0x5e5e64, 1.0),
    ("tabs.hover.bg", 0x000000, 0.05),
    ("tabs.seamless.line", 0x000000, 0.12),
    ("tabs.seamless.veil", 0x000000, 0.06),
    ("segments.bg", 0x000000, 0.06),
    ("segments.chosen.bg", 0x0a64d8, 1.0),
    ("segments.chosen.fg", 0xffffff, 1.0),
    ("segments.box.bg", 0x000000, 0.03),
    ("segments.box.border", 0x000000, 0.1),
    ("search.match", 0xffd60a, 0.5),
    ("search.match.current", 0xff7a00, 0.6),
    ("gutter.running", 0x0a64d8, 1.0),
    ("gutter.succeeded", 0x000000, 0.22),
    ("gutter.failed", 0xd63b3b, 1.0),
    ("cursor.guide", 0x1e1e20, 0.1),
    ("hints.label.bg", 0x0a64d8, 1.0),
    ("hints.label.fg", 0xffffff, 1.0),
    ("blocks.divider", 0x000000, 0.1),
    ("blocks.selected.bg", 0x0a64d8, 0.12),
    ("blocks.hover.bg", 0x0a64d8, 0.06),
    ("blocks.failed", 0xd63b3b, 0.08),
];

/// The interface's colors in the dark variant.
pub const DARK_TOKENS: &[TokenColor] = &[
    ("text", 0xe6e6e6, 1.0),
    ("text.secondary", 0x9a9aa0, 1.0),
    ("text.disabled", 0x5e5e64, 1.0),
    ("text.success", 0x30d158, 1.0),
    ("text.warning", 0xffd60a, 1.0),
    ("accent", 0x4c9bff, 1.0),
    ("separator", 0xffffff, 0.1),
    ("pane.divider", 0xffffff, 0.16),
    ("pane.drop", 0x4c9bff, 0.3),
    ("shadow", 0x000000, 0.5),
    ("panel.bg", 0x2a2a2d, 1.0),
    ("panel.border", 0xffffff, 0.12),
    ("field.bg", 0x1e1e20, 1.0),
    ("field.border", 0xffffff, 0.16),
    ("field.focus", 0x4c9bff, 0.6),
    ("field.selection", 0x3f638b, 1.0),
    ("focus.ring", 0x4c9bff, 0.6),
    ("button.bg", 0xffffff, 0.08),
    ("button.hover.bg", 0xffffff, 0.1),
    ("button.pressed.bg", 0xffffff, 0.18),
    ("button.default.bg", 0x356db3, 1.0),
    ("button.default.pressed.bg", 0x2d5c98, 1.0),
    ("button.default.fg", 0xffffff, 1.0),
    ("button.destructive.fg", 0xf25f64, 1.0),
    ("toggle.on.bg", 0x4c9bff, 0.2),
    ("toggle.on.fg", 0x7ab6ff, 1.0),
    ("switch.off.bg", 0xe6e6e6, 0.1),
    ("switch.on.bg", 0x4c9bff, 1.0),
    ("switch.knob.bg", 0xffffff, 1.0),
    ("switch.knob.shadow", 0x000000, 0.0),
    ("check.off.bg", 0xe6e6e6, 0.1),
    ("check.off.pressed.bg", 0xe6e6e6, 0.19),
    ("check.on.bg", 0x356db3, 1.0),
    ("check.on.pressed.bg", 0x3062a1, 1.0),
    ("check.mark", 0xffffff, 1.0),
    ("group.bg", 0xe6e6e6, 0.03),
    ("group.separator", 0xe6e6e6, 0.05),
    ("keycap.bg", 0xe6e6e6, 0.08),
    ("keycap.fg", 0x9a9aa0, 1.0),
    ("callout.info.bg", 0x4c9bff, 0.12),
    ("callout.info.fg", 0x4c9bff, 1.0),
    ("callout.success.bg", 0x30d158, 0.12),
    ("callout.success.fg", 0x30d158, 1.0),
    ("callout.warning.bg", 0xffd60a, 0.12),
    ("callout.warning.fg", 0xffd60a, 1.0),
    ("callout.error.bg", 0xff5f57, 0.12),
    ("callout.error.fg", 0xff5f57, 1.0),
    ("tooltip.bg", 0x323235, 1.0),
    ("tooltip.fg", 0xe6e6e6, 1.0),
    ("tooltip.border", 0xffffff, 0.15),
    ("list.selection.bg", 0x2462c4, 1.0),
    ("list.selection.fg", 0xffffff, 1.0),
    ("list.selection.inactive.bg", 0xffffff, 0.12),
    ("scrollbar", 0xffffff, 0.35),
    ("scrollbar.hover", 0xffffff, 0.55),
    ("scrollbar.track", 0xffffff, 0.07),
    ("scrollbar.mark", 0xffd60a, 1.0),
    ("scrollbar.mark.current", 0xff9f0a, 1.0),
    ("progress.track", 0xffffff, 0.14),
    ("progress.normal", 0x4c9bff, 1.0),
    ("progress.error", 0xff5f57, 1.0),
    ("progress.paused", 0xffbd2e, 1.0),
    ("tabs.bar.bg", 0x232326, 1.0),
    ("tabs.track.bg", 0x000000, 0.3),
    ("tabs.active.bg", 0x404044, 1.0),
    ("tabs.active.fg", 0xf4f4f6, 1.0),
    ("tabs.active.border", 0xffffff, 0.12),
    ("tabs.active.shadow", 0x000000, 0.4),
    ("tabs.inactive.bg", 0x000000, 0.0),
    ("tabs.inactive.fg", 0x9a9aa0, 1.0),
    ("tabs.hover.bg", 0xffffff, 0.06),
    ("tabs.seamless.line", 0xffffff, 0.12),
    ("tabs.seamless.veil", 0x000000, 0.27),
    ("segments.bg", 0xffffff, 0.08),
    ("segments.chosen.bg", 0x2462c4, 1.0),
    ("segments.chosen.fg", 0xffffff, 1.0),
    ("segments.box.bg", 0xffffff, 0.03),
    ("segments.box.border", 0xffffff, 0.1),
    ("search.match", 0xffd60a, 0.3),
    ("search.match.current", 0xff7a00, 0.5),
    ("gutter.running", 0x4c9bff, 1.0),
    ("gutter.succeeded", 0xffffff, 0.25),
    ("gutter.failed", 0xff5f57, 1.0),
    ("cursor.guide", 0xe6e6e6, 0.1),
    ("hints.label.bg", 0x4c9bff, 1.0),
    ("hints.label.fg", 0x000000, 1.0),
    ("blocks.divider", 0xffffff, 0.1),
    ("blocks.selected.bg", 0x4c9bff, 0.2),
    ("blocks.hover.bg", 0x4c9bff, 0.08),
    ("blocks.failed", 0xff5f57, 0.12),
];

/// The sixteen colors of a variant, from black to bright white.
pub type Sixteen = [u32; 16];

/// The light variant's sixteen colors, darker than its background, and its
/// text, background and selection.
pub const LIGHT: (Sixteen, u32, u32, u32) = (
    [
        0x1e1e20, 0xc4262e, 0x1d7f3c, 0x9a6500, 0x1f5fcc, 0x9b3cb5, 0x137f8b, 0x7a7a82, //
        0x5a5a62, 0xe0444b, 0x2a9a4c, 0xb87d00, 0x3a7be6, 0xb557cf, 0x1a9cab, 0x3a3a40,
    ],
    0x1e1e20,
    0xfafafa,
    0xb8d4ff,
);

/// The dark variant's sixteen colors, lighter than its background, and its
/// text, background and selection.
pub const DARK: (Sixteen, u32, u32, u32) = (
    [
        0x45454b, 0xf25d62, 0x58c672, 0xe3bd4c, 0x5aa2f7, 0xc983de, 0x4cc4cf, 0xc4c4ca, //
        0x75757d, 0xff8488, 0x7edb94, 0xf5d47a, 0x86bcff, 0xdca3ec, 0x7adce4, 0xf4f4f6,
    ],
    0xe6e6e6,
    0x1e1e20,
    0x34445f,
);

/// The tokens of [`LIGHT_TOKENS`] that the light theme leaves to their
/// rules, which give their colors within a step of 1/255: the accent's of
/// its opacities, the text's, and the texts on the accent.
pub const LIGHT_RULED: &[&str] = &[
    "text",
    "pane.drop",
    "shadow",
    "field.focus",
    "focus.ring",
    "button.default.bg",
    "button.default.pressed.bg",
    "button.default.fg",
    "button.destructive.fg",
    "toggle.on.bg",
    "switch.off.bg",
    "switch.on.bg",
    "switch.knob.bg",
    "switch.knob.shadow",
    "check.off.bg",
    "check.off.pressed.bg",
    "check.on.bg",
    "check.on.pressed.bg",
    "check.mark",
    "group.bg",
    "group.separator",
    "keycap.bg",
    "keycap.fg",
    "callout.info.bg",
    "callout.info.fg",
    "callout.success.bg",
    "callout.success.fg",
    "callout.warning.bg",
    "callout.warning.fg",
    "callout.error.bg",
    "callout.error.fg",
    "tooltip.fg",
    "list.selection.bg",
    "list.selection.fg",
    "progress.normal",
    "tabs.active.fg",
    "tabs.active.shadow",
    "tabs.inactive.bg",
    "tabs.seamless.veil",
    "segments.chosen.bg",
    "segments.chosen.fg",
    "gutter.running",
    "cursor.guide",
    "hints.label.bg",
    "hints.label.fg",
    "blocks.divider",
    "blocks.selected.bg",
    "blocks.hover.bg",
    "blocks.failed",
];

/// The tokens of [`DARK_TOKENS`] that the dark theme leaves to their
/// rules, as [`LIGHT_RULED`].
pub const DARK_RULED: &[&str] = &[
    "text",
    "pane.drop",
    "shadow",
    "field.bg",
    "field.focus",
    "focus.ring",
    "button.default.bg",
    "button.default.pressed.bg",
    "button.default.fg",
    "button.destructive.fg",
    "toggle.on.bg",
    "switch.off.bg",
    "switch.on.bg",
    "switch.knob.bg",
    "switch.knob.shadow",
    "check.off.bg",
    "check.off.pressed.bg",
    "check.on.bg",
    "check.on.pressed.bg",
    "check.mark",
    "group.bg",
    "group.separator",
    "keycap.bg",
    "keycap.fg",
    "callout.info.bg",
    "callout.info.fg",
    "callout.success.bg",
    "callout.success.fg",
    "callout.warning.bg",
    "callout.warning.fg",
    "callout.error.bg",
    "callout.error.fg",
    "tooltip.fg",
    "list.selection.fg",
    "progress.normal",
    "tabs.active.shadow",
    "tabs.inactive.bg",
    "tabs.seamless.veil",
    "segments.chosen.fg",
    "gutter.running",
    "cursor.guide",
    "hints.label.bg",
    "hints.label.fg",
    "blocks.divider",
    "blocks.selected.bg",
    "blocks.hover.bg",
    "blocks.failed",
];

/// The built-in theme of the light appearance: its own tokens, the others
/// of their rules, so that a theme made of it whose accent changes takes it
/// where the rules name it.
pub fn light() -> Theme {
    ruled(
        of(LIGHT_NAME, ThemeVariant::Light, LIGHT, LIGHT_TOKENS),
        LIGHT_RULED,
    )
}

/// The built-in theme of the dark appearance, as [`light`].
pub fn dark() -> Theme {
    ruled(
        of(DARK_NAME, ThemeVariant::Dark, DARK, DARK_TOKENS),
        DARK_RULED,
    )
}

/// `theme` without its tokens of `names`, which their rules give.
fn ruled(mut theme: Theme, names: &[&str]) -> Theme {
    theme
        .tokens
        .retain(|name, _| !names.contains(&name.as_str()));
    theme
}

/// The theme of `name` of the colors of `sixteen`, `foreground`,
/// `background` and `selection`, and of every token of `tokens`.
pub fn of(
    name: &str,
    variant: ThemeVariant,
    (sixteen, foreground, background, selection): (Sixteen, u32, u32, u32),
    tokens: &[TokenColor],
) -> Theme {
    let mut theme = Theme::of_terminal(
        name,
        variant,
        TerminalColors {
            background: ThemeColor::hex(background),
            foreground: ThemeColor::hex(foreground),
            palette: sixteen.map(ThemeColor::hex),
            cursor: None,
            cursor_text: None,
            selection: Some(ThemeColor::hex(selection)),
            selection_text: None,
            bold: None,
            link: None,
            underline: None,
        },
    );
    theme.tokens = tokens
        .iter()
        .map(|&(name, value, alpha)| {
            let alpha = (alpha * 255.0).round() as u8;
            (
                name.to_owned(),
                TokenValue::Color(ThemeColor::hex(value).with_alpha(alpha)),
            )
        })
        .collect();
    theme
}
