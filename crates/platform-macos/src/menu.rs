//! The main menu, built from the app's commands, with the system's own
//! items, as hiding the application and minimizing a window; and the
//! context menus.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::{Rc, Weak};

use lokyr_platform_core::{
    AppEvent, ContextMenu, KeyCode, MainMenu, Menu, MenuItem, MenuRole, PopUpButton, Shortcut,
    ShortcutKey, StandardAction, WindowEvent, WindowId,
};
use objc2::rc::Retained;
use objc2::runtime::{ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSControlSize, NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSFont,
    NSMenu, NSMenuDelegate, NSMenuItem, NSPopUpButtonCell,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};

use crate::shared::{Pending, Shared};

/// Commands that the backend keeps and the menus' targets read, as those
/// whose items are dimmed.
pub(crate) type Commands = Rc<RefCell<BTreeSet<String>>>;

pub(crate) struct MenuTargetIvars {
    shared: Weak<Shared>,
    /// The window of a context menu, whose items' commands come as its
    /// events; `None` for the main menu.
    window: Option<WindowId>,
    /// The commands of the menu's items, by their tags.
    commands: Vec<String>,
    /// The commands whose items are dimmed, and those whose items are
    /// ticked, which the backend keeps for the main menu.
    disabled: Commands,
    checked: Commands,
}

