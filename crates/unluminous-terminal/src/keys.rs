//! Turning a key press into the bytes a terminal sends.
//!
//! The encoding is Termoil's (`termoil_input`, `task-2489`): the legacy xterm sequences, xterm's
//! `modifyOtherKeys`, and the kitty keyboard protocol with all five of its flags. Which one is used is the
//! program's choice, made with an escape sequence and reported in [`Mode`]: a shell gets the sequences every
//! terminal has sent since the eighties, and `claude` or `codex`, which turn the kitty protocol on, are told
//! `Shift+Enter` apart from `Enter` and `Ctrl+Backspace` apart from `Backspace`.
//!
//! Two older modes change what some keys send, and both are set by the program on the far side rather than
//! chosen here. Application cursor keys turn the arrows and Home and End from `ESC [ A` into `ESC O A`, which
//! is what a full screen program asks for so it can tell an arrow key from a program printing the same
//! characters. Bracketed paste wraps pasted text so that a shell does not run each pasted line as it arrives.
//!
//! There is no dependency on egui here. The window turns egui's key and modifier types into [`Key`] and
//! [`Modifiers`], so this crate stays free of any user interface.

use termoil_input::{
    encode_focus, encode_key, encode_paste, Key as InputKey, KeyEvent, KeyboardModes,
    Modifiers as InputModifiers, NamedKey,
};

/// A key, named the way a terminal thinks of keys rather than the way a keyboard is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Right,
    Left,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    /// A function key, from 1 to 20.
    Function(u8),
    /// An ordinary key, held as the character it produces with no modifiers, in lower case.
    Character(char),
}

/// Which modifiers were held down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
    /// The Apple key. Not a terminal modifier: it belongs to the application, so a key press holding it
    /// sends nothing.
    pub command: bool,
}

impl Modifiers {
    pub const NONE: Self = Self { shift: false, alt: false, control: false, command: false };

    pub const fn control() -> Self {
        Self { control: true, ..Self::NONE }
    }

    pub const fn shift() -> Self {
        Self { shift: true, ..Self::NONE }
    }

    pub const fn alt() -> Self {
        Self { alt: true, ..Self::NONE }
    }

    /// The same set as Termoil writes it.
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

/// What the program on the far side has asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mode {
    /// The arrows and Home and End send `ESC O` sequences instead of `ESC [` ones.
    pub application_cursor: bool,
    /// Pasted text is wrapped in `ESC [ 200 ~` and `ESC [ 201 ~`.
    pub bracketed_paste: bool,
    /// The keypad sends its own sequences (`DECKPAM`).
    pub application_keypad: bool,
    /// Backspace sends `BS` rather than `DEL` (`DECBKM`).
    pub backspace_sends_bs: bool,
    /// xterm's `modifyOtherKeys` level, 0 to 2: how keys with modifiers that have no legacy sequence are sent.
    pub modify_other_keys: u8,
    /// The kitty keyboard protocol's flags: 1 tells keys apart, 2 reports releases, 4 alternate keys, 8 every
    /// key as an escape code, 16 the text a key makes. Zero is the legacy encoding.
    pub kitty_flags: u8,
}

impl Mode {
    /// The modes as Termoil's encoder reads them.
    fn input(&self) -> KeyboardModes {
        KeyboardModes {
            app_cursor: self.application_cursor,
            app_keypad: self.application_keypad,
            modify_other_keys: self.modify_other_keys,
            kitty_flags: self.kitty_flags,
            backarrow_is_bs: self.backspace_sends_bs,
            alt_sends_escape: true,
        }
    }

    /// True when the program asked the kitty protocol to report every key as an escape code, letters included.
    fn every_key_is_a_code(&self) -> bool {
        self.kitty_flags & 8 != 0
    }
}

/// One key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl KeyPress {
    pub const fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    pub const fn plain(key: Key) -> Self {
        Self { key, modifiers: Modifiers::NONE }
    }
}

/// The bytes a key press sends, or nothing when the key sends nothing.
pub fn encode(press: KeyPress, mode: Mode) -> Option<Vec<u8>> {
    let KeyPress { key, modifiers } = press;
    // The Apple key belongs to the application: command and C copies, command and V pastes, and nothing
    // holding it reaches the shell.
    if modifiers.command {
        return None;
    }
    let mods = modifiers.input();
    let event = match key {
        Key::Character(character) => {
            let character = character.to_ascii_lowercase();
            let event = KeyEvent::press(InputKey::Char(character), mods);
            // The text the key makes, which the encoder sends for an ordinary key. None with control held,
            // where the control code is what is sent.
            if modifiers.control {
                event
            } else {
                let shown = if modifiers.shift {
                    character.to_uppercase().next().unwrap_or(character)
                } else {
                    character
                };
                event.with_text(&shown.to_string())
            }
        }
        // Function keys beyond 20 are not on any keyboard Unluminous is used with, and the legacy encoding has
        // no agreed sequence for them.
        Key::Function(number) if !(1..=20).contains(&number) => return None,
        named => KeyEvent::press(InputKey::Named(named_key(named)), mods),
    };
    let mut out = Vec::new();
    (encode_key(&event, &mode.input(), &mut out) && !out.is_empty()).then_some(out)
}

