//! `window`, `browser` and `input` -- the whole window's own geometry and the two ways to reach
//! it without a person's hands on the keyboard or the mouse.
//!
//! `window` is size, position and a screenshot; `browser` drives the one native child view a tab
//! can hold, through the same host and commands as the menu and toolbar; `input` is the synthetic
//! `egui::Event`s `unluminous-cli input` queues for `raw_input_hook` to feed the window one step a
//! frame, which is how a script drives Unluminous without bringing it to the front -- see
//! `tasks/task-1914-testing-without-stealing-focus-tdd.md`.

use super::*;

/// How long a screenshot waits. It is a handful of frames away, so this is only ever the answer when
/// the window has stopped drawing altogether.
const SCREENSHOT_WAIT: Duration = Duration::from_millis(5_000);
/// How long a screenshot lets the window settle before asking for the picture.
///
/// A quarter of a second, which is three times egui's own animation time of a twelfth. It is a
/// **duration rather than a number of frames**, because how many frames a quarter of a second is
/// depends on the machine: six frames was tried first and left a modal at 97 per cent of its fade,
/// which is exactly the sort of nearly-right that a picture is supposed to settle. See
/// [`Waiting::Screenshot`].
const SETTLE: Duration = Duration::from_millis(250);

impl UnluminousApp {
    /// Click, type, drag and scroll in this window, without it being in front.
    ///
    /// **The one command family that produces input**, and the reason it exists rather than a script
    /// sending `mouse_event` is that synthetic operating system input goes to the *foreground* window —
    /// so a script had to bring Unluminous to the front, and on Windows bringing a window on another
    /// virtual desktop to the front switches the desktop with it. That is `task-1914`'s report and
    /// `tasks/task-1914-testing-without-stealing-focus-tdd.md` is the design.
    ///
    /// What is queued is `egui::Event`, which is what `egui-winit` builds out of a real device, and it is
    /// fed to `RawInput` one step a frame from `raw_input_hook`. So the window is driven down the same
    /// path a mouse drives it, with no focus, no pointer moving on the person's screen, and no desktop
    /// switch. Positions are the window's own points, which is what `window screenshot` writes out.
    pub(crate) fn cli_input(&mut self, request: &Request, verb: &str) -> Outcome {
        use crate::services::input;
        let modifiers = input_modifiers(request);
        let at = |name: &str, down: &str| -> Option<egui::Pos2> {
            Some(egui::Pos2::new(request.number(name)? as f32, request.number(down)? as f32))
        };
        let steps = match verb {
            "move" => {
                let Some(at) = at("x", "y") else {
                    return no(
                        request,
                        code::USAGE,
                        "Say where, as an x and a y in window points.",
                    );
                };
                input::moved(at)
            }
            "click" => {
                let Some(at) = at("x", "y") else {
                    return no(
                        request,
                        code::USAGE,
                        "Say where, as an x and a y in window points.",
                    );
                };
                let button = match (request.switch("right"), request.switch("middle")) {
                    (true, _) => input::Button::Secondary,
                    (_, true) => input::Button::Middle,
                    _ => input::Button::Primary,
                };
                let times = if request.switch("twice") { 2 } else { 1 };
                input::clicked(at, button, modifiers, times)
            }
            "drag" => {
                let Some(from) = at("x", "y") else {
                    return no(request, code::USAGE, "Say where the drag starts, as an x and a y.");
                };
                let Some(to) = at("to-x", "to-y") else {
                    return no(request, code::USAGE, "Say where it ends, with --to-x and --to-y.");
                };
                let along = request.number("steps").unwrap_or(20.0).max(1.0) as usize;
                input::dragged(from, to, along, modifiers)
            }
            "key" => {
                let Some(name) = request.text("key") else {
                    return no(request, code::USAGE, "Say which key, by its name.");
                };
                let Some(key) = input::key_named(&name) else {
                    return no(
                        request,
                        code::USAGE,
                        format!(
                            "There is no key called {name}. A letter, a digit, or a name like Enter, Escape, Tab, Backspace, Space, ArrowDown or F2."
                        ),
                    );
                };
                // Refused rather than clamped, so the caller is told (`task-1984` S1). It was a
                // saturating float cast into an unbounded loop: `--times 1e18` allocated until the
                // process died and `--times 100000` was fifty five minutes of a window that answered
                // nothing, with no way to cancel it.
                let times = request.number("times").unwrap_or(1.0);
                if !times.is_finite() || times < 1.0 || times > input::REPEATS as f64 {
                    return no(
                        request,
                        code::USAGE,
                        format!(
                            "--times takes a whole number from 1 to {}. Each repetition is two \
                             frames, so more than that is a window that answers nothing for \
                             minutes with no way to stop it.",
                            input::REPEATS
                        ),
                    );
                }
                input::pressed(key, modifiers, times as usize)
            }
            "text" => {
                let Some(text) = request.text("text") else {
                    return no(request, code::USAGE, "Say what to type.");
                };
                if text.is_empty() {
                    return no(request, code::USAGE, "Say what to type.");
                }
                // Each character is two frames, so a thousand of them is already half a minute of a
                // window doing nothing else (`task-1984` L9). Refused rather than cut short, because
                // a caller told it typed something it did not type is worse than one told to send it
                // in two commands.
                if text.chars().count() > input::LONGEST_TEXT {
                    return no(
                        request,
                        code::USAGE,
                        format!(
                            "--text takes at most {} characters, and that is {}. Each character is \
                             two frames. Send it in several commands.",
                            input::LONGEST_TEXT,
                            text.chars().count()
                        ),
                    );
                }
                input::typed(&text)
            }
            "wheel" => {
                let Some(notches) = request.number("notches") else {
                    return no(request, code::USAGE, "Say how many notches, negative for down.");
                };
                let across = request.number("across").unwrap_or(0.0) as f32;
                input::wheeled(notches as f32, across, modifiers)
            }
            _ => return unknown(request),
        };
        // **The window has to be drawing for any of this to land**, and an idle one is asleep: nothing has
        // happened yet, so nothing has asked for a frame. The wait asks for one on every pass, and
        // `raw_input_hook` asks for the next while any step is left.
        // **A gesture that held a modifier lets go at the end of it.** The state is held until something
        // says otherwise, so without this a `--cmd` click would leave the window believing the command
        // key was down for the rest of the session. See `services::input::let_go`.
        let mut steps = steps;
        if modifiers != egui::Modifiers::NONE {
            steps.extend(input::let_go());
        }
        let frames = steps.len() as u64;
        if !self.input.push(steps) {
            return no(
                request,
                code::REFUSED,
                format!(
                    "There are already {} steps waiting to be fed to the window, which is as far \
                     behind as it is allowed to get. Wait for them and send this again.",
                    input::LIMIT
                ),
            );
        }
        Outcome::Hold(Waiting::Input {
            target: self.input.fed() + frames,
            // One frame after the last step, so what the input did has been drawn before the caller is
            // told it happened — which is what makes `input click` then `window screenshot` a picture of
            // the window after the click. `Waiting::Screenshot` settles for the same reason.
            settle: 1,
            until: Instant::now() + DEFAULT_WAIT,
        })
    }