define_class!(
    // SAFETY: `NSObject` has no requirement of its subclasses, and the
    // target does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = MenuTargetIvars]
    pub(crate) struct MenuTarget;

    impl MenuTarget {
        // SAFETY: an action's signature.
        #[unsafe(method(runCommand:))]
        fn run_command(&self, sender: &NSMenuItem) {
            let ivars = self.ivars();
            let command = usize::try_from(sender.tag())
                .ok()
                .and_then(|tag| ivars.commands.get(tag));
            if let (Some(command), Some(shared)) = (command, ivars.shared.upgrade()) {
                let command = command.clone();
                shared.post(match ivars.window {
                    Some(window) => Pending::Window(window, WindowEvent::ContextCommand(command)),
                    None => Pending::App(AppEvent::Command(command)),
                });
            }
        }

        // SAFETY: `NSMenuItemValidation`'s signature: AppKit asks it before
        // it shows the item, and before its key runs it, which is when the
        // item takes its tick too.
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            let ivars = self.ivars();
            let command = usize::try_from(item.tag())
                .ok()
                .and_then(|tag| ivars.commands.get(tag));
            if let Some(command) = command {
                let ticked = ivars.checked.borrow().contains(command);
                item.setState(if ticked {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            }
            command.is_none_or(|command| !ivars.disabled.borrow().contains(command))
        }
    }

    // SAFETY: `NSObjectProtocol` has no requirement.
    unsafe impl NSObjectProtocol for MenuTarget {}

    // SAFETY: `NSMenuDelegate`'s signatures; a context menu's own menus
    // have the target as their delegate.
    unsafe impl NSMenuDelegate for MenuTarget {
        // The item that the arrows or the pointer highlight, of a context
        // menu, as its window's event: of a command, or none, as of no item
        // or of a submenu's.
        #[unsafe(method(menu:willHighlightItem:))]
        fn menu_will_highlight_item(&self, _menu: &NSMenu, item: Option<&NSMenuItem>) {
            let ivars = self.ivars();
            let Some(window) = ivars.window else {
                return;
            };
            let command = item
                .filter(|item| !item.hasSubmenu())
                .and_then(|item| usize::try_from(item.tag()).ok())
                .and_then(|tag| ivars.commands.get(tag))
                .cloned();
            if let Some(shared) = ivars.shared.upgrade() {
                shared.post(Pending::Window(
                    window,
                    WindowEvent::ContextHighlighted(command),
                ));
            }
        }
    }
);

/// A menu bar that [`build`] made.
pub(crate) struct Built {
    /// The bar.
    pub(crate) bar: Retained<NSMenu>,
    /// The target of its items, which the caller keeps as long as the menu.
    pub(crate) target: Retained<MenuTarget>,
    /// Its menus that play a part for the system.
    pub(crate) roles: Vec<(MenuRole, Retained<NSMenu>)>,
}

/// The menu bar of `menu`, whose items' target dims those of the commands
/// in `disabled`, and ticks those in `checked`.
pub(crate) fn build(
    mtm: MainThreadMarker,
    shared: Weak<Shared>,
    menu: &MainMenu,
    (disabled, checked): (Commands, Commands),
) -> Built {
    let mut commands = Vec::new();
    for menu in &menu.menus {
        collect(&menu.items, &mut commands);
    }
    let target = target(mtm, shared, None, commands, (disabled, checked));
    let bar = NSMenu::new(mtm);
    let mut tag = 0;
    let mut roles = Vec::new();
    for menu in &menu.menus {
        let item = NSMenuItem::new(mtm);
        let ns_menu = submenu(mtm, menu, &target, &mut tag);
        item.setSubmenu(Some(&ns_menu));
        bar.addItem(&item);
        if let Some(role) = menu.role {
            roles.push((role, ns_menu));
        }
    }
    Built { bar, target, roles }
}

/// The menu of `menu`, over `window`, and the target of its items, which
/// the caller keeps as long as the menu.
pub(crate) fn context(
    mtm: MainThreadMarker,
    shared: Weak<Shared>,
    window: WindowId,
    menu: &ContextMenu,
) -> (Retained<NSMenu>, Retained<MenuTarget>) {
    let mut commands = Vec::new();
    collect(&menu.items, &mut commands);
    let dimmed = Rc::new(RefCell::new(menu.dimmed.iter().cloned().collect()));
    let ticked = Rc::new(RefCell::new(menu.ticked.iter().cloned().collect()));
    let target = target(mtm, shared, Some(window), commands, (dimmed, ticked));
    let menu = Menu {
        title: String::new(),
        items: menu.items.clone(),
        role: None,
    };
    let ns_menu = submenu(mtm, &menu, &target, &mut 0);
    delegate(&ns_menu, &target);
    (ns_menu, target)
}

/// A pop-up button's cell of `menu`, as `button` has it, of its item
/// chosen by its index, `chosen`, if one is: it opens the menu over the
/// button as the system's pop-up buttons open theirs, the item chosen
/// ticked over the button's text, of the system's size of controls nearest
/// the button's height and of its text's size.
pub(crate) fn pop_up_cell(
    mtm: MainThreadMarker,
    menu: &NSMenu,
    (button, chosen): (&PopUpButton, Option<isize>),
) -> Retained<NSPopUpButtonCell> {
    let cell = NSPopUpButtonCell::initTextCell_pullsDown(
        NSPopUpButtonCell::alloc(mtm),
        &NSString::new(),
        false,
    );
    cell.setMenu(Some(menu));
    cell.selectItemAtIndex(chosen.unwrap_or(-1));
    cell.setControlSize(control_size(button.height));
    let font = NSFont::systemFontOfSize(button.text_size);
    cell.setFont(Some(&font));
    // SAFETY: the font is the system's, which the menu keeps.
    unsafe { menu.setFont(Some(&font)) };
    cell
}

/// The system's size of controls nearest a control's `height`, in points:
/// mini 16, small 20, regular 24 and large 28.
fn control_size(height: f64) -> NSControlSize {
    match height {
        ..18.0 => NSControlSize::Mini,
        ..22.0 => NSControlSize::Small,
        ..26.0 => NSControlSize::Regular,
        _ => NSControlSize::Large,
    }
}

/// Makes `target` the delegate of `menu` and of its submenus, which tells
/// the item that each highlights.
fn delegate(menu: &NSMenu, target: &MenuTarget) {
    menu.setDelegate(Some(ProtocolObject::from_ref(target)));
    for at in 0..menu.numberOfItems() {
        if let Some(submenu) = menu.itemAtIndex(at).and_then(|item| item.submenu()) {
            delegate(&submenu, target);
        }
    }
}

/// The target of a menu's items, which runs `commands`, by their tags,
/// dims those in `disabled` and ticks those in `checked`.
fn target(
    mtm: MainThreadMarker,
    shared: Weak<Shared>,
    window: Option<WindowId>,
    commands: Vec<String>,
    (disabled, checked): (Commands, Commands),
) -> Retained<MenuTarget> {
    let target = MenuTarget::alloc(mtm).set_ivars(MenuTargetIvars {
        shared,
        window,
        commands,
        disabled,
        checked,
    });
    // SAFETY: `NSObject`'s `init` has this signature.
    unsafe { msg_send![super(target), init] }
}

/// The commands of `items`, in the order that [`submenu`] tags them.
fn collect(items: &[MenuItem], commands: &mut Vec<String>) {
    for item in items {
        match item {
            MenuItem::Command { command, .. } => commands.push(command.clone()),
            MenuItem::Standard { .. } | MenuItem::Separator => {}
            MenuItem::Submenu(menu) => collect(&menu.items, commands),
        }
    }
}

fn submenu(
    mtm: MainThreadMarker,
    menu: &Menu,
    target: &MenuTarget,
    tag: &mut isize,
) -> Retained<NSMenu> {
    let ns_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(&menu.title));
    for item in &menu.items {
        let ns_item = match item {
            MenuItem::Command {
                title, shortcut, ..
            } => {
                let key = shortcut
                    .and_then(|shortcut| key_equivalent(shortcut.key))
                    .unwrap_or_default();
                // SAFETY: the action is the target's, with an action's
                // signature.
                let ns_item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(title),
                        Some(sel!(runCommand:)),
                        &NSString::from_str(&key),
                    )
                };
                if let Some(shortcut) = shortcut {
                    ns_item.setKeyEquivalentModifierMask(modifier_mask(shortcut));
                }
                // SAFETY: the target is a `MenuTarget`, which answers the
                // item's action, `runCommand:`. The item holds it weakly, and
                // the caller keeps it as long as the menu.
                unsafe { ns_item.setTarget(Some(target)) };
                ns_item.setTag(*tag);
                *tag += 1;
                ns_item
            }
            MenuItem::Standard {
                title,
                action,
                shortcut,
            } => {
                let key = shortcut
                    .and_then(|shortcut| key_equivalent(shortcut.key))
                    .unwrap_or_default();
                // SAFETY: an item without a target sends its action along the
                // responder chain, where the application or the window that
                // has the focus answers it, as AppKit's own items do.
                let ns_item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(title),
                        Some(selector(*action)),
                        &NSString::from_str(&key),
                    )
                };
                if let Some(shortcut) = shortcut {
                    ns_item.setKeyEquivalentModifierMask(modifier_mask(shortcut));
                }
                ns_item
            }
            MenuItem::Separator => NSMenuItem::separatorItem(mtm),
            MenuItem::Submenu(menu) => {
                let ns_item = NSMenuItem::new(mtm);
                ns_item.setTitle(&NSString::from_str(&menu.title));
                ns_item.setSubmenu(Some(&submenu(mtm, menu, target, tag)));
                ns_item
            }
        };
        ns_menu.addItem(&ns_item);
    }
    ns_menu
}

