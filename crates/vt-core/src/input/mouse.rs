//! Mouse reports, as xterm sends them:
//! <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Mouse-Tracking>.

use std::io::Write;

use super::keyboard::Mods;
use crate::terminal::{InputModes, MouseEncoding, MouseTracking};

/// Something the mouse did, over a cell of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    /// What it did.
    pub action: MouseAction,
    /// The modifiers held: Shift, Control, and either Alt, which reports
    /// count as Meta.
    pub mods: Mods,
    /// The column of the cell under the pointer, from 0.
    pub col: u16,
    /// The row of the cell under the pointer, from 0.
    pub row: u16,
    /// The pointer's distance from the left of the cells, in pixels, for
    /// reports in pixels.
    pub x: u32,
    /// The pointer's distance from the top of the cells, in pixels.
    pub y: u32,
}

/// What the mouse did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    /// A button went down.
    Press(MouseButton),
    /// A button went up.
    Release(MouseButton),
    /// The pointer moved, with a button held, the lowest-numbered of those
    /// held, or none. The platform reports a motion only when the cell
    /// changes, or the pixel, for reports in pixels.
    Motion(Option<MouseButton>),
    /// The wheel turned a step.
    Wheel(Wheel),
}

/// A button of the mouse, numbered as X11 numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    /// Button 1.
    Left,
    /// Button 2.
    Middle,
    /// Button 3.
    Right,
    /// Button 8.
    Back,
    /// Button 9.
    Forward,
    /// Button 10.
    Button10,
    /// Button 11.
    Button11,
}

/// Which way the wheel turned, or was tilted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wheel {
    /// Up, X11's button 4.
    Up,
    /// Down, button 5.
    Down,
    /// Left, button 6.
    Left,
    /// Right, button 7.
    Right,
}

/// The largest column or row, from 0, that the X10 encoding has a byte
/// for: those past it are sent as it, which is the byte 0.
const X10_LIMIT: u16 = 255 - 32;

/// Appends to `out` what `event` sends with the mouse modes in `modes`,
/// on the alternate screen if `alternate_screen`, and returns whether the
/// event goes to the program, rather than to the terminal, which can
/// select text or scroll with it.
///
/// While the program tracks the mouse, every event goes to it, though the
/// mode it asked for may report nothing of some. Without tracking, on the
/// alternate screen, alternate scroll (mode 1007) makes the vertical wheel
/// send the up and down cursor keys, a step each.
pub fn encode_mouse(
    event: &MouseEvent,
    modes: &InputModes,
    alternate_screen: bool,
    out: &mut Vec<u8>,
) -> bool {
    let tracking = modes.mouse_tracking;
    if tracking == MouseTracking::None {
        let MouseAction::Wheel(wheel @ (Wheel::Up | Wheel::Down)) = event.action else {
            return false;
        };
        if !(alternate_screen && modes.alternate_scroll) {
            return false;
        }
        let introducer = if modes.cursor_keys_application {
            b'O'
        } else {
            b'['
        };
        let final_byte = if wheel == Wheel::Up { b'A' } else { b'B' };
        out.extend_from_slice(&[0x1B, introducer, final_byte]);
        return true;
    }
    let Some(code) = code(event, modes) else {
        return true;
    };
    match modes.mouse_encoding {
        MouseEncoding::X10 => {
            let position = |at: u16| match at.min(X10_LIMIT) {
                X10_LIMIT => 0,
                // At most 222, so the byte holds 33 more.
                at => 33 + at as u8,
            };
            out.extend_from_slice(&[
                0x1B,
                b'[',
                b'M',
                32 + code,
                position(event.col),
                position(event.row),
            ]);
        }
        MouseEncoding::Sgr | MouseEncoding::SgrPixels => {
            let (x, y) = if modes.mouse_encoding == MouseEncoding::Sgr {
                (u32::from(event.col), u32::from(event.row))
            } else {
                (event.x, event.y)
            };
            let release = matches!(event.action, MouseAction::Release(_));
            let final_byte = if release { 'm' } else { 'M' };
            write!(
                out,
                "\x1b[<{code};{};{}{final_byte}",
                u64::from(x) + 1,
                u64::from(y) + 1
            )
            .expect("writing to a Vec");
        }
    }
    true
}

