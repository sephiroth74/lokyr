//! The main menu, built from the app's commands.

use std::rc::Weak;

use bolt_platform_core::{AppEvent, KeyCode, MainMenu, Menu, MenuItem, Shortcut, ShortcutKey};
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSEventModifierFlags, NSMenu, NSMenuItem};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};

use crate::shared::{Pending, Shared};

pub(crate) struct MenuTargetIvars {
    shared: Weak<Shared>,
    /// The commands of the menu's items, by their tags.
    commands: Vec<String>,
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
                shared.post(Pending::App(AppEvent::Command(command.clone())));
            }
        }
    }

    // SAFETY: `NSObjectProtocol` has no requirement.
    unsafe impl NSObjectProtocol for MenuTarget {}
);

/// The menu bar of `menu`, and the target of its items, which the caller
/// keeps as long as the menu.
pub(crate) fn build(
    mtm: MainThreadMarker,
    shared: Weak<Shared>,
    menu: &MainMenu,
) -> (Retained<NSMenu>, Retained<MenuTarget>) {
    let mut commands = Vec::new();
    for menu in &menu.menus {
        collect(&menu.items, &mut commands);
    }
    let target = MenuTarget::alloc(mtm).set_ivars(MenuTargetIvars { shared, commands });
    // SAFETY: `NSObject`'s `init` has this signature.
    let target: Retained<MenuTarget> = unsafe { msg_send![super(target), init] };

    let bar = NSMenu::new(mtm);
    let mut tag = 0;
    for menu in &menu.menus {
        let item = NSMenuItem::new(mtm);
        item.setSubmenu(Some(&submenu(mtm, menu, &target, &mut tag)));
        bar.addItem(&item);
    }
    (bar, target)
}

/// The commands of `items`, in the order that [`submenu`] tags them.
fn collect(items: &[MenuItem], commands: &mut Vec<String>) {
    for item in items {
        match item {
            MenuItem::Command { command, .. } => commands.push(command.clone()),
            MenuItem::Separator => {}
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
                // SAFETY: the target outlives the menu, as its caller keeps
                // it.
                unsafe { ns_item.setTarget(Some(target)) };
                ns_item.setTag(*tag);
                *tag += 1;
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