    pub(crate) fn cli_window(
        &mut self,
        request: &Request,
        verb: &str,
        ctx: &egui::Context,
    ) -> Outcome {
        match verb {
            "screenshot" => {
                let Some(path) = self.cli_path_argument(request, "file") else {
                    return no(request, code::USAGE, "Say where to write the picture.");
                };
                ctx.request_repaint();
                Outcome::Hold(Waiting::Screenshot {
                    path,
                    until: waits_for(request, "timeout", SCREENSHOT_WAIT),
                    settled: Instant::now() + SETTLE,
                    asked: false,
                    crop: None,
                })
            }
            "focus" => {
                ctx.send_viewport_cmd(ViewportCommand::Focus);
                done(request, "Brought the window to the front.")
            }
            "size" => self.cli_window_size(request, ctx),
            "position" => self.cli_window_position(request, ctx),
            "message" => {
                if request.has("text") {
                    self.message = request.text("text");
                    done(request, format!("Showing {}", self.message.clone().unwrap_or_default()))
                } else if request.arguments.contains_key("text") {
                    self.message = None;
                    done(request, "Cleared the status bar message.")
                } else {
                    ok(
                        request,
                        self.message
                            .clone()
                            .unwrap_or_else(|| "The status bar has no message.".to_owned()),
                        json!({ "message": self.message }),
                    )
                }
            }
            _ => unknown(request),
        }
    }

