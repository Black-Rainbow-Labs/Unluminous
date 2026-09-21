//! Feeding the window the events a mouse and a keyboard produce, without either.
//!
//! `tasks/task-1914-testing-without-stealing-focus-tdd.md` is the design and the reason. The short of
//! it: **synthetic operating system input goes to whatever window is in front**, so a script that
//! wanted to click something in Unluminous had to bring Unluminous to the front — and on Windows
//! activating a window that is on another virtual desktop switches the desktop with it. That is
//! `task-1914`'s report, and it is the one hole left in `task-1848`'s rule that driving the real
//! window must never take the keyboard out of whatever a person is typing into.
//!
//! What goes in instead is [`egui::Event`], which is what `egui-winit` builds out of a real device.
//! It arrives down the control channel, so the window need not be in front, need not be on this
//! desktop, and the person's pointer never moves.
//!
//! ## One step a frame, and why it cannot be one
//!
//! A click is not one event. `egui` decides a widget was clicked from a press and a release that were
//! near each other in place and in time, and it decides what is *hovered* from where the pointer was
//! when the pass began. So a click is three [`Step`]s — the pointer moves, the button goes down, the
//! button comes up — and each one is a frame.
//!
//! ## Before the pass, never inside it
//!
//! The events are put into `RawInput` from `eframe::App::raw_input_hook`, which runs before
//! `Context::begin_pass`. `InputState::pointer` is **derived** during that pass, so an event pushed
//! into `ctx.input_mut().events` half way through a frame reaches anything that reads the event list
//! and nothing that reads the pointer — a press would be seen by nobody, because every widget asks
//! `Response::clicked`, which is the pointer's answer. There is no second place this could go.

use std::collections::VecDeque;

/// One frame's worth of input.
///
/// The pointer's position is carried beside the events rather than only inside them, because the
/// window keeps its own answer to "where is the pointer" for the frames between a move and a press —
/// see [`Queue::pointer`].
#[derive(Debug, Clone, Default)]
pub struct Step {
    pub events: Vec<egui::Event>,
    /// Where the pointer is left after this step, when it moved.
    pub pointer: Option<egui::Pos2>,
    /// Which modifiers are **held** while this step is fed to a frame.
    ///
    /// **Beside the events rather than only inside them, because `egui` keeps two answers and they are
    /// different questions.** `Event::Key` and `Event::PointerButton` each carry the modifiers that were
    /// held when they happened; `InputState::modifiers` is the frame's own *state*, which `egui` builds
    /// from `Event::ModifiersChanged` and which `egui-winit` sends from the real keyboard. Anything
    /// asking the frame — and `agent_chat::composer` asks it, to tell Enter from Shift+Enter — was
    /// therefore asking about the physical keyboard rather than about the gesture. Measured on
    /// `task-1914`: `input key Enter` in a chat node put a new line in the draft and sent nothing,
    /// because `input.modifiers.is_none()` was answering about a keyboard nobody was touching.
    ///
    /// [`Queue::next_frame`] turns a change of this into the `Event::ModifiersChanged` a device sends.
    pub modifiers: egui::Modifiers,
}

impl Step {
    /// A step holding one event and moving nothing.
    pub fn of(event: egui::Event) -> Self {
        let modifiers = modifiers_of(&event);
        Self { events: vec![event], pointer: None, modifiers }
    }
}

/// The modifiers an event was built with, so a step carries the same state the event does.
fn modifiers_of(event: &egui::Event) -> egui::Modifiers {
    match event {
        egui::Event::Key { modifiers, .. }
        | egui::Event::PointerButton { modifiers, .. }
        | egui::Event::MouseWheel { modifiers, .. } => *modifiers,
        _ => egui::Modifiers::NONE,
    }
}

