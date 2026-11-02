//! Telling a program about the mouse.
//!
//! A program that has asked for mouse reporting, which `claude` does, expects a click inside it to arrive
//! as an escape sequence rather than selecting text. The encoding is Termoil's (`termoil_input`, `task-2489`),
//! which writes every form a program can ask for: the X10 and normal tracking modes, button and any motion,
//! and the default, UTF-8, SGR, urxvt and SGR pixel encodings. A program chooses; this module only encodes.
//!
//! Holding shift always selects locally instead of reporting, which is the convention every terminal
//! follows and the only way to copy out of a program that has taken over the mouse. That decision is the
//! window's.

use termoil_input::{
    encode_mouse, Modifiers as InputModifiers, MouseButton, MouseEncoding, MouseEvent,
    MouseEventKind, MouseModes, MouseTracking,
};

/// What the program has asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseMode {
    /// Report presses only, with no modifiers (mode 9).
    pub x10: bool,
    /// Report a press and a release (mode 1000).
    pub report_click: bool,
    /// Report movement while a button is held (mode 1002).
    pub drag: bool,
    /// Report movement even with no button held (mode 1003).
    pub motion: bool,
    /// Use the SGR encoding rather than the older one (mode 1006).
    pub sgr: bool,
    /// SGR with pixel positions (mode 1016). Reported in cells times the cell size, which is what Unluminous has.
    pub sgr_pixels: bool,
    /// The urxvt encoding (mode 1015).
    pub urxvt: bool,
    /// The older encoding with UTF-8 coded positions (mode 1005).
    pub utf8: bool,
}

impl MouseMode {
    /// True when a click should be reported rather than starting a selection.
    pub fn reports_clicks(&self) -> bool {
        self.x10 || self.report_click || self.drag || self.motion
    }

    /// The modes as Termoil's encoder reads them. The most permissive tracking mode the program set wins, and the
    /// most capable encoding.
    fn input(&self) -> MouseModes {
        let tracking = if self.motion {
            MouseTracking::AnyEvent
        } else if self.drag {
            MouseTracking::ButtonEvent
        } else if self.report_click {
            MouseTracking::Normal
        } else if self.x10 {
            MouseTracking::X10
        } else {
            MouseTracking::Off
        };
        let encoding = if self.sgr_pixels {
            MouseEncoding::SgrPixels
        } else if self.sgr {
            MouseEncoding::Sgr
        } else if self.urxvt {
            MouseEncoding::Urxvt
        } else if self.utf8 {
            MouseEncoding::Utf8
        } else {
            MouseEncoding::Default
        };
        MouseModes { tracking, encoding }
    }
}

/// Which button, as a terminal numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

impl Button {
    fn input(self) -> MouseButton {
        match self {
            Button::Left => MouseButton::Left,
            Button::Middle => MouseButton::Middle,
            Button::Right => MouseButton::Right,
            Button::WheelUp => MouseButton::WheelUp,
            Button::WheelDown => MouseButton::WheelDown,
        }
    }
}

/// The modifiers a mouse report carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
}

impl Modifiers {
    fn input(&self) -> InputModifiers {
        let mut bits = 0;
        if self.shift {
            bits |= InputModifiers::SHIFT.0;
        }
        if self.alt {
            bits |= InputModifiers::ALT.0;
        }
        if self.control {
            bits |= InputModifiers::CTRL.0;
        }
        InputModifiers(bits)
    }
}

/// A press, a release, or movement with a button held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Press,
    Release,
    Drag,
}

/// The size of one cell in pixels that a pixel position report counts in. Unluminous reports where in a cell
/// the pointer is no finer than the cell, so the corner of the cell is what is said.
const PIXELS_A_CELL: (u32, u32) = (8, 16);

