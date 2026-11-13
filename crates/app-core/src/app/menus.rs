//! The menus: the menu bar of the commands, with their keys and the
//! state of their items as the focused pane has it; the context menu at
//! the pointer, and what its commands of a cell do, Look Up and the man
//! page of the selection among them; and the plugins' items of both, each
//! a command of its arguments.

use lokyr_kernel::commands::{Args, Category, Command, Kind, Registry, Value};
use lokyr_kernel::keymap;
use lokyr_kernel::keymap::{Context, Invocation, Keymap, When};
use lokyr_kernel::l10n::{Arg, Texts};
use lokyr_kernel::plugins::MenuEntry;
use lokyr_kernel::plugins::host::MenuPlace;
use lokyr_kernel::workspace::Pane;
use lokyr_pkg_model::is_hidden;
use lokyr_platform_core::{
    ContextMenu, KeyCode, MainMenu, Menu, MenuItem, MenuRole, Platform, Point, Shortcut,
    ShortcutKey, StandardAction, WindowId,
};
use lokyr_pty::Program;
use lokyr_vt_core::terminal as vt;
use lokyr_vt_core::terminal::{SelectionShape, SelectionUnit};

use super::{
    ABOUT, APPEARANCES, App, CHECK_UPDATES, CONTEXT_ONLY, DEFAULT_TERMINAL, FLOAT_ON_TOP,
    INSTALL_CLAUDE_CODE, INSTALL_TOOLS, LOOK_UP, LOOK_UP_TITLE, MAN_KEYWORDS, MAN_PAGE,
    MAN_PAGE_NAME, MAN_SEARCH, MAN_SECTION, NEXT_AGENT, OPEN_THEMES, PERFORMANCE, REPORT_ISSUE,
    SEARCH_WEB, SECURE_INPUT, SETTINGS, SETTINGS_TAB, SHOW_TIP, context, show_selection, shown_top,
};
use crate::drawing::Backend;
use crate::mouse;
use crate::window::{NativeMenu, Selecting};

/// The command palette's theme picker, after which the view's menu holds the
/// themes' folder.
const PICK_THEME: &str = "command-palette.theme";