/// What has been asked for and not yet fed to a frame.
#[derive(Debug, Default)]
pub struct Queue {
    steps: VecDeque<Step>,
    /// Where the pointer was left by the last step that moved it.
    ///
    /// **Kept, because `egui` forgets.** A frame carrying no pointer event at all is a frame in which
    /// `InputState::pointer` has no position, and the frame after a `PointerMoved` is exactly that —
    /// so a press sent on the next frame would land with nothing hovered. Every step therefore repeats
    /// the position, which is what a real mouse does: `winit` sends `CursorMoved` whenever the pointer
    /// is over the window and `egui` holds the last one.
    pointer: Option<egui::Pos2>,
    /// How many gestures have been fed all the way through, so a command can wait for its own.
    ///
    /// A counter rather than a flag, which is the shape `DebugState::reads` already has: two commands
    /// in flight would each see the other's empty queue and answer early.
    done: u64,
    /// The modifier state the last step was fed under. See [`Step::modifiers`].
    held: egui::Modifiers,
}

/// How many times one key press may be repeated.
///
/// **`task-1984` S1.** `input key --times` was a saturating float cast into an unbounded loop, so
/// `--times 1e18` allocated two steps a repetition until the process died and `--times 100000` was
/// fifty five minutes of a window that answered nothing, with no way to cancel it. Two hundred is
/// what `input drag --steps` already clamps to, and it is more presses than any gesture needs.
pub const REPEATS: usize = 200;

/// How many characters one `input text` may carry.
///
/// Each character is two steps and therefore two frames, so a thousand of them is already half a
/// minute of a window doing nothing else. A caller with more to type sends it in several commands,
/// which is also the only shape in which anything can be asserted between them.
pub const LONGEST_TEXT: usize = 1_000;

/// How many steps may be waiting to be fed.
///
/// One frame each, so this is how far behind the window may be. Several commands in flight add up,
/// which is why the limit is on the queue as well as on each command.
pub const LIMIT: usize = 10_000;

impl Queue {
    /// Add a gesture. Each [`Step`] is one frame.
    ///
    /// Answers false and adds nothing when the queue is already at [`LIMIT`], so the caller is told
    /// rather than the gesture being half fed -- which is `cli_window`'s own rule about an argument
    /// that is too large, and is what every other over large argument in the catalogue does.
    pub fn push(&mut self, steps: Vec<Step>) -> bool {
        if self.steps.len() + steps.len() > LIMIT {
            return false;
        }
        self.steps.extend(steps);
        true
    }

    /// Whether anything is waiting to be fed to a frame.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// How many steps have been fed all the way through since the window opened.
    pub fn fed(&self) -> u64 {
        self.done
    }

    /// Take the next frame's events.
    ///
    /// Answers nothing when there is nothing to feed, so a frame that was not asked for costs one
    /// comparison — `task-1666`'s rule about anything that runs once a frame.
    pub fn next_frame(&mut self) -> Option<Vec<egui::Event>> {
        let step = self.steps.pop_front()?;
        if let Some(at) = step.pointer {
            self.pointer = Some(at);
        }
        self.done += 1;
        let mut events = Vec::with_capacity(step.events.len() + 2);
        // **A change of modifier state is an event of its own**, which is how `egui` learns it:
        // `InputState::modifiers` is updated by `Event::ModifiersChanged` and by nothing else, so a key
        // event carrying `ctrl` tells the frame's own state nothing. `egui-winit` sends one whenever the
        // real keyboard's modifiers move, and this is that. See [`Step::modifiers`].
        if step.modifiers != self.held {
            self.held = step.modifiers;
            events.push(egui::Event::ModifiersChanged(step.modifiers));
        }
        // The position next, so a press in the same frame lands on the widget the pointer is over
        // rather than on whatever it was over before.
        if let Some(at) = self.pointer {
            events.push(egui::Event::PointerMoved(at));
        }
        events.extend(step.events);
        Some(events)
    }
}

/// Which button a click is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Primary,
    Secondary,
    Middle,
}

impl Button {
    pub fn of(self) -> egui::PointerButton {
        match self {
            Button::Primary => egui::PointerButton::Primary,
            Button::Secondary => egui::PointerButton::Secondary,
            Button::Middle => egui::PointerButton::Middle,
        }
    }
}

/// The pointer moves and stays there.
pub fn moved(at: egui::Pos2) -> Vec<Step> {
    vec![Step { events: Vec::new(), pointer: Some(at), modifiers: egui::Modifiers::NONE }]
}

