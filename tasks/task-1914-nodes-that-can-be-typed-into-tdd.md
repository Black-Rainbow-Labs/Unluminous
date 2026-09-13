# Nodes that can be typed into, and controls that fit their own text

`task-1914`, QA pass:

> In base of infinite space, i cant type in a terminal. If base of infinite space is maximized, i
> can't move the main window around, only resize it. the web browser node url text is not vertically
> centered in the bar, so its up too high and partially clipped. after i type an agent message in
> agent chat node, then stop, I can't type any more into it. Also the placeholder text isn't
> vertically centered. I cant type in the search of agent tasks node. When I reopen after closing, my
> cursor focus is stuck in the web browser url. we need all these use cases thoroughly tested, with
> extensive automated tests … exhaustively analyze the screens, states, etc, and verify that there are
> no visual issues.

## 1. Nothing could be asked who holds the keyboard, and that is why there are four of these

Four of the seven reports are "I cannot type in X". Every one of them is a question about **who holds
the keyboard**, and until this ticket that was the one thing about the window nothing could be asked:
not a person, not an agent, not a test. Each had to be found by trying to type and watching where the
letters went.

So the first change is a diagnostic, and it is what turned the rest of this from guesswork into
measurement. `unluminous-cli status --section keyboard` answers with three things that are three
different questions:

| Field | Whose answer | What it means |
|---|---|---|
| `holder` | Unluminous's `Focus` | the editing area, the explorer, a terminal tile, the canvas or a plugin |
| `textBox` | `egui`'s `text_edit_focused` | some `TextEdit` has the focus, so **every** other surface stands aside |
| `node` | the canvas | which node is chosen, when the canvas holds the keys |

The second row is the one that explains the shape of these reports. `text_box_has_the_keyboard` is a
question about the **whole window**: while it is true the editing area, every terminal grid and every
provider stand aside, deliberately, so that typing a filter does not also type into the file behind
it. One field holding the focus therefore reads as *the terminal is broken*.

Measured with it, on the released 0.44.0, driving the window with `unluminous-cli input`:

```
0 start                  : holder=editor textBox=false node=
1 clicked address field  : holder=space  textBox=true  node=3
2 clicked terminal grid  : holder=space  textBox=false node=2
```

So the mechanism is sound: a field takes the keyboard and gives it back. What is not sound is where
the keyboard **starts**.

## 2. A project that comes back does not give the canvas the keyboard — reports 1 and 7

Reproduced. Open a project whose canvas has a terminal node on it, close it, open it again, and type:

```
after reopen : holder=editor textBox=false node=
after typing : the letters are in the untitled tab
```

The canvas is showing, its nodes are running, and the keys go to the editing area — which, when the
canvas is **maximised**, is not even on the screen. That is "I cant type in a terminal": nothing is
broken about the terminal, the keyboard was never given to the canvas.

Two things are missing and both are `task-1906`'s own promise that a canvas comes back as it was.

**Which node was chosen is not written down.** `store::read` builds every view with `chosen: None`,
so a canvas comes back with nothing selected and the first key press has nowhere to go. It is one
value a view already has and one line in the file.

**And the keyboard goes to a surface that is on the screen.** `restore_project` leaves `Focus::Editor`
whatever is showing. The rule this adds is the narrow one: *if the editing area is not showing and the
canvas is, the canvas holds the keyboard.* Where both are showing the editing area keeps it, which is
what a text editor should do and what every existing test asserts.

**"Cursor focus is stuck in the web browser url"** is the same fault seen from the other end: on a
canvas where the browser node is the only thing with a text box in it, the address bar is the only
place a caret is drawn, so it looks like the place the keyboard went. Nothing holds it — `textBox` is
`false` after a reopen, measured above — and once the canvas has the keys and a node is chosen, the
caret is where the person put it.

## 3. A field is as tall as the interface says, and its text is not — reports 3 and 5

`appearance.ui.font.size` is **24** on the machine that reported this, against egui's default of about
12.5. Two controls are wrong at any size but the default, and both were reported.

