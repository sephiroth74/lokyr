//! The plugin against the SDK's fake host, which records what it does.

use lokyr_plugin_sdk::fake::FakeHost;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/keymap-macos";

/// The config's guide, whose table of the defaults tells the plugin's keys.
const GUIDE: &str = include_str!("../../../docs/CONFIGURATION.md");

#[test]
fn it_binds_its_keys_in_their_order_and_registers_no_command() {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.activate(&mut KeymapMacos).unwrap();
    assert!(host.commands().is_empty());
    let (plain, rest) = host.bindings().split_at(BINDINGS.len());
    let (windows, rest) = rest.split_at(ARGUED.len());
    let (directed, rest) = rest.split_at(DIRECTED.len());
    let (in_panels, rest) = rest.split_at(IN_PANELS.len());
    let (copy_mode, counts) = rest.split_at(COPY_MODE.len());
    let bound: Vec<(&str, &str)> = plain
        .iter()
        .map(|binding| {
            assert_eq!((binding.args.len(), &binding.when), (0, &None));
            (binding.keys.as_str(), binding.command.as_str())
        })
        .collect();
    assert_eq!(bound, BINDINGS);
    // Then those of an argument.
    let argued: Vec<(&str, &str, &str, i64)> = windows
        .iter()
        .map(|binding| {
            let [(name, Value::Integer(value))] = binding.args.iter().collect::<Vec<_>>()[..]
            else {
                panic!("one argument, a whole number: {binding:?}");
            };
            (
                binding.keys.as_str(),
                binding.command.as_str(),
                name.as_str(),
                *value,
            )
        })
        .collect();
    assert_eq!(argued, ARGUED);
    // Then those of a direction.
    let pointed: Vec<(&str, &str, &str)> = directed
        .iter()
        .map(|binding| {
            let [(name, Value::String(value))] = binding.args.iter().collect::<Vec<_>>()[..] else {
                panic!("one argument, a text: {binding:?}");
            };
            assert_eq!(name, "direction");
            (
                binding.keys.as_str(),
                binding.command.as_str(),
                value.as_str(),
            )
        })
        .collect();
    assert_eq!(pointed, DIRECTED);
    // Then those of a panel's context.
    let panels: Vec<(&str, &str, &str, i64, &str)> = in_panels
        .iter()
        .map(|binding| {
            let [(name, Value::Integer(value))] = binding.args.iter().collect::<Vec<_>>()[..]
            else {
                panic!("one argument, a whole number: {binding:?}");
            };
            (
                binding.keys.as_str(),
                binding.command.as_str(),
                name.as_str(),
                *value,
                binding.when.as_deref().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(panels, IN_PANELS);
    // Then the copy mode's, of its context.
    let copying: Vec<(&str, &str, &str, &str)> = copy_mode
        .iter()
        .map(|binding| {
            let [(name, Value::String(value))] = binding.args.iter().collect::<Vec<_>>()[..] else {
                panic!("one argument, a text: {binding:?}");
            };
            assert_eq!(binding.when.as_deref(), Some(IN_COPY_MODE));
            (
                binding.keys.as_str(),
                binding.command.as_str(),
                name.as_str(),
                value.as_str(),
            )
        })
        .collect();
    assert_eq!(copying, COPY_MODE);
    let digits: Vec<(&str, i64)> = counts
        .iter()
        .map(|binding| {
            assert_eq!(
                (binding.command.as_str(), binding.when.as_deref()),
                ("copy-mode.count", Some(IN_COPY_MODE))
            );
            let [(name, Value::Integer(value))] = binding.args.iter().collect::<Vec<_>>()[..]
            else {
                panic!("one argument, a whole number: {binding:?}");
            };
            assert_eq!(name, "digit");
            (binding.keys.as_str(), *value)
        })
        .collect();
    assert_eq!(digits, COPY_MODE_COUNTS);
}

#[test]
fn the_guide_s_table_of_the_defaults_is_the_plugin_s_keys() {
    let section = GUIDE
        .split_once("\n## `[keys]`\n")
        .and_then(|(_, rest)| {
            rest.split_once("\nThe defaults, of the built-in plugin `keymap-macos`")
        })
        .and_then(|(_, rest)| rest.split_once("\n| Keys | Command | What it does |\n"))
        .map(|(_, rest)| rest)
        .expect("the guide's `[keys]` has its defaults");
    let quoted = |cell: &str| -> Vec<String> {
        cell.split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect()
    };
    let mut told = Vec::new();
    for row in section
        .lines()
        .take_while(|line| line.starts_with('|'))
        .skip(1)
    {
        let cells: Vec<&str> = row.split(" | ").collect();
        let [keys, command, ..] = cells[..] else {
            panic!("a row of the keys, the command and what it does: {row}");
        };
        let command = quoted(command);
        assert_eq!(command.len(), 1, "a row of one command: {row}");
        for keys in quoted(keys.trim_start_matches("| ")) {
            told.push((keys, command[0].clone()));
        }
    }
    let mut bound: Vec<(String, String)> = BINDINGS
        .iter()
        .map(|&(keys, command)| (keys.to_owned(), command.to_owned()))
        .chain(
            ARGUED
                .iter()
                .map(|&(keys, command, ..)| (keys.to_owned(), command.to_owned())),
        )
        .chain(
            DIRECTED
                .iter()
                .map(|&(keys, command, _)| (keys.to_owned(), command.to_owned())),
        )
        .chain(
            IN_PANELS
                .iter()
                .map(|&(keys, command, ..)| (keys.to_owned(), command.to_owned())),
        )
        .chain(
            COPY_MODE
                .iter()
                .map(|&(keys, command, ..)| (keys.to_owned(), command.to_owned())),
        )
        .chain(
            COPY_MODE_COUNTS
                .iter()
                .map(|&(keys, _)| (keys.to_owned(), "copy-mode.count".to_owned())),
        )
        .collect();
    told.sort();
    bound.sort();
    assert_eq!(told, bound);
}