/// The code of the button, the motion and the modifiers that `event`
/// reports with `modes`, or `None` if it reports nothing of it.
fn code(event: &MouseEvent, modes: &InputModes) -> Option<u8> {
    let tracking = modes.mouse_tracking;
    let x10 = tracking == MouseTracking::X10;
    let (button, motion) = match event.action {
        MouseAction::Press(button) => (button_code(button), false),
        // The X10 encoding says only that a button went up.
        MouseAction::Release(_) if !x10 && modes.mouse_encoding == MouseEncoding::X10 => (3, false),
        MouseAction::Release(button) if !x10 => (button_code(button), false),
        MouseAction::Wheel(wheel) => (wheel_code(wheel), false),
        MouseAction::Motion(held)
            if tracking == MouseTracking::AnyEvent
                || (tracking == MouseTracking::ButtonEvent && held.is_some()) =>
        {
            (held.map_or(3, button_code), true)
        }
        _ => return None,
    };
    if x10 {
        // X10 reported the presses of the first three buttons, without
        // modifiers.
        return (button <= 2).then_some(button);
    }
    let mods = event.mods;
    Some(
        button
            + 32 * u8::from(motion)
            + 4 * u8::from(mods.shift)
            + 8 * u8::from(mods.left_alt || mods.right_alt)
            + 16 * u8::from(mods.control),
    )
}

/// A button's code: the first three are 0 to 2, and buttons 8 to 11 are
/// 128 to 131.
fn button_code(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::Back => 128,
        MouseButton::Forward => 129,
        MouseButton::Button10 => 130,
        MouseButton::Button11 => 131,
    }
}

/// The wheel's codes, 64 to 67, those of buttons 4 to 7.
fn wheel_code(wheel: Wheel) -> u8 {
    match wheel {
        Wheel::Up => 64,
        Wheel::Down => 65,
        Wheel::Left => 66,
        Wheel::Right => 67,
    }
}