/// The key as Termoil names it. Only called for keys that are not ordinary characters.
fn named_key(key: Key) -> NamedKey {
    match key {
        Key::Enter => NamedKey::Enter,
        Key::Backspace => NamedKey::Backspace,
        Key::Tab => NamedKey::Tab,
        Key::Escape => NamedKey::Escape,
        Key::Up => NamedKey::Up,
        Key::Down => NamedKey::Down,
        Key::Right => NamedKey::Right,
        Key::Left => NamedKey::Left,
        Key::Home => NamedKey::Home,
        Key::End => NamedKey::End,
        Key::PageUp => NamedKey::PageUp,
        Key::PageDown => NamedKey::PageDown,
        Key::Insert => NamedKey::Insert,
        Key::Delete => NamedKey::Delete,
        Key::Function(number) => NamedKey::F(number),
        Key::Character(_) => NamedKey::Escape,
    }
}

/// The bytes for text the keyboard typed: a letter, a word from an input method, a character a dead key made.
///
/// The text itself, which is what every program expects, unless the program asked the kitty protocol to report
/// every key as an escape code. Then each character is sent as the key that makes it, with the character as its
/// text, which is what such a program reads.
pub fn text(typed: &str, mode: Mode) -> Vec<u8> {
    if !mode.every_key_is_a_code() {
        return typed.as_bytes().to_vec();
    }
    let mut out = Vec::new();
    for character in typed.chars() {
        let lower = character.to_lowercase().next().unwrap_or(character);
        let mods = if lower != character { InputModifiers::SHIFT } else { InputModifiers::NONE };
        let event = KeyEvent::press(InputKey::Char(lower), mods).with_text(&character.to_string());
        if !encode_key(&event, &mode.input(), &mut out) {
            let mut buffer = [0_u8; 4];
            out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
        }
    }
    out
}

/// The bytes for text arriving from the clipboard.
///
/// Wrapped in the bracketed paste sequences when the program asked for them, so that a shell takes several
/// pasted lines as text to edit rather than as commands to run one after another. Line breaks are turned into
/// carriage returns, because a terminal treats a carriage return as Return being pressed and pasted text with
/// both would run every line twice.
pub fn paste(text: &str, mode: Mode) -> Vec<u8> {
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    let mut out = Vec::new();
    encode_paste(&text, mode.bracketed_paste, &mut out);
    out
}

