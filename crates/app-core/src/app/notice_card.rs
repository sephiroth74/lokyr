//! The notice of what went wrong, as a card that comes down at the panes'
//! top right, over them, not as wide as the window: an icon of its kind, its
//! title, its text in two lines at most, and two buttons, one that copies
//! its whole text and one that closes it. A failure of an action, a
//! plugin's or a command's, goes by itself after a while unless the pointer
//! rests on it; one that a fix takes away, as a config's, stays until the
//! fix or a click.

use std::time::{Duration, Instant};

use lokyr_kernel::commands::{Code, ErrorCategory};
use lokyr_kernel::diagnostics::{About, Diagnostic};
use lokyr_kernel::plugins::ui::{
    Anchor as PanelAnchor, Corner as PanelCorner, PanelId, PanelOptions, Role as PanelRole,
    UiEvent, ViewId,
};
use lokyr_kernel::plugins::view::{
    Align, Border, Direction, Edges, FontWeight, Hover, Justify, Layout, Length, Node,
    Role as NodeRole, Shadow, Style, TextSize, Token, Transition, UiEventKind,
};
use lokyr_platform_core::{Platform, WindowId};
use lokyr_ui_core::panel::{Anchor, Dismiss, Options, Role};

use super::App;
use super::chrome::{OVERFLOW, PanelPlace, SlotView, free_access};
use crate::drawing::Backend;

/// The id of the card's panel, the app's own, of no plugin's.
pub(super) const NOTICE: &str = "notice";

/// The card's width, in points.
const CARD: f32 = 380.0;

/// The card's padding, its icon's side, and its buttons' side, in points.
const PADDING: f32 = 12.0;
const ICON: f32 = 16.0;
const BUTTON: f32 = 22.0;

/// The room of the card's text: its width less its padding, its icon, its
/// two buttons and the gaps between them.
const TEXT: f32 = CARD - 2.0 * PADDING - ICON - 2.0 * BUTTON - 4.0 * GAP;

/// The gap between the card's parts, in points.
const GAP: f32 = 8.0;

/// The most lines of the card's text; the rest, cut of an ellipsis, the copy
/// takes whole.
const LINES: usize = 2;

/// How far the card comes down as it shows, in points.
const DOWN: f32 = 8.0;

/// How long a failure's card shows before it goes by itself, from its
/// showing or from the pointer leaving it.
pub(super) const SHOWS: Duration = Duration::from_secs(8);

/// The notice that the card shows, as it was last settled: what tells it
/// apart from another, its card's revision, whether its card shows its first
/// frame, before it comes down, the timer of a failure's going, and whether
/// the pointer rests on it.
#[derive(Debug, Default)]
pub(super) struct NoticeCard {
    shown: Option<(About, Code, String)>,
    pub(super) revision: u64,
    entering: bool,
    pub(super) timer: Option<(lokyr_platform_core::TimerId, Instant)>,
    hovered: bool,
}

/// The card's view.
pub(super) fn notice_view() -> ViewId {
    ViewId::Panel(PanelId::from(NOTICE))
}

/// Whether `diagnostic` goes by itself after a while: a failure of an
/// action, which no fix takes away.
fn goes_by_itself(diagnostic: &Diagnostic) -> bool {
    diagnostic.about == About::Other
}

/// The message of the catalogs of the title of `diagnostic`'s card.
fn title_of(diagnostic: &Diagnostic) -> &'static str {
    match (&diagnostic.about, diagnostic.category()) {
        (About::Config, _) => "notice-config",
        (About::Font, _) => "notice-font",
        (About::GlobalKeys, _) => "notice-global-keys",
        (About::Term, _) => "notice-term",
        (About::Theme, _) => "notice-theme",
        (About::PluginConfig(_), _) => "notice-plugin-config",
        (About::KeptWindows, _) => "notice-kept-windows",
        (About::Permissions, _) => "notice-permissions",
        (About::InstalledPlugins, _) => "notice-installed-plugins",
        (About::Other, ErrorCategory::Fault) => "notice-fault",
        (About::Other, _) => "notice-failed",
    }
}