/// Appends to `out` what the terminal's gaining or losing the focus sends:
/// `CSI I` or `CSI O`, when the program asked for them (mode 1004).
pub fn encode_focus(focused: bool, modes: &InputModes, out: &mut Vec<u8>) {
    if modes.focus_events {
        out.extend_from_slice(if focused { b"\x1b[I" } else { b"\x1b[O" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: MouseAction = MouseAction::Press(MouseButton::Left);

    fn modes(tracking: MouseTracking, encoding: MouseEncoding) -> InputModes {
        InputModes {
            mouse_tracking: tracking,
            mouse_encoding: encoding,
            ..InputModes::default()
        }
    }

    fn event(action: MouseAction, col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            action,
            mods: Mods::NONE,
            col,
            row,
            x: 0,
            y: 0,
        }
    }

    /// What `event` sends with `modes` on the primary screen, if the
    /// program takes it.
    fn send(event: MouseEvent, modes: &InputModes) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        encode_mouse(&event, modes, false, &mut out).then_some(out)
    }

    /// The X10 encoding of `code` at the cell `col`, `row`.
    fn x10(code: u8, col: u8, row: u8) -> Vec<u8> {
        vec![0x1B, b'[', b'M', 32 + code, 33 + col, 33 + row]
    }

    fn sgr(text: &str) -> Option<Vec<u8>> {
        Some(format!("\x1b[<{text}").into_bytes())
    }

    #[test]
    fn x10_mode_reports_presses_of_the_first_three_buttons() {
        let x10_mode = modes(MouseTracking::X10, MouseEncoding::X10);
        assert_eq!(send(event(LEFT, 0, 0), &x10_mode), Some(x10(0, 0, 0)));
        let right = event(MouseAction::Press(MouseButton::Right), 9, 4);
        assert_eq!(send(right, &x10_mode), Some(x10(2, 9, 4)));
        let shifted = MouseEvent {
            mods: Mods {
                shift: true,
                control: true,
                ..Mods::NONE
            },
            ..event(LEFT, 0, 0)
        };
        assert_eq!(send(shifted, &x10_mode), Some(x10(0, 0, 0)), "no modifiers");
        // The rest goes to the program, which asked for nothing of it.
        for action in [
            MouseAction::Release(MouseButton::Left),
            MouseAction::Wheel(Wheel::Up),
            MouseAction::Press(MouseButton::Back),
            MouseAction::Motion(Some(MouseButton::Left)),
            MouseAction::Motion(None),
        ] {
            assert_eq!(
                send(event(action, 0, 0), &x10_mode),
                Some(Vec::new()),
                "{action:?}"
            );
        }
        let sgr_x10 = modes(MouseTracking::X10, MouseEncoding::Sgr);
        assert_eq!(send(event(LEFT, 0, 0), &sgr_x10), sgr("0;1;1M"));
    }

    #[test]
    fn normal_tracking_reports_presses_releases_and_modifiers() {
        let normal = modes(MouseTracking::Normal, MouseEncoding::X10);
        let at = |action| send(event(action, 0, 0), &normal);
        let with = |mods, action| {
            send(
                MouseEvent {
                    mods,
                    ..event(action, 0, 0)
                },
                &normal,
            )
        };
        assert_eq!(at(LEFT), Some(x10(0, 0, 0)));
        assert_eq!(
            at(MouseAction::Press(MouseButton::Middle)),
            Some(x10(1, 0, 0))
        );
        // A release says only that a button went up.
        assert_eq!(
            at(MouseAction::Release(MouseButton::Left)),
            Some(x10(3, 0, 0))
        );
        assert_eq!(
            at(MouseAction::Release(MouseButton::Right)),
            Some(x10(3, 0, 0))
        );
        assert_eq!(
            at(MouseAction::Release(MouseButton::Back)),
            Some(x10(3, 0, 0))
        );
        let shift = Mods {
            shift: true,
            ..Mods::NONE
        };
        let alt = Mods {
            right_alt: true,
            ..Mods::NONE
        };
        let control = Mods {
            control: true,
            ..Mods::NONE
        };
        let all = Mods {
            shift: true,
            left_alt: true,
            control: true,
            ..Mods::NONE
        };
        assert_eq!(with(shift, LEFT), Some(x10(4, 0, 0)));
        assert_eq!(with(alt, LEFT), Some(x10(8, 0, 0)));
        assert_eq!(with(control, LEFT), Some(x10(16, 0, 0)));
        assert_eq!(
            with(all, MouseAction::Release(MouseButton::Left)),
            Some(x10(31, 0, 0))
        );
        let wheels = [
            (Wheel::Up, 64),
            (Wheel::Down, 65),
            (Wheel::Left, 66),
            (Wheel::Right, 67),
        ];
        for (wheel, code) in wheels {
            assert_eq!(
                at(MouseAction::Wheel(wheel)),
                Some(x10(code, 0, 0)),
                "{wheel:?}"
            );
        }
        let buttons = [
            (MouseButton::Back, 128),
            (MouseButton::Forward, 129),
            (MouseButton::Button10, 130),
            (MouseButton::Button11, 131),
        ];
        for (button, code) in buttons {
            assert_eq!(
                at(MouseAction::Press(button)),
                Some(x10(code, 0, 0)),
                "{button:?}"
            );
        }
        assert_eq!(
            at(MouseAction::Motion(Some(MouseButton::Left))),
            Some(Vec::new())
        );
        assert_eq!(
            send(event(LEFT, 41, 7), &normal),
            Some(x10(0, 41, 7)),
            "positions count from 1, after 32"
        );
    }

    #[test]
    fn button_event_tracking_reports_motion_with_a_button_held() {
        let button = modes(MouseTracking::ButtonEvent, MouseEncoding::X10);
        // xterm's examples: `CSI M @ Cx Cy` with button 1, `CSI M B` with 3.
        let motion = |held| send(event(MouseAction::Motion(held), 0, 0), &button).unwrap();
        assert_eq!(motion(Some(MouseButton::Left))[3], b'@');
        assert_eq!(motion(Some(MouseButton::Right))[3], b'B');
        assert_eq!(motion(None), Vec::<u8>::new());
        assert_eq!(send(event(LEFT, 0, 0), &button), Some(x10(0, 0, 0)));
    }

    #[test]
    fn any_event_tracking_reports_every_motion() {
        let any = modes(MouseTracking::AnyEvent, MouseEncoding::X10);
        let motion = |held| send(event(MouseAction::Motion(held), 2, 3), &any);
        assert_eq!(motion(None), Some(x10(35, 2, 3)));
        assert_eq!(motion(Some(MouseButton::Middle)), Some(x10(33, 2, 3)));
        let sgr_any = modes(MouseTracking::AnyEvent, MouseEncoding::Sgr);
        assert_eq!(
            send(event(MouseAction::Motion(None), 2, 3), &sgr_any),
            sgr("35;3;4M")
        );
    }

    #[test]
    fn sgr_reports_which_button_went_up_and_any_position() {
        let normal = modes(MouseTracking::Normal, MouseEncoding::Sgr);
        let at = |action, col, row| send(event(action, col, row), &normal);
        assert_eq!(at(LEFT, 0, 0), sgr("0;1;1M"));
        assert_eq!(
            at(MouseAction::Release(MouseButton::Left), 0, 0),
            sgr("0;1;1m")
        );
        assert_eq!(
            at(MouseAction::Release(MouseButton::Right), 0, 0),
            sgr("2;1;1m")
        );
        assert_eq!(
            at(MouseAction::Release(MouseButton::Back), 0, 0),
            sgr("128;1;1m")
        );
        assert_eq!(at(LEFT, 300, 500), sgr("0;301;501M"));
        assert_eq!(at(LEFT, u16::MAX, u16::MAX), sgr("0;65536;65536M"));
        assert_eq!(at(MouseAction::Wheel(Wheel::Up), 4, 5), sgr("64;5;6M"));
        let control = MouseEvent {
            mods: Mods {
                control: true,
                ..Mods::NONE
            },
            ..event(MouseAction::Wheel(Wheel::Down), 0, 0)
        };
        assert_eq!(send(control, &normal), sgr("81;1;1M"));
        // The cell is what counts, not the pixel.
        let pixel = MouseEvent {
            x: 700,
            y: 900,
            ..event(LEFT, 1, 2)
        };
        assert_eq!(send(pixel, &normal), sgr("0;2;3M"));
    }

    #[test]
    fn sgr_pixels_reports_the_pixel_from_1() {
        let pixels = modes(MouseTracking::Normal, MouseEncoding::SgrPixels);
        let at = |action, x, y| {
            send(
                MouseEvent {
                    x,
                    y,
                    ..event(action, 3, 4)
                },
                &pixels,
            )
        };
        assert_eq!(at(LEFT, 0, 0), sgr("0;1;1M"));
        assert_eq!(at(LEFT, 1234, 567), sgr("0;1235;568M"));
        assert_eq!(
            at(MouseAction::Release(MouseButton::Left), 9, 9),
            sgr("0;10;10m")
        );
        assert_eq!(at(LEFT, u32::MAX, 0), sgr("0;4294967296;1M"));
    }

    #[test]
    fn x10_positions_past_its_limit_are_the_byte_0() {
        let normal = modes(MouseTracking::Normal, MouseEncoding::X10);
        let at = |col, row| send(event(LEFT, col, row), &normal).unwrap();
        assert_eq!(at(222, 0), vec![0x1B, b'[', b'M', 32, 255, 33]);
        assert_eq!(at(223, 0), vec![0x1B, b'[', b'M', 32, 0, 33]);
        assert_eq!(at(1000, 5000), vec![0x1B, b'[', b'M', 32, 0, 0]);
        // The largest code still fits in its byte.
        let largest = MouseEvent {
            mods: Mods {
                shift: true,
                left_alt: true,
                control: true,
                ..Mods::NONE
            },
            ..event(MouseAction::Motion(Some(MouseButton::Button11)), 0, 0)
        };
        let any = modes(MouseTracking::AnyEvent, MouseEncoding::X10);
        assert_eq!(send(largest, &any).unwrap()[3], 32 + 191);
    }

    #[test]
    fn without_tracking_the_mouse_is_the_terminal_s() {
        let none = InputModes::default();
        let up = event(MouseAction::Wheel(Wheel::Up), 0, 0);
        let mut out = Vec::new();
        assert!(!encode_mouse(&event(LEFT, 0, 0), &none, false, &mut out));
        assert!(!encode_mouse(&up, &none, false, &mut out));
        assert!(!encode_mouse(&up, &none, true, &mut out), "without 1007");
        assert!(out.is_empty());
        // Alternate scroll: on the alternate screen, the wheel sends the
        // cursor keys.
        let scroll = InputModes {
            alternate_scroll: true,
            ..InputModes::default()
        };
        let wheel = |wheel, modes: &InputModes, alternate| {
            let mut out = Vec::new();
            let event = event(MouseAction::Wheel(wheel), 0, 0);
            encode_mouse(&event, modes, alternate, &mut out).then_some(out)
        };
        assert_eq!(wheel(Wheel::Up, &scroll, true), Some(b"\x1b[A".to_vec()));
        assert_eq!(wheel(Wheel::Down, &scroll, true), Some(b"\x1b[B".to_vec()));
        assert_eq!(
            wheel(Wheel::Up, &scroll, false),
            None,
            "the primary screen scrolls"
        );
        assert_eq!(wheel(Wheel::Left, &scroll, true), None);
        let application = InputModes {
            cursor_keys_application: true,
            ..scroll
        };
        assert_eq!(
            wheel(Wheel::Down, &application, true),
            Some(b"\x1bOB".to_vec())
        );
        assert!(!encode_mouse(&event(LEFT, 0, 0), &scroll, true, &mut out));
        // With tracking, the wheel is reported.
        let tracked = InputModes {
            mouse_tracking: MouseTracking::Normal,
            ..scroll
        };
        assert_eq!(wheel(Wheel::Up, &tracked, true), Some(x10(64, 0, 0)));
    }

    #[test]
    fn focus_events_come_when_asked_for() {
        let mut out = Vec::new();
        encode_focus(true, &InputModes::default(), &mut out);
        assert!(out.is_empty());
        let focus = InputModes {
            focus_events: true,
            ..InputModes::default()
        };
        encode_focus(true, &focus, &mut out);
        assert_eq!(out, b"\x1b[I");
        out.clear();
        encode_focus(false, &focus, &mut out);
        assert_eq!(out, b"\x1b[O");
    }
}
