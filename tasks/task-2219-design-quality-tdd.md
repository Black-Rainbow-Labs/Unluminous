# task-2219: raising the design bar on the chat pane, the components, the rail and the title bar

## What was asked

> In general, we really need to raise the bar on our design by making better choices, and pay much
> more attention to detail.

Four areas were named, each with a screenshot taken in Unluminous Light:

1. **Agent-Chat.** The history button, the new chat button and the history rows need better design.
   The message bubbles do not achieve the neumorphic effect; the shading and blur look careless and
   lack clarity.
2. **The components inside an answer.** The faders, their labels and their ticks look primitive. The
   lit bar runs past the notches, which makes no sense.
3. **The rail of pane buttons** down the left: the icons and the colour of the chosen ones look bad.
4. **The title bar's icons**, the run widget in particular.

Four more reports arrived while the work was under way, and they are part of the same ticket:

5. *"The little dots next to the button labels need to go."*
6. *"When I switch themes, the font colours sometimes don't immediately update unless I resize. And the
   agent chat components don't match."* The screenshot is a dark plugin theme: the person's own
   questions are drawn in the light theme's near black on a dark bubble, and the component plates are
   `rux`'s own blue grey while the bubbles beside them are the theme's purple grey.
7. *"This little tool icon in agent chat is vestigial and should be removed."* That is the round
   terminal button in a pill above the prompt.

The ticket also asks for Fable to be consulted. The Opus guard (`task-2153`) refuses any sub-agent
that is not Sonnet from an Opus session, so Fable could not be asked from this run. The design below
was made from the research in §2, the `impeccable` skill's polish checklist and the project's own
`design/style-guide.md`. If a Fable review is wanted afterwards, it can be filed as its own ticket.

## 1. What is wrong, measured off the pictures

Every number here was read off `tests/snapshots/survey-light/windows/*.png` and the accepted dark
pictures, enlarged four times.

### 1.1 The bubbles

- A bubble from the person is `Chrome::raised` at `Lift::Medium`: a shadow offset six points and
  blurred seven, and a white one offset the other way. Seven points of Gaussian on a surface twelve
  points from its neighbour means the shadows of two bubbles overlap and the space between them goes
  grey. Nothing has a hard edge. That is the "not enough pinpoint clarity".
- An answer is `Chrome::sunken` at `Lift::Medium`. On a light ground the inset shadow is a grey band
  four points deep inside the top and left edges, which reads as dirt on the bubble rather than as a
  dent.
- The component plates (`rux`) use `rux`'s own `raised` recipe, which is different again. A
  conversation therefore shows three kinds of depth in one column.
- Real neumorphic and soft UI work that holds up (Apple's macOS controls, the reference this board
  came from) separates two things that this recipe merges: a **contact shadow**, tight and fairly dark,
  which says where the surface meets the ground, and an **ambient shadow**, wide and very faint, which
  says how high it is. A one point **lit edge** along the top of the surface is what gives it a crisp
  outline. The current recipe has only the ambient half, at a strength the contact half should have.

### 1.2 The fader

Read from `rux::components::Fader::show`:

- The groove runs the full width of the rectangle, but the ticks and the value run from
  `left + cap/2` to `right - cap/2`. So at the smallest value the lit run is still seven points long,
  starting left of the zero notch, and the groove goes on past the last notch. That is the reported
  "bars going over the top of the notches".
- The lit run is a two point line with a rectangle of the same colour at 12% four points wider drawn
  under it. Off a light ground that rectangle is a pale blue smear wider than the groove.
- The cap is 22 points tall centred on the groove, so its bottom edge is one point above the ticks and
  its six point shadow lies across them.
- The value is set in a mono face and the label in spaced capitals, and they do not share a baseline.

### 1.3 The rail

- The chosen pane is a 24 point square filled `SELECTED_ROW` (`#D6E3FF` in Light) with the icon in
  near black. Three panes are usually open, so the rail is three pale blue squares. The fill is the
  full size of the hit area, so the chosen state reads as a highlighter mark rather than as a control.
- The marks are drawn at fractional positions with a 1.3 to 1.6 point stroke, so at one pixel a point
  each stroke covers two or three pixels at partial alpha. The folder and the file look heavy and the
  branch looks faint, although they are drawn with nearly the same stroke.

### 1.4 The title bar

- The chosen view mode is a solid `ACCENT` square with a white mark: the most saturated thing in the
  window, for a setting that is nearly always on.
