//! Built-in plugin of the commands' blocks: each command that the shell
//! marks, its prompt, its line and its output, is a block. While a pane's
//! view shows a command's output and the command's line is above it, the
//! command's header sits at the pane's top, a pill of the line on a
//! hairline across the pane, which a click scrolls to the command's
//! prompt; while the view does not show the block's end, a button beside
//! the pill scrolls there. The header is `lokyr/command-header`'s, of the
//! module that both hold, which this plugin replaces while it runs. Like
//! every built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{UiEvent, ViewId};
use lokyr_plugin_sdk::view::Node;

// The command's header, of `lokyr/command-header`'s source, as a built-in
// plugin depends on no crate but the SDK.
#[path = "../../command-header/src/header.rs"]
mod header;

use header::Headers;

lokyr_plugin_sdk::export_plugin!(Blocks);

/// The plugin's catalogs, of the header's label and tooltips.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/blocks.ftl")),
    ("it", include_str!("../locales/it/blocks.ftl")),
];

/// The prefix of a header's panel's id, of its pane's id.
const PANEL: &str = "block-header-";

/// The plugin: the panes' headers, of a button to their blocks' ends.
#[derive(Debug)]
pub struct Blocks {
    headers: Headers,
}

impl Default for Blocks {
    fn default() -> Self {
        Self {
            headers: Headers::new(PANEL, true),
        }
    }
}

impl Plugin for Blocks {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.headers.activate(cx)
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn config_schema(&self) -> Option<std::borrow::Cow<'static, str>> {
        Some(include_str!("../blocks.schema.json").into())
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        self.headers.on_event(cx, events)
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        self.headers.render(cx, view)
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        self.headers.on_ui_event(cx, event)
    }
}

#[cfg(test)]
mod tests;
