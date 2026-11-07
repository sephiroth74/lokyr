//! The quick terminal's place, as the config gives it to the platform, and
//! the keys that the app takes from every app, as the platform takes them.

use lokyr_kernel::config::{Extent, QuickPosition, QuickScreen, QuickSpaces, QuickTerminalConfig};
use lokyr_kernel::keymap::{Combo, Named, Press};
use lokyr_platform_core::{GlobalKey, KeyCode, Length, QuickEdge, QuickPlace};

/// The place of the quick terminal that `config` gives.
pub(crate) fn place(config: &QuickTerminalConfig) -> QuickPlace {
    let length = |extent: Extent| match extent {
        Extent::Percent(percent) => Length::Percent(f64::from(percent)),
        Extent::Points(points) => Length::Points(f64::from(points)),
    };
    QuickPlace {
        edge: match config.position {
            QuickPosition::Top => QuickEdge::Top,
            QuickPosition::Bottom => QuickEdge::Bottom,
            QuickPosition::Left => QuickEdge::Left,
            QuickPosition::Right => QuickEdge::Right,
            QuickPosition::Center => QuickEdge::Center,
        },
        primary: length(config.size.primary),
        secondary: config.size.secondary.map(length),
        screen: match config.screen {
            QuickScreen::Main => lokyr_platform_core::QuickScreen::Main,
            QuickScreen::Mouse => lokyr_platform_core::QuickScreen::Pointer,
            QuickScreen::MenuBar => lokyr_platform_core::QuickScreen::MenuBar,
        },
        animation: config.animation.duration(),
        every_space: config.spaces == QuickSpaces::Move,
    }
}

/// The platform's place of a quick window of a plugin's `place`.
pub(crate) fn host_place(place: &lokyr_kernel::plugins::host::QuickPlace) -> QuickPlace {
    use lokyr_kernel::plugins::host::{QuickEdge as Edge, QuickLength, QuickScreen as Screen};
    let length = |length: QuickLength| match length {
        QuickLength::Percent(percent) => Length::Percent(f64::from(percent)),
        QuickLength::Points(points) => Length::Points(f64::from(points)),
    };
    QuickPlace {
        edge: match place.edge {
            Edge::Top => QuickEdge::Top,
            Edge::Bottom => QuickEdge::Bottom,
            Edge::Left => QuickEdge::Left,
            Edge::Right => QuickEdge::Right,
            Edge::Center => QuickEdge::Center,
        },
        primary: length(place.primary),
        secondary: place.secondary.map(length),
        screen: match place.screen {
            Screen::Main => lokyr_platform_core::QuickScreen::Main,
            Screen::Pointer => lokyr_platform_core::QuickScreen::Pointer,
            Screen::MenuBar => lokyr_platform_core::QuickScreen::MenuBar,
        },
        animation: std::time::Duration::from_millis(u64::from(place.animation)),
        every_space: place.every_space,
    }
}

/// The key of `combo`, a global binding's, by its place on the US layout;
/// `None` for a character that no key of that layout types.
pub(crate) fn global_key(combo: Combo) -> Option<GlobalKey> {
    let code = match combo.press {
        Press::Char(character) => KeyCode::from_base_layout(character)?,
        Press::Named(named) => match named {
            Named::Enter => KeyCode::Enter,
            Named::Tab => KeyCode::Tab,
            Named::Backspace => KeyCode::Backspace,
            Named::Escape => KeyCode::Escape,
            Named::Space => KeyCode::Space,
            Named::Delete => KeyCode::Delete,
            Named::Insert => KeyCode::Insert,
            Named::Home => KeyCode::Home,
            Named::End => KeyCode::End,
            Named::PageUp => KeyCode::PageUp,
            Named::PageDown => KeyCode::PageDown,
            Named::Up => KeyCode::ArrowUp,
            Named::Down => KeyCode::ArrowDown,
            Named::Left => KeyCode::ArrowLeft,
            Named::Right => KeyCode::ArrowRight,
            Named::F(number) => KeyCode::F(number),
        },
        Press::Wheel(_) | Press::Click => return None,
    };
    Some(GlobalKey {
        code,
        command: combo.mods.cmd,
        control: combo.mods.ctrl,
        alt: combo.mods.alt,
        shift: combo.mods.shift,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lokyr_kernel::config::Config;

    use super::*;

    #[test]
    fn the_config_gives_the_quick_terminal_s_place() {
        let config = Config::parse(
            "[quick-terminal]\nposition = \"right\"\nsize = \"30%,400px\"\nscreen = \"mouse\"\nanimation = 0\nspaces = \"remain\"\n",
            std::path::Path::new("config.toml"),
        )
        .unwrap();
        assert_eq!(
            place(&config.quick_terminal),
            QuickPlace {
                edge: QuickEdge::Right,
                primary: Length::Percent(30.0),
                secondary: Some(Length::Points(400.0)),
                screen: lokyr_platform_core::QuickScreen::Pointer,
                animation: Duration::ZERO,
                every_space: false,
            }
        );
        assert_eq!(
            place(&QuickTerminalConfig::default()),
            QuickPlace::default()
        );
    }

    #[test]
    fn a_global_key_goes_by_its_place_on_the_us_layout() {
        let key = |text: &str| global_key(Combo::parse(text).unwrap());
        assert_eq!(
            key("ctrl+`"),
            Some(GlobalKey {
                code: KeyCode::Backquote,
                command: false,
                control: true,
                alt: false,
                shift: false,
            })
        );
        assert_eq!(
            key("cmd+shift+f12").map(|key| key.code),
            Some(KeyCode::F(12))
        );
        assert_eq!(key("alt+space").map(|key| key.code), Some(KeyCode::Space));
        // A character of another layout's has no place of the US one's.
        assert_eq!(key("ctrl+è"), None);
    }
}