/// The same, holding some modifiers, for the frames of a gesture that carries them.
fn moved_with(at: egui::Pos2, modifiers: egui::Modifiers) -> Vec<Step> {
    vec![Step { events: Vec::new(), pointer: Some(at), modifiers }]
}

/// A press and a release at one place: three frames, which is what `egui` needs to call it a click.
///
/// **The move is a frame of its own**, because what a widget is hovering is worked out at the start of
/// a pass: a press arriving in the same frame as the first sight of the pointer lands on a window that
/// has not yet noticed anything is under it.
pub fn clicked(
    at: egui::Pos2,
    button: Button,
    modifiers: egui::Modifiers,
    times: usize,
) -> Vec<Step> {
    let mut steps = moved_with(at, modifiers);
    for _ in 0..times.max(1) {
        steps.push(Step::of(egui::Event::PointerButton {
            pos: at,
            button: button.of(),
            pressed: true,
            modifiers,
        }));
        steps.push(Step::of(egui::Event::PointerButton {
            pos: at,
            button: button.of(),
            pressed: false,
            modifiers,
        }));
    }
    steps
}

/// A drag: the pointer arrives, the button goes down, it is moved `steps` times, and it is let go.
///
/// The moves are steps of their own because a drag is a thing that happens **over frames** — every
/// drag in Unluminous is settled from `Response::drag_delta`, which is the difference between two
/// frames' pointer positions, and a press and a release in consecutive frames is a click.
pub fn dragged(
    from: egui::Pos2,
    to: egui::Pos2,
    steps: usize,
    modifiers: egui::Modifiers,
) -> Vec<Step> {
    let steps = steps.clamp(1, 200);
    let mut out = moved_with(from, modifiers);
    out.push(Step::of(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers,
    }));
    for step in 1..=steps {
        let along = step as f32 / steps as f32;
        let at = from + (to - from) * along;
        out.push(Step { events: Vec::new(), pointer: Some(at), modifiers });
    }
    out.push(Step::of(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    }));
    out
}

/// A key pressed and released.
///
/// The press and the release are separate frames, which is what a keyboard does and what anything
/// reading `key_pressed` expects: both in one frame is a key that was never down.
///
/// **Except the two clipboard chords, which a real keyboard never delivers as a key press at all.**
/// `egui-winit` recognises `Cmd`/`Ctrl`+`C` and `Cmd`/`Ctrl`+`X` in `on_keyboard_input`, pushes
/// `Event::Copy` or `Event::Cut`, and **returns** — so the key press is swallowed and nothing in
/// Unluminous ever sees one. That is why `Copy` is marked in `actions::menus` as not coming from the
/// keyboard, and why `UnluminousApp::route_the_preview_copy` claims the event rather than the chord.
/// Sending the key press here would therefore have been sending something no window receives, and
/// `unluminous-cli input key c --cmd` copied nothing anywhere in the window. `task-2060` found it
/// against a selection in a chat message; it was as true of the Markdown preview.
///
/// Paste is deliberately **not** one of these. `egui-winit` reads the clipboard's *text* to build
/// `Event::Paste`, and this has no clipboard; the key going back up is what
/// `components::agent_chat::pasting` reads for a picture, which is the one report of the chord that
/// reaches Unluminous at all.
pub fn pressed(key: egui::Key, modifiers: egui::Modifiers, times: usize) -> Vec<Step> {
    let mut steps = Vec::new();
    for _ in 0..times.clamp(1, REPEATS) {
        if let Some(event) = the_clipboard_chord(key, modifiers) {
            steps.push(Step::of(event));
            continue;
        }
        steps.push(Step::of(egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }));
        steps.push(Step::of(egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: false,
            repeat: false,
            modifiers,
        }));
    }
    steps
}

/// The event `egui-winit` sends for a clipboard chord, or [`None`] for an ordinary key.
///
/// The four shapes it recognises, written out so the two agree: the command key with `C` or `X`, and
/// the two older spellings — `Ctrl`+`Insert` copies and `Shift`+`Delete` cuts — that Windows has had
/// since before either of the letters did.
fn the_clipboard_chord(key: egui::Key, modifiers: egui::Modifiers) -> Option<egui::Event> {
    let copy =
        (modifiers.command && key == egui::Key::C) || (modifiers.ctrl && key == egui::Key::Insert);
    if copy {
        return Some(egui::Event::Copy);
    }
    let cut =
        (modifiers.command && key == egui::Key::X) || (modifiers.shift && key == egui::Key::Delete);
    cut.then_some(egui::Event::Cut)
}

