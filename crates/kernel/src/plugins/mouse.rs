//! A mouse event that a plugin sends a pane, as the pointer over a cell of
//! its screen makes it.

use lokyr_plugin_sdk::host::{MouseButton, MouseDoes, MouseInput};
use lokyr_vt_core::input::{self, MouseAction, MouseEvent, Wheel};
use lokyr_vt_core::terminal::Scroll;

use crate::workspace::Pane;

/// Sends `pane` the mouse event `input`, over its cell's middle, as the
/// program's modes encode it: of a program that takes no mouse, a scroll
/// scrolls the pane's view, a row a step, and the others do nothing.
pub(super) fn send(pane: &Pane, input: MouseInput) {
    let grid = pane.grid();
    let middle = |cell: u16, size: u16| u32::from(cell) * u32::from(size) + u32::from(size) / 2;
    let event = |action| MouseEvent {
        action,
        mods: input::Mods {
            shift: input.shift,
            control: input.control,
            left_alt: input.alt,
            ..input::Mods::NONE
        },
        col: input.column,
        row: input.row,
        x: middle(input.column, grid.cell_width),
        y: middle(input.row, grid.cell_height),
    };
    let wheel = match input.action {
        MouseDoes::Press => {
            pane.mouse(&event(MouseAction::Press(button(
                input.button.unwrap_or_default(),
            ))));
            return;
        }
        MouseDoes::Release => {
            pane.mouse(&event(MouseAction::Release(button(
                input.button.unwrap_or_default(),
            ))));
            return;
        }
        MouseDoes::Move => {
            pane.mouse(&event(MouseAction::Motion(input.button.map(button))));
            return;
        }
        MouseDoes::ScrollUp => Wheel::Up,
        MouseDoes::ScrollDown => Wheel::Down,
        MouseDoes::ScrollLeft => Wheel::Left,
        MouseDoes::ScrollRight => Wheel::Right,
    };
    let steps = input.steps.unwrap_or(1);
    for _ in 0..steps {
        if !pane.mouse(&event(MouseAction::Wheel(wheel))) {
            match wheel {
                Wheel::Up => pane.scroll(Scroll::Up(usize::from(steps))),
                Wheel::Down => pane.scroll(Scroll::Down(usize::from(steps))),
                Wheel::Left | Wheel::Right => {}
            }
            return;
        }
    }
}

/// The encoder's button of `button`.
fn button(button: MouseButton) -> input::MouseButton {
    match button {
        MouseButton::Left => input::MouseButton::Left,
        MouseButton::Middle => input::MouseButton::Middle,
        MouseButton::Right => input::MouseButton::Right,
    }
}
