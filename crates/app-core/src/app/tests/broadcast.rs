//! A window's broadcast of the typed input from the menus: the Shell menu's
//! Broadcast Input and its keys, ticked as the window in front broadcasts;
//! a pane's Leave Broadcast and Join Broadcast; and what accessibility
//! calls a pane that takes it.

use accesskit::Role;
use lokyr_kernel::plugins::host::BroadcastTo;

use super::*;

fn command(fake: &mut Fake, app: &mut App, name: &str) {
    fake.send_app(app, AppEvent::Command(name.to_owned()));
}

/// The items of the Shell menu's Broadcast Input, each of its keys.
fn broadcast_items(fake: &Fake) -> Vec<(String, Option<Shortcut>)> {
    let shell = &fake.main_menu().unwrap().menus[1];
    shell
        .items
        .iter()
        .find_map(|item| match item {
            MenuItem::Submenu(menu) if menu.title == "Broadcast Input" => Some(menu),
            _ => None,
        })
        .expect("the Broadcast Input menu")
        .items
        .iter()
        .filter_map(|item| match item {
            MenuItem::Command {
                title, shortcut, ..
            } => Some((title.clone(), *shortcut)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_shell_menu_s_broadcast_input_sends_the_input_and_ticks_where_it_goes() {
    let (mut fake, mut app, window) = started_running(idle());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let shown = app.windows[&window].window;
    // Its items, the tab's and the window's of their keys.
    let items = broadcast_items(&fake);
    let titles: Vec<&str> = items.iter().map(|(title, _)| title.as_str()).collect();
    assert_eq!(
        titles,
        ["Off", "To the Tab's Panes", "To the Window's Panes"]
    );
    let alt_cmd = |shift: bool| Shortcut {
        key: ShortcutKey::Char('i'),
        command: true,
        alt: true,
        shift,
        control: false,
    };
    assert_eq!(items[1].1, Some(alt_cmd(false)));
    assert_eq!(items[2].1, Some(alt_cmd(true)));
    // Off, ticked.
    assert!(fake.command_checked(BROADCAST_OFF));
    // To the tab's panes, then again off.
    command(&mut fake, &mut app, BROADCAST_TAB);
    assert_eq!(app.workspace.broadcast(shown), BroadcastTo::Tab);
    assert!(fake.command_checked(BROADCAST_TAB) && !fake.command_checked(BROADCAST_OFF));
    command(&mut fake, &mut app, BROADCAST_TAB);
    assert_eq!(app.workspace.broadcast(shown), BroadcastTo::Off);
    assert!(fake.command_checked(BROADCAST_OFF));
    // To the window's, then off of its own item.
    command(&mut fake, &mut app, BROADCAST_WINDOW);
    assert_eq!(app.workspace.broadcast(shown), BroadcastTo::Window);
    assert!(fake.command_checked(BROADCAST_WINDOW));
    command(&mut fake, &mut app, BROADCAST_OFF);
    assert_eq!(app.workspace.broadcast(shown), BroadcastTo::Off);
    // A plugin's change ticks it too.
    assert!(app.workspace.set_broadcast(shown, BroadcastTo::Tab));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(fake.command_checked(BROADCAST_TAB));
}

#[test]
fn a_pane_leaves_the_broadcast_and_joins_it_again_from_its_context_menu() {
    let (mut fake, mut app, window) = started_with(reader(""));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let shown = app.windows[&window].window;
    let pane = app.focused(window).unwrap();
    let at = Point { x: 10.0, y: 10.0 };
    let open = |fake: &mut Fake, app: &mut App| {
        let menus = fake.context_menus().len();
        right_click(fake, app, window, at, Modifiers::NONE);
        run_until(fake, app, |fake, _| fake.context_menus().len() > menus);
        context_menu(fake, window).1
    };
    // Of a window that does not broadcast, neither.
    let items = open(&mut fake, &mut app);
    assert!(
        !items.iter().any(|item| item.contains("Broadcast")),
        "{items:?}"
    );
    // Of one that does, Leave Broadcast, then Join Broadcast.
    assert!(app.workspace.set_broadcast(shown, BroadcastTo::Tab));
    let items = open(&mut fake, &mut app);
    assert!(items.contains(&"Leave Broadcast".to_owned()), "{items:?}");
    let (_, _, menu) = fake.context_menus().last().unwrap();
    let leave = commands_of(&menu.items)
        .find_map(|(title, command)| (title == "Leave Broadcast").then(|| command.clone()))
        .unwrap();
    fake.send(&mut app, window, WindowEvent::ContextCommand(leave));
    assert!(app.workspace.pane(pane).unwrap().broadcast_left());
    let items = open(&mut fake, &mut app);
    assert!(items.contains(&"Join Broadcast".to_owned()), "{items:?}");
    command(&mut fake, &mut app, JOIN_BROADCAST);
    assert!(!app.workspace.pane(pane).unwrap().broadcast_left());
}

#[test]
fn a_pane_that_takes_the_broadcast_tells_it_to_accessibility() {
    let (mut fake, mut app, window) = started_running(idle());
    let shown = app.windows[&window].window;
    let label = |fake: &Fake| {
        let tree = fake.window(window).unwrap().accessibility.last().cloned();
        tree.and_then(|tree| {
            tree.nodes
                .iter()
                .find(|(_, node)| node.role() == Role::Terminal)
                .and_then(|(_, node)| node.label().map(str::to_owned))
        })
    };
    assert!(fake.ask_accessibility(&mut app, window));
    assert_eq!(label(&fake).as_deref(), Some("Terminal"));
    assert!(app.workspace.set_broadcast(shown, BroadcastTo::Window));
    run_until(&mut fake, &mut app, |fake, _| {
        label(fake).as_deref() == Some("Terminal, Broadcasting Input")
    });
}
