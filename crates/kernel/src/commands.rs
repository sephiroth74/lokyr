//! The registry of commands: those that the app and the plugins register,
//! and the check of each call, its name and its arguments, before its
//! action runs, where it can run, in the app or in the plugin that
//! registered it. A plugin's call is checked against the commands that its
//! manifest says it runs, and the capabilities that it has.

use std::collections::BTreeMap;

pub use lokyr_plugin_sdk::command::{
    Args, Capability, Category, Code, Command, Error, ErrorCategory, Kind, Param, Uses, Value,
    is_name, is_part,
};

/// The commands, by name.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    commands: BTreeMap<String, Command>,
    /// The commands' names, in the order of their registrations.
    order: Vec<String>,
    /// The plugins that registered commands, by the commands' names; the
    /// app's own commands have none.
    owners: BTreeMap<String, String>,
}

impl Registry {
    /// A registry without commands.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `command`, whose name is `namespace.action` and is not
    /// registered yet, and whose parameters have names of their own and
    /// defaults of their kinds.
    pub fn register(&mut self, command: Command) -> Result<(), Error> {
        let name = &command.name;
        if self.commands.contains_key(name) {
            return Err(Error::new(
                Code::AlreadyRegistered,
                format!("`{name}` is registered already"),
            ));
        }
        command.check()?;
        self.order.push(name.clone());
        self.commands.insert(name.clone(), command);
        Ok(())
    }

    /// Registers `command` of the plugin `plugin`, as
    /// [`Registry::register`] does, which [`Registry::owner`] then tells.
    pub fn register_for(&mut self, command: Command, plugin: &str) -> Result<(), Error> {
        let name = command.name.clone();
        self.register(command)?;
        self.owners.insert(name, plugin.to_owned());
        Ok(())
    }

    /// The plugin that registered the command `name`, by its name; `None`
    /// of the app's own commands.
    pub fn owner(&self, name: &str) -> Option<&str> {
        self.owners.get(name).map(String::as_str)
    }

    /// The command `name`, if it is registered.
    pub fn get(&self, name: &str) -> Option<&Command> {
        self.commands.get(name)
    }

    /// Takes the command `name` away, which a plugin that is disabled
    /// registered.
    pub fn unregister(&mut self, name: &str) -> Option<Command> {
        self.order.retain(|registered| registered != name);
        self.owners.remove(name);
        self.commands.remove(name)
    }