- The three view modes are three loose buttons. They are one choice, and nothing groups them.
- The run widget is a word, a three pixel caret, a triangle and a beetle, each a different weight.

### 1.5 The two faults

- `markdown_text::RenderedCache::rendered` re-renders when the source or the width changes. The
  colours are baked into the layout and are not part of the check, so a change of theme keeps every
  message in the old theme's colours until something changes the width. A resize changes the width,
  which is exactly the workaround reported.
- `theme::rux_theme` answers `rux`'s built-in light or dark theme. A plugin theme's surfaces never
  reach `rux`, so every component plate is drawn in `rux`'s grey beside bubbles in the theme's grey.

## 2. Research

- Neumorphism's accepted weakness is contrast: the surface is the colour of the ground and only the
  shadows separate them (setproduct.com, builtin.com, uxdesign.cc). The fixes the literature agrees
  on are a visible edge on anything interactive, real contrast on text and focus, and restraint:
  depth on the few surfaces that are objects, flat everywhere else. WCAG 2.2 1.4.11 asks for 3:1 on
  the boundary of a control that has to be identified.
- JetBrains' new UI rail and VS Code's activity bar both mark the chosen tool window with a quiet
  state rather than a saturated block: VS Code with a two pixel accent bar at the edge and the icon at
  full contrast, JetBrains with a soft rounded fill. Neither paints the icon black on pale blue.
- macOS segmented controls group mutually exclusive modes in one recessed track and raise only the
  chosen segment. That is the right model for the three view modes.

Sources: <https://www.setproduct.com/blog/neumorphism-design-guide>,
<https://builtin.com/design-ux/neumorphism-accessibility>,
<https://uxdesign.cc/is-accessible-neumorphism-possible-87ae1b4b1077>,
<https://www.jetbrains.com/help/rider/New_UI.html>.

## 3. Design

One rule runs through all of it: **depth is for objects, light is for state, and every edge lands on
a pixel.**

### 3.1 One elevation recipe, with a contact shadow and a lit edge

`services::vello_canvas::Chrome::raised` becomes three layers instead of two:

| Layer | Light theme | Dark theme | Purpose |
|---|---|---|---|
| Contact | offset (0, 1), blur 1.5, cool grey at 22% | black at 45% | where the surface meets the ground |
| Ambient | offset (lift/2, lift), blur 2 x lift, cool grey at 10% | black at 30% | how high it is |
| Highlight | offset (-lift/2, -lift/2), blur lift, white at 85% | surface lifted 6% | the light from the top left |

`lift` is 2, 3 and 5 points for `Small`, `Medium` and `Large`, against 4, 6 and 10 before, so a
bubble's shadow reaches about eight points rather than twenty and two neighbours no longer share a
grey band.

On top of the surface, a **one point lit edge** is drawn along the top and the left, white at 70% in
Light and the surface lifted 8% in Dark, fading out along the bottom and the right. That is what makes
the outline sharp. In the light theme a hairline of the theme's divider at 50% goes round the whole
surface as well, for the 3:1 boundary.

`Chrome::sunken` keeps its shape and loses most of its strength: the inset shadow is offset by 1.5
points and blurred by 3 at `Medium`, and the pale half inside the bottom edge is kept. It is a dent,
not a band.

### 3.2 The bubbles

- Both sides are raised, with `Lift::Small`. A conversation is a column of objects of one kind, and
  the side and the tint say who spoke. The person's bubble takes the accent mixed into the card at 6%
  in Light and 10% in Dark; an answer is the plain card.
- The component plates use the same recipe and the same card colour, because §3.6 makes `rux` read
  the same surfaces.

### 3.3 The header and the history

- History and new chat become two small raised round keys, 26 points across, the same height as the
  model selector and lined up on its middle. Hovered, the mark goes from `text_dim` to `text_strong`.
  When the history is open its key is pressed in and its mark is the accent.
- A history row is two lines: the conversation's name in `text_strong`, and under it, in `text_faint`
  at 0.75 of the size, the agent, the number of messages and how long ago, through the `ago` the
  welcome page already uses. The chosen row is a raised card; a hovered row is a hover wash. The cross
  that removes a conversation is drawn only on the row under the pointer, because a column of crosses
  is a column of invitations to delete something.

### 3.4 The fader

- **The groove is the scale.** The groove, the lit run and the ticks all run from the centre of the
  cap at its smallest value to the centre of the cap at its largest. At the smallest value nothing is
  lit, and nothing runs past the last notch.
