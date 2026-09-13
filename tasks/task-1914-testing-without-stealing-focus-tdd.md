# Testing Unluminous without stealing the keyboard

`task-1914`, second half:

> we can't have the window take focus while testing. we need a mechanism for testing and taking
> screenshots that you can see that doesn't impact my work. e.g. right now im switched to a different
> desktop, but get switched to another desktop with unluminous open. Search online, create a tdd, and
> figure out how to test without stealing my focus, but ensuring you can perfectly see the entire
> unluminous window.

## 1. What actually happens today, and which part of it is at fault

Driving the real window on Windows was three things, and only one of them was innocent.

| Step | What it did | Did it take the focus |
|---|---|---|
| `Start-Process unluminous.exe <folder>` | opened a window | **yes**, and it switched virtual desktop |
| `unluminous-cli window focus` before input | brought it to the front | **yes**, on purpose |
| `keybd_event` / `mouse_event` from `tools/windows-input.ps1` | typed and clicked | needs the window in front, so it forced the two above |
| `unluminous-cli window screenshot` | photographed it | **no** |

So the photograph was never the problem. The problem is that **synthetic operating system input goes
to whatever window is in front**, so every script that wanted to click something had to put Unluminous
in front first — and on Windows, activating a window that is on another virtual desktop switches the
desktop with it. `task-1848` recorded the same fault on macOS and answered the *launching* half of it
with `open -g`; the *input* half was never answered anywhere.

`tools/windows-input.ps1` is not deleted by this. It is the only way to drive something that is not
Unluminous, and its held-key safety machinery is the answer to a real hazard. What changes is that
nothing about **Unluminous** has to use it.

## 2. What was considered

### 2.1 Photographing a window that is not in front

Three ways, and the one already in the product wins for everything it can reach.

- **`ViewportCommand::Screenshot`, which is `unluminous-cli window screenshot`.** egui reads back the
  surface Unluminous painted. It needs no focus, no foreground and no pointer, and a window that is
  covered or on another desktop still paints: `eframe`'s wgpu loop records
  `WindowEvent::Occluded` into `viewport.info.occluded` and gates painting on **visible** rather than
  on occluded, so the frames keep coming. What it cannot hold is a **native child**: a browser node's
  page is composited by the operating system on top of the surface, which is why
  `documentation/overview.md` was taken with a desktop capture and why `space browser … shot` says so.
- **`PrintWindow` with `PW_RENDERFULLCONTENT`.** Documented to work on a window that is minimised or
  overlapped, and `PW_RENDERFULLCONTENT` exists for exactly the GPU-composited case. It is the
  fallback for the native child, and §6 records what it really did here.
- **`Windows.Graphics.Capture`.** The modern answer, and it is what an offscreen WebView2 is captured
  with. It also draws a yellow capture border unless the process opts out, needs WinRT projection
  from PowerShell or a helper program, and — on the case this ticket is about, a window on another
  virtual desktop — has nothing composited to capture. Weighed and left out: it is a large dependency
  for the one case `PrintWindow` may already answer.

### 2.2 Driving a window that is not in front

- **`PostMessage` of `WM_LBUTTONDOWN` / `WM_CHAR` to the window's own handle.** No focus needed in
  principle. In practice it depends on what the toolkit does with a message it did not receive
  through the ordinary path: `WM_CHAR` is documented as going to *the window with the keyboard
  focus*, `winit` builds its key events from `GetKeyboardState` and `ToUnicode` rather than from the
  message alone, and it calls `SetCapture` on a press. Every one of those is a way for a posted
  message to be read differently from a real one — and a test that drives the window differently from
  a person is a test about the harness.
- **A second desktop or a hidden window station.** Real, and enormous: a whole desktop of its own,
  no GPU compositing, and the thing being tested would no longer be the thing that ships.
- **Feeding `egui` the events directly, through the channel Unluminous already has.** This is the one
  chosen, and §3 is why.

## 3. The shape: input is a command, like everything else

`CLAUDE.md`'s first rule is that everything a person can do in this window an agent can do too,
through the same command, and both are covered by tests. Input was the one exception: a click was
something only a hand or a synthetic device could produce.

So **`unluminous-cli input`** is a new area, and what it feeds the window is `egui::Event` — the same
values `egui-winit` builds out of a real mouse and a real keyboard. Five things follow, and each is
better than what the synthetic device gave:

1. **No focus, ever.** The events arrive down the control channel, so the window need not be in front,
   need not be on this desktop, and the person's pointer never moves.
2. **The coordinates are the screenshot's.** They are the window's own points, which is exactly what
   `window screenshot` writes out — so a position read off a picture is the position to click, with no
   arithmetic about where the window happens to be on the screen.
3. **It is deterministic.** A press lands on a known frame rather than after a sleep that hopes the
   pointer arrived.
4. **It is the real input path.** `raw_input_hook` puts the events into `RawInput` **before** the pass,
   so `egui` builds its `PointerState`, its click detection and its double-click timing from them the
   way it does from a device. Pushing them mid-frame would not: `InputState::pointer` is derived
   during `begin_pass` and a `PointerMoved` arriving after that changes nothing.
5. **Nothing can be left held.** The queue drains and the command answers when it has; there is no
   state outside the window to strand, which is the hazard `tools/unstick-keyboard.ps1` exists for.

### 3.1 The commands