**The address bar.** `controls::field_text_rect` centres a strip of
`ui.text_style_height(TextStyle::Body)` inside the field — the **interface's** row height, 28 points
at 24 point text. `browser_view::address_field` then draws its text at a hard-coded
`FontId::proportional(12.0)` at the **top** of that strip. So the words sit about six points above
centre, and where the strip is taller than the field they are drawn over its border and clipped. At
the default interface size the two happen to agree, which is why it shipped.

The fix is the one `field_text_rect` was written for, finished: **a field centres the row it is going
to draw**, so the size the text will be set in is an argument rather than an assumption.
`field_text_rect_at` and `field_takes_the_whole_rectangle_at` take it; the existing pair keep the Body
row and are what every field that draws at the interface size goes on using.

**The composer's placeholder.** `agent_chat::composer` gives its `TextEdit` the whole of the field,
which is right once there is more than one line in it and wrong when there is one — egui lays text out
at the top, so a two-line-tall box shows its hint against the top edge with a gap under it. The
placeholder is centred by giving the box a rectangle centred on the text it will hold **while the
draft is one line or empty**, and the whole field the moment it is more.

### What reports 2 and 6 turned out to be

Two of the seven did not reproduce, and saying so plainly is part of the answer.

**"I cant type in the search of agent tasks node."** Driven against 0.44.0 with `unluminous-cli input`,
a click in the box takes the keyboard (`textBox=true`) and the letters land in the board's own query.
`the_tasks_node_search_takes_what_is_typed_into_it` is that measurement kept. What is very likely behind
the report is the sentence in §1: while **any** field holds egui's focus every other surface stands
aside, so one field somebody could not get out of reads as the window having stopped answering.

**"If base of infinite space is maximized, i can't move the main window around, only resize it."**
The title bar's drag area is live while a pane is maximised — a double click on it toggles the window,
which is the same `Response`. What was wrong is that the control **had no name**, so nothing outside the
window could ask about it at all: not a test, not an agent, not the accessibility tree. That is the style
guide's own rule broken, and it is why this went unmeasured for as long as it did. See §5.

## 4. Enter in a chat node put a new line in the draft instead of sending — report 4

Reproduced on the released build:

```
input click <composer> ; input text "say OK" ; input key Enter ; input text "hello again"
draft = "say OK\nhello again"   state = idle
```

The message was never sent, and a composer that keeps swallowing Enter is a composer somebody reads
as "I can't type any more into it". `composer::show` asks
`ui.input(|i| i.key_pressed(Enter) && i.modifiers.is_none())`, and `InputState::modifiers` is
**`RawInput::modifiers`** — the frame's modifier *state*, which `egui-winit` maintains from the real
keyboard and which `services::input` never set. Every event `input` queued carried its modifiers on
the event and left the frame's state alone, so anything asking the frame rather than the event was
asking about the real keyboard.

That is a fault in the input mechanism rather than in the composer, and it is the kind that makes a
test lie: `input key s --cmd` would have run no shortcut that reads `input.modifiers`. A `Step` carries
the modifier state now and `Queue::next_frame` turns a change of it into an `Event::ModifiersChanged`,
which is what a device sends and the only thing `egui` builds `InputState::modifiers` from — `RawInput`
has no `modifiers` field to set. A gesture that held one ends with `input::let_go`, so a `--cmd` click
does not leave the window believing the command key is down for the rest of the session.

## 5. A maximised pane and the title bar's drag — report 2

`ViewportCommand::StartDrag` hands the drag to the operating system, so no synthetic event can test
it: what a test can assert is that the control **is there and is the size it should be**. It had no
`widget_info` at all, so nothing could find it — which is the style guide's own rule broken
(`design/style-guide.md`: every control has a plain name), and it is why this went unnoticed. It is
named `Move window` now, and the test asserts that maximising a pane leaves it at least as wide as it
was and still spanning the title bar.

## 6. What the sweep found

