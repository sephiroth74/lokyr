//! Built-in plugin of the quick terminal: a window that a global key slides
//! in from an edge of the screen, over every app's windows, the ones in full
//! screen too, and out again, a terminal at hand from anywhere.
//!
//! Its command shows the quick terminal, opening it first, and takes the
//! focus; again, with the focus, it hides it, and the app that had the focus
//! takes it back. Hidden, it keeps its shell, as a window does. It hides as
//! it loses the focus, as its config's `autohide` says. Its place, its size
//! and its slide are its config's, `[plugins.quick-terminal]`. Like every
//! built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::command::{Args, Capability, Category, Code, Command, Error};
use lokyr_plugin_sdk::host::{
    QuickEdge, QuickLength, QuickPlace, QuickScreen, WindowId, WindowInfo,
};
use lokyr_plugin_sdk::message::{Data, Event, WINDOW_FOCUS};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

lokyr_plugin_sdk::export_plugin!(QuickTerminal);

/// Shows the quick terminal, or hides it.
const TOGGLE: &str = "window.toggle-quick-terminal";

static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/quick-terminal.ftl")),
    ("it", include_str!("../locales/it/quick-terminal.ftl")),
];

/// The plugin of the quick terminal: its config, and its window, once it
/// opened.
#[derive(Debug, Default)]
pub struct QuickTerminal {
    settings: Settings,
    window: Option<WindowId>,
}

/// Its config, as `[plugins.quick-terminal]` gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Settings {
    /// Where it shows, and how it slides.
    place: QuickPlace,
    /// Whether it hides as it loses the focus.
    autohide: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            place: QuickPlace::default(),
            autohide: true,
        }
    }
}

impl Settings {
    /// The settings of `config`, its section, which the host checked
    /// against the schema and gave its defaults; a size that does not read
    /// leaves the default's, and its error.
    fn of(config: &Data) -> (Self, Option<Error>) {
        let mut settings = Self::default();
        let Data::Map(config) = config else {
            return (settings, None);
        };
        let text = |name: &str| match config.get(name) {
            Some(Data::Text(text)) => Some(text.as_str()),
            _ => None,
        };
        let place = &mut settings.place;
        place.edge = match text("position") {
            Some("bottom") => QuickEdge::Bottom,
            Some("left") => QuickEdge::Left,
            Some("right") => QuickEdge::Right,
            Some("center") => QuickEdge::Center,
            _ => QuickEdge::Top,
        };
        place.screen = match text("screen") {
            Some("mouse") => QuickScreen::Pointer,
            Some("menu-bar") => QuickScreen::MenuBar,
            _ => QuickScreen::Main,
        };
        if let Some(Data::Integer(millis)) = config.get("animation") {
            place.animation = u32::try_from(*millis).unwrap_or(place.animation);
        }
        place.every_space = text("spaces") != Some("remain");
        if let Some(Data::Boolean(autohide)) = config.get("autohide") {
            settings.autohide = *autohide;
        }
        let mut error = None;
        if let Some(size) = text("size") {
            match size_of(size) {
                Some((primary, secondary)) => {
                    settings.place.primary = primary;
                    settings.place.secondary = secondary;
                }
                None => {
                    error = Some(Error::new(
                        Code::Failed,
                        format!(
                            "`{size}` is no size of the quick terminal: a percentage from 1% to 100%, or points, as `300px`, and another after a comma, along its edge; it takes 50%"
                        ),
                    ));
                }
            }
        }
        (settings, error)
    }
}

/// The lengths of the size `text`: the first, into the screen, and the
/// second, along its edge, after a comma, if it has one; `None` for a text
/// that is no size.
fn size_of(text: &str) -> Option<(QuickLength, Option<QuickLength>)> {
    let (primary, secondary) = match text.split_once(',') {
        Some((primary, secondary)) => (primary, Some(secondary)),
        None => (text, None),
    };
    let primary = length_of(primary)?;
    let secondary = match secondary {
        Some(secondary) => Some(length_of(secondary)?),
        None => None,
    };
    Some((primary, secondary))
}

/// The length of `text`: a percentage from 1 to 100, as `50%`, or points,
/// as `300px`, from 1 to 100 000.
fn length_of(text: &str) -> Option<QuickLength> {
    let text = text.trim();
    if let Some(percent) = text.strip_suffix('%') {
        let percent: f32 = percent.trim().parse().ok()?;
        (percent > 0.0 && percent <= 100.0).then_some(QuickLength::Percent(percent))
    } else {
        let points: f32 = text.strip_suffix("px")?.trim().parse().ok()?;
        (1.0..=100_000.0)
            .contains(&points)
            .then_some(QuickLength::Points(points))
    }
}

impl Plugin for QuickTerminal {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: TOGGLE.to_owned(),
            title: "Quick Terminal".to_owned(),
            category: Category::Windows,
            capabilities: vec![Capability::GlobalHotkey],
            params: Vec::new(),
            when: None,
        })?;
        cx.subscribe(&[WINDOW_FOCUS])?;
        // A size that does not read leaves the default's, and the plugin
        // starts all the same.
        match self.configure(cx) {
            Err(error) if error.code != Code::Failed => Err(error),
            _ => Ok(()),
        }
    }

    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &CATALOGS
    }

    fn config_schema(&self) -> Option<&'static str> {
        Some(include_str!("../quick-terminal.schema.json"))
    }

    /// A change of its config takes it; the quick terminal that loses the
    /// focus hides, as `autohide` says.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            match event {
                Event::ConfigChanged => self.configure(cx)?,
                Event::WindowFocus {
                    window,
                    focused: false,
                } if self.window == Some(*window) && self.settings.autohide => {
                    self.hide(cx)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Shows the quick terminal, at its config's place, opening it first,
    /// in the folder of a new window's; or hides it, if it shows with the
    /// focus. Shown without the focus, it takes the focus.
    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        if name != TOGGLE {
            return Err(Error::new(
                Code::NotFound,
                format!("quick-terminal has no command `{name}`"),
            ));
        }
        let place = self.settings.place;
        match self.shown(cx)? {
            None => {
                self.window = Some(cx.open_quick_window(place, None)?);
                Ok(())
            }
            Some(info) if !info.hidden && cx.focused_window() == Some(info.id) => {
                cx.show_quick_window(info.id, None)
            }
            Some(info) => cx.show_quick_window(info.id, Some(place)),
        }
    }
}

impl QuickTerminal {
    /// Takes its section of the config; a size that does not read takes
    /// the default's, and tells why.
    fn configure(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let (settings, error) = Settings::of(&cx.config()?);
        self.settings = settings;
        error.map_or(Ok(()), Err)
    }

    /// Hides its quick window, if it shows.
    fn hide(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        match self.shown(cx)? {
            Some(info) if !info.hidden => cx.show_quick_window(info.id, None),
            _ => Ok(()),
        }
    }

    /// Its quick window, as the host has it, if it is open: once it closed,
    /// as its shell ended, the next command opens another.
    fn shown(&mut self, cx: &mut Cx<'_>) -> Result<Option<WindowInfo>, Error> {
        let Some(window) = self.window else {
            return Ok(None);
        };
        let found = cx
            .windows()?
            .into_iter()
            .find(|info| info.id == window && info.quick);
        if found.is_none() {
            self.window = None;
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests;