```
unluminous-cli input move   <x> <y>
unluminous-cli input click  <x> <y> [--right] [--middle] [--twice]
unluminous-cli input drag   <x> <y> --to-x <x> --to-y <y> [--steps N]
unluminous-cli input key    <name> [--ctrl] [--shift] [--alt] [--cmd] [--times N]
unluminous-cli input text   <the rest of the line>
unluminous-cli input wheel  <notches> [--across N] [--ctrl]
```

`<x>` and `<y>` are the window's own points, the top left of the window being `0, 0`.

### 3.2 One step a frame, and the command waits for the last of them

A click is not one event. `egui` decides that a widget was clicked from a press and a release that
were near each other in place and time, and it decides what is *hovered* from where the pointer was
when the pass began. So a click is three steps — the pointer moves, the button goes down, the button
comes up — and each step is one frame. A drag is a move, a press, `--steps` moves and a release.

`services::input::Queue` holds the steps. `raw_input_hook` takes the front one and appends its events
to `raw_input.events`, and asks for another frame while any are left. The command **holds** — the
`Outcome::Hold` every waiting command in `app::cli` already uses — until the queue is empty and one
more frame has been drawn, so `input click` followed by `window screenshot` photographs the window
*after* the click rather than during it.

### 3.3 What is deliberately not modelled

**Text is `Event::Text`, and a key is `Event::Key`.** A real keyboard produces both, and which one a
part of the window reads is not the same everywhere: the editing area reads `Event::Text` for a
letter and `Event::Key` for `Backspace`, an `egui::TextEdit` reads both. `input text` sends a key
press, the text, and the key release for each character, which is what `egui-winit` does. `input key`
sends the key alone, because `Ctrl+S` produces no text on a real keyboard either.

**No `input hover`, no `input scroll to`.** Hovering is what `input move` leaves behind, and a
scroll is `input wheel`.

## 4. Starting a window without taking the focus

`unluminous --background`. `egui::ViewportBuilder::with_active(false)` reaches
`winit::WindowAttributes::with_active(false)`, and `winit`'s Windows backend turns that into
`SW_SHOWNOACTIVATE` — the window appears, on the desktop it was started from, without becoming the
foreground window and therefore without dragging the desktop switch with it. On macOS the same
attribute is what `open -g` asks for.

`tools/drive-a-window.ps1` is the sibling of `tools/drive-a-window.sh`: start Unluminous that way,
wait for the control channel to answer, print the process id. Everything after that is
`unluminous-cli --instance <pid> …`.

## 5. The rule this leaves behind

> **Never bring an Unluminous window to the front to drive it.** Start it with `--background`, drive
> it with `unluminous-cli`, click and type with `unluminous-cli input`, photograph it with
> `unluminous-cli window screenshot`. `window focus` is a thing a person asks for, not a thing a
> script does.

It is the rule `task-1848` wrote for macOS, with the hole in the middle of it filled in.

## 6. What was measured

On this machine, against the release build, with Firefox in front the whole time. The pictures are in
`_agent_output/task-1914-space-nodes/`.

**The focus is kept, and it was asked rather than assumed.** `GetForegroundWindow` was read before and
after each step and compared:

| Step | Foreground before | Foreground after |
|---|---|---|
| `drive-a-window.ps1 <folder>` | `AI Studio — Mozilla Firefox` | the same |
| `window size`, `window screenshot`, `input click`, `input text` | `AI Studio — Mozilla Firefox` | the same |

**`input` reaches a text box in a window that is not in front.** `input click 300 92` on the explorer's
filter and `input text read`: the box holds `read` and the list has narrowed to `readme.md`
(`21-typed-unfocused.png`). The synthetic-device path could not do this **even with the window in
front** — measured earlier in the same session, three times, against the filter box, the chat pane's
composer and a chat node's composer, and not one of them took a character. That is what had made a click
look like something only a hand could do.

And the same click into a **chat node's composer**, which is a `TextEdit` inside a transformed sublayer:
`hello from a node` is in it and the send button lit (`22-crop.png`).

**`window screenshot` needs no focus and no foreground**, which it never did: every picture above was
taken of a window behind Firefox.

**`PrintWindow` with `PW_RENDERFULLCONTENT` does hold the page.** Two pictures of the same frame:

- `23-egui-shot-node.png` — `window screenshot`. The browser node's toolbar and address are there and
  the page area is **empty**, because a native child is composited on top of the surface egui reads back.
- `24-printwindow-node.png` — `tools/capture-window.ps1`. The same node with the page in it, WebView2 and
  all, taken without the window being in front.

So the answer to "perfectly see the entire window" is two commands with a line between them:
`window screenshot` for everything Unluminous draws, and `capture-window.ps1` when a browser node's page
has to be in the picture.

## 7. Deliberately left out

- **Windows.Graphics.Capture.** §2.1, and §6 is why it is not needed: `PrintWindow` answered the one
  case it was for.
- **A screenshot of a browser node's page from inside Unluminous.** There is no such thing: the page is
  a native child window and `ViewportCommand::Screenshot` reads the surface underneath it. That was
  measured on `task-1904` and is unchanged.
- **Replacing `tools/windows-input.ps1`.** It stays, for driving things that are not Unluminous.
- **A recorder.** `input` is a way to send one gesture, not a way to record and replay a session; a
  session that has to be replayed is a screenshot test.