/// The card of `title` and the lines of its text, `lines`, of a fault's look
/// where `fault`, as it comes down where `entering`, faded and at its top,
/// before it slides into place; `copy` and `close` its buttons' tooltips.
fn card(
    (title, lines, fault): (&str, &[String], bool),
    (copy, close): (&str, &str),
    entering: bool,
) -> Node {
    let (icon, tint) = if fault {
        ("error", "progress.error")
    } else {
        ("warning", "progress.paused")
    };
    let mut texts = vec![Node::text(title).style(Style {
        font_weight: Some(FontWeight::Semibold),
        ..Style::default()
    })];
    for line in lines {
        texts.push(Node::text(line.as_str()).style(Style {
            text_size: Some(TextSize::Small),
            color: Some(Token::from("text.secondary")),
            ..Style::default()
        }));
    }
    let button = |icon: &str, key: &str, tooltip: &str| {
        Node::icon_button(icon)
            .key(key)
            .tooltip(tooltip)
            .layout(Layout {
                width: Length::Points(BUTTON),
                height: Length::Points(BUTTON),
                justify: Justify::Center,
                align: Align::Center,
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from("transparent")),
                radius: 5.0,
                hover: Some(Hover {
                    background: Some(Token::from("button.hover.bg")),
                    ..Hover::default()
                }),
                pressed: Some(Hover {
                    background: Some(Token::from("button.pressed.bg")),
                    ..Hover::default()
                }),
                ..Style::default()
            })
    };
    let label = format!("{title}: {}", lines.join(" "));
    let body = Node::container(vec![
        Node::icon(icon).style(Style {
            color: Some(Token::from(tint)),
            ..Style::default()
        }),
        Node::container(texts).layout(Layout {
            direction: Direction::Column,
            gap: 2.0,
            grow: 1.0,
            width: Length::Points(TEXT),
            ..Layout::default()
        }),
        button("copy", "copy", copy),
        button("close", "close", close),
    ])
    .key("card")
    .role(NodeRole::Alert)
    .label(label)
    .layout(Layout {
        direction: Direction::Row,
        gap: GAP,
        align: Align::Start,
        width: Length::Points(CARD),
        padding: Edges {
            top: PADDING,
            right: PADDING,
            bottom: PADDING,
            left: PADDING,
        },
        ..Layout::default()
    })
    .style(Style {
        background: Some(Token::from("panel.bg")),
        border: Some(Border {
            color: Token::from("panel.border"),
            width: 1.0,
        }),
        radius: 10.0,
        shadow: Some(Shadow::default()),
        opacity: Some(if entering { 0.0 } else { 1.0 }),
        transition: Some(Transition::default()),
        hover: Some(Hover::default()),
        ..Style::default()
    });
    // The room above it, which it comes down through as it shows.
    Node::container(vec![body]).key("notice").layout(Layout {
        padding: Edges {
            top: if entering { 0.0 } else { DOWN },
            ..Edges::default()
        },
        ..Layout::default()
    })
}

impl<B: Backend> App<B> {
    /// The card of the notice that shows, as it comes down or in place,
    /// its text in lines of the interface's small text; `None` of none.
    pub(super) fn notice_tree(&mut self) -> Option<Node> {
        let shown = self.diagnostics.shown()?.clone();
        let title = self.texts.text(title_of(&shown));
        let lines = match &mut self.drawing {
            Some(drawing) => {
                drawing.font.set_text_size(TextSize::Small);
                let lines = drawing.font.interface_lines(&shown.text, TEXT, LINES);
                drawing.font.set_text_size(TextSize::Regular);
                lines
            }
            None => vec![shown.text.clone()],
        };
        let fault = shown.category() == ErrorCategory::Fault;
        let (copy, close) = (
            self.texts.text("notice-copy"),
            self.texts.text("notice-close"),
        );
        Some(card(
            (&title, &lines, fault),
            (&copy, &close),
            self.notice.entering,
        ))
    }