/// The bytes that tell the program about one mouse event.
///
/// `row` and `column` are counted from zero, as the grid counts them; the sequences count from one.
/// Returns nothing when the program has not asked to be told, or when the position cannot be expressed in
/// the encoding it asked for.
pub fn report(
    mode: MouseMode,
    kind: Kind,
    button: Button,
    row: usize,
    column: usize,
    modifiers: Modifiers,
) -> Option<Vec<u8>> {
    let wheel = matches!(button, Button::WheelUp | Button::WheelDown);
    // The wheel has no release, and reporting one would look like a second turn.
    if wheel && kind != Kind::Press {
        return None;
    }
    let event = MouseEvent {
        kind: match kind {
            Kind::Press => MouseEventKind::Press,
            Kind::Release => MouseEventKind::Release,
            Kind::Drag => MouseEventKind::Motion,
        },
        button: button.input(),
        col: column as u32,
        row: row as u32,
        pixel_x: column as u32 * PIXELS_A_CELL.0,
        pixel_y: row as u32 * PIXELS_A_CELL.1,
        mods: modifiers.input(),
    };
    let mut out = Vec::new();
    encode_mouse(&event, &mode.input(), None, &mut out).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sgr() -> MouseMode {
        MouseMode {
            report_click: true,
            drag: true,
            motion: false,
            sgr: true,
            ..MouseMode::default()
        }
    }

    fn text(bytes: Vec<u8>) -> String {
        String::from_utf8_lossy(&bytes).replace('\x1b', "ESC ")
    }

    #[test]
    fn nothing_is_sent_when_the_program_has_not_asked() {
        let quiet = MouseMode::default();
        assert_eq!(
            report(quiet, Kind::Press, Button::Left, 0, 0, Modifiers::default()),
            None,
            "a click in a program that has not asked about the mouse is for selecting text"
        );
    }

    #[test]
    fn a_press_and_a_release_are_told_apart_by_the_last_letter() {
        let press =
            report(sgr(), Kind::Press, Button::Left, 4, 9, Modifiers::default()).expect("a press");
        assert_eq!(text(press), "ESC [<0;10;5M");
        let release = report(sgr(), Kind::Release, Button::Left, 4, 9, Modifiers::default())
            .expect("a release");
        assert_eq!(text(release), "ESC [<0;10;5m", "a small m is a release");
    }

    #[test]
    fn the_three_buttons_are_numbered_from_zero() {
        let at = |button| {
            text(report(sgr(), Kind::Press, button, 0, 0, Modifiers::default()).expect("a press"))
        };
        assert_eq!(at(Button::Left), "ESC [<0;1;1M");
        assert_eq!(at(Button::Middle), "ESC [<1;1;1M");
        assert_eq!(at(Button::Right), "ESC [<2;1;1M");
    }

    #[test]
    fn the_wheel_is_reported_as_the_high_numbered_buttons_and_has_no_release() {
        let up = report(sgr(), Kind::Press, Button::WheelUp, 0, 0, Modifiers::default())
            .expect("a turn");
        assert_eq!(text(up), "ESC [<64;1;1M");
        let down = report(sgr(), Kind::Press, Button::WheelDown, 0, 0, Modifiers::default())
            .expect("a turn");
        assert_eq!(text(down), "ESC [<65;1;1M");
        assert_eq!(
            report(sgr(), Kind::Release, Button::WheelUp, 0, 0, Modifiers::default()),
            None,
            "a wheel release would look like a second turn"
        );
    }

    #[test]
    fn a_drag_adds_thirty_two_and_is_only_sent_when_the_program_asked_for_movement() {
        let dragging =
            report(sgr(), Kind::Drag, Button::Left, 1, 1, Modifiers::default()).expect("a drag");
        assert_eq!(text(dragging), "ESC [<32;2;2M");
        let clicks_only = MouseMode {
            report_click: true,
            drag: false,
            motion: false,
            sgr: true,
            ..MouseMode::default()
        };
        assert_eq!(report(clicks_only, Kind::Drag, Button::Left, 1, 1, Modifiers::default()), None);
    }

    #[test]
    fn the_modifiers_are_added_to_the_button() {
        let control = Modifiers { control: true, ..Modifiers::default() };
        let bytes = report(sgr(), Kind::Press, Button::Left, 0, 0, control).expect("a press");
        assert_eq!(text(bytes), "ESC [<16;1;1M", "control adds sixteen");
        let both = Modifiers { control: true, shift: true, alt: false };
        let bytes = report(sgr(), Kind::Press, Button::Left, 0, 0, both).expect("a press");
        assert_eq!(text(bytes), "ESC [<20;1;1M");
    }

    #[test]
    fn the_older_encoding_puts_the_position_in_one_byte_each() {
        let old = MouseMode {
            report_click: true,
            drag: false,
            motion: false,
            sgr: false,
            ..MouseMode::default()
        };
        let bytes =
            report(old, Kind::Press, Button::Left, 4, 9, Modifiers::default()).expect("a press");
        assert_eq!(bytes, vec![0x1b, b'[', b'M', 32, 32 + 10, 32 + 5]);
    }

    #[test]
    fn the_older_encoding_says_nothing_rather_than_something_wrong_past_its_limit() {
        let old = MouseMode {
            report_click: true,
            drag: false,
            motion: false,
            sgr: false,
            ..MouseMode::default()
        };
        assert_eq!(report(old, Kind::Press, Button::Left, 0, 300, Modifiers::default()), None);
        // The same click in SGR is fine, because it counts in numbers rather than in bytes.
        let bytes = report(sgr(), Kind::Press, Button::Left, 0, 300, Modifiers::default())
            .expect("a press");
        assert_eq!(text(bytes), "ESC [<0;301;1M");
    }
}