impl<B: Backend> App<B> {
    /// Opens `window`'s context menu at `position`, over the cell there,
    /// once the click has selected, as macOS's Terminal does: the
    /// selection stays when it holds the cell; else the cell's word is
    /// selected, as by a double click, or nothing on a blank cell. Its
    /// items are the commands of a cell, which take its `column` and `row`,
    /// then the clipboard's and the editing's that run without arguments,
    /// then the plugins' items; an item is dimmed where its command's
    /// context does not hold, or where its plugin tells that it would do
    /// nothing.
    pub(super) fn context_menu(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
    ) {
        let Some(pointer) = self.pointer(platform, window, position) else {
            return;
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(pane) = self.workspace.focused(state.window) else {
            return;
        };
        // Pinned, so that output that scrolls while the menu is open leaves
        // its commands on the clicked text.
        let top = shown_top(pane);
        pane.mark_cell((pointer.col, top + i64::from(pointer.row)));
        let within = pane
            .selection()
            .is_some_and(|selected| selected.contains(pointer.col, pointer.row));
        if !within {
            let grid = pane.grid();
            let spot = mouse::spot(pointer, top, grid.cell_width, grid.columns);
            pane.anchor_selection((spot.col, spot.row));
            let mut selecting = Selecting {
                anchor: spot,
                last: spot,
                shape: SelectionShape::Cells,
                unit: SelectionUnit::Word,
                held: false,
                beyond: 0,
            };
            show_selection(pane, &mut selecting);
            let blank = pane
                .selection_text()
                .is_none_or(|text| text.trim().is_empty());
            if blank {
                pane.clear_selection();
            }
            state.selecting = (!blank).then_some(selecting);
            platform.request_frame(window);
        }
        let context = context(Some(pane));
        // The cell by its row's number, as the marked cell's run takes it.
        let (column, row) = (pointer.col, top + i64::from(pointer.row));
        // Each item: its id, its command, and the arguments that it runs
        // with.
        let of = |command: &Command| {
            let args = if of_cell(command) {
                cell_args(column, row)
            } else {
                Args::new()
            };
            (command.name.clone(), command.clone(), args)
        };
        let taken = taken(&self.menu_items, |place| place == MenuPlace::PaneContext);
        let edits = |category: Category| -> Vec<(String, Command, Args)> {
            self.commands
                .in_order()
                .filter(|command| {
                    command.category == category
                        && command.params.iter().all(|param| param.default.is_some())
                        && !CONTEXT_ONLY.contains(&command.name.as_str())
                        && !taken.contains(&command.name.as_str())
                })
                .map(of)
                .collect()
        };
        // As in Terminal: what opens, the man pages among it, then Look Up
        // and the web's search, then the clipboard's, then the editing's;
        // the plugins' last, a group of each namespace of their commands. A
        // command of a cell that runs without one too, as a command's
        // output, goes with its category's, of the cell.
        let mut groups: Vec<Vec<(String, Command, Args)>> = vec![
            self.commands
                .in_order()
                .filter(|command| {
                    of_cell(command) && !command.params.iter().all(|param| param.default.is_some())
                })
                .chain(self.commands.get(MAN_PAGE))
                .chain(self.commands.get(MAN_SEARCH))
                .map(of)
                .collect(),
            [LOOK_UP, SEARCH_WEB]
                .into_iter()
                .filter_map(|name| self.commands.get(name))
                .map(of)
                .collect(),
            edits(Category::Clipboard),
            edits(Category::Edit),
        ];
        let mut last = None;
        for (at, entry) in self.menu_items.iter().enumerate() {
            let Some(command) = self
                .commands
                .get(&entry.invocation.command)
                .filter(|_| entry.place == MenuPlace::PaneContext)
            else {
                continue;
            };
            // Each plugin's items in a group of their own.
            let space = entry.plugin.clone();
            if last.as_ref() != Some(&space) {
                groups.push(Vec::new());
            }
            last = Some(space);
            if let Some(group) = groups.last_mut() {
                group.push((item_id(at), command.clone(), entry.invocation.args.clone()));
            }
        }
        self.acting = Some(window);
        let dimmed = groups
            .iter()
            .flatten()
            .filter(|(_, command, args)| !self.applies(platform, command, args, &context))
            .map(|(id, ..)| id.clone())
            .collect();
        self.acting = None;
        self.context_window = Some(window);
        // Look Up and the man page name what they take.
        let selection = self.looked_up(window).map(|(text, _)| text);
        let named = |name: &str| {
            let text = selection.as_deref()?;
            let (message, variable) = match name {
                LOOK_UP => ("menu-look-up", "word"),
                MAN_PAGE if man_page(text).is_some() => ("menu-man-page", "page"),
                MAN_SEARCH if man_keywords(text).is_some() => ("menu-man-search", "words"),
                SEARCH_WEB => ("menu-search-web", "text"),
                _ => return None,
            };
            let text = shortened(text, LOOK_UP_TITLE);
            Some(
                self.texts
                    .text_with(message, &[(variable, Arg::Text(&text))]),
            )
        };
        let title = |id: &str, command: &Command| {
            match self.menu_item_entry(id) {
                Some(entry) => entry.title.clone(),
                None => named(&command.name),
            }
            .unwrap_or_else(|| command.title.clone())
        };
        let mut items = Vec::new();
        for group in groups.iter().filter(|group| !group.is_empty()) {
            if !items.is_empty() {
                items.push(MenuItem::Separator);
            }
            items.extend(nested(
                group
                    .iter()
                    .map(|(id, command, _)| {
                        let item = MenuItem::Command {
                            title: title(id, command),
                            command: id.clone(),
                            shortcut: None,
                        };
                        let submenu = self
                            .menu_item_entry(id)
                            .and_then(|entry| entry.submenu.clone());
                        (item, submenu)
                    })
                    .collect(),
            ));
        }
        let items = unique_titles(items);
        let menu = ContextMenu {
            items,
            dimmed,
            ticked: Vec::new(),
        };
        platform.show_context_menu(window, position, &menu);
        if let Some(state) = self.windows.get_mut(&window) {
            state.native.push_back(NativeMenu::Window);
        }
    }

    /// Whether `command` would do something with `args` in `context`: its
    /// context holds, and its plugin, if one runs it, tells that it would.
    fn applies(
        &mut self,
        platform: &mut dyn Platform,
        command: &Command,
        args: &Args,
        context: &Context,
    ) -> bool {
        let when = command.when.as_deref().map(When::parse);
        if when.is_some_and(|when| when.is_ok_and(|when| !context.holds_when(&when))) {
            return false;
        }
        let looked_up = || self.acting.and_then(|window| self.looked_up(window));
        match command.name.as_str() {
            LOOK_UP | SEARCH_WEB => return looked_up().is_some(),
            MAN_PAGE => return looked_up().is_some_and(|(text, _)| man_page(&text).is_some()),
            MAN_SEARCH => {
                return looked_up().is_some_and(|(text, _)| man_keywords(&text).is_some());
            }
            _ => {}
        }
        if !self.plugins.owns(&command.name) {
            return true;
        }
        let applies = self
            .commands
            .check(&command.name, args.clone())
            .and_then(|args| {
                self.with_plugins(platform, |plugins, context| {
                    plugins.applies(&command.name, &args, context)
                })
            });
        applies.unwrap_or_else(|error| {
            eprintln!("lokyr: {error}");
            false
        })
    }

    /// What Look Up looks up in `window`: the selection's text, of one line,
    /// without the spaces around it, and the cell where that text starts,
    /// in the view; `None` without one.
    fn looked_up(&self, window: WindowId) -> Option<(String, (u16, i64))> {
        let pane = self.workspace.pane(self.focused(window)?)?;
        let (column, row) = pane.selection()?.start;
        let selected = pane.selection_text()?;
        let text = selected.trim();
        if text.is_empty() || text.contains('\n') {
            return None;
        }
        // Each space before the text takes a cell.
        let spaces = selected[..selected.len() - selected.trim_start().len()]
            .chars()
            .count();
        let column = column.saturating_add(u16::try_from(spaces).unwrap_or(u16::MAX));
        Some((text.to_owned(), (column, row)))
    }

    /// Shows the system's definition of the selection of the window that
    /// the commands run in, over the selection's first cell, in the text's
    /// font.
    pub(super) fn look_up(&mut self, platform: &mut dyn Platform) {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let Some((text, (column, row))) = self.looked_up(window) else {
            return;
        };
        let (Some(size), Some(state)) = (platform.window_size(window), self.windows.get(&window))
        else {
            return;
        };
        let Some(pane) = self.workspace.focused(state.window) else {
            return;
        };
        let grid = pane.grid();
        // A selection that starts above the view, or below it, shows at its
        // edge.
        let row = row.clamp(0, i64::from(grid.rows.saturating_sub(1))) as f64;
        let Some(id) = self.workspace.active_pane(state.window) else {
            return;
        };
        let (left, top) = self.pane_origin(window, id, size);
        let baseline = self
            .drawing
            .as_ref()
            .map_or(f64::from(grid.cell_height) * 0.75, |drawing| {
                f64::from(drawing.font.cell().baseline)
            });
        let at = Point {
            x: (f64::from(left) + f64::from(column) * f64::from(grid.cell_width)) / size.scale,
            y: (f64::from(top) + row * f64::from(grid.cell_height) + baseline) / size.scale,
        };
        let points = f64::from(self.font_size.unwrap_or(self.config.font.size.points()));
        let family = self
            .drawing
            .as_ref()
            .map(|drawing| drawing.font.family().to_owned());
        platform.show_definition(
            window,
            &text,
            at,
            family.as_deref().map(|family| (family, points)),
        );
    }

    /// Opens a window whose pane shows the man page that the selection of
    /// the window that the commands run in names, rather than the shell:
    /// `man`, which the user's login shell runs, as their `PATH` finds the
    /// pages of their own programs.
    pub(super) fn open_man_page(&mut self, platform: &mut dyn Platform) {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let Some((text, args)) = self
            .looked_up(window)
            .and_then(|(text, _)| Some((text.clone(), man_page(&text)?)))
        else {
            return;
        };
        let program = man_program(self.man.as_deref(), &args);
        self.open_window_running(platform, &program, Some(format!("man {text}")), None);
    }

    /// Opens a window whose pane lists the man pages that the words of the
    /// selection of the window that the commands run in describe, as
    /// `man -k` finds them, which the user's login shell runs.
    pub(super) fn search_man_pages(&mut self, platform: &mut dyn Platform) {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let Some((words, args)) = self
            .looked_up(window)
            .and_then(|(text, _)| Some((text.clone(), man_keywords(&text)?)))
        else {
            return;
        };
        let mut all = vec!["-k".to_owned(), "--".to_owned()];
        all.extend(args);
        let program = man_program(self.man.as_deref(), &all);
        self.open_window_running(platform, &program, Some(format!("man -k {words}")), None);
    }

    /// Searches the web for the selection of the window that the commands
    /// run in, with the engine of `[links]`, in the browser, without asking,
    /// as the user chose the search.
    pub(super) fn search_web(&mut self, platform: &mut dyn Platform) {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let Some((text, _)) = self.looked_up(window) else {
            return;
        };
        let url = self.config.links.search_engine.url(&text);
        let opened = platform
            .resolve_target(&url)
            .and_then(|target| platform.open(&target));
        if let Err(error) = opened {
            eprintln!("lokyr: the web's search does not open, {error}");
        }
    }

    /// Runs `name`, which `window`'s context menu chose, in its pane, as
    /// though the window had the focus: a command of a cell takes the cell
    /// that the menu opened on, wherever output moved its text, and does
    /// not run once its text left the scrollback.
    pub(super) fn context_command(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        name: &str,
    ) {
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        let Some(id) = self.workspace.active_pane(state.window) else {
            return;
        };
        let pane = self.workspace.pane(id);
        // The cell's line and link are read with it, which the command
        // reads, until the mark goes after it.
        let marked = pane
            .filter(|_| self.context_window == Some(window))
            .and_then(Pane::marked_cell);
        let invocation = match self.menu_item(name) {
            Some(invocation) => Some(invocation),
            None => match self.commands.get(name) {
                Some(command) if of_cell(command) => marked.map(|(column, row)| Invocation {
                    command: name.to_owned(),
                    args: cell_args(column, row),
                }),
                _ => Some(Invocation {
                    command: name.to_owned(),
                    args: Args::new(),
                }),
            },
        };
        if let Some(invocation) = invocation {
            self.acting = Some(window);
            self.run(platform, &invocation.command, invocation.args);
            self.acting = None;
        }
        if let Some(pane) = self.workspace.pane(id) {
            pane.unmark_cell();
        }
    }

    /// Gives the platform the main menu of the commands and their keys,
    /// in place of the one before, and the windows theirs; the items that
    /// it dimmed are enabled again, until the next frame of the focused
    /// pane settles them.
    pub(super) fn set_menu(&mut self, platform: &mut dyn Platform) {
        self.menu_items = self.plugins.menu_items_in(self.texts.language());
        platform.set_main_menu(&main_menu(
            &self.commands,
            &self.keymap,
            &self.texts,
            &self.menu_items,
        ));
        // The windows' keys follow the keymap's too, and the plugins learn
        // that the keys may be others, as they are.
        self.window_keys(platform);
        if self.keys_told.as_ref() != Some(&self.keymap) {
            self.keys_told = Some(self.keymap.clone());
            self.plugins.post(lokyr_kernel::plugins::Event::KeysChanged);
        }
        for (command, enabled) in std::mem::take(&mut self.menu_enabled) {
            if !enabled {
                platform.set_command_enabled(&command, true);
            }
        }
        self.menu_contexts = menu_contexts(&self.commands, &self.menu_items);
        // The plugins' items of the menu bar ticked as their plugins say,
        // each, as the items' places change with the menu.
        for (at, entry) in self.menu_items.iter().enumerate() {
            if matches!(entry.place, MenuPlace::Bar(_)) {
                platform.set_command_checked(&item_id(at), entry.ticked);
            }
        }
    }

    /// What the plugins' item of the menus of `id` runs, if `id` names one.
    pub(super) fn menu_item(&self, id: &str) -> Option<Invocation> {
        self.menu_item_entry(id)
            .map(|entry| entry.invocation.clone())
    }

    /// The plugins' item of the menus of `id`, if `id` names one.
    fn menu_item_entry(&self, id: &str) -> Option<&MenuEntry> {
        let at: usize = id.strip_prefix(ITEM)?.parse().ok()?;
        self.menu_items.get(at)
    }

    /// Enables the menus' items of the commands that can run in `context`,
    /// and dims the others, as each command's own context says: only those
    /// whose state changes reach the platform.
    pub(super) fn settle_menu(&mut self, platform: &mut dyn Platform, context: &Context) {
        for (command, when) in &self.menu_contexts {
            let enabled = context.holds_when(when);
            if self.menu_enabled.get(command) != Some(&enabled) {
                platform.set_command_enabled(command, enabled);
                self.menu_enabled.insert(command.clone(), enabled);
            }
        }
    }
}

/// The palette's list of the agents, which the windows' menu holds beside
/// the next agent.
const SHOW_AGENTS: &str = "command-palette.agents";
/// The arguments of `man` for the page that `text` names: its name, and
/// its section before it when `text` ends with one in brackets, as
/// `ls(1)`, after `--`; `None` for a text that is no page's name, which
/// only letters, digits and `.`, `_`, `+`, `:` and `-` make, not from `-`
/// or `.`, as `man` would take an option, or a path.
pub(super) fn man_page(text: &str) -> Option<Vec<String>> {
    let (name, section) = match text.strip_suffix(')').and_then(|text| text.split_once('(')) {
        Some((name, section)) => (name, Some(section)),
        None => (text, None),
    };
    let named = !name.is_empty()
        && name.len() <= MAN_PAGE_NAME
        && !name.starts_with(['-', '.'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._+:-".contains(c));
    // A section starts with its number, or `n`, as `3pm` and `n`.
    let sectioned = section.is_none_or(|section| {
        section.len() <= MAN_SECTION
            && section.starts_with(|c: char| c.is_ascii_digit() || c == 'n')
            && section.chars().all(|c| c.is_ascii_alphanumeric())
    });
    (named && sectioned).then(|| {
        let mut args = vec!["--".to_owned()];
        args.extend(section.map(str::to_owned));
        args.push(name.to_owned());
        args
    })
}

/// The words of `text` that the man pages' search takes, each as a page's
/// name is made, of letters, digits and `.`, `_`, `+`, `:` and `-`, not
/// from `-` or `.`, so that a shell takes them without quotes; `None` for a
/// text of another character, or of more than [`MAN_KEYWORDS`] words.
pub(super) fn man_keywords(text: &str) -> Option<Vec<String>> {
    let words: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
    let taken = !words.is_empty()
        && words.len() <= MAN_KEYWORDS
        && words.iter().all(|word| {
            word.len() <= MAN_PAGE_NAME
                && !word.starts_with(['-', '.'])
                && word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._+:-".contains(c))
        });
    taken.then_some(words)
}

/// The program that shows the man page of `args`: `man`, the path, or the
/// login shell's `man`, with them in its command, which they need no
/// quotes in, whatever the shell. Its pager is `ul`, which prints the whole
/// page, its bold and its underlined text as the terminal shows them, for
/// the window to keep.
pub(super) fn man_program(man: Option<&std::path::Path>, args: &[String]) -> Program {
    let mut all = vec!["-P".to_owned(), "ul".to_owned()];
    all.extend(args.iter().cloned());
    match man {
        Some(man) => Program::command(man, all),
        None => Program::LoginCommand {
            command: format!("exec man {}", all.join(" ")).into(),
        },
    }
}

/// `text`, shortened to its first `most` characters and `…` when it has
/// more.
pub(super) fn shortened(text: &str, most: usize) -> String {
    match text.char_indices().nth(most) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

/// Whether `command` is one of a cell: it takes the cell's `column` and
/// `row`, whole numbers, and needs no other argument.
fn of_cell(command: &Command) -> bool {
    let cell = |name: &str| {
        command
            .params
            .iter()
            .any(|param| param.name == name && param.kind == Kind::Integer)
    };
    cell("column")
        && cell("row")
        && command
            .params
            .iter()
            .all(|param| param.name == "column" || param.name == "row" || param.default.is_some())
}

/// The arguments of a command of a cell for the cell at `column` of the row
/// numbered `row`.
fn cell_args(column: u16, row: i64) -> Args {
    Args::from([
        ("column".to_owned(), Value::Integer(i64::from(column))),
        ("row".to_owned(), Value::Integer(row)),
    ])
}

/// The main menu, of the commands of `commands` that run without arguments,
/// by their categories, in the order of their registrations, each with the
/// keys that `keymap` binds it to: the application's menu, with the system's
/// items to hide it; the shell's, of the windows', the tabs', the panes' and
/// the input's; the edit's, of the clipboard's and the editing's, with the
/// system's palette of emoji and symbols; the view's; the windows', with the
/// system's items; and the help's, which the system gives a search of the
/// menus. A line parts the commands of a category or a namespace from the
/// next. The plugins' items of a category, `plugins_items`, come after its
/// commands, a group of their own. Its titles are `texts`'.
pub(super) fn main_menu(
    commands: &Registry,
    keymap: &Keymap,
    texts: &Texts,
    plugins_items: &[MenuEntry],
) -> MainMenu {
    let taken = taken(plugins_items, |place| matches!(place, MenuPlace::Bar(_)));
    let items = |categories: &[Category]| {
        let mut shown: Vec<Shown> = commands
            .in_order()
            .filter(|command| {
                categories.contains(&command.category)
                    && command.params.iter().all(|param| param.default.is_some())
                    && APPEARANCES.iter().all(|(name, ..)| *name != command.name)
                    && command.name != PERFORMANCE
                    && command.name != OPEN_THEMES
                    && command.name != SETTINGS_TAB
                    && command.name != FLOAT_ON_TOP
                    && command.name != NEXT_AGENT
                    && command.name != SHOW_AGENTS
                    && !CONTEXT_ONLY.contains(&command.name.as_str())
                    && !taken.contains(&command.name.as_str())
            })
            .map(|command| Shown {
                group: (command.category, namespace(&command.name), false),
                title: command.title.clone(),
                id: command.name.clone(),
                shortcut: keymap.shortcut(&command.name).and_then(shortcut),
                submenu: None,
            })
            .collect();
        // By the menu's categories, each of its commands in the order of
        // their registrations, as a plugin's that registers after the
        // others.
        shown.sort_by_key(|item| {
            categories
                .iter()
                .position(|category| *category == item.group.0)
        });
        for (at, entry) in plugins_items.iter().enumerate() {
            let MenuPlace::Bar(category) = entry.place else {
                continue;
            };
            let Some(command) = commands
                .get(&entry.invocation.command)
                .filter(|_| categories.contains(&category))
            else {
                continue;
            };
            let after = shown
                .iter()
                .rposition(|shown| shown.group.0 == category)
                .map_or(shown.len(), |last| last + 1);
            let Invocation {
                command: name,
                args,
            } = &entry.invocation;
            shown.insert(
                after,
                Shown {
                    group: (category, entry.plugin.clone(), true),
                    title: entry.title.clone().unwrap_or_else(|| command.title.clone()),
                    id: item_id(at),
                    shortcut: keymap.shortcut_with(name, args).and_then(shortcut),
                    submenu: entry.submenu.clone(),
                },
            );
        }
        command_items(shown)
    };
    // The appearances, in a menu of their own, by their short titles.
    let appearances = APPEARANCES
        .into_iter()
        .filter(|(name, ..)| commands.get(name).is_some())
        .map(|(name, _, title)| MenuItem::Command {
            title: texts.text(&format!("menu-{title}")),
            command: name.to_owned(),
            shortcut: keymap.shortcut(name).and_then(shortcut),
        })
        .collect();
    let mut view = items(&[Category::View]);
    // The themes' folder right after the palette's theme picker, where the
    // palette is there; else after the view's other commands.
    if let Some(command) = commands.get(OPEN_THEMES) {
        let item = MenuItem::Command {
            title: command.title.clone(),
            command: OPEN_THEMES.to_owned(),
            shortcut: keymap.shortcut(OPEN_THEMES).and_then(shortcut),
        };
        let picker = view.iter().position(
            |item| matches!(item, MenuItem::Command { command, .. } if command == PICK_THEME),
        );
        match picker {
            Some(at) => view.insert(at + 1, item),
            None => {
                if !view.is_empty() {
                    view.push(MenuItem::Separator);
                }
                view.push(item);
            }
        }
    }
    if !view.is_empty() {
        view.push(MenuItem::Separator);
    }
    view.push(MenuItem::Submenu(Menu {
        title: texts.text("menu-appearance"),
        items: appearances,
        role: None,
    }));
    // The performance's panel, a tool to see how Lokyr does, last.
    if let Some(command) = commands.get(PERFORMANCE) {
        view.push(MenuItem::Separator);
        view.push(MenuItem::Command {
            title: command.title.clone(),
            command: PERFORMANCE.to_owned(),
            shortcut: keymap.shortcut(PERFORMANCE).and_then(shortcut),
        });
    }
    let standard = |key: &str, action, shortcut: Option<Shortcut>| MenuItem::Standard {
        title: texts.text(key),
        action,
        shortcut,
    };
    let cmd = |key: char, alt: bool| Shortcut {
        key: ShortcutKey::Char(key),
        command: true,
        shift: false,
        alt,
        control: false,
    };
    let menu = |key: &str, items, role| Menu {
        title: texts.text(key),
        items,
        role,
    };
    // The system's palette of emoji and symbols last, of its key on the
    // Mac, as AppKit adds it to the Edit menus of its own items.
    let mut edit = items(&[Category::Clipboard, Category::Edit, Category::Find]);
    if !edit.is_empty() {
        edit.push(MenuItem::Separator);
    }
    edit.push(standard(
        "menu-emoji-symbols",
        StandardAction::CharacterPalette,
        Some(Shortcut {
            key: ShortcutKey::Char(' '),
            command: true,
            shift: false,
            alt: false,
            control: true,
        }),
    ));
    // About and Check for Updates… first, where macOS's apps have them,
    // then Settings, Secure Keyboard Entry, the default terminal and the
    // command line tools, then the system's items.
    let of = |item: &MenuItem, names: &[&str]| matches!(item, MenuItem::Command { command, .. } if names.contains(&command.as_str()));
    let first = |item: &MenuItem| {
        of(
            item,
            &[
                ABOUT,
                CHECK_UPDATES,
                SETTINGS,
                SECURE_INPUT,
                DEFAULT_TERMINAL,
                INSTALL_TOOLS,
                INSTALL_CLAUDE_CODE,
            ],
        )
    };
    let mut application = Vec::new();
    for group in [
        &[ABOUT, CHECK_UPDATES][..],
        &[
            SETTINGS,
            SECURE_INPUT,
            DEFAULT_TERMINAL,
            INSTALL_TOOLS,
            INSTALL_CLAUDE_CODE,
        ],
    ] {
        let group: Vec<MenuItem> = items(&[Category::App])
            .into_iter()
            .filter(|item| of(item, group))
            .collect();
        if !group.is_empty() {
            application.extend(group);
            application.push(MenuItem::Separator);
        }
    }
    application.extend([
        standard("menu-hide", StandardAction::Hide, Some(cmd('h', false))),
        standard(
            "menu-hide-others",
            StandardAction::HideOthers,
            Some(cmd('h', true)),
        ),
        standard("menu-show-all", StandardAction::ShowAll, None),
    ]);
    let app_commands: Vec<MenuItem> = items(&[Category::App])
        .into_iter()
        .filter(|item| !first(item) && !of(item, &[SHOW_TIP, REPORT_ISSUE]))
        .collect();
    // Show a Tip and Report an Issue… in the help's menu, whose search the
    // system gives.
    let help: Vec<MenuItem> = [SHOW_TIP, REPORT_ISSUE]
        .into_iter()
        .flat_map(|name| {
            items(&[Category::App])
                .into_iter()
                .filter(move |item| of(item, &[name]))
        })
        .collect();
    if !app_commands.is_empty() {
        application.push(MenuItem::Separator);
        application.extend(app_commands);
    }
    let mut main = MainMenu {
        menus: vec![
            Menu {
                title: "Lokyr".to_owned(),
                items: application,
                role: None,
            },
            menu(
                "menu-shell",
                items(&[
                    Category::Windows,
                    Category::Tabs,
                    Category::Panes,
                    Category::Input,
                ]),
                None,
            ),
            menu("menu-edit", edit, None),
            menu("menu-view", view, None),
            menu(
                "menu-window",
                vec![
                    standard(
                        "menu-minimize",
                        StandardAction::Minimize,
                        Some(cmd('m', false)),
                    ),
                    standard("menu-zoom", StandardAction::Zoom, None),
                    MenuItem::Separator,
                ]
                .into_iter()
                // Float on Top, and the agents, in the windows' menu.
                .chain(
                    [FLOAT_ON_TOP, NEXT_AGENT, SHOW_AGENTS]
                        .into_iter()
                        .filter_map(|name| {
                            commands.get(name).map(|command| MenuItem::Command {
                                title: command.title.clone(),
                                command: name.to_owned(),
                                shortcut: keymap.shortcut(name).and_then(shortcut),
                            })
                        }),
                )
                .chain(commands.get(FLOAT_ON_TOP).map(|_| MenuItem::Separator))
                .chain([standard(
                    "menu-bring-all-to-front",
                    StandardAction::BringAllToFront,
                    None,
                )])
                .collect(),
                Some(MenuRole::Window),
            ),
            menu("menu-help", help, Some(MenuRole::Help)),
        ],
    };
    for menu in &mut main.menus {
        menu.items = unique_titles(std::mem::take(&mut menu.items));
    }
    main
}

/// A menu item's title as the user reads it: of no case, nor the dots of
/// an item that asks more, nor the characters that show nothing, its
/// full-width letters and the letters of Cyrillic and Greek that look as
/// Latin ones read as those, so that a plugin's title that only looks as
/// one of the app's is taken for it.
fn read_as_shown(title: &str) -> String {
    title
        .trim()
        .trim_end_matches('…')
        .trim_end_matches("...")
        .trim()
        .chars()
        .filter(|&c| !c.is_control() && !is_hidden(c))
        .map(as_latin)
        .flat_map(char::to_lowercase)
        .collect()
}

/// The Latin letter that `c` looks as, of the full-width forms and of
/// Cyrillic's and Greek's letters that look as Latin ones; `c` itself
/// otherwise.
fn as_latin(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(u32::from(c) - 0xFEE0).unwrap_or(c),
        'а' | 'α' => 'a',
        'А' | 'Α' => 'A',
        'В' | 'Β' => 'B',
        'с' | 'ϲ' => 'c',
        'С' | 'Ϲ' => 'C',
        'ԁ' => 'd',
        'е' => 'e',
        'Е' | 'Ε' => 'E',
        'Н' | 'Η' => 'H',
        'і' | 'ι' => 'i',
        'І' | 'Ι' => 'I',
        'ј' => 'j',
        'Ј' => 'J',
        'К' | 'Κ' => 'K',
        'М' | 'Μ' => 'M',
        'Ν' => 'N',
        'о' | 'ο' => 'o',
        'О' | 'Ο' => 'O',
        'р' | 'ρ' => 'p',
        'Р' | 'Ρ' => 'P',
        'ԛ' => 'q',
        'ѕ' => 's',
        'Ѕ' => 'S',
        'Т' | 'Τ' => 'T',
        'ν' => 'v',
        'ԝ' => 'w',
        'х' | 'χ' => 'x',
        'Х' | 'Χ' => 'X',
        'у' | 'γ' => 'y',
        'У' | 'Υ' => 'Y',
        'Ζ' => 'Z',
        _ => c,
    }
}

/// `items`, a menu's, but the plugins' items and submenus whose titles are
/// those of the app's items of the menu, as a plugin's "Copy" in the Edit
/// menu, which the user would take for the app's; the lines that parted
/// them go too.
pub(super) fn unique_titles(items: Vec<MenuItem>) -> Vec<MenuItem> {
    let read = |title: &str| read_as_shown(title);
    let of_items = |item: &MenuItem| matches!(item, MenuItem::Command { command, .. } if command.starts_with(ITEM));
    // A plugin's item, or its submenu, of its items alone.
    let of_plugin = |item: &MenuItem| match item {
        MenuItem::Submenu(menu) => !menu.items.is_empty() && menu.items.iter().all(of_items),
        item => of_items(item),
    };
    let title_of = |item: &MenuItem| match item {
        MenuItem::Command { title, .. } | MenuItem::Standard { title, .. } => Some(read(title)),
        MenuItem::Submenu(menu) => Some(read(&menu.title)),
        MenuItem::Separator => None,
    };
    let apps: Vec<String> = items
        .iter()
        .filter(|item| !of_plugin(item))
        .filter_map(title_of)
        .collect();
    let mut kept: Vec<MenuItem> = Vec::with_capacity(items.len());
    for item in items {
        if of_plugin(&item)
            && let Some(title) = title_of(&item)
            && apps.contains(&title)
        {
            eprintln!(
                "lokyr: a plugin's menu item “{title}” is left out, as an item of the app's has its title"
            );
            continue;
        }
        // A plugin's title of its catalogs that hides what it says.
        let hides = |title: &str| title.chars().any(|c| c.is_control() || is_hidden(c));
        let shown = match &item {
            MenuItem::Command { title, .. } => Some(title.as_str()),
            MenuItem::Submenu(menu) => Some(menu.title.as_str()),
            _ => None,
        };
        if of_plugin(&item) && shown.is_some_and(hides) {
            eprintln!("lokyr: a plugin's menu item is left out, as its title hides characters");
            continue;
        }
        // No line first, after another, nor last.
        let line = matches!(item, MenuItem::Separator);
        if line
            && kept
                .last()
                .is_none_or(|last| matches!(last, MenuItem::Separator))
        {
            continue;
        }
        kept.push(item);
    }
    if kept
        .last()
        .is_some_and(|last| matches!(last, MenuItem::Separator))
    {
        kept.pop();
    }
    kept
}

/// An item of the menu bar, of a command or of a plugin's item: its group,
/// of its category, its command's namespace or the plugin that adds the
/// item, and whether a plugin's item makes it, its title, its id, its keys
/// and the title of the submenu that it goes into.
struct Shown {
    group: (Category, String, bool),
    title: String,
    id: String,
    shortcut: Option<Shortcut>,
    submenu: Option<String>,
}

/// The items of `shown`, in their order, each group's of a submenu
/// gathered into it: a line parts a group from the next.
fn command_items(shown: Vec<Shown>) -> Vec<MenuItem> {
    let mut items = Vec::new();
    let mut run = Vec::new();
    let mut group = None;
    for shown in shown {
        if group.as_ref().is_some_and(|group| *group != shown.group) {
            items.extend(nested(std::mem::take(&mut run)));
            items.push(MenuItem::Separator);
        }
        group = Some(shown.group);
        let item = MenuItem::Command {
            title: shown.title,
            command: shown.id,
            shortcut: shown.shortcut,
        };
        run.push((item, shown.submenu));
    }
    items.extend(nested(run));
    items
}

/// `items`, each of the title of the submenu that it goes into, if one:
/// those of a submenu gathered into it, in their order, the submenu where
/// the first of them is.
fn nested(items: Vec<(MenuItem, Option<String>)>) -> Vec<MenuItem> {
    let mut nested: Vec<MenuItem> = Vec::with_capacity(items.len());
    // Each submenu's title, and its place in `nested`.
    let mut submenus: Vec<(String, usize)> = Vec::new();
    for (item, submenu) in items {
        let Some(title) = submenu else {
            nested.push(item);
            continue;
        };
        match submenus.iter().find(|(kept, _)| *kept == title) {
            Some(&(_, at)) => {
                if let Some(MenuItem::Submenu(menu)) = nested.get_mut(at) {
                    menu.items.push(item);
                }
            }
            None => {
                submenus.push((title.clone(), nested.len()));
                nested.push(MenuItem::Submenu(Menu {
                    title,
                    items: vec![item],
                    role: None,
                }));
            }
        }
    }
    nested
}

/// The commands that the plugins' items of `items` of a place that `of`
/// takes run of no arguments, each of its own plugin's: their items take
/// the commands' own places in those menus, as a plugin's own item of a
/// submenu, ticked.
fn taken(items: &[MenuEntry], of: impl Fn(MenuPlace) -> bool) -> Vec<&str> {
    items
        .iter()
        .filter(|entry| entry.own && entry.invocation.args.is_empty() && of(entry.place))
        .map(|entry| entry.invocation.command.as_str())
        .collect()
}

/// The namespace of the command `name`, before its `.`.
fn namespace(name: &str) -> String {
    name.split('.').next().unwrap_or_default().to_owned()
}

/// The prefix of the ids of the plugins' items of the menus, which no
/// command's name has, as a name has no `:`.
const ITEM: &str = "menu-item:";

/// The id of the plugins' item of the menus at `at` of their list, which
/// the platform gives back as it is chosen.
fn item_id(at: usize) -> String {
    format!("{ITEM}{at}")
}

/// The contexts in which the commands of `commands` that tell one can run,
/// and the plugins' items of `items` of their commands, as the menus
/// enable their items.
fn menu_contexts(commands: &Registry, items: &[MenuEntry]) -> Vec<(String, When)> {
    let when = |name: &str| When::parse(commands.get(name)?.when.as_deref()?).ok();
    commands
        .in_order()
        .filter_map(|command| Some((command.name.clone(), when(&command.name)?)))
        .chain(
            items
                .iter()
                .enumerate()
                .filter_map(|(at, item)| Some((item_id(at), when(&item.invocation.command)?))),
        )
        .collect()
}

/// The keys of `combo` as a menu's item shows them: a key that types a
/// character, or a named one, with its modifiers.
pub(super) fn shortcut(combo: keymap::Combo) -> Option<Shortcut> {
    use keymap::{Named, Press};
    let key = match combo.press {
        Press::Char(character) => ShortcutKey::Char(character),
        Press::Named(named) => ShortcutKey::Code(match named {
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
            Named::F(n) => KeyCode::F(n),
        }),
        Press::Wheel(_) | Press::Click => return None,
    };
    Some(Shortcut {
        key,
        command: combo.mods.cmd,
        shift: combo.mods.shift,
        alt: combo.mods.alt,
        control: combo.mods.ctrl,
    })
}

/// The context in which the menus' commands run, of a pane whose frame
/// shows `snapshot`: the pane has the focus, and may show the alternate
/// screen and a selection.
pub(super) fn menu_context(snapshot: &vt::Snapshot) -> Context {
    let mut names = vec!["pane.focused"];
    if snapshot.alternate_screen() {
        names.push("alternate-screen");
    }
    if snapshot.selection().is_some() {
        names.push("selection");
    }
    Context::new(names)
}