    /// Settles the card of `window` to the notice that shows, before the
    /// frame takes its trees: a new notice's card, of a new revision, comes
    /// down from its first frame, its going timed of a failure's; the
    /// window's card opens as one shows and closes as none does.
    pub(super) fn settle_notice(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let shown = self
            .diagnostics
            .shown()
            .map(|shown| (shown.about.clone(), shown.code, shown.text.clone()));
        if shown != self.notice.shown {
            self.notice.shown = shown;
            self.notice.revision += 1;
            self.notice.entering = self.notice.shown.is_some();
            self.notice.hovered = false;
            self.time_notice(platform);
            // Its next frame slides it into place.
            for &other in self.windows.keys() {
                platform.request_frame(other);
            }
        } else if self.notice.entering {
            self.notice.entering = false;
            self.notice.revision += 1;
        }
        let open = self.windows.get(&window).and_then(|state| {
            state
                .chrome
                .iter()
                .find(|view| view.plugin == OVERFLOW && view.view == notice_view())
                .map(|view| view.id)
        });
        match (self.notice.shown.is_some(), open) {
            (true, None) => self.open_notice(platform, window),
            (false, Some(view)) => self.close_panel(platform, window, view, false),
            _ => {}
        }
    }

    /// Opens `window`'s card, over its panes at their top right, of the
    /// pointer that goes to it and no keyboard.
    fn open_notice(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let scale = platform.window_size(window).map_or(1.0, |size| size.scale) as f32;
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        state.next_view += 1;
        let view = state.next_view;
        let access = free_access(state.chrome.iter());
        state.panels.open(
            view,
            Options {
                anchor: Anchor::At((0.0, 0.0)),
                role: Role::Overlay,
                dismiss: Dismiss {
                    outside: false,
                    escape: false,
                },
            },
        );
        let options = PanelOptions {
            anchor: PanelAnchor::WindowCorner {
                window: state.window.into(),
                corner: PanelCorner::TopRight,
            },
            role: PanelRole::Overlay,
            close_outside: false,
            close_escape: false,
            takes_keys: false,
            close_typing: false,
        };
        state.chrome.push(SlotView::panel(
            (view, access),
            (OVERFLOW.to_owned(), PanelId::from(NOTICE)),
            scale,
            PanelPlace {
                options,
                pointer: (0.0, 0.0),
            },
        ));
    }

    /// Starts the timer of the shown notice's going, of a failure's, in
    /// [`SHOWS`]; stops it otherwise.
    fn time_notice(&mut self, platform: &mut dyn Platform) {
        if let Some((timer, _)) = self.notice.timer.take() {
            platform.cancel_timer(timer);
        }
        if self.diagnostics.shown().is_some_and(goes_by_itself) && !self.notice.hovered {
            let at = Instant::now() + SHOWS;
            self.notice.timer = Some((platform.start_timer(at), at));
        }
    }

    /// The time of the shown failure's card came: it goes, unless the
    /// pointer rests on it.
    pub(super) fn notice_timer_fired(&mut self, platform: &mut dyn Platform) {
        self.notice.timer = None;
        if self.notice.hovered || !self.diagnostics.shown().is_some_and(goes_by_itself) {
            return;
        }
        self.diagnostics.dismiss();
        self.request_frames(platform);
    }

    /// An event of the card's nodes: its close closes it, its copy puts the
    /// notice's whole text on the clipboard, of its title, and the pointer
    /// that rests on it holds a failure's going, which starts again as it
    /// leaves.
    pub(super) fn notice_event(&mut self, platform: &mut dyn Platform, event: &UiEvent) {
        match (&event.kind, event.key.0.as_str()) {
            (UiEventKind::Click, "close") => {
                self.diagnostics.dismiss();
                self.request_frames(platform);
            }
            (UiEventKind::Click, "copy") => {
                if let Some(shown) = self.diagnostics.shown() {
                    let title = self.texts.text(title_of(shown));
                    let whole = format!("{title}\n\n{}\n", shown.text);
                    platform.set_clipboard_text(&whole);
                }
            }
            (UiEventKind::HoverEnter, "card") => {
                self.notice.hovered = true;
                if let Some((timer, _)) = self.notice.timer.take() {
                    platform.cancel_timer(timer);
                }
            }
            (UiEventKind::HoverLeave, "card") => {
                self.notice.hovered = false;
                self.time_notice(platform);
            }
            _ => {}
        }
    }
}
