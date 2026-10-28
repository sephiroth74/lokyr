//! The plugin against a host of the tests', which records what it asks.

use lokyr_plugin_sdk::host::{HostApi, PaneId, Scroll, WindowId};

use super::*;

/// A host that records the plugin's calls, with a pane, a selection and a
/// clipboard of the test's own.
#[derive(Default)]
struct Host {
    pane: Option<PaneId>,
    selection: Option<String>,
    clipboard: Option<String>,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
    calls: Vec<String>,
}

impl HostApi for Host {
    fn register_command(&mut self, command: Command) -> Result<(), Error> {
        self.commands.push(command);
        Ok(())
    }

    fn run_command(&mut self, name: &str, _: Args) -> Result<(), Error> {
        self.calls.push(format!("run {name}"));
        Ok(())
    }

    fn bind(&mut self, binding: Binding) -> Result<(), Error> {
        self.bindings.push(binding);
        Ok(())
    }

    fn clipboard_text(&mut self) -> Result<Option<String>, Error> {
        Ok(self.clipboard.clone())
    }

    fn set_clipboard_text(&mut self, text: &str) -> Result<(), Error> {
        self.clipboard = Some(text.to_owned());
        Ok(())
    }

    fn focused_window(&self) -> Option<WindowId> {
        self.pane.map(|_| WindowId(1))
    }

    fn focused_pane(&self) -> Option<PaneId> {
        self.pane
    }

    fn selection_text(&mut self, _: PaneId) -> Result<Option<String>, Error> {
        Ok(self.selection.clone())
    }

    fn select_all(&mut self, pane: PaneId) -> Result<(), Error> {
        self.calls.push(format!("select all {}", pane.0));
        Ok(())
    }

    fn clear_scrollback(&mut self, pane: PaneId) -> Result<(), Error> {
        self.calls.push(format!("clear scrollback {}", pane.0));
        Ok(())
    }

    fn clear(&mut self, pane: PaneId) -> Result<(), Error> {
        self.calls.push(format!("clear {}", pane.0));
        Ok(())
    }

    fn paste(&mut self, pane: PaneId, text: &str) -> Result<(), Error> {
        self.calls.push(format!("paste {} {text:?}", pane.0));
        Ok(())
    }

    fn scroll(&mut self, _: PaneId, _: Scroll) -> Result<(), Error> {
        Ok(())
    }

    fn font_size(&self) -> f32 {
        13.0
    }

    fn set_font_size(&mut self, _: Option<f32>) -> Result<(), Error> {
        Ok(())
    }

    fn open_window(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn close_window(&mut self, _: WindowId) -> Result<(), Error> {
        Ok(())
    }

    fn quit(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

/// Runs `name` on `host`.
fn run(host: &mut Host, name: &str) -> Result<(), Error> {
    CoreActions.on_command(&mut Cx::new(host), name, Args::new())
}

#[test]
fn the_commands_register_with_their_keys() {
    let mut host = Host::default();
    CoreActions.activate(&mut Cx::new(&mut host)).unwrap();
    let registered: Vec<(&str, Category, Vec<String>)> = host
        .commands
        .iter()
        .map(|command| {
            assert!(command.params.is_empty());
            let capabilities = command.capabilities.iter().map(ToString::to_string);
            (
                command.name.as_str(),
                command.category,
                capabilities.collect(),
            )
        })
        .collect();
    let names = |names: &[&str]| -> Vec<String> { names.iter().map(|&n| n.to_owned()).collect() };
    assert_eq!(
        registered,
        [
            (
                "pane.copy",
                Category::Clipboard,
                names(&["terminal-read", "clipboard-write"])
            ),
            (
                "pane.paste",
                Category::Clipboard,
                names(&["clipboard-read", "pty-write"])
            ),
            ("pane.select-all", Category::View, Vec::new()),
            ("pane.clear", Category::View, Vec::new()),
        ]
    );
    let bound: Vec<(&str, &str)> = host
        .bindings
        .iter()
        .map(|binding| {
            assert_eq!((binding.args.len(), &binding.when), (0, &None));
            (binding.keys.as_str(), binding.command.as_str())
        })
        .collect();
    assert_eq!(
        bound,
        [
            ("cmd+c", "pane.copy"),
            ("cmd+v", "pane.paste"),
            ("cmd+a", "pane.select-all"),
            ("cmd+k", "pane.clear"),
        ]
    );
}

#[test]
fn copy_and_paste_go_through_the_clipboard() {
    let mut host = Host {
        pane: Some(PaneId(7)),
        selection: Some("ls -la".to_owned()),
        ..Host::default()
    };
    run(&mut host, COPY).unwrap();
    assert_eq!(host.clipboard.as_deref(), Some("ls -la"));
    run(&mut host, PASTE).unwrap();
    assert_eq!(host.calls, ["paste 7 \"ls -la\""]);
    // Without a selection, or with an empty one, the clipboard keeps its
    // text; an empty clipboard pastes nothing.
    for selection in [None, Some(String::new())] {
        host.selection = selection;
        run(&mut host, COPY).unwrap();
        assert_eq!(host.clipboard.as_deref(), Some("ls -la"));
    }
    host.calls.clear();
    for clipboard in [None, Some(String::new())] {
        host.clipboard = clipboard;
        run(&mut host, PASTE).unwrap();
    }
    assert!(host.calls.is_empty());
}

#[test]
fn select_all_and_clear_act_on_the_focused_pane() {
    let mut host = Host {
        pane: Some(PaneId(3)),
        ..Host::default()
    };
    run(&mut host, SELECT_ALL).unwrap();
    run(&mut host, CLEAR).unwrap();
    assert_eq!(host.calls, ["select all 3", "clear 3"]);
    // Without a focused pane, nothing.
    let mut host = Host::default();
    for name in [COPY, PASTE, SELECT_ALL, CLEAR] {
        run(&mut host, name).unwrap();
    }
    assert!(host.calls.is_empty() && host.clipboard.is_none());
    let mut host = Host {
        pane: Some(PaneId(3)),
        ..Host::default()
    };
    assert_eq!(
        run(&mut host, "pane.other").unwrap_err().code,
        Code::NotFound
    );
}