    fn cli_window_size(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        let screen = ctx.content_rect();
        let width = request.number("width").map(|value| value as f32);
        let height = request.number("height").map(|value| value as f32);
        if width.is_none() && height.is_none() {
            return ok(
                request,
                format!("{} by {} points", screen.width(), screen.height()),
                json!({ "width": screen.width(), "height": screen.height() }),
            );
        }
        // Clamped to the size the window is *built* with rather than to a second, smaller pair of
        // numbers: `main.rs` names `SMALLEST_WINDOW` as the minimum inner size, so a request under it
        // was answered `ok` with a number the window then refused to become. `task-2062`.
        let smallest = crate::app::SMALLEST_WINDOW;
        let wanted = egui::Vec2::new(
            width.unwrap_or(screen.width()).max(smallest.x),
            height.unwrap_or(screen.height()).max(smallest.y),
        );
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(wanted));
        ok(
            request,
            format!("Set the window to {} by {} points", wanted.x, wanted.y),
            json!({ "width": wanted.x, "height": wanted.y }),
        )
    }

    fn cli_window_position(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        let outer = ctx.input(|input| input.viewport().outer_rect);
        let x = request.number("x").map(|value| value as f32);
        let y = request.number("y").map(|value| value as f32);
        if x.is_none() && y.is_none() {
            let at = outer.map(|rect| rect.min).unwrap_or(egui::Pos2::ZERO);
            return ok(
                request,
                format!("The window is at {}, {}", at.x, at.y),
                json!({ "x": at.x, "y": at.y, "known": outer.is_some() }),
            );
        }
        let at = outer.map(|rect| rect.min).unwrap_or(egui::Pos2::ZERO);
        let wanted = egui::Pos2::new(x.unwrap_or(at.x), y.unwrap_or(at.y));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(wanted));
        ok(
            request,
            format!("Moved the window to {}, {}", wanted.x, wanted.y),
            json!({ "x": wanted.x, "y": wanted.y }),
        )
    }

    /// `unluminous-cli browser ...` through the same host and commands as the menu and toolbar.
    pub(crate) fn cli_browser(&mut self, request: &Request, verb: &str) -> Outcome {
        if verb == "open" {
            let Some(address) = request.text("address") else {
                return no(request, code::USAGE, "Give an HTTP address or HTML path.");
            };
            return match self.open_browser(&address) {
                Ok(id) => ok(
                    request,
                    format!("Opened browser tab {id}"),
                    json!({ "id": id, "address": address }),
                ),
                Err(problem) => no(request, code::FAILED, problem),
            };
        }
        let Some(tab) = self.files.active().browser.as_ref() else {
            return no(
                request,
                code::NOT_APPLICABLE,
                "The tab that is showing is not a browser tab.",
            );
        };
        if verb == "status" {
            return ok(
                request,
                tab.name(),
                json!({
                    "id": tab.id,
                    "title": tab.title,
                    "url": tab.current_url(),
                    "loading": tab.loading,
                    "showing": self.browser.showing() == Some(tab.id),
                    // **Whether the page is being drawn right now.** A menu, a dropdown or a modal
                    // over a page hides it, because a native child view paints above everything egui
                    // draws — and nothing Unluminous photographs holds a page, so this is the only
                    // way to ask. `task-2009`.
                    "covered": self.page_is_covered,
                    "canGoBack": tab.can_go_back(),
                    "canGoForward": tab.can_go_forward(),
                    "problem": tab.problem,
                }),
            );
        }
        let command = match verb {
            "back" => BrowserCommand::Back,
            "forward" => BrowserCommand::Forward,
            "reload" => BrowserCommand::Reload,
            _ => return unknown(request),
        };
        let id = tab.id;
        let before = self.message.take();
        self.run_browser_command(id, command);
        match std::mem::replace(&mut self.message, before) {
            Some(problem) => no(request, code::NOT_APPLICABLE, problem),
            None => done(request, format!("Sent {verb} to browser tab {id}")),
        }
    }
}

/// The modifiers an `input` command was given, as `egui` counts them.
///
/// `command` is the key a menu shortcut names — the Apple key on macOS and control on Windows — which is
/// why it is set beside `ctrl` rather than instead of it: on Windows `Ctrl+Enter` really does arrive with
/// both, which is the trap `task-1682` recorded.
fn input_modifiers(request: &Request) -> egui::Modifiers {
    let command = request.switch("cmd") || request.switch("command");
    egui::Modifiers {
        alt: request.switch("alt"),
        ctrl: request.switch("ctrl") || (command && !cfg!(target_os = "macos")),
        shift: request.switch("shift"),
        mac_cmd: command && cfg!(target_os = "macos"),
        command: command || request.switch("ctrl"),
    }
}