/// Text typed, one character a frame.
///
/// **A key press and the text together**, which is what `egui-winit` sends for a letter: the editing
/// area reads `Event::Text` and an `egui::TextEdit` reads both, so a character sent as text alone is
/// taken by one and not the other. A character with no [`egui::Key`] of its own — an accent, an emoji —
/// is sent as text, which is also what a real keyboard does through an input method.
pub fn typed(text: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    for character in text.chars() {
        let key = egui::Key::from_name(&character.to_string());
        let mut events = Vec::new();
        if let Some(key) = key {
            events.push(egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        events.push(egui::Event::Text(character.to_string()));
        steps.push(Step { events, pointer: None, modifiers: egui::Modifiers::NONE });
        if let Some(key) = key {
            steps.push(Step::of(egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }));
        }
    }
    steps
}

/// The wheel turned, in the notches `egui` counts in points.
///
/// One notch is [`NOTCH`] points, which is what `egui-winit` multiplies a line of scrolling by, so a
/// number here means the same thing as a number of clicks on a real wheel.
pub fn wheeled(notches: f32, across: f32, modifiers: egui::Modifiers) -> Vec<Step> {
    vec![Step::of(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::Vec2::new(across * NOTCH, notches * NOTCH),
        // A wheel is not a trackpad, and `TouchPhase::Move` is what `egui` says to use when there is no
        // phase to report — which there is not, because a notch is one event with nothing either side.
        phase: egui::TouchPhase::Move,
        modifiers,
    })]
}

/// How many points one notch of the wheel is, which is `egui-winit`'s own `points_per_scroll_line`.
pub const NOTCH: f32 = 50.0;

/// One more frame with nothing held, which is a hand coming off the modifier keys.
///
/// Every gesture that holds one ends with this, because the state is **held** until something says
/// otherwise: without it a `--cmd` click would leave the window believing the command key was down for
/// the rest of the session, and the next ordinary Enter would be read as `Cmd+Enter`.
pub fn let_go() -> Vec<Step> {
    vec![Step { events: Vec::new(), pointer: None, modifiers: egui::Modifiers::NONE }]
}