- The groove is four points deep, `pressed_sm`, with round ends.
- The lit run is a crisp two point line inside the groove, with a one point glow of the colour at 25%
  either side and nothing wider.
- The cap is 12 by 18, rounded by 4, raised, with a lit edge and an engraved grip line of two strokes.
  The ticks start four points below the cap's bottom edge, so no shadow reaches them.
- Ticks sit on whole pixels: eleven, the ends and the middle five points tall, the rest three.
- The label and the value share a baseline.

### 3.5 No dots beside a button's word

`Key::led` is no longer called for a button, a choice, a tab or a form's send key. A chosen choice or
tab is a key that stays down with its word in the accent colour, which says "this one" without a
second mark. A checkbox draws a tick in its cap rather than a lit dot, because a tick is what a person
reads as "ticked". The status dots that are not beside a button's word, such as the welcome page's
status rows and a readout's trend, stay.

### 3.6 `rux` follows Unluminous's palette

`rux` gains `Theme::retoned(surface, ink, blue)`, which copies a theme, replaces its surfaces, its ink
ladder and its accent, and works its elevations out again from them. `theme::rux_theme` builds one
from the active palette:

| `rux` | Unluminous (dark) | Unluminous (light) |
|---|---|---|
| `s0` | `board_page` | `board_page` |
| `s1` | `board_lane` | `board_lane` |
| `s2` (a plate) | `board_card` | `board_card` |
| `s3` | `board_card` lifted 4% | `board_card` |
| `sunken` | `board_well` | `board_well` |
| `i900` … `i300` | `text_strong`, `text`, `text_control`, `text_dim`, `text_faint` | the same |
| `accent.blue` | `accent` | `accent` |

A theme is built once per palette and kept for the life of the process, so `in_step`'s pointer
comparison still costs nothing on a frame where the theme did not change.

### 3.7 The rendered text follows the theme

`markdown_text::Rendered` remembers the colours, the size and the family it was set in, and
`RenderedCache::rendered` makes it again when any of them differs. A change of theme is then one
re-render of what is on the screen, on the frame the theme changes.

### 3.8 The rail

- The chosen pane is drawn as a soft accent wash, 26 by 26 rounded by 7, at 14% in Light and 20% in
  Dark, with the mark in the accent colour and a three by fourteen point accent bar against the rail's
  left edge. The hover is a hover wash at the same size.
- The marks are drawn on the pixel grid (§3.10).

### 3.9 The title bar

- The three view modes become one segmented control: a sunken track 82 by 26, with the chosen segment
  a raised key in the card colour and its mark in the accent. The other two marks are `icon` and go to
  `text_strong` when hovered.
- The run widget's name is a chip with a chevron rather than a three pixel caret, and the play and
  debug buttons are 24 point keys whose marks share one optical size. Play is the green the style
  guide already gives a run button.

### 3.10 Marks land on pixels

`theme::icon::scaled` and the drawing behind every mark snap the centre they are given to the
display's pixel grid, plus half a pixel for an odd stroke width, before anything is drawn. A mark then
draws its verticals and horizontals as one or two full pixels instead of three partial ones.

### 3.11 The pill above the prompt is gone

The tools switch is on `Settings -> Agent-Chat` already, and a picture waiting to go up is shown as a
thumbnail with its own cross on the strip below. The pill, `Act::ToggleTools` and the twenty eight
points it took above the prompt are removed.

## 4. What does not change

- No measurement in `rux`'s reference is changed except the fader's own geometry, which the reference
  does not describe.
- The board's lanes and cards use the same `Chrome::raised`, so they take the new recipe. That is
  intended: the board is the same visual language and was reported with the same blur in `task-1765`.
- The welcome page's status dots and a readout's trend dot are status, not buttons, and are kept.

## 5. Verification

- The light survey, `UNLUMINOUS_SURVEY_THEME=light UPDATE_SNAPSHOTS=1`, before and after, for the chat
  pane, the gallery, the rail, the title bar and the run widget.
- The accepted dark pictures re-accepted after each one changed has been opened.
- A test that switches the theme and checks that a message's rendered colours follow on the next frame
  with no resize.
- A test that a fader at its smallest value lights nothing and that its groove ends at the last notch.
- Pictures at pane zooms of 1, 1.5 and 2 for the chat pane, and the installed build photographed in
  both themes after the release.
