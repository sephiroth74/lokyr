//! Built-in plugin with the basic terminal actions.
//!
//! Copy and paste, select all, clearing the scrollback, scrolling, font size,
//! and opening and closing windows. Like every built-in plugin, it depends
//! only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::command::{
    Args, Capability, Category, Code, Command, Error, Kind, Param, Value,
};
use lokyr_plugin_sdk::host::{Binding, Scroll};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

/// Copies the focused pane's selection to the clipboard.
const COPY: &str = "pane.copy";

/// Pastes the clipboard's text into the focused pane.
const PASTE: &str = "pane.paste";

/// Selects the focused pane's scrollback and screen.
const SELECT_ALL: &str = "pane.select-all";

/// Clears the focused pane's scrollback and screen, but its prompt.
const CLEAR: &str = "pane.clear";

/// Makes the text a point larger, in every window.
const FONT_BIGGER: &str = "font.bigger";

/// Makes the text a point smaller.
const FONT_SMALLER: &str = "font.smaller";

/// Gives the text the config's size again.
const FONT_RESET: &str = "font.reset";

/// Scroll the focused pane's view through its scrollback.
const SCROLL_LINE_UP: &str = "pane.scroll-line-up";
const SCROLL_LINE_DOWN: &str = "pane.scroll-line-down";
const SCROLL_PAGE_UP: &str = "pane.scroll-page-up";
const SCROLL_PAGE_DOWN: &str = "pane.scroll-page-down";
const SCROLL_TOP: &str = "pane.scroll-top";
const SCROLL_BOTTOM: &str = "pane.scroll-bottom";

/// Sends its `text` to the focused pane's program, as it is: for keys that
/// the config binds, as those of a shell's line editing.
const SEND_TEXT: &str = "pane.send-text";

/// The sizes of the text, in points, that the commands go between.
const SIZES: (f32, f32) = (4.0, 200.0);

/// A command: its name, title, category, capabilities and keys.
type Spec = (
    &'static str,
    &'static str,
    Category,
    &'static [Capability],
    &'static [&'static str],
);

/// The commands.
const COMMANDS: [Spec; 13] = [
    (
        COPY,
        "Copy",
        Category::Clipboard,
        &[Capability::TerminalRead, Capability::ClipboardWrite],
        &["cmd+c"],
    ),
    (
        PASTE,
        "Paste",
        Category::Clipboard,
        &[Capability::ClipboardRead, Capability::PtyWrite],
        &["cmd+v"],
    ),
    (SELECT_ALL, "Select All", Category::Edit, &[], &["cmd+a"]),
    (CLEAR, "Clear", Category::Edit, &[], &["cmd+k"]),
    // ⌘+ on a layout of its own `+` key, as the Italian, and with Shift on
    // the US layout's `=`.
    (
        FONT_BIGGER,
        "Bigger",
        Category::View,
        &[],
        &["cmd+=", "cmd+shift+=", "cmd++"],
    ),
    (FONT_SMALLER, "Smaller", Category::View, &[], &["cmd+-"]),
    (FONT_RESET, "Default Size", Category::View, &[], &["cmd+0"]),
    (
        SCROLL_LINE_UP,
        "Scroll Up a Line",
        Category::View,
        &[],
        &["cmd+up"],
    ),
    (
        SCROLL_LINE_DOWN,
        "Scroll Down a Line",
        Category::View,
        &[],
        &["cmd+down"],
    ),
    (
        SCROLL_PAGE_UP,
        "Scroll Up a Page",
        Category::View,
        &[],
        &["cmd+pageup", "shift+pageup"],
    ),
    (
        SCROLL_PAGE_DOWN,
        "Scroll Down a Page",
        Category::View,
        &[],
        &["cmd+pagedown", "shift+pagedown"],
    ),
    (
        SCROLL_TOP,
        "Scroll to the Top",
        Category::View,
        &[],
        &["cmd+home"],
    ),
    (
        SCROLL_BOTTOM,
        "Scroll to the Bottom",
        Category::View,
        &[],
        &["cmd+end"],
    ),
];

/// The plugin of the basic terminal actions.
#[derive(Debug, Default)]
pub struct CoreActions;

impl Plugin for CoreActions {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for (name, title, category, capabilities, keys) in COMMANDS {
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category,
                capabilities: capabilities.to_vec(),
                params: Vec::new(),
                // Copying takes a selection.
                when: (name == COPY).then(|| "selection".to_owned()),
            })?;
            for keys in keys {
                cx.bind(Binding {
                    keys: (*keys).to_owned(),
                    command: name.to_owned(),
                    args: Args::new(),
                    when: None,
                })?;
            }
        }
        cx.register_command(Command {
            name: SEND_TEXT.to_owned(),
            title: "Send Text".to_owned(),
            category: Category::Input,
            capabilities: vec![Capability::PtyWrite],
            params: vec![Param {
                name: "text".to_owned(),
                kind: Kind::String,
                default: None,
            }],
            when: None,
        })
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        // The font's size is every window's; it stays within its sizes.
        let (smallest, largest) = SIZES;
        let size = cx.font_size();
        match name {
            FONT_BIGGER if size < largest => {
                return cx.set_font_size(Some((size.floor() + 1.0).min(largest)));
            }
            FONT_SMALLER if size > smallest => {
                return cx.set_font_size(Some((size.ceil() - 1.0).max(smallest)));
            }
            FONT_RESET => return cx.set_font_size(None),
            FONT_BIGGER | FONT_SMALLER => return Ok(()),
            _ => {}
        }
        // The others act on the focused pane; without one, there is nothing
        // to do.
        let Some(pane) = cx.focused_pane() else {
            return Ok(());
        };
        let scroll = match name {
            SCROLL_LINE_UP => Some(Scroll::Up(1)),
            SCROLL_LINE_DOWN => Some(Scroll::Down(1)),
            SCROLL_PAGE_UP => Some(Scroll::PageUp),
            SCROLL_PAGE_DOWN => Some(Scroll::PageDown),
            SCROLL_TOP => Some(Scroll::Top),
            SCROLL_BOTTOM => Some(Scroll::Bottom),
            _ => None,
        };
        if let Some(scroll) = scroll {
            return cx.scroll(pane, scroll);
        }
        if name == SEND_TEXT {
            // The registry checked that the text is there, a string.
            return match args.get("text") {
                Some(Value::String(text)) => cx.send_text(pane, text),
                _ => Ok(()),
            };
        }
        match name {
            COPY => {
                if let Some(text) = cx.selection_text(pane)?
                    && !text.is_empty()
                {
                    cx.set_clipboard_text(&text)?;
                }
                Ok(())
            }
            PASTE => {
                if let Some(text) = cx.clipboard_text()?
                    && !text.is_empty()
                {
                    cx.paste(pane, &text)?;
                }
                Ok(())
            }
            SELECT_ALL => cx.select_all(pane),
            CLEAR => cx.clear(pane),
            _ => Err(Error::new(
                Code::NotFound,
                format!("core-actions has no command `{name}`"),
            )),
        }
    }
}

#[cfg(test)]
mod tests;