/// Read a key by the name the command line uses.
///
/// `egui::Key::from_name` already knows the letters, the digits and most of the named keys; the four
/// here are the spellings a person types that it does not have.
pub fn key_named(name: &str) -> Option<egui::Key> {
    let name = name.trim();
    match name.to_lowercase().as_str() {
        "esc" => Some(egui::Key::Escape),
        "return" => Some(egui::Key::Enter),
        "space" => Some(egui::Key::Space),
        "del" => Some(egui::Key::Delete),
        _ => egui::Key::from_name(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every step repeats where the pointer was left, because a frame with no pointer event has no
    /// pointer at all as far as `egui` is concerned — so a press on the frame after a move would land
    /// with nothing under it.
    #[test]
    fn the_pointer_is_repeated_on_every_frame_after_it_moved() {
        let mut queue = Queue::default();
        queue.push(clicked(egui::pos2(40.0, 20.0), Button::Primary, egui::Modifiers::NONE, 1));
        let mut frames = Vec::new();
        while let Some(events) = queue.next_frame() {
            frames.push(events);
        }
        assert_eq!(frames.len(), 3, "a move, a press and a release");
        for (at, events) in frames.iter().enumerate() {
            assert!(
                matches!(events.first(), Some(egui::Event::PointerMoved(pos)) if *pos == egui::pos2(40.0, 20.0)),
                "frame {at} does not say where the pointer is: {events:?}",
            );
        }
        assert!(matches!(frames[1].get(1), Some(egui::Event::PointerButton { pressed: true, .. })));
        assert!(matches!(
            frames[2].get(1),
            Some(egui::Event::PointerButton { pressed: false, .. })
        ));
    }

    /// A drag is a press, some moves and a release, and the moves are frames of their own — a drag
    /// settled from `Response::drag_delta` is the difference between two frames.
    /// `task-2060`: a real keyboard never delivers `Cmd`/`Ctrl`+`C` as a key press.
    ///
    /// `egui-winit` turns it into `Event::Copy` and returns, swallowing the press — so a window is
    /// only ever offered the event. Sending the press was sending something nothing in Unluminous
    /// reads, and `input key c --cmd` therefore copied nothing anywhere: not in a chat message, not
    /// in the Markdown preview, not in a text field.
    #[test]
    fn the_clipboard_chords_are_sent_as_the_events_a_real_keyboard_produces() {
        let command = egui::Modifiers { command: true, ..Default::default() };
        let steps = pressed(egui::Key::C, command, 1);
        assert_eq!(steps.len(), 1, "one event, not a press and a release");
        assert!(matches!(steps[0].events.as_slice(), [egui::Event::Copy]));
        assert!(matches!(
            pressed(egui::Key::X, command, 1)[0].events.as_slice(),
            [egui::Event::Cut]
        ));
        // The older spellings Windows has always had.
        let control = egui::Modifiers { ctrl: true, ..Default::default() };
        assert!(matches!(
            pressed(egui::Key::Insert, control, 1)[0].events.as_slice(),
            [egui::Event::Copy]
        ));
        let shift = egui::Modifiers { shift: true, ..Default::default() };
        assert!(matches!(
            pressed(egui::Key::Delete, shift, 1)[0].events.as_slice(),
            [egui::Event::Cut]
        ));
        // **Paste is not one of them**: `egui-winit` reads the clipboard's text to build its event
        // and this has no clipboard, and the key going back up is the one report of the chord
        // `components::agent_chat::pasting` can read.
        let paste = pressed(egui::Key::V, command, 1);
        assert_eq!(paste.len(), 2, "a press and a release, as for any other key");
        // And an ordinary letter is untouched.
        assert_eq!(pressed(egui::Key::C, egui::Modifiers::default(), 1).len(), 2);
    }

    #[test]
    fn a_drag_moves_over_frames_rather_than_all_at_once() {
        let mut queue = Queue::default();
        queue.push(dragged(egui::pos2(0.0, 0.0), egui::pos2(100.0, 0.0), 4, egui::Modifiers::NONE));
        let mut places = Vec::new();
        let mut pressed = 0;
        let mut released = 0;
        while let Some(events) = queue.next_frame() {
            for event in &events {
                match event {
                    egui::Event::PointerMoved(at) => places.push(at.x),
                    egui::Event::PointerButton { pressed: true, .. } => pressed += 1,
                    egui::Event::PointerButton { pressed: false, .. } => released += 1,
                    _ => {}
                }
            }
        }
        assert_eq!((pressed, released), (1, 1), "one press and one release");
        assert_eq!(places.first(), Some(&0.0), "it starts where it was picked up");
        assert_eq!(places.last(), Some(&100.0), "and ends where it was let go");
        assert!(places.len() >= 6, "the moves are frames of their own: {places:?}");
        assert!(
            places.windows(2).all(|pair| pair[1] >= pair[0]),
            "and it goes one way: {places:?}",
        );
    }

    /// A letter is a key press **and** the text, which is what a real keyboard produces: the editing
    /// area reads one and a `TextEdit` reads both.
    #[test]
    fn typing_a_letter_sends_the_key_and_the_text() {
        let mut queue = Queue::default();
        queue.push(typed("Hi"));
        let mut text = String::new();
        let mut keys = 0;
        while let Some(events) = queue.next_frame() {
            for event in &events {
                match event {
                    egui::Event::Text(said) => text.push_str(said),
                    egui::Event::Key { pressed: true, .. } => keys += 1,
                    _ => {}
                }
            }
        }
        assert_eq!(text, "Hi");
        assert_eq!(keys, 2, "one key press a letter");
    }

    /// A gesture that holds a modifier says so as an event, and lets go afterwards.
    ///
    /// `InputState::modifiers` is built from `Event::ModifiersChanged` and from nothing else, so a key
    /// event carrying `command` leaves the frame's own state alone — and everything that tells `Enter`
    /// from `Cmd+Enter` asks the frame. `task-1914` measured what that costs: Enter in a chat node put a
    /// new line in the draft and sent nothing.
    #[test]
    fn holding_a_modifier_is_an_event_and_letting_go_is_another() {
        let command = egui::Modifiers { command: true, ctrl: true, ..egui::Modifiers::NONE };
        let mut queue = Queue::default();
        queue.push(pressed(egui::Key::S, command, 1));
        queue.push(let_go());
        let mut said = Vec::new();
        while let Some(events) = queue.next_frame() {
            for event in events {
                if let egui::Event::ModifiersChanged(now) = event {
                    said.push(now);
                }
            }
        }
        assert_eq!(said, vec![command, egui::Modifiers::NONE], "held, then let go");
    }

    /// And a gesture that holds nothing says nothing, so an ordinary click costs no extra event.
    #[test]
    fn a_gesture_that_holds_nothing_sends_no_modifier_event() {
        let mut queue = Queue::default();
        queue.push(clicked(egui::pos2(10.0, 10.0), Button::Primary, egui::Modifiers::NONE, 1));
        while let Some(events) = queue.next_frame() {
            assert!(
                !events.iter().any(|event| matches!(event, egui::Event::ModifiersChanged(_))),
                "nothing was held, so nothing changed: {events:?}",
            );
        }
    }

    /// A key that is only a name is read, and one that is nothing is refused rather than guessed at.
    #[test]
    fn the_keys_a_person_types_are_the_keys_that_are_read() {
        for (name, key) in [
            ("Escape", egui::Key::Escape),
            ("esc", egui::Key::Escape),
            ("Enter", egui::Key::Enter),
            ("return", egui::Key::Enter),
            ("space", egui::Key::Space),
            ("F2", egui::Key::F2),
            ("a", egui::Key::A),
            ("ArrowDown", egui::Key::ArrowDown),
        ] {
            assert_eq!(key_named(name), Some(key), "{name}");
        }
        assert_eq!(key_named("nothing-like-this"), None);
    }

    /// Nothing is fed to a frame that asked for nothing.
    #[test]
    fn an_empty_queue_feeds_no_frame() {
        let mut queue = Queue::default();
        assert!(queue.is_empty());
        assert!(queue.next_frame().is_none());
        assert_eq!(queue.fed(), 0);
        queue.push(moved(egui::pos2(1.0, 2.0)));
        assert!(!queue.is_empty());
        assert!(queue.next_frame().is_some());
        assert_eq!(queue.fed(), 1);
        assert!(queue.is_empty());
    }

    // ------------------------------------------------------------------------------- task-1984

    /// One key press cannot be repeated until the process dies.
    ///
    /// `task-1984` S1: `input key --times` was a saturating float cast into an unbounded loop, so
    /// `--times 1e18` allocated two steps a repetition until there was no memory left.
    #[test]
    fn a_key_is_repeated_at_most_two_hundred_times() {
        let many = pressed(egui::Key::A, egui::Modifiers::NONE, usize::MAX);
        assert_eq!(many.len(), REPEATS * 2, "a press and a release each");
        let one = pressed(egui::Key::A, egui::Modifiers::NONE, 0);
        assert_eq!(one.len(), 2, "and none at all still means once");
    }

    /// The queue refuses rather than growing without end.
    ///
    /// Several commands in flight add up, which is why the limit is on the queue as well as on each
    /// command. A refusal rather than a truncation, so the caller is told: a gesture half fed is a
    /// window in a state nothing asked for.
    #[test]
    fn the_queue_refuses_what_would_take_it_past_its_limit() {
        let mut queue = Queue::default();
        let one_frame = || vec![Step::of(egui::Event::PointerMoved(egui::pos2(1.0, 1.0)))];
        for _ in 0..LIMIT {
            assert!(queue.push(one_frame()), "up to the limit is taken");
        }
        assert!(!queue.push(one_frame()), "and the one past it is refused");
        assert_eq!(queue.steps.len(), LIMIT, "with nothing added");
    }
}