/// The bytes that tell a program the terminal has gained or lost the keyboard, when it asked to be told.
pub fn focus(gained: bool) -> Vec<u8> {
    let mut out = Vec::new();
    encode_focus(gained, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(key: Key, modifiers: Modifiers, mode: Mode) -> Vec<u8> {
        encode(KeyPress::new(key, modifiers), mode)
            .unwrap_or_else(|| panic!("{key:?} should send something"))
    }

    fn text(key: Key, modifiers: Modifiers, mode: Mode) -> String {
        String::from_utf8_lossy(&bytes(key, modifiers, mode)).replace('\x1b', "ESC ")
    }

    #[test]
    fn return_sends_a_carriage_return_and_backspace_sends_delete() {
        assert_eq!(bytes(Key::Enter, Modifiers::NONE, Mode::default()), vec![0x0d]);
        assert_eq!(bytes(Key::Backspace, Modifiers::NONE, Mode::default()), vec![0x7f]);
        assert_eq!(bytes(Key::Tab, Modifiers::NONE, Mode::default()), vec![0x09]);
        assert_eq!(bytes(Key::Escape, Modifiers::NONE, Mode::default()), vec![0x1b]);
    }

    #[test]
    fn alt_and_backspace_puts_escape_in_front_of_it() {
        assert_eq!(bytes(Key::Backspace, Modifiers::alt(), Mode::default()), vec![0x1b, 0x7f]);
    }

    #[test]
    fn shift_and_tab_sends_a_back_tab() {
        assert_eq!(text(Key::Tab, Modifiers::shift(), Mode::default()), "ESC [Z");
    }

    #[test]
    fn the_arrows_are_the_ordinary_sequences() {
        let mode = Mode::default();
        assert_eq!(text(Key::Up, Modifiers::NONE, mode), "ESC [A");
        assert_eq!(text(Key::Down, Modifiers::NONE, mode), "ESC [B");
        assert_eq!(text(Key::Right, Modifiers::NONE, mode), "ESC [C");
        assert_eq!(text(Key::Left, Modifiers::NONE, mode), "ESC [D");
        assert_eq!(text(Key::Home, Modifiers::NONE, mode), "ESC [H");
        assert_eq!(text(Key::End, Modifiers::NONE, mode), "ESC [F");
    }

    #[test]
    fn application_cursor_keys_turn_the_bracket_into_an_o() {
        let mode = Mode { application_cursor: true, ..Mode::default() };
        assert_eq!(text(Key::Up, Modifiers::NONE, mode), "ESC OA");
        assert_eq!(text(Key::Left, Modifiers::NONE, mode), "ESC OD");
        assert_eq!(text(Key::Home, Modifiers::NONE, mode), "ESC OH");
        assert_eq!(text(Key::End, Modifiers::NONE, mode), "ESC OF");
    }

    #[test]
    fn a_modified_arrow_carries_the_modifier_as_a_number() {
        let mode = Mode::default();
        assert_eq!(text(Key::Left, Modifiers::shift(), mode), "ESC [1;2D");
        assert_eq!(text(Key::Left, Modifiers::alt(), mode), "ESC [1;3D");
        assert_eq!(text(Key::Left, Modifiers::control(), mode), "ESC [1;5D");
        let shift_control = Modifiers { shift: true, control: true, ..Modifiers::NONE };
        assert_eq!(text(Key::Left, shift_control, mode), "ESC [1;6D");
        let all = Modifiers { shift: true, control: true, alt: true, command: false };
        assert_eq!(text(Key::Left, all, mode), "ESC [1;8D");
    }

    #[test]
    fn a_modified_arrow_keeps_the_bracket_form_even_in_application_cursor_keys() {
        // There is nowhere to put the modifier in the `ESC O` form, so it is not used.
        let mode = Mode { application_cursor: true, ..Mode::default() };
        assert_eq!(text(Key::Left, Modifiers::control(), mode), "ESC [1;5D");
    }

    #[test]
    fn the_tilde_keys_are_numbered() {
        let mode = Mode::default();
        assert_eq!(text(Key::Insert, Modifiers::NONE, mode), "ESC [2~");
        assert_eq!(text(Key::Delete, Modifiers::NONE, mode), "ESC [3~");
        assert_eq!(text(Key::PageUp, Modifiers::NONE, mode), "ESC [5~");
        assert_eq!(text(Key::PageDown, Modifiers::NONE, mode), "ESC [6~");
        assert_eq!(text(Key::Delete, Modifiers::control(), mode), "ESC [3;5~");
    }

    #[test]
    fn the_function_keys_follow_the_standard_including_its_gaps() {
        let mode = Mode::default();
        assert_eq!(text(Key::Function(1), Modifiers::NONE, mode), "ESC OP");
        assert_eq!(text(Key::Function(4), Modifiers::NONE, mode), "ESC OS");
        assert_eq!(text(Key::Function(5), Modifiers::NONE, mode), "ESC [15~");
        assert_eq!(text(Key::Function(6), Modifiers::NONE, mode), "ESC [17~");
        assert_eq!(text(Key::Function(10), Modifiers::NONE, mode), "ESC [21~");
        assert_eq!(text(Key::Function(11), Modifiers::NONE, mode), "ESC [23~");
        assert_eq!(text(Key::Function(12), Modifiers::NONE, mode), "ESC [24~");
        assert_eq!(encode(KeyPress::plain(Key::Function(30)), mode), None, "there is no F30");
    }

    #[test]
    fn control_and_a_letter_is_the_control_code() {
        let mode = Mode::default();
        assert_eq!(bytes(Key::Character('c'), Modifiers::control(), mode), vec![0x03], "Control C");
        assert_eq!(bytes(Key::Character('d'), Modifiers::control(), mode), vec![0x04], "Control D");
        assert_eq!(bytes(Key::Character('a'), Modifiers::control(), mode), vec![0x01]);
        assert_eq!(bytes(Key::Character('z'), Modifiers::control(), mode), vec![0x1a]);
        // Held with shift as well, which is what happens on a keyboard with caps lock on.
        let shift_control = Modifiers { shift: true, control: true, ..Modifiers::NONE };
        assert_eq!(bytes(Key::Character('C'), shift_control, mode), vec![0x03]);
    }

    #[test]
    fn control_and_a_symbol_is_the_code_after_z() {
        let mode = Mode::default();
        assert_eq!(bytes(Key::Character('['), Modifiers::control(), mode), vec![0x1b]);
        assert_eq!(bytes(Key::Character('\\'), Modifiers::control(), mode), vec![0x1c]);
        assert_eq!(bytes(Key::Character(']'), Modifiers::control(), mode), vec![0x1d]);
        assert_eq!(bytes(Key::Character(' '), Modifiers::control(), mode), vec![0x00]);
    }

    #[test]
    fn alt_and_a_letter_puts_escape_in_front() {
        let mode = Mode::default();
        assert_eq!(bytes(Key::Character('f'), Modifiers::alt(), mode), vec![0x1b, b'f']);
    }

    #[test]
    fn the_apple_key_sends_nothing_because_it_belongs_to_the_application() {
        let mode = Mode::default();
        let command = Modifiers { command: true, ..Modifiers::NONE };
        assert_eq!(encode(KeyPress::new(Key::Character('c'), command), mode), None);
        assert_eq!(encode(KeyPress::new(Key::Character('v'), command), mode), None);
        assert_eq!(encode(KeyPress::new(Key::Left, command), mode), None);
    }

    #[test]
    fn pasted_text_is_wrapped_only_when_the_program_asked_for_it() {
        let plain = Mode::default();
        assert_eq!(paste("one two", plain), b"one two".to_vec());
        let bracketed = Mode { bracketed_paste: true, ..Mode::default() };
        assert_eq!(
            String::from_utf8_lossy(&paste("one two", bracketed)).replace('\x1b', "ESC "),
            "ESC [200~one twoESC [201~"
        );
    }

    #[test]
    fn pasted_line_breaks_become_carriage_returns() {
        let mode = Mode::default();
        assert_eq!(paste("one\ntwo", mode), b"one\rtwo".to_vec());
        assert_eq!(paste("one\r\ntwo", mode), b"one\rtwo".to_vec(), "and not two of them");
    }

    #[test]
    fn a_focus_report_says_which_way_round_it_is() {
        assert_eq!(String::from_utf8_lossy(&focus(true)).replace('\x1b', "ESC "), "ESC [I");
        assert_eq!(String::from_utf8_lossy(&focus(false)).replace('\x1b', "ESC "), "ESC [O");
    }

    #[test]
    fn shift_and_return_is_told_apart_when_the_program_asks_for_the_kitty_protocol() {
        // `task-2489`. `claude` and `codex` turn the kitty keyboard protocol on and read `Shift+Enter` as a new
        // line in the prompt. With alacritty's encoder it reached them as a plain carriage return.
        let kitty = Mode { kitty_flags: 1, ..Mode::default() };
        assert_eq!(text(Key::Enter, Modifiers::shift(), kitty), "ESC [13;2u");
        assert_eq!(
            text(Key::Enter, Modifiers::NONE, kitty),
            "\r",
            "a plain Return is still a carriage return"
        );
        assert_eq!(
            text(Key::Escape, Modifiers::NONE, kitty),
            "ESC [27u",
            "and Escape is told apart from a sequence"
        );
    }

    #[test]
    fn shift_and_return_is_told_apart_under_modify_other_keys() {
        let modify = Mode { modify_other_keys: 2, ..Mode::default() };
        assert_eq!(text(Key::Enter, Modifiers::shift(), modify), "ESC [27;2;13~");
    }

    #[test]
    fn control_and_backspace_is_told_apart_under_the_kitty_protocol() {
        let kitty = Mode { kitty_flags: 1, ..Mode::default() };
        assert_eq!(text(Key::Backspace, Modifiers::control(), kitty), "ESC [127;5u");
    }

    #[test]
    fn typed_text_is_sent_as_itself_unless_every_key_is_to_be_a_code() {
        assert_eq!(super::text("héllo", Mode::default()), "héllo".as_bytes().to_vec());
        let every = Mode { kitty_flags: 1 | 8, ..Mode::default() };
        let sent = String::from_utf8_lossy(&super::text("a", every)).replace('\x1b', "ESC ");
        assert_eq!(sent, "ESC [97u", "a letter is the key that makes it");
    }

    #[test]
    fn a_paste_cannot_end_its_own_bracket() {
        // A pasted `ESC [ 201 ~` would end the bracket early and run what follows as typed commands.
        let bracketed = Mode { bracketed_paste: true, ..Mode::default() };
        let sent =
            String::from_utf8_lossy(&paste("a\x1b[201~rm -rf", bracketed)).replace('\x1b', "ESC ");
        assert_eq!(sent, "ESC [200~arm -rfESC [201~");
    }
}
