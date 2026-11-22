//! Built-in plugin of the command's header: the command whose output a
//! pane's view shows, at the pane's top, while the command's line is above
//! the view, as the shell marks its commands. It sits over the pane's first
//! row, a pill of the command's line on a hairline across the pane, and
//! takes none of the grid's rows, so that the grid keeps its size as it
//! shows and hides. A click scrolls the view to the command's prompt. The
//! header is the `header` module's, which `lokyr/blocks` shows too, of a
//! button to its block's end: this plugin is the plain header while that
//! one, which replaces it, is off. Like every built-in plugin, it depends
//! only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{UiEvent, ViewId};
use lokyr_plugin_sdk::view::Node;

mod header;

use header::Headers;

lokyr_plugin_sdk::export_plugin!(Header);

/// The plugin's catalogs, of the header's label and tooltip.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/command-header.ftl")),
    ("it", include_str!("../locales/it/command-header.ftl")),
];

/// The prefix of a header's panel's id, of its pane's id.
const PANEL: &str = "command-header-";

/// The plugin: the panes' headers, of no button to their blocks' ends.
#[derive(Debug)]
pub struct Header(Headers);

impl Default for Header {
    fn default() -> Self {
        Self(Headers::new(PANEL, false))
    }
}

impl Plugin for Header {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.0.activate(cx)
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn config_schema(&self) -> Option<std::borrow::Cow<'static, str>> {
        Some(include_str!("../command-header.schema.json").into())
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        self.0.on_event(cx, events)
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        self.0.render(cx, view)
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        self.0.on_ui_event(cx, event)
    }
}

#[cfg(test)]
mod tests;
