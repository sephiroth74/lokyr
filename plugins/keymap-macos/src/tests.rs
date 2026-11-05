//! The plugin against the SDK's fake host, which records what it does.

use lokyr_plugin_sdk::fake::FakeHost;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/keymap-macos";

/// The config's guide, whose table of the defaults tells the plugin's keys.
const GUIDE: &str = include_str!("../../../docs/CONFIGURATION.md");

#[test]
fn it_binds_its_keys_in_their_order_and_registers_no_command() {
    let mut host = FakeHost::new(NAME);
    host.activate(&mut KeymapMacos).unwrap();
    assert!(host.commands().is_empty());
    let (plain, windows) = host.bindings().split_at(BINDINGS.len());
    let bound: Vec<(&str, &str)> = plain
        .iter()
        .map(|binding| {
            assert_eq!((binding.args.len(), &binding.when), (0, &None));
            (binding.keys.as_str(), binding.command.as_str())
        })
        .collect();
    assert_eq!(bound, BINDINGS);
    // Then the windows', each of its number.
    let numbered: Vec<(&str, &str, Option<&Value>)> = windows
        .iter()
        .map(|binding| {
            (
                binding.keys.as_str(),
                binding.command.as_str(),
                binding.args.get("index"),
            )
        })
        .collect();
    let expected: Vec<(&str, &str, Option<&Value>)> = WINDOWS
        .iter()
        .zip(&INDICES)
        .map(|(keys, index)| (*keys, "window.goto", Some(index)))
        .collect();
    assert_eq!(numbered, expected);
}

/// The windows' numbers, from 1 to 9.
const INDICES: [Value; 9] = [
    Value::Integer(1),
    Value::Integer(2),
    Value::Integer(3),
    Value::Integer(4),
    Value::Integer(5),
    Value::Integer(6),
    Value::Integer(7),
    Value::Integer(8),
    Value::Integer(9),
];

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
            WINDOWS
                .iter()
                .map(|keys| ((*keys).to_owned(), "window.goto".to_owned())),
        )
        .collect();
    told.sort();
    bound.sort();
    assert_eq!(told, bound);
}