The two visual reports are both "a control drawn at one size inside a box measured at another", and
`appearance.ui.font.size = 24` is what exposed them. So the sweep is every node kind, at its opening
size and at its smallest, at the interface size that found these and at 50% and 200% camera zoom,
checked against the one rule they broke — that **no control's text may be drawn outside the box that was
measured for it**. Three things nobody had reported came out of it.

**A node's content escaped the pane.** An Agent Tasks node scrolled off the left of the canvas drew one
of its cards over the **editing area beside it**. `Ui::set_clip_rect` is an assignment, so a lane writing
its own rectangle over the one it was handed threw away the pane's edge the node had been cut to — which
is `components::explorer`'s own rule, written down there since `task-1905`, broken again a lane at a
time. Five calls were replacing rather than intersecting: the board's lanes, its three listings, and the
chat pane's transcript and history. `a_node_scrolled_off_the_canvas_draws_nothing_outside_the_pane`
compares a band of the window above the canvas with a node inside the pane and then hanging off it, so
it needs no accepted picture and answers the same on every machine.

**A Folder node drew the panel's file count.** `footer_top` has said since the node was built that a
node has no footer — *"the strip that counts the project's files belongs to the panel"* — and the count
was drawn anyway, centred on a rectangle of no height sitting on the node's own bottom edge. Half of it
was inside the node and half below, cut through the middle by the node's clip. The footer is also
**named** now, `File count`, which is what made a test of it possible: it had no `widget_info` at all and
so reached the accessibility tree not at all.

**An Agent Tasks node could be made too small to draw a board.** At 320 by 240 — its floor — the rail,
the sprint name and the Add Task button filled the whole node and the first lane's heading was drawn over
its own cards. The board is a rail, a lane and a card at the very least and each has a size of its own,
so the floor is 480 by 360. A chat node's is 300 by 280 for the same reason, and beside it
`composer::hint` drops to the short form where the long one would wrap: a hint that wraps grows the box
past the well measured for one line, and at a node's smallest size the last line of it was drawn below
the node.

## 7. How each of these is tested

| Report | Test |
|---|---|
| 1, 7 — the keyboard after a reopen | `a_project_that_comes_back_with_a_canvas_gives_it_the_keyboard`, and `a_project_showing_both_leaves_the_keyboard_in_the_editing_area` for the half the rule must not widen into |
| 1 — a node's own keys | `a_terminal_node_takes_the_keyboard_when_it_is_clicked` |
| 1, 7 — the choice is written down | `which_node_was_chosen_comes_back_with_the_canvas` |
| 2 — the title bar under a maximise | `the_title_bar_can_still_be_dragged_while_a_pane_is_maximised` |
| 3 — the address bar | `controls::tests::a_fields_text_row_is_measured_at_the_size_the_text_will_be_set_in`, `an_address_bar_draws_its_text_inside_its_own_box` |
| 4 — Enter in a chat node | `a_chat_node_sends_on_enter_rather_than_putting_a_new_line_in_the_draft`, and the four `services::input` unit tests under it |
| 5 — the composer | `a_chat_node_at_its_smallest_keeps_the_composer_inside_it`, `composer::tests::the_hint_is_the_long_one_only_where_it_fits_on_one_line` |
| 6 — the board's search | `the_tasks_node_search_takes_what_is_typed_into_it` |
| the sweep | `a_node_scrolled_off_the_canvas_draws_nothing_outside_the_pane`, `a_folder_node_draws_no_file_count_over_its_own_edge` |

**Every one of the node tests drives the node through `unluminous-cli input`**, which is the point:
`Harness::key_press` sets the real modifiers and could never have found the fault in §4, and a press at
the rectangle the accessibility tree reports lands where the widget in a transformed sublayer is not —
so `on_the_screen` puts a node's layer coordinates back into the window's own points, which is what
`input` takes and what `window screenshot` writes out.

## 8. Deliberately not done

- **Reading `appearance.ui.font.size` in `theme::size`.** The style guide's measurements are absolute
  points and 483 accepted pictures rest on them. What is fixed here is the *disagreement* between a
  box and its text, not the sizes themselves.
- **Making `StartDrag` testable.** It is an operating system modal loop; a test that drove it would be
  a test of Windows.
