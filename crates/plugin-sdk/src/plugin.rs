//! The plugins' side: the trait that a plugin implements, and the context
//! through which its callbacks reach the host.
//!
//! ```
//! use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error};
//! use lokyr_plugin_sdk::host::Binding;
//! use lokyr_plugin_sdk::plugin::{Cx, Plugin};
//!
//! #[derive(Default)]
//! struct Copying;
//!
//! impl Plugin for Copying {
//!     fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
//!         cx.register_command(Command {
//!             name: "selection.copy".to_owned(),
//!             title: "Copy".to_owned(),
//!             category: Category::Clipboard,
//!             capabilities: Vec::new(),
//!             params: Vec::new(),
//!             when: Some("selection".to_owned()),
//!         })?;
//!         cx.bind(Binding {
//!             keys: "cmd+c".to_owned(),
//!             command: "selection.copy".to_owned(),
//!             args: Args::new(),
//!             when: Some("selection".to_owned()),
//!         })
//!     }
//!
//!     fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _args: Args) -> Result<(), Error> {
//!         match name {
//!             "selection.copy" => {
//!                 let Some(pane) = cx.focused_pane() else {
//!                     return Ok(());
//!                 };
//!                 if let Some(text) = cx.selection_text(pane)? {
//!                     cx.set_clipboard_text(&text)?;
//!                 }
//!                 Ok(())
//!             }
//!             _ => Err(Error::new(Code::NotFound, format!("no command `{name}`"))),
//!         }
//!     }
//! }
//! ```

use std::ops::{Deref, DerefMut};

use crate::command::{Args, Code, Error};
use crate::host::HostApi;

/// A plugin: its activation, which registers its commands and binds its
/// keys, and the calls of its commands, which the host checked against
/// their parameters, with their defaults.
///
/// The host calls it on the main thread, and never while it runs: what its
/// calls ask of the app comes once they return. A call that panics
/// disables the plugin, and takes its commands and its bindings away.
pub trait Plugin {
    /// Starts the plugin.
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let _ = cx;
        Ok(())
    }

    /// Runs `name`, one of the commands that it registered, with `args`.
    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        let _ = (cx, args);
        Err(Error::new(
            Code::NotFound,
            format!("the plugin has no command `{name}`"),
        ))
    }
}

/// What a plugin's callbacks reach the host through: the host's API.
pub struct Cx<'a> {
    host: &'a mut dyn HostApi,
}

impl<'a> Cx<'a> {
    /// The context of a callback, on `host`.
    pub fn new(host: &'a mut dyn HostApi) -> Self {
        Self { host }
    }
}

impl<'a> Deref for Cx<'a> {
    type Target = dyn HostApi + 'a;

    fn deref(&self) -> &Self::Target {
        self.host
    }
}

impl DerefMut for Cx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.host
    }
}