/// The action of AppKit's own item that does `action`.
fn selector(action: StandardAction) -> Sel {
    match action {
        StandardAction::Hide => sel!(hide:),
        StandardAction::HideOthers => sel!(hideOtherApplications:),
        StandardAction::ShowAll => sel!(unhideAllApplications:),
        StandardAction::Minimize => sel!(performMiniaturize:),
        StandardAction::Zoom => sel!(performZoom:),
        StandardAction::BringAllToFront => sel!(arrangeInFront:),
        StandardAction::CharacterPalette => sel!(orderFrontCharacterPalette:),
    }
}

fn modifier_mask(shortcut: &Shortcut) -> NSEventModifierFlags {
    let mut mask = NSEventModifierFlags::empty();
    for (held, flag) in [
        (shortcut.command, NSEventModifierFlags::Command),
        (shortcut.shift, NSEventModifierFlags::Shift),
        (shortcut.alt, NSEventModifierFlags::Option),
        (shortcut.control, NSEventModifierFlags::Control),
    ] {
        if held {
            mask |= flag;
        }
    }
    mask
}

/// The key equivalent of a key, as AppKit writes it: the character that the
/// key types, or for a key that types none, the character that `NSEvent`
/// gives it among its function keys.
fn key_equivalent(key: ShortcutKey) -> Option<String> {
    let code = match key {
        ShortcutKey::Char(character) => return Some(character.to_string()),
        ShortcutKey::Code(code) => code,
    };
    let character = match code {
        KeyCode::ArrowUp => '\u{F700}',
        KeyCode::ArrowDown => '\u{F701}',
        KeyCode::ArrowLeft => '\u{F702}',
        KeyCode::ArrowRight => '\u{F703}',
        // F1 to F35 follow each other from NSF1FunctionKey.
        KeyCode::F(n @ 1..=35) => char::from_u32(0xF704 + u32::from(n) - 1)?,
        KeyCode::Insert => '\u{F727}',
        KeyCode::Delete => '\u{F728}',
        KeyCode::Home => '\u{F729}',
        KeyCode::End => '\u{F72B}',
        KeyCode::PageUp => '\u{F72C}',
        KeyCode::PageDown => '\u{F72D}',
        KeyCode::Backspace => '\u{8}',
        KeyCode::Tab => '\t',
        KeyCode::Enter | KeyCode::NumpadEnter => '\r',
        KeyCode::Escape => '\u{1b}',
        KeyCode::Space => ' ',
        _ => return None,
    };
    Some(character.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_that_type_nothing_take_appkit_s_function_keys() {
        assert_eq!(key_equivalent(ShortcutKey::Char('q')).as_deref(), Some("q"));
        assert_eq!(
            key_equivalent(ShortcutKey::Code(KeyCode::F(1))).as_deref(),
            Some("\u{F704}")
        );
        assert_eq!(
            key_equivalent(ShortcutKey::Code(KeyCode::F(12))).as_deref(),
            Some("\u{F70F}")
        );
        assert_eq!(
            key_equivalent(ShortcutKey::Code(KeyCode::PageDown)).as_deref(),
            Some("\u{F72D}")
        );
        assert_eq!(key_equivalent(ShortcutKey::Code(KeyCode::F(36))), None);
        assert_eq!(key_equivalent(ShortcutKey::Code(KeyCode::CapsLock)), None);
    }

    #[test]
    fn each_standard_item_sends_an_action_that_appkit_answers() {
        use objc2::ClassType;
        use objc2_app_kit::{NSApplication, NSWindow};
        let answers = |class: &objc2::runtime::AnyClass, action: Sel| -> bool {
            // SAFETY: `instancesRespondToSelector:` of `NSObject`'s class,
            // which takes a selector.
            unsafe { msg_send![class, instancesRespondToSelector: action] }
        };
        // The application's, and the window's that has the focus.
        for action in [
            StandardAction::Hide,
            StandardAction::HideOthers,
            StandardAction::ShowAll,
            StandardAction::BringAllToFront,
            StandardAction::CharacterPalette,
        ] {
            assert!(
                answers(NSApplication::class(), selector(action)),
                "{action:?}"
            );
        }
        for action in [StandardAction::Minimize, StandardAction::Zoom] {
            assert!(answers(NSWindow::class(), selector(action)), "{action:?}");
        }
        assert!(!answers(
            NSApplication::class(),
            sel!(orderFrontCharacterPalettes:)
        ));
    }

    #[test]
    fn a_shortcut_s_modifiers_make_its_mask() {
        let shortcut = Shortcut {
            key: ShortcutKey::Char(']'),
            command: true,
            shift: true,
            alt: false,
            control: false,
        };
        assert_eq!(
            modifier_mask(&shortcut),
            NSEventModifierFlags::Command | NSEventModifierFlags::Shift
        );
    }
}