    /// The commands, by name.
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.commands.values()
    }

    /// The commands, in the order of their registrations, as the menus
    /// list them.
    pub fn in_order(&self) -> impl Iterator<Item = &Command> {
        self.order.iter().filter_map(|name| self.commands.get(name))
    }

    /// Checks a call of `name` with `args`, and returns its arguments, with
    /// the defaults of those that it leaves out: an integer for a number
    /// becomes a number.
    pub fn check(&self, name: &str, args: Args) -> Result<Args, Error> {
        let Some(command) = self.commands.get(name) else {
            return Err(Error::new(Code::NotFound, format!("no command `{name}`")));
        };
        command.check_call(args)
    }

    /// Checks a call of `name` with `args` that a plugin makes, which
    /// `uses` says it runs, with the capabilities `granted`, as
    /// [`Registry::check`] does; the commands that the plugin registered,
    /// `own`, need neither, whatever their names.
    pub fn check_for_plugin(
        &self,
        name: &str,
        args: Args,
        own: &[Command],
        uses: &Uses,
        granted: &[Capability],
    ) -> Result<Args, Error> {
        let args = self.check(name, args)?;
        if own.iter().any(|command| command.name == name) {
            return Ok(args);
        }
        if !uses.allows(name) {
            return Err(Error::new(
                Code::Undeclared,
                format!("the plugin runs `{name}`, which its manifest does not declare"),
            ));
        }
        let command = &self.commands[name];
        if let Some(missing) = command
            .capabilities
            .iter()
            .find(|capability| !granted.contains(capability))
        {
            return Err(Error::new(
                Code::MissingCapability,
                format!("`{name}` needs `{missing}`, which the plugin does not have"),
            ));
        }
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(name: &str, params: Vec<Param>) -> Command {
        Command {
            name: name.to_owned(),
            title: name.to_owned(),
            category: Category::Tabs,
            capabilities: Vec::new(),
            params,
            when: None,
        }
    }

    fn param(name: &str, kind: Kind, default: Option<Value>) -> Param {
        Param {
            name: name.to_owned(),
            kind,
            default,
        }
    }

    fn args(values: &[(&str, Value)]) -> Args {
        values
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect()
    }

    /// `tabs.goto { index }`, and `tabs.move { delta = 1, wrap = false,
    /// speed = 1.5, label = "" }`.
    fn registry() -> Registry {
        let mut registry = Registry::new();
        registry
            .register(command(
                "tabs.goto",
                vec![param("index", Kind::Integer, None)],
            ))
            .unwrap();
        registry
            .register(command(
                "tabs.move",
                vec![
                    param("delta", Kind::Integer, Some(Value::Integer(1))),
                    param("wrap", Kind::Boolean, Some(Value::Boolean(false))),
                    param("speed", Kind::Number, Some(Value::Number(1.5))),
                    param("label", Kind::String, Some(Value::String(String::new()))),
                ],
            ))
            .unwrap();
        registry
    }

    fn code(result: Result<Args, Error>) -> u16 {
        result.unwrap_err().code.number()
    }

    #[test]
    fn the_commands_keep_the_order_of_their_registrations() {
        let mut registry = Registry::new();
        for name in ["view.b", "view.a", "view.c"] {
            registry.register(command(name, Vec::new())).unwrap();
        }
        let names = |registry: &Registry| -> Vec<String> {
            registry
                .in_order()
                .map(|command| command.name.clone())
                .collect()
        };
        assert_eq!(names(&registry), ["view.b", "view.a", "view.c"]);
        registry.unregister("view.a");
        registry.register(command("view.a", Vec::new())).unwrap();
        assert_eq!(names(&registry), ["view.b", "view.c", "view.a"]);
        // By name, as before.
        let by_name: Vec<&str> = registry
            .commands()
            .map(|command| command.name.as_str())
            .collect();
        assert_eq!(by_name, ["view.a", "view.b", "view.c"]);
    }

    #[test]
    fn a_command_s_context_is_one() {
        let mut registry = Registry::new();
        let when = |when: &str| Command {
            when: Some(when.to_owned()),
            ..command("view.x", Vec::new())
        };
        let error = registry.register(when("selection &&")).unwrap_err();
        assert_eq!(error.code, Code::InvalidSchema);
        assert!(registry.get("view.x").is_none());
        assert_eq!(registry.in_order().count(), 0);
        registry
            .register(when("selection && !alternate-screen"))
            .unwrap();
    }

    #[test]
    fn a_call_takes_its_arguments_and_the_defaults_of_those_left_out() {
        let registry = registry();
        let index = args(&[("index", Value::Integer(3))]);
        assert_eq!(registry.check("tabs.goto", index.clone()), Ok(index));
        let given = args(&[("delta", Value::Integer(-2)), ("speed", Value::Integer(2))]);
        let full = registry.check("tabs.move", given).unwrap();
        assert_eq!(
            full,
            args(&[
                ("delta", Value::Integer(-2)),
                ("label", Value::String(String::new())),
                // An integer for a number.
                ("speed", Value::Number(2.0)),
                ("wrap", Value::Boolean(false)),
            ])
        );
        let names: Vec<&str> = registry
            .commands()
            .map(|command| command.name.as_str())
            .collect();
        assert_eq!(names, ["tabs.goto", "tabs.move"]);
        assert_eq!(registry.get("tabs.goto").unwrap().params.len(), 1);
    }

    #[test]
    fn a_call_that_does_not_match_is_rejected_by_its_code() {
        let registry = registry();
        assert_eq!(code(registry.check("tabs.gone", Args::new())), 101);
        let extra = args(&[("index", Value::Integer(1)), ("into", Value::Integer(2))]);
        let error = registry.check("tabs.goto", extra).unwrap_err();
        assert_eq!(error.code.number(), 102);
        assert!(error.message.contains("`into`"), "{error}");
        assert_eq!(code(registry.check("tabs.goto", Args::new())), 103);
        let text = args(&[("index", Value::String("one".to_owned()))]);
        let error = registry.check("tabs.goto", text).unwrap_err();
        assert_eq!(error.code.number(), 104);
        assert_eq!(
            error.message,
            "the argument `index` of `tabs.goto` is an integer, not a string"
        );
        // A number for an integer is another kind.
        let number = args(&[("index", Value::Number(1.0))]);
        assert_eq!(code(registry.check("tabs.goto", number)), 104);
    }

    #[test]
    fn a_registration_needs_a_name_a_place_and_a_schema_of_its_own() {
        let mut registry = registry();
        let error = |command| {
            registry
                .clone()
                .register(command)
                .unwrap_err()
                .code
                .number()
        };
        for name in [
            "tabs",
            "Tabs.new",
            "tabs.",
            ".new",
            "tabs.new tab",
            "1tabs.new",
        ] {
            assert_eq!(error(command(name, Vec::new())), 105, "{name}");
        }
        assert_eq!(error(command("tabs.goto", Vec::new())), 106);
        let twice = vec![
            param("index", Kind::Integer, None),
            param("index", Kind::String, None),
        ];
        assert_eq!(error(command("tabs.pick", twice)), 107);
        let default = vec![param("index", Kind::Integer, Some(Value::Boolean(true)))];
        assert_eq!(error(command("tabs.pick", default)), 107);
        let name = vec![param("Index", Kind::Integer, None)];
        assert_eq!(error(command("tabs.pick", name)), 105);
        // Namespaces of several parts, and names with `-`.
        registry
            .register(command("search-plus.open.recent", Vec::new()))
            .unwrap();
    }

    #[test]
    fn a_plugin_runs_the_commands_that_it_declares_with_their_capabilities() {
        let mut registry = registry();
        let mut paste = command("pane.paste", Vec::new());
        paste.capabilities = vec![Capability::ClipboardRead, Capability::PtyWrite];
        registry.register(paste).unwrap();
        for name in ["search.open", "search.other"] {
            registry.register(command(name, Vec::new())).unwrap();
        }
        let goto = || args(&[("index", Value::Integer(1))]);
        let uses = Uses::new(["tabs.*", "pane.paste"]);
        let granted = [Capability::ClipboardRead, Capability::PtyWrite];
        let own = [command("search.open", Vec::new())];
        let check = |name: &str, args, uses: &Uses, granted: &[Capability]| {
            registry.check_for_plugin(name, args, &own, uses, granted)
        };
        assert!(check("tabs.goto", goto(), &uses, &[]).is_ok());
        assert!(check("pane.paste", Args::new(), &uses, &granted).is_ok());
        // Its own commands need no declaration; another of their prefix
        // does.
        assert!(check("search.open", Args::new(), &Uses::default(), &[]).is_ok());
        assert_eq!(
            code(check("search.other", Args::new(), &Uses::default(), &[])),
            301
        );
        // One that it did not declare, and one whose capability it lacks.
        assert_eq!(code(check("tabs.goto", goto(), &Uses::default(), &[])), 301);
        let partly = [Capability::ClipboardRead];
        let error = check("pane.paste", Args::new(), &uses, &partly).unwrap_err();
        assert_eq!(error.code.number(), 302);
        assert!(error.message.contains("pty-write"), "{error}");
        // The call's own errors come first.
        assert_eq!(code(check("tabs.goto", Args::new(), &uses, &[])), 103);
    }

    #[test]
    fn a_namespace_takes_its_own_commands_alone() {
        let uses = Uses::new(["tabs.*", "pane.copy"]);
        assert!(uses.allows("tabs.new"));
        assert!(uses.allows("pane.copy"));
        assert!(!uses.allows("pane.paste"));
        assert!(!uses.allows("tabsx.new"));
        assert!(!uses.allows("tabs"));
        // A pattern without its dot names nothing.
        assert!(!Uses::new(["tabs*"]).allows("tabs.new"));
    }
}
