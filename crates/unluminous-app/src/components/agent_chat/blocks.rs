//! The components an agent writes into an answer, drawn as the Agent-Chat design's parts.
//!
//! `tasks/task-2211-agent-chat-intelligent-ui-tdd.md` is what the components are; the Claude Design
//! canvas "Unluminous", page Agent-Chat, artboard ChatParts, is how they look since `task-2235`. One
//! light, top left: a part is a **card** extruded from the pane with the design's paired shadow, and
//! data sits in a **well** carved into it. Everything pressed is round or a pill. There are no dots,
//! no lamps and no segmented meters: state is a mark in a status well, an amount is a slider track, a
//! ring, a dial, an area chart or a sparkline, and colour is the action's blue gradient for data, coral
//! for slower and mint for faster. Every one of those is a `rux` part, and this file only composes them.
//!
//! ## Measured and drawn by one function
//!
//! Every component is laid out by one function that runs twice: once with [`Kit::draw`] false, to say
//! how tall it is before the conversation allocates its row, and once true, to draw. A component cannot
//! be measured as one thing and drawn as another, which is the fault `message::pieces` exists to rule
//! out for the bubbles, and the same rule here.
//!
//! ## What a component remembers
//!
//! A tab chosen, a table's order, a checklist's ticks, a form's values and a calculator's inputs are
//! the person's, and they live in [`BlockState`] on the pane rather than in the transcript: writing
//! them into the conversation would change what goes back up the wire. They last as long as the window.
//!
//! ## What a component can do
//!
//! Send a message, fill the composer, open a project file at a line, or copy text. Nothing else, and
//! only on a press. That is the safety argument for the whole library: the worst an answer can do is put
//! words in the composer.

use std::collections::HashMap;

use egui::{Color32, Id, Pos2, Rect, Sense, Vec2};
use rux::components::{
    card_header, chat_card, chat_well, AreaChart, Dial, DiffCard, FileChip, Lead, Legend,
    LegendRow, Line, PillButton, PillSwitch, Readout, Ring, Sign, Slice, SlicePaint, Status,
    StatusWell, Track, CARD_RADIUS, CHART_RADIUS, WELL_RADIUS,
};
use rux::{Icon, Rux, Style};
use unluminous_chat::rich::component::{
    Action, Align, ChartKind, Component, FieldKind, Kind, Progress as StepState, Tone, Trend,
};
use unluminous_chat::rich::{self, format, Problem};

use super::Act;
use crate::services::plugin_ui::Look;

/// A card's padding: `padding: 12px` for the cards with a header, more for a chart.
const PAD: f32 = 14.0;
/// A chart card's padding: `padding: 20px 22px`.
const CHART_PAD: f32 = 20.0;
/// Between the title and what is under it, and between the rows of a card: `gap: 12px`.
const GAP: f32 = 12.0;
/// Between two components side by side, or one under another inside a card.
const BETWEEN: f32 = 14.0;
/// The title of a component: `font-size: 12.5px; font-weight: 600`.
const TITLE: Style = Style::sans(12.5).semibold().leading(1.35);
/// A quiet line: a detail, a label, a legend, `font-size: 11px`.
const SMALL: Style = Style::sans(11.0).leading(1.4);
/// A path or a figure.
const MONO: Style = Style::mono(11.5);
/// A figure in a row: `font-family: var(--font-sans); font-size: 14px; font-weight: 500`.
const FIGURE: Style = Style::sans(14.0).medium();

/// What one component remembers between frames. Keyed by the message, the block and the path to it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BlockState {
    pub tab: usize,
    /// The column a table is ordered by, and whether it is descending.
    pub sort: Option<(usize, bool)>,
    /// A checklist's items the person has toggled, against what the answer said.
    pub ticks: HashMap<usize, bool>,
    /// A form's values, by field name.
    pub values: HashMap<String, String>,
    /// A calculator's inputs, by name.
    pub inputs: HashMap<String, f64>,
    /// The label of the button or the choice that was pressed, so it stays down.
    pub pressed: Option<String>,
    /// Whether a component that could not be read is showing its source.
    pub source: bool,
    /// The open state of each select in a form.
    pub selects: HashMap<String, rux::components::SelectState>,
}

/// The things a component reports that reach beyond the pane.
pub type Acts = Vec<Act>;

/// Everything a component needs while it is laid out and drawn.
pub struct Kit<'a, 'r> {
    pub rux: &'a mut Rux<'r>,
    /// False while measuring: nothing is painted and nothing is pressed.
    pub draw: bool,
    pub look: &'a Look<'a>,
    pub rendered: &'a mut crate::components::markdown_text::Cache,
    pub states: &'a mut HashMap<String, BlockState>,
    pub scenes: &'a mut crate::services::mermaid_scene::MermaidScenes,
    pub acts: Acts,
    /// The id every control in the component is named under: the conversation's own, so two chats on
    /// one canvas, each with a message 2, do not name their controls the same.
    pub scope: Id,
}

impl Kit<'_, '_> {
    fn z(&self, points: f32) -> f32 {
        self.rux.z(points)
    }

    fn style(&self, style: Style) -> Style {
        self.rux.zs(style)
    }

    fn id(&self, key: &str, part: impl std::hash::Hash + std::fmt::Debug) -> Id {
        self.scope.with(("agent-chat-ui", key)).with(part)
    }

    fn state(&mut self, key: &str) -> &mut BlockState {
        self.states.entry(key.to_owned()).or_default()
    }

    /// One line of words in `style`, elided to `width`, its capitals centred on `middle`. Drawn when
    /// drawing. Answers how wide it came out.
    fn line(
        &mut self,
        style: Style,
        text: &str,
        colour: Color32,
        left: f32,
        middle: f32,
        width: f32,
    ) -> f32 {
        let style = self.style(style);
        let galley = rux::text::elided(self.rux.painter(), style, text, colour, width.max(0.0));
        let wide = galley.size().x;
        if self.draw {
            rux::text::draw_left_capitals(
                self.rux.painter(),
                Pos2::new(left, middle),
                galley,
                style,
                colour,
            );
        }
        wide
    }

    /// How wide `text` is in `style`.
    fn width_of(&self, style: Style, text: &str) -> f32 {
        self.rux.measure(self.style(style), text).x
    }

    /// How tall one line of `style` is.
    fn height_of(&self, style: Style) -> f32 {
        self.rux.measure(self.style(style), "Ag").y
    }

    /// Wrapped words in `style`, drawn when drawing. Answers their height.
    fn words(&mut self, style: Style, text: &str, colour: Color32, at: Pos2, width: f32) -> f32 {
        let style = self.style(style);
        let galley = rux::text::wrapped(self.rux.painter(), style, text, colour, width.max(1.0));
        let height = galley.size().y;
        if self.draw {
            self.rux.painter().galley(at, galley, colour);
        }
        height
    }

    /// Markdown, through the same renderer the answers use. Answers its height.
    fn markdown(&mut self, key: &str, text: &str, at: Pos2, width: f32) -> f32 {
        let look = self.look;
        let made = self.rendered.rendered(
            key,
            text,
            look.renderer,
            &look.font_family,
            look.font_size * 0.86,
            super::message::colours(look),
            width.max(24.0),
            look.highlighter,
        );
        let height = made.height();
        if self.draw {
            let rect = Rect::from_min_size(at, Vec2::new(width, height));
            let code = super::message::code_colours(look);
            crate::components::markdown_text::show_with(
                self.rux.ui,
                rect,
                made,
                look.renderer,
                0.0,
                Some(code),
            );
        }
        height
    }

    /// A component's title, when it has one, with quiet words at its right, and the gap under it.
    fn title(&mut self, title: &str, at: Pos2, width: f32, right: Option<&str>) -> f32 {
        if title.is_empty() {
            return 0.0;
        }
        let theme = self.rux.theme();
        let right_width = match right {
            Some(words) if !words.is_empty() => self.width_of(SMALL, words) + self.z(10.0),
            _ => 0.0,
        };
        let height = self.words(TITLE, title, theme.ink.i900, at, width - right_width);
        if let Some(words) = right.filter(|words| !words.is_empty()) {
            let middle = at.y + self.height_of(TITLE) / 2.0;
            let wide = self.width_of(SMALL, words);
            self.line(SMALL, words, theme.ink.i400, at.x + width - wide, middle, wide + 1.0);
        }
        height + self.z(GAP)
    }

    /// Lay out `body` inside a card: measured first, then the card drawn, then the body drawn in it.
    fn carded(
        &mut self,
        at: Pos2,
        width: f32,
        padding: f32,
        radius: f32,
        stretch: f32,
        body: &mut dyn FnMut(&mut Self, Pos2, f32) -> f32,
    ) -> f32 {
        let pad = self.z(padding);
        let inner = Pos2::new(at.x + pad, at.y + pad);
        let inner_width = (width - pad * 2.0).max(1.0);
        let drawing = self.draw;
        self.draw = false;
        let height = (body(self, inner, inner_width) + pad * 2.0).max(stretch);
        self.draw = drawing;
        if drawing {
            let rect = Rect::from_min_size(at, Vec2::new(width, height));
            chat_card(self.rux, rect, self.z(radius));
            body(self, inner, inner_width);
        }
        height
    }

    /// A well carved into whatever is under it.
    fn well(&mut self, rect: Rect, radius: f32) {
        if self.draw {
            chat_well(self.rux, rect, self.z(radius));
        }
    }

    /// A card's header row: a well holding a state or a mark, a title and a detail line.
    fn header(&mut self, lead: Lead, title: &str, detail: &str, at: Pos2, width: f32) -> f32 {
        let height = self.z(34.0);
        if self.draw {
            let row = Rect::from_min_size(at, Vec2::new(width, height));
            card_header(self.rux, row, lead, title, detail, None);
        }
        height
    }
}

/// How a component sits on the pane: on a card of its own, or straight on the pane.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Frame {
    Card { padding: f32, radius: f32 },
    Bare,
}

/// How each kind of component is framed, read off the canvas: readouts, a table, a diff, buttons and
/// pills are their own surfaces and sit on the pane; everything else is on a card.
fn frame_of(kind: &Kind, title: &str) -> Frame {
    let card = Frame::Card { padding: PAD, radius: CARD_RADIUS };
    let chart = Frame::Card { padding: CHART_PAD, radius: CHART_RADIUS };
    match kind {
        Kind::Stats { .. }
        | Kind::Table { .. }
        | Kind::Diff { .. }
        | Kind::Actions { .. }
        | Kind::KeyValue { .. } => Frame::Bare,
        Kind::Badges { .. } if title.is_empty() => Frame::Bare,
        Kind::Choices { .. } => Frame::Bare,
        Kind::Chart(_) | Kind::Progress { .. } => chart,
        _ => card,
    }
}

/// Lay out one block of an answer at `at`, `width` wide. Answers how tall it is.
pub fn block(
    kit: &mut Kit<'_, '_>,
    read: &Result<Component, Vec<Problem>>,
    source: &str,
    key: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    match read {
        Ok(component) => component_at(kit, component, key, at, width, true, 0.0),
        Err(problems) => problem(kit, problems, source, key, at, width),
    }
}

/// One component, on a card of its own when `carded` and its kind sits on one. `stretch` is the least
/// height it may have, so a row of cards side by side can be one height.
fn component_at(
    kit: &mut Kit<'_, '_>,
    component: &Component,
    key: &str,
    at: Pos2,
    width: f32,
    carded: bool,
    stretch: f32,
) -> f32 {
    let title = component.title.as_str();
    match &component.kind {
        Kind::Pending => pending(kit, at, width),
        Kind::Card { subtitle, text, badge, children } => {
            kit.carded(at, width, PAD + 2.0, CARD_RADIUS, stretch, &mut |kit, at, width| {
                card(kit, key, title, subtitle, text, badge, children, at, width)
            })
        }
        Kind::Columns { columns } => columns_at(kit, key, title, columns, at, width),
        Kind::Tabs { tabs } => tabs_at(kit, key, title, tabs, at, width),
        Kind::Stack { children } => {
            let mut pen = at.y + bare_title(kit, title, at, width);
            for (index, child) in children.iter().enumerate() {
                pen += component_at(
                    kit,
                    child,
                    &format!("{key}/{index}"),
                    Pos2::new(at.x, pen),
                    width,
                    true,
                    0.0,
                ) + kit.z(BETWEEN);
            }
            (pen - at.y - kit.z(BETWEEN)).max(0.0)
        }
        _ if carded => match frame_of(&component.kind, title) {
            Frame::Card { padding, radius } => {
                kit.carded(at, width, padding, radius, stretch, &mut |kit, at, width| {
                    leaf(kit, component, key, at, width)
                })
            }
            Frame::Bare => {
                // Choices draws its question as its heading, which is the title when it has no question.
                let top = match component.kind {
                    Kind::Choices { .. } => 0.0,
                    _ => bare_title(kit, title, at, width),
                };
                top + leaf_untitled(kit, component, key, Pos2::new(at.x, at.y + top), width)
            }
        },
        _ => leaf(kit, component, key, at, width),
    }
}

/// The title of a component that sits straight on the pane, inset a little so it lines up with the
/// words inside the cards beside it.
fn bare_title(kit: &mut Kit<'_, '_>, title: &str, at: Pos2, width: f32) -> f32 {
    match title.is_empty() {
        true => 0.0,
        false => kit.title(title, Pos2::new(at.x + kit.z(4.0), at.y), width - kit.z(4.0), None),
    }
}

/// A component that holds no others, with its title.
fn leaf(kit: &mut Kit<'_, '_>, component: &Component, key: &str, at: Pos2, width: f32) -> f32 {
    let title = component.title.as_str();
    // The kinds whose title is part of their own header row draw it themselves.
    let own_title = matches!(
        component.kind,
        Kind::Callout { .. }
            | Kind::Files { .. }
            | Kind::Diff { .. }
            | Kind::Chart(_)
            | Kind::Choices { .. }
    );
    let top = match own_title {
        true => 0.0,
        false => kit.title(title, at, width, None),
    };
    top + leaf_untitled(kit, component, key, Pos2::new(at.x, at.y + top), width)
}

/// A component that holds no others, without its title, which the caller has drawn.
fn leaf_untitled(
    kit: &mut Kit<'_, '_>,
    component: &Component,
    key: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    let title = component.title.as_str();
    match &component.kind {
        Kind::Callout { tone, text } => callout(kit, key, title, *tone, text, at, width),
        Kind::Steps { items, timeline } => steps(kit, items, *timeline, at, width),
        Kind::KeyValue { items } => keyvalue(kit, items, at, width),
        Kind::Badges { items } => {
            let items: Vec<(String, Tone)> =
                items.iter().map(|b| (b.label.clone(), b.tone)).collect();
            badges(kit, &items, at, width)
        }
        Kind::Stats { items } => {
            let cells: Vec<Cell> = items
                .iter()
                .map(|stat| Cell {
                    label: stat.label.clone(),
                    value: stat.value.clone(),
                    delta: stat.delta.clone(),
                    good: match stat.trend {
                        Trend::Flat => None,
                        Trend::Up => Some(stat.up_is_good),
                        Trend::Down => Some(!stat.up_is_good),
                    },
                    note: stat.note.clone(),
                    history: stat.history.iter().map(|value| *value as f32).collect(),
                })
                .collect();
            readouts(kit, &cells, at, width)
        }
        Kind::Progress { items } => progress(kit, items, at, width),
        Kind::Chart(chart) => {
            let series: Vec<(String, Vec<f64>)> =
                chart.series.iter().map(|s| (s.name.clone(), s.values.clone())).collect();
            chart_at(kit, key, title, chart.kind, &chart.labels, &series, &chart.unit, at, width)
        }
        Kind::Table { columns, rows } => table(kit, key, columns, rows, at, width),
        Kind::Files { items } => files(kit, key, title, items, at, width),
        Kind::Diff { path, text } => diff(kit, key, title, path, text, at, width),
        Kind::Diagram { source } => diagram(kit, source, at, width),
        Kind::Actions { items } => {
            let buttons: Vec<(String, Option<Action>, bool)> =
                items.iter().map(|b| (b.label.clone(), b.action.clone(), b.primary)).collect();
            pills(kit, key, &buttons, at, width)
        }
        Kind::Choices { question, options } => {
            let heading = if question.is_empty() { title } else { question.as_str() };
            let top = bare_title(kit, heading, at, width);
            let buttons: Vec<(String, Option<Action>, bool)> =
                options.iter().map(|o| (o.clone(), Some(Action::Send(o.clone())), false)).collect();
            top + pills(kit, key, &buttons, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Checklist { items } => checklist(kit, key, items, at, width),
        Kind::Form { fields, submit } => form(kit, key, title, fields, submit, at, width),
        Kind::Calculator { inputs, outputs, chart } => {
            calculator(kit, key, inputs, outputs, chart.as_ref(), at, width)
        }
        // The containers are handled by `component_at`; drawn bare here they are their children.
        Kind::Card { .. }
        | Kind::Columns { .. }
        | Kind::Tabs { .. }
        | Kind::Stack { .. }
        | Kind::Pending => component_at(kit, component, key, at, width, false, 0.0),
    }
}

/// A component whose `type` has not arrived yet: a card whose status well is running.
fn pending(kit: &mut Kit<'_, '_>, at: Pos2, width: f32) -> f32 {
    kit.carded(at, width, 12.0, CARD_RADIUS, 0.0, &mut |kit, at, width| {
        kit.header(
            Lead::Status(Status::Running),
            "Assembling",
            "The agent is still writing this part",
            at,
            width,
        )
    })
}

/// A block that did not read: an error card saying what is wrong, with the source on request.
fn problem(
    kit: &mut Kit<'_, '_>,
    problems: &[Problem],
    source: &str,
    key: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    let showing = kit.states.get(key).is_some_and(|state| state.source);
    let lines: Vec<String> =
        problems.iter().filter(|p| p.is_error()).take(4).map(Problem::line).collect();
    let key = key.to_owned();
    let source = source.to_owned();
    kit.carded(at, width, 12.0, CARD_RADIUS, 0.0, &mut |kit, at, width| {
        let theme = kit.rux.theme();
        let detail = lines.first().cloned().unwrap_or_default();
        let mut pen =
            at.y + kit.header(
                Lead::Status(Status::Failed),
                "This component could not be drawn",
                &detail,
                at,
                width,
            ) + kit.z(GAP);
        if lines.len() > 1 || showing {
            let text = match showing {
                true => source.clone(),
                false => lines[1..].join("\n"),
            };
            let pad = kit.z(14.0);
            let style = kit.style(MONO.at(11.0));
            let galley = rux::text::wrapped(
                kit.rux.painter(),
                style,
                &text,
                theme.ink.i700,
                width - pad * 2.0,
            );
            let rect = Rect::from_min_size(
                Pos2::new(at.x, pen),
                Vec2::new(width, galley.size().y + kit.z(24.0)),
            );
            kit.well(rect, WELL_RADIUS);
            if kit.draw {
                kit.rux.painter().galley(
                    rect.min + Vec2::new(pad, kit.z(12.0)),
                    galley,
                    theme.ink.i700,
                );
            }
            pen += rect.height() + kit.z(GAP);
        }
        let label = if showing { "Hide the source" } else { "Show the source" };
        let button = PillButton::new(label).ghost();
        let size = button.measure(kit.rux);
        if kit.draw {
            let rect = Rect::from_min_size(Pos2::new(at.x, pen), size);
            if in_scope(kit, &key, "source", |rux| button.show(rux, rect).clicked()) {
                kit.state(&key).source = !showing;
            }
        }
        pen + size.y - at.y
    })
}

/// Draw a control under an id of the component's own, so two answers' buttons do not share one.
fn in_scope<R>(
    kit: &mut Kit<'_, '_>,
    key: &str,
    part: impl std::hash::Hash + std::fmt::Debug,
    add: impl FnOnce(&mut Rux<'_>) -> R,
) -> R {
    let id = kit.id(key, part);
    let rux = &mut *kit.rux;
    let mut child = rux.ui.new_child(egui::UiBuilder::new().id_salt(id));
    let mut inner = Rux { ui: &mut child, state: rux.state, chrome: rux.chrome };
    add(&mut inner)
}

/// A card: its title and subtitle, a pill at the top right, its words, and its children laid bare.
#[allow(clippy::too_many_arguments)]
fn card(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    subtitle: &str,
    text: &str,
    badge: &str,
    children: &[Component],
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let mut pen = at.y;
    let badge_width = match badge.is_empty() {
        true => 0.0,
        false => pill_width(kit, badge) + kit.z(10.0),
    };
    if !title.is_empty() {
        let height =
            kit.words(TITLE, title, theme.ink.i900, Pos2::new(at.x, pen), width - badge_width);
        if !badge.is_empty() {
            let middle = pen + kit.height_of(TITLE) / 2.0;
            pill(
                kit,
                badge,
                Tone::Success,
                Pos2::new(at.x + width - badge_width + kit.z(10.0), middle),
            );
        }
        pen += height;
    }
    if !subtitle.is_empty() {
        pen += kit.z(2.0) + kit.words(SMALL, subtitle, theme.ink.i400, Pos2::new(at.x, pen), width);
    }
    if !text.is_empty() {
        if pen > at.y {
            pen += kit.z(10.0);
        }
        pen += kit.markdown(&format!("{key}#text"), text, Pos2::new(at.x, pen), width);
    }
    for (index, child) in children.iter().enumerate() {
        if pen > at.y {
            pen += kit.z(GAP);
        }
        pen += component_at(
            kit,
            child,
            &format!("{key}/{index}"),
            Pos2::new(at.x, pen),
            width,
            false,
            0.0,
        );
    }
    pen - at.y
}

/// How tall a pill is: `padding: 4px 12px` round 11 point words.
const PILL: f32 = 24.0;

/// How wide a pill with `label` is.
fn pill_width(kit: &Kit<'_, '_>, label: &str) -> f32 {
    kit.width_of(SMALL, label) + kit.z(24.0)
}

/// A pill carved into the card with its words in its tone's colour, centred on `left_centre.y`.
///
/// **No lamp beside the word**, `task-2219`: "the little dots next to the button labels need to go".
/// The tone is the colour of the words, which says the same thing with one mark rather than two.
fn pill(kit: &mut Kit<'_, '_>, label: &str, tone: Tone, left_centre: Pos2) {
    if !kit.draw {
        return;
    }
    let theme = kit.rux.theme();
    let rect = Rect::from_min_size(
        Pos2::new(left_centre.x, left_centre.y - kit.z(PILL / 2.0)),
        Vec2::new(pill_width(kit, label), kit.z(PILL)),
    );
    kit.well(rect, PILL / 2.0);
    let ink = tone_colour(theme, tone);
    kit.line(
        SMALL,
        label,
        ink,
        rect.left() + kit.z(12.0),
        rect.center().y,
        rect.width() - kit.z(20.0),
    );
}

/// The colour a tone is shown in: the design's coral for danger, mint for success, the waiting amber
/// for a warning and the sky for information. Neutral is the quiet ink.
fn tone_colour(theme: &rux::Theme, tone: Tone) -> Color32 {
    match tone {
        Tone::Neutral => theme.ink.i500,
        Tone::Info => theme.chat.sky,
        Tone::Success => theme.chat.faster,
        Tone::Warning => theme.chat.waiting,
        Tone::Danger => theme.chat.slower,
        Tone::Tip => theme.accent.violet,
    }
}

/// What a callout's well holds: a state for the tones that are states, and a mark for the others.
fn tone_lead(tone: Tone) -> Lead {
    match tone {
        Tone::Success => Lead::Status(Status::Done),
        Tone::Warning => Lead::Status(Status::NeedsOk),
        Tone::Danger => Lead::Status(Status::Failed),
        Tone::Tip => Lead::Mark(Icon::Spark),
        Tone::Info | Tone::Neutral => Lead::Mark(Icon::Docs),
    }
}

/// Columns side by side, one card each and all one height, or one under another when the pane is too
/// narrow for them.
fn columns_at(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    columns: &[Component],
    at: Pos2,
    width: f32,
) -> f32 {
    let mut pen = at.y + bare_title(kit, title, at, width);
    let count = columns.len().max(1);
    let gap = kit.z(BETWEEN);
    let each = (width - gap * (count as f32 - 1.0)) / count as f32;
    if each < kit.z(200.0) {
        for (index, column) in columns.iter().enumerate() {
            pen += component_at(
                kit,
                column,
                &format!("{key}/{index}"),
                Pos2::new(at.x, pen),
                width,
                true,
                0.0,
            ) + kit.z(BETWEEN);
        }
        return (pen - at.y - kit.z(BETWEEN)).max(0.0);
    }
    let drawing = kit.draw;
    kit.draw = false;
    let tallest = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            component_at(
                kit,
                column,
                &format!("{key}/{index}"),
                Pos2::new(at.x, pen),
                each,
                true,
                0.0,
            )
        })
        .fold(0.0_f32, f32::max);
    kit.draw = drawing;
    if drawing {
        for (index, column) in columns.iter().enumerate() {
            let x = at.x + index as f32 * (each + gap);
            component_at(
                kit,
                column,
                &format!("{key}/{index}"),
                Pos2::new(x, pen),
                each,
                true,
                tallest,
            );
        }
    }
    pen + tallest - at.y
}

/// A pill switch, one pill a tab, over the chosen tab's components.
fn tabs_at(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    tabs: &[rich::component::Tab],
    at: Pos2,
    width: f32,
) -> f32 {
    let mut pen = at.y + bare_title(kit, title, at, width);
    let chosen = kit.states.get(key).map_or(0, |state| state.tab).min(tabs.len().saturating_sub(1));
    let labels: Vec<String> = tabs.iter().map(|tab| tab.label.clone()).collect();
    let height = kit.z(46.0);
    if kit.draw {
        let rect = Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, height));
        let picked = in_scope(kit, key, "tabs", |rux| {
            PillSwitch::new(&labels, Some(chosen), "Tab").show(rux, rect)
        });
        if let Some(index) = picked {
            kit.state(key).tab = index;
        }
    }
    pen += height + kit.z(BETWEEN);
    if let Some(tab) = tabs.get(chosen) {
        for (index, child) in tab.children.iter().enumerate() {
            pen += component_at(
                kit,
                child,
                &format!("{key}/{chosen}/{index}"),
                Pos2::new(at.x, pen),
                width,
                true,
                0.0,
            ) + kit.z(BETWEEN);
        }
    }
    pen - kit.z(BETWEEN) - at.y
}

/// A callout: the tone as a mark in a well beside the title, the words under the title.
#[allow(clippy::too_many_arguments)]
fn callout(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    tone: Tone,
    text: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let side = kit.z(34.0);
    let indent = side + kit.z(12.0);
    if kit.draw {
        let well = Rect::from_min_size(at, Vec2::splat(side));
        match tone_lead(tone) {
            Lead::Status(status) => {
                StatusWell::new(status).show(kit.rux, well);
            }
            Lead::Mark(icon) => {
                chat_well(kit.rux, well, side / 2.0);
                let colour = tone_colour(theme, tone);
                kit.rux.mark(rux::Mark::new(icon, kit.z(18.0)), well.center(), colour);
            }
        }
    }
    let mut pen = at.y;
    let words_left = at.x + indent;
    let room = width - indent;
    let title_height = kit.height_of(TITLE);
    if !title.is_empty() {
        // The first line of the title sits on the middle of the well, as a card header's does.
        let top = match text.is_empty() {
            true => at.y + side / 2.0 - title_height / 2.0,
            false => at.y + kit.z(2.0),
        };
        pen = top
            + kit.words(TITLE, title, theme.ink.i900, Pos2::new(words_left, top), room)
            + kit.z(4.0);
    } else if !text.is_empty() {
        pen += kit.z(7.0);
    }
    if !text.is_empty() {
        pen += kit.markdown(&format!("{key}#text"), text, Pos2::new(words_left, pen), room);
    }
    (pen - at.y).max(side)
}

/// Steps and timelines: a status well for each item, its title and words beside it, and its time.
fn steps(
    kit: &mut Kit<'_, '_>,
    items: &[rich::component::Step],
    timeline: bool,
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let side = kit.z(34.0);
    let indent = side + kit.z(14.0);
    let mut pen = at.y;
    for (index, step) in items.iter().enumerate() {
        if index > 0 {
            pen += kit.z(16.0);
        }
        let status = match step.state {
            StepState::Done => Status::Done,
            StepState::Active => Status::Running,
            StepState::Todo => Status::Waiting,
        };
        let time = match timeline {
            true => step.time.clone(),
            false => String::new(),
        };
        let time_width = match time.is_empty() {
            true => 0.0,
            false => kit.width_of(SMALL, &time) + kit.z(10.0),
        };
        let room = width - indent;
        let title = plain(&step.title);
        let text = plain(&step.text);
        let title_height = kit.height_of(Style::sans(12.5));
        let text_height = match text.is_empty() {
            true => 0.0,
            false => {
                let galley = rux::text::wrapped(
                    kit.rux.painter(),
                    kit.style(SMALL),
                    &text,
                    theme.ink.i400,
                    room.max(1.0),
                );
                galley.size().y + kit.z(1.0)
            }
        };
        let block = title_height + text_height;
        let height = block.max(side);
        let top = pen + (height - block) / 2.0;
        if kit.draw {
            let well = Rect::from_min_size(
                Pos2::new(at.x, pen + (height - side) / 2.0),
                Vec2::splat(side),
            );
            StatusWell::new(status).show(kit.rux, well);
            let ink = match step.state {
                StepState::Todo => theme.ink.i500,
                _ => theme.ink.i900,
            };
            kit.line(
                Style::sans(12.5),
                &title,
                ink,
                at.x + indent,
                top + title_height / 2.0,
                room - time_width,
            );
            if !time.is_empty() {
                let wide = kit.width_of(SMALL, &time);
                kit.line(
                    SMALL,
                    &time,
                    theme.ink.i400,
                    at.x + width - wide,
                    top + title_height / 2.0,
                    wide + 1.0,
                );
            }
            if !text.is_empty() {
                kit.words(
                    SMALL,
                    &text,
                    theme.ink.i400,
                    Pos2::new(at.x + indent, top + title_height + kit.z(1.0)),
                    room,
                );
            }
        }
        pen += height;
    }
    pen - at.y
}

/// Rows of a name and a value in a well, a hairline between each.
fn keyvalue(kit: &mut Kit<'_, '_>, items: &[(String, String)], at: Pos2, width: f32) -> f32 {
    let theme = kit.rux.theme();
    let row = kit.z(36.0);
    let pad_y = kit.z(8.0);
    let height = row * items.len() as f32 + pad_y * 2.0;
    let rect = Rect::from_min_size(at, Vec2::new(width, height));
    kit.well(rect, CARD_RADIUS);
    if kit.draw {
        let column = items
            .iter()
            .map(|(key, _)| kit.width_of(Style::sans(12.0), key))
            .fold(0.0_f32, f32::max)
            .min(width * 0.45)
            + kit.z(28.0);
        for (index, (key, value)) in items.iter().enumerate() {
            let top = rect.top() + pad_y + row * index as f32;
            let middle = top + row / 2.0;
            if index > 0 {
                hairline(kit, rect.left() + kit.z(14.0), rect.right() - kit.z(14.0), top);
            }
            let left = rect.left() + kit.z(14.0);
            kit.line(Style::sans(12.0), key, theme.ink.i500, left, middle, column - kit.z(20.0));
            let value = plain(value);
            let style = match rux::components::is_figure(&value) || value.contains(['_', '/', '\\'])
            {
                true => MONO.at(12.0),
                false => Style::sans(12.0),
            };
            kit.line(
                style,
                &value,
                theme.ink.i900,
                left + column,
                middle,
                width - column - kit.z(28.0),
            );
        }
    }
    height
}

/// A hairline across a well: `border-top: 1px solid rgba(255, 255, 255, 0.04)` on the dark ground.
fn hairline(kit: &mut Kit<'_, '_>, left: f32, right: f32, y: f32) {
    if !kit.draw {
        return;
    }
    let theme = kit.rux.theme();
    let colour = match theme.dark {
        true => Color32::from_white_alpha(12), // any ground: the canvas's hairline on the dark well
        false => Color32::from_black_alpha(14), // any ground: the same line on the light well
    };
    kit.rux.painter().hline(left..=right, y, egui::Stroke::new(1.0, colour));
}

/// A wrapped row of pills.
fn badges(kit: &mut Kit<'_, '_>, items: &[(String, Tone)], at: Pos2, width: f32) -> f32 {
    let row = kit.z(PILL);
    let gap = kit.z(8.0);
    let mut x = at.x;
    let mut y = at.y;
    for (label, tone) in items {
        let need = pill_width(kit, label);
        if x > at.x && x + need > at.x + width {
            x = at.x;
            y += row + gap;
        }
        pill(kit, label, *tone, Pos2::new(x, y + row / 2.0));
        x += need + gap;
    }
    y + row - at.y
}

/// One readout: a stat, or a calculator's output.
struct Cell {
    label: String,
    value: String,
    delta: String,
    /// Whether the change is good news, bad news, or neither.
    good: Option<bool>,
    note: String,
    /// The last few values, drawn as a sparkline.
    history: Vec<f32>,
}

/// `178 s` as the number and the unit drawn smaller after it: ` s`. A value with no short unit after
/// its number is all number.
pub fn split_unit(value: &str) -> (&str, &str) {
    let Some(space) = value.rfind(' ') else { return (value, "") };
    let (number, unit) = value.split_at(space);
    let unit_word = unit.trim();
    let short = !unit_word.is_empty()
        && unit_word.chars().count() <= 4
        && unit_word.chars().all(char::is_alphabetic);
    match short && number.chars().any(|c| c.is_ascii_digit()) {
        true => (number, unit),
        false => (value, ""),
    }
}

/// Readout cards side by side, as many to a row as are at least 104 points wide.
fn readouts(kit: &mut Kit<'_, '_>, cells: &[Cell], at: Pos2, width: f32) -> f32 {
    if cells.is_empty() {
        return 0.0;
    }
    let theme = kit.rux.theme();
    let gap = kit.z(14.0);
    let columns = (((width + gap) / (kit.z(104.0) + gap)).floor() as usize).clamp(1, cells.len());
    let each = (width - gap * (columns as f32 - 1.0)) / columns as f32;
    let readout = |cell: &'_ Cell| {
        let colour = match cell.good {
            Some(true) => theme.chat.faster,
            Some(false) => theme.chat.slower,
            None => theme.ink.i400,
        };
        let (value, unit) = split_unit(&cell.value);
        (value.to_owned(), unit.to_owned(), colour)
    };
    let mut pen = at.y;
    for chunk in cells.chunks(columns) {
        let height = chunk
            .iter()
            .map(|cell| {
                let (value, unit, colour) = readout(cell);
                Readout::new(&cell.label, &value, &unit)
                    .delta(&cell.delta, colour)
                    .note(&cell.note)
                    .spark(&cell.history)
                    .height(kit.rux)
            })
            .fold(0.0_f32, f32::max);
        if kit.draw {
            for (index, cell) in chunk.iter().enumerate() {
                let rect = Rect::from_min_size(
                    Pos2::new(at.x + index as f32 * (each + gap), pen),
                    Vec2::new(each, height),
                );
                let (value, unit, colour) = readout(cell);
                Readout::new(&cell.label, &value, &unit)
                    .delta(&cell.delta, colour)
                    .note(&cell.note)
                    .spark(&cell.history)
                    .show(kit.rux, rect);
            }
        }
        pen += height + gap;
    }
    pen - gap - at.y
}

/// Words a component sets as plain text, with the markdown a model writes out of habit taken off:
/// the backticks round code and the asterisks and underscores of emphasis.
pub fn plain(text: &str) -> String {
    text.replace("**", "").replace("__", "").replace('`', "")
}

/// `value` with its first number replaced by `fraction` of itself, written the same way.
///
/// `178 s` at a half is `89 s`; `$1,204.50` at a half is `$602.25`. Text with no number in it is
/// unchanged.
pub fn rolled(value: &str, fraction: f32) -> String {
    if fraction >= 1.0 {
        return value.to_owned();
    }
    let bytes: Vec<char> = value.chars().collect();
    let Some(start) = bytes.iter().position(|c| c.is_ascii_digit()) else {
        return value.to_owned();
    };
    let mut end = start;
    while end < bytes.len()
        && (bytes[end].is_ascii_digit()
            || ((bytes[end] == ',' || bytes[end] == '.')
                && bytes.get(end + 1).is_some_and(char::is_ascii_digit)))
    {
        end += 1;
    }
    let number: String = bytes[start..end].iter().collect();
    let plain = number.replace(',', "");
    let Ok(parsed) = plain.parse::<f64>() else {
        return value.to_owned();
    };
    let decimals = plain.split_once('.').map_or(0, |(_, after)| after.len());
    let now = parsed * fraction.clamp(0.0, 1.0) as f64;
    let mut written = format!("{now:.decimals$}");
    if number.contains(',') {
        written = group(&written);
    }
    let before: String = bytes[..start].iter().collect();
    let after: String = bytes[end..].iter().collect();
    format!("{before}{written}{after}")
}

/// Thousands separators on a number written with `.` decimals.
fn group(text: &str) -> String {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let mut out = String::new();
    for (at, digit) in whole.chars().enumerate() {
        if at > 0 && (whole.len() - at) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    match fraction {
        Some(fraction) => format!("{out}.{fraction}"),
        None => out,
    }
}

/// Labelled amounts, each on the design's slider track: its name and figure over a carved groove
/// filled with the action's gradient up to the amount, with a raised knob at its end.
fn progress(kit: &mut Kit<'_, '_>, items: &[rich::component::Bar], at: Pos2, width: f32) -> f32 {
    let theme = kit.rux.theme();
    let line = kit.height_of(FIGURE);
    let track = kit.z(22.0);
    let mut pen = at.y;
    for (index, bar) in items.iter().enumerate() {
        if index > 0 {
            pen += kit.z(18.0);
        }
        let fraction = (bar.value / bar.max).clamp(0.0, 1.0) as f32;
        let figure = match (bar.max - 100.0).abs() < f64::EPSILON {
            true => format!("{}%", format::plain(bar.value)),
            false => format!("{} of {}", format::plain(bar.value), format::plain(bar.max)),
        };
        let figure_width = kit.width_of(FIGURE, &figure);
        kit.line(
            FIGURE,
            &figure,
            theme.ink.i900,
            at.x + width - figure_width,
            pen + line / 2.0,
            figure_width + 1.0,
        );
        kit.line(
            Style::sans(12.5),
            &bar.label,
            theme.ink.i900,
            at.x,
            pen + line / 2.0,
            width - figure_width - kit.z(12.0),
        );
        if kit.draw {
            let rect = Rect::from_min_size(
                Pos2::new(at.x, pen + line + kit.z(10.0)),
                Vec2::new(width, track),
            );
            Track::new(fraction, 1.0).label(bar.label.clone()).show(kit.rux, rect);
        }
        pen += line + kit.z(10.0) + track;
    }
    pen - at.y
}

/// A value written the way a chart's figures are: its unit after it, or `$` in front.
fn figure_of(value: f64, unit: &str) -> String {
    let text = match value.abs() >= 10_000.0 {
        true => format::number(value, "compact", ""),
        false => format::number(value, "number", ""),
    };
    match unit {
        "" => text,
        "$" | "£" | "€" => format!("{unit}{text}"),
        _ => format!("{text} {unit}"),
    }
}

/// How one value changed against another, as the design writes it: `+39%`, `−7%`.
fn change_of(before: f64, now: f64) -> Option<String> {
    if before.abs() < f64::EPSILON {
        return None;
    }
    let percent = ((now - before) / before.abs() * 100.0).round();
    Some(match percent >= 0.0 {
        true => format!("+{percent:.0}%"),
        false => format!("\u{2212}{:.0}%", percent.abs()),
    })
}

/// The colour a change is drawn in: coral when the number went up and mint when it came down, which
/// is the design's slower and faster, and the quiet ink when it did not move.
fn change_colour(theme: &rux::Theme, before: f64, now: f64) -> Color32 {
    match now.partial_cmp(&before) {
        Some(std::cmp::Ordering::Greater) => theme.chat.slower,
        Some(std::cmp::Ordering::Less) => theme.chat.faster,
        _ => theme.ink.i400,
    }
}

/// A chart from labels and named series, drawn as the design's charts: a ring with its legend for a
/// donut, a dial for what was added, tracks with last time's mark for bars, and a smooth area chart in
/// a well for a line or an area.
#[allow(clippy::too_many_arguments)]
fn chart_at(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    kind: ChartKind,
    labels: &[String],
    series: &[(String, Vec<f64>)],
    unit: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    if series.is_empty() || series.iter().all(|(_, values)| values.is_empty()) {
        return kit.title(title, at, width, None);
    }
    match kind {
        ChartKind::Donut => ring_chart(kit, title, labels, &series[0].1, unit, at, width),
        ChartKind::Dial => dial_chart(kit, title, labels, &series[0].1, unit, at, width),
        ChartKind::Bar => {
            let top = kit.title(title, at, width, None);
            top + track_chart(kit, labels, series, unit, Pos2::new(at.x, at.y + top), width)
        }
        ChartKind::Line | ChartKind::Area => {
            area_chart(kit, key, title, labels, series, unit, at, width)
        }
    }
}

/// The figure beside a ring or a dial, or under it when the card is too narrow for both: the ring is
/// at most 150 points and keeps 130 for the legend.
fn ring_layout(kit: &Kit<'_, '_>, width: f32) -> (f32, bool) {
    let diameter = kit.z(150.0).min(width);
    let beside = width - diameter - kit.z(22.0) >= kit.z(130.0);
    (diameter, beside)
}

/// A donut: the ring of the values with the share of the first in the middle, and a legend.
#[allow(clippy::too_many_arguments)]
fn ring_chart(
    kit: &mut Kit<'_, '_>,
    title: &str,
    labels: &[String],
    values: &[f64],
    unit: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let chat = theme.chat;
    let total: f64 = values.iter().filter(|value| **value > 0.0).sum();
    let paint = |index: usize| match index {
        0 => SlicePaint::Action,
        1 => SlicePaint::Colour(chat.slices[0]),
        2 => SlicePaint::Colour(chat.slices[1]),
        _ => SlicePaint::Colour(chat.quiet),
    };
    let rows: Vec<LegendRow> = values
        .iter()
        .enumerate()
        .map(|(index, value)| LegendRow {
            name: labels.get(index).cloned().unwrap_or_else(|| format!("{}", index + 1)),
            value: figure_of(*value, unit),
            paint: paint(index),
        })
        .collect();
    let share = match total > 0.0 {
        true => format!("{:.0}%", values.first().copied().unwrap_or(0.0).max(0.0) / total * 100.0),
        false => "0%".to_owned(),
    };
    let first = labels.first().cloned().unwrap_or_default();
    let slices: Vec<Slice> = values
        .iter()
        .enumerate()
        .map(|(index, value)| Slice::new(value.max(0.0) as f32, paint(index)))
        .collect();
    let (diameter, beside) = ring_layout(kit, width);
    let legend = Legend::new(&rows);
    let legend_height = legend.height(kit.rux);
    let heading = title.to_owned();
    let heading_height = match heading.is_empty() {
        true => 0.0,
        false => kit.height_of(Style::sans(12.0)) + kit.z(14.0),
    };
    let legend_block = heading_height + legend_height;
    let ring_rect = Rect::from_min_size(at, Vec2::splat(diameter));
    let (legend_at, legend_width, height) = match beside {
        true => {
            let left = at.x + diameter + kit.z(22.0);
            let top = at.y + ((diameter - legend_block) / 2.0).max(0.0);
            (Pos2::new(left, top), at.x + width - left, diameter.max(legend_block))
        }
        false => {
            let top = at.y + diameter + kit.z(18.0);
            (Pos2::new(at.x, top), width, diameter + kit.z(18.0) + legend_block)
        }
    };
    if kit.draw {
        let ring_rect = match beside {
            true => ring_rect,
            false => Rect::from_center_size(
                Pos2::new(at.x + width / 2.0, ring_rect.center().y),
                ring_rect.size(),
            ),
        };
        let label = format!(
            "{title}: {}",
            rows.iter()
                .map(|row| format!("{} {}", row.name, row.value))
                .collect::<Vec<_>>()
                .join(", ")
        );
        Ring::new(slices)
            .centre(&share, "")
            .caption(&first, theme.ink.i400)
            .label(&label)
            .show(kit.rux, ring_rect);
        if !heading.is_empty() {
            let middle = legend_at.y + kit.height_of(Style::sans(12.0)) / 2.0;
            kit.line(
                Style::sans(12.0),
                &heading,
                theme.ink.i500,
                legend_at.x,
                middle,
                legend_width,
            );
        }
        let rect = Rect::from_min_size(
            Pos2::new(legend_at.x, legend_at.y + heading_height),
            Vec2::new(legend_width, legend_height),
        );
        legend.show(kit.rux, rect);
    }
    height
}

/// A dial: what something was and what was added, as a ring of two with the total and its growth in
/// the middle, and beside it each of the two with a short bar of its colour.
#[allow(clippy::too_many_arguments)]
fn dial_chart(
    kit: &mut Kit<'_, '_>,
    title: &str,
    labels: &[String],
    values: &[f64],
    unit: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let chat = theme.chat;
    let before = values.first().copied().unwrap_or(0.0).max(0.0);
    let added: f64 = values.iter().skip(1).sum::<f64>().max(0.0);
    let total = before + added;
    let (value, unit_words) = {
        let written = figure_of(total, unit);
        let (number, after) = split_unit(&written);
        (number.to_owned(), after.to_owned())
    };
    let growth = change_of(before, total).unwrap_or_default();
    let (diameter, beside) = ring_layout(kit, width);
    let name_height = kit.height_of(SMALL);
    let figure_style = Style::sans(17.0).medium();
    let figure_height = kit.height_of(figure_style);
    let entry = name_height + kit.z(2.0) + figure_height;
    let entries = values.len().min(2) as f32;
    let heading_height = match title.is_empty() {
        true => 0.0,
        false => kit.height_of(Style::sans(12.0)) + kit.z(14.0),
    };
    let block = heading_height + entry * entries + kit.z(14.0) * (entries - 1.0).max(0.0);
    let (left, top, room, height) = match beside {
        true => {
            let left = at.x + diameter + kit.z(22.0);
            (
                left,
                at.y + ((diameter - block) / 2.0).max(0.0),
                at.x + width - left,
                diameter.max(block),
            )
        }
        false => (at.x, at.y + diameter + kit.z(18.0), width, diameter + kit.z(18.0) + block),
    };
    if kit.draw {
        let ring = match beside {
            true => Rect::from_min_size(at, Vec2::splat(diameter)),
            false => Rect::from_center_size(
                Pos2::new(at.x + width / 2.0, at.y + diameter / 2.0),
                Vec2::splat(diameter),
            ),
        };
        let label = format!("{title}: {value}{unit_words}, {growth}");
        Dial::new(before as f32, added as f32)
            .centre(&value, &unit_words)
            .caption(&growth, change_colour(theme, before, total))
            .label(&label)
            .show(kit.rux, ring);
        let mut pen = top;
        if !title.is_empty() {
            kit.line(
                Style::sans(12.0),
                title,
                theme.ink.i500,
                left,
                pen + kit.height_of(Style::sans(12.0)) / 2.0,
                room,
            );
            pen += heading_height;
        }
        let amounts = [before, added];
        for (index, amount) in amounts.iter().enumerate().take(values.len().min(2)) {
            let name = labels.get(index).cloned().unwrap_or_default();
            let middle = pen + name_height / 2.0;
            // `width: 14px; height: 4px; border-radius: 2px` in the slice's colour.
            let swatch = Rect::from_center_size(
                Pos2::new(left + kit.z(7.0), middle),
                Vec2::new(kit.z(14.0), kit.z(4.0)),
            );
            match index {
                0 => kit.rux.chrome.rect(swatch, kit.z(2.0), chat.quiet),
                _ => kit.rux.chrome.rect(
                    swatch,
                    kit.z(2.0),
                    rux::Fill::gradient(chat.action, swatch),
                ),
            }
            kit.line(SMALL, &name, theme.ink.i400, left + kit.z(22.0), middle, room - kit.z(22.0));
            let figure = figure_of(*amount, unit);
            let ink = if index == 0 { theme.ink.i700 } else { theme.ink.i900 };
            let figure_top = pen + name_height + kit.z(2.0);
            kit.line(
                figure_style,
                &figure,
                ink,
                left + kit.z(22.0),
                figure_top + figure_height / 2.0,
                room - kit.z(22.0),
            );
            pen += entry + kit.z(14.0);
        }
    }
    height
}

/// Bars, drawn as the design's tracks: a row for each label with its figure over a slider track filled
/// to this series' value, and when there is an earlier series a quiet mark where it was and how much
/// it changed.
fn track_chart(
    kit: &mut Kit<'_, '_>,
    labels: &[String],
    series: &[(String, Vec<f64>)],
    unit: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let now = &series[series.len() - 1];
    let before = (series.len() >= 2).then(|| &series[series.len() - 2]);
    let largest =
        series.iter().flat_map(|(_, values)| values.iter().copied()).fold(0.0_f64, f64::max);
    let scale = (largest * 1.07).max(f64::EPSILON);
    let line = kit.height_of(FIGURE);
    let track = kit.z(22.0);
    let mut pen = at.y;
    for (index, value) in now.1.iter().enumerate() {
        if index > 0 {
            pen += kit.z(18.0);
        }
        let label = labels.get(index).cloned().unwrap_or_default();
        let earlier = before.and_then(|(_, values)| values.get(index).copied());
        let middle = pen + line / 2.0;
        let mut right = at.x + width;
        if let Some(earlier) = earlier {
            let change = change_of(earlier, *value).unwrap_or_default();
            let room = kit.z(40.0);
            let wide = kit.width_of(SMALL, &change);
            kit.line(
                SMALL,
                &change,
                change_colour(theme, earlier, *value),
                right - wide,
                middle,
                room,
            );
            right -= room + kit.z(8.0);
        }
        let figure = figure_of(*value, unit);
        let wide = kit.width_of(FIGURE, &figure);
        kit.line(FIGURE, &figure, theme.ink.i900, right - wide, middle, wide + 1.0);
        right -= wide + kit.z(8.0);
        if let Some(earlier) = earlier {
            let was = format!("{} \u{2192}", figure_of(earlier, unit));
            let wide = kit.width_of(SMALL, &was);
            kit.line(SMALL, &was, theme.ink.i400, right - wide, middle, wide + 1.0);
            right -= wide + kit.z(8.0);
        }
        kit.line(Style::sans(12.5), &label, theme.ink.i900, at.x, middle, right - at.x);
        if kit.draw {
            let rect = Rect::from_min_size(
                Pos2::new(at.x, pen + line + kit.z(10.0)),
                Vec2::new(width, track),
            );
            let mut bar =
                Track::new(*value as f32, scale as f32).label(format!("{label}: {figure}"));
            if let Some(earlier) = earlier {
                bar = bar.marker(earlier as f32);
            }
            bar.show(kit.rux, rect);
        }
        pen += line + kit.z(10.0) + track;
    }
    // `Last week` beside the mark that stands for it.
    if let Some((name, _)) = before {
        pen += kit.z(18.0);
        let caption = kit.height_of(SMALL);
        if kit.draw {
            let mark = Rect::from_min_size(
                Pos2::new(at.x, pen + caption / 2.0 - kit.z(6.0)),
                Vec2::new(kit.z(2.0), kit.z(12.0)),
            );
            kit.rux.painter().rect_filled(mark, kit.z(1.0), theme.ink.i400.gamma_multiply(0.6));
        }
        kit.line(
            SMALL,
            name,
            theme.ink.i400,
            at.x + kit.z(10.0),
            pen + caption / 2.0,
            width - kit.z(10.0),
        );
        pen += caption;
    }
    pen - at.y
}

/// A line or an area: the title with the last value and its change since the first at the right, and
/// the smooth area chart in a well under it, the earlier series as a quiet line.
#[allow(clippy::too_many_arguments)]
fn area_chart(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    labels: &[String],
    series: &[(String, Vec<f64>)],
    unit: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let now: Vec<f32> = series[series.len() - 1].1.iter().map(|value| *value as f32).collect();
    let earlier: Vec<f32> = match series.len() >= 2 {
        true => series[series.len() - 2].1.iter().map(|value| *value as f32).collect(),
        false => Vec::new(),
    };
    let first = now.first().copied().unwrap_or(0.0) as f64;
    let last = now.last().copied().unwrap_or(0.0) as f64;
    let figure_style = Style::sans(20.0).medium().tracking(-0.02);
    let head = kit.height_of(figure_style);
    let written = figure_of(last, unit);
    let (number, unit_words) = split_unit(&written);
    let change = change_of(first, last).unwrap_or_default();
    let middle = at.y + head / 2.0;
    let mut right = at.x + width;
    let change_width = kit.width_of(SMALL, &change);
    kit.line(
        SMALL,
        &change,
        change_colour(theme, first, last),
        right - change_width,
        middle,
        change_width + 1.0,
    );
    right -= change_width + kit.z(10.0);
    let unit_width = kit.width_of(SMALL, unit_words);
    kit.line(SMALL, unit_words, theme.ink.i400, right - unit_width, middle, unit_width + 1.0);
    right -= unit_width;
    let number_width = kit.width_of(figure_style, number);
    kit.line(
        figure_style,
        number,
        theme.ink.i900,
        right - number_width,
        middle,
        number_width + 1.0,
    );
    right -= number_width + kit.z(10.0);
    kit.line(Style::sans(12.0), title, theme.ink.i500, at.x, middle, right - at.x);
    let pen = at.y + head + kit.z(14.0);
    let first_label = labels.first().cloned().unwrap_or_default();
    let last_label = labels.last().cloned().unwrap_or_default();
    let mut chart = AreaChart::new(&now).compare(&earlier).label(title);
    if !labels.is_empty() {
        chart = chart.ends(&first_label, &last_label);
    }
    let height = chart.measure(kit.rux).y;
    if kit.draw {
        let _ = key;
        chart.show(kit.rux, Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, height)));
    }
    pen + height - at.y
}

/// A table in a well, ordered the way the person last pressed its headers.
fn table(
    kit: &mut Kit<'_, '_>,
    key: &str,
    columns: &[rich::component::Column],
    rows: &[Vec<String>],
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let sort = kit.states.get(key).and_then(|state| state.sort);
    let mut ordered: Vec<Vec<String>> =
        rows.iter().map(|row| row.iter().map(|cell| plain(cell)).collect()).collect();
    if let Some((by, descending)) = sort {
        ordered.sort_by(|a, b| {
            let (a, b) = (
                a.get(by).map(String::as_str).unwrap_or(""),
                b.get(by).map(String::as_str).unwrap_or(""),
            );
            let order = match (leading_number(a), leading_number(b)) {
                (Some(x), Some(y)) => x.total_cmp(&y),
                _ => a.to_lowercase().cmp(&b.to_lowercase()),
            };
            if descending {
                order.reverse()
            } else {
                order
            }
        });
    }
    // `padding: 8px 4px` round the table, `padding: 10px 14px 8px` for a header and `9px 14px` a row.
    let head = kit.z(10.0 + 8.0) + kit.height_of(SMALL);
    let row = kit.z(18.0) + kit.height_of(Style::sans(12.0));
    let height = kit.z(16.0) + head + row * ordered.len() as f32;
    let rect = Rect::from_min_size(at, Vec2::new(width, height));
    kit.well(rect, CARD_RADIUS);
    if !kit.draw || columns.is_empty() {
        return height;
    }
    // Each column as wide as what is in it, the first given what is left.
    let inner = Rect::from_min_max(
        rect.min + Vec2::new(kit.z(4.0), kit.z(8.0)),
        rect.max - Vec2::new(kit.z(4.0), kit.z(8.0)),
    );
    let pad = kit.z(14.0);
    let natural: Vec<f32> = (0..columns.len())
        .map(|index| {
            let head = kit.width_of(SMALL, &columns[index].label);
            ordered
                .iter()
                .map(|row| {
                    kit.width_of(
                        Style::sans(12.0),
                        row.get(index).map(String::as_str).unwrap_or(""),
                    )
                })
                .fold(head, f32::max)
                + pad * 2.0
        })
        .collect();
    let widths = column_widths(&natural, inner.width(), kit.z(56.0));
    let mut lefts = Vec::with_capacity(widths.len());
    let mut x = inner.left();
    for width in &widths {
        lefts.push(x);
        x += width;
    }
    let place = |kit: &Kit<'_, '_>, index: usize, style: Style, text: &str| -> (f32, f32) {
        let room = (widths[index] - pad * 2.0).max(0.0);
        let wide = kit.width_of(style, text).min(room);
        let align = match index {
            0 => Align::Left,
            _ => columns[index].align,
        };
        let left = match align {
            Align::Left => lefts[index] + pad,
            Align::Right => lefts[index] + widths[index] - pad - wide,
            Align::Centre => lefts[index] + (widths[index] - wide) / 2.0,
        };
        (left, room)
    };
    // The headers, each pressable to order the table by its column.
    let head_middle = inner.top() + kit.z(10.0) + kit.height_of(SMALL) / 2.0;
    for (index, column) in columns.iter().enumerate() {
        let hit = Rect::from_min_size(
            Pos2::new(lefts[index], inner.top()),
            Vec2::new(widths[index], head),
        );
        let response = kit.rux.ui.interact(hit, kit.id(key, ("head", index)), Sense::click());
        let sorted = sort.filter(|(by, _)| *by == index);
        let label = match sorted {
            Some((_, true)) => format!("{} \u{2193}", column.label),
            Some((_, false)) => format!("{} \u{2191}", column.label),
            None => column.label.clone(),
        };
        let ink = match (response.hovered(), sorted.is_some()) {
            (true, _) => theme.chat.sky,
            (false, true) => theme.ink.i700,
            (false, false) => theme.ink.i400,
        };
        if response.hovered() {
            kit.rux.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let (left, room) = place(kit, index, SMALL, &label);
        kit.line(SMALL, &label, ink, left, head_middle, room);
        let name = format!("Order by {}", column.label);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name.clone())
        });
        if response.clicked() {
            let state = kit.state(key);
            state.sort = match state.sort {
                Some((by, true)) if by == index => Some((index, false)),
                Some((by, false)) if by == index => None,
                _ => Some((index, true)),
            };
        }
    }
    for (number, cells) in ordered.iter().enumerate() {
        let top = inner.top() + head + row * number as f32;
        hairline(kit, inner.left(), inner.right(), top);
        let middle = top + row / 2.0;
        for (index, cell) in cells.iter().enumerate().take(columns.len()) {
            let ink = match (index, delta_sign(cell)) {
                (0, _) => theme.ink.i900,
                (_, Some(true)) => theme.chat.slower,
                (_, Some(false)) => theme.chat.faster,
                _ if index == columns.len() - 1 || columns.len() == 2 => theme.ink.i900,
                _ => theme.ink.i500,
            };
            let (left, room) = place(kit, index, Style::sans(12.0), cell);
            kit.line(Style::sans(12.0), cell, ink, left, middle, room);
        }
    }
    height
}

/// How wide each column of a table is drawn, from how wide what is in it wants to be.
///
/// **Only the widest columns give up room.** A table whose words do not fit takes the room from its
/// longest column first, down to the next longest, and so on, so a column of short figures keeps its
/// figures whole while a column of sentences is cut. Squeezing every column by the same share left a
/// real answer's narrow columns as rows of ellipses. With room to spare, the first column takes it.
fn column_widths(natural: &[f32], room: f32, smallest: f32) -> Vec<f32> {
    let mut widths = natural.to_vec();
    let total: f32 = widths.iter().sum();
    if total <= room {
        if let Some(first) = widths.first_mut() {
            *first += room - total;
        }
        return widths;
    }
    let mut over = total - room;
    while over > 0.5 {
        let widest = widths.iter().copied().fold(0.0_f32, f32::max);
        if widest <= smallest {
            break;
        }
        let next = widths.iter().copied().filter(|width| *width < widest).fold(smallest, f32::max);
        let at_widest = widths.iter().filter(|width| **width >= widest).count() as f32;
        let take = ((widest - next) * at_widest).min(over);
        for width in widths.iter_mut().filter(|width| **width >= widest) {
            *width -= take / at_widest;
        }
        over -= take;
    }
    // Still too wide when every column is at its smallest: share what is left.
    let total: f32 = widths.iter().sum();
    if total > room {
        widths.iter_mut().for_each(|width| *width *= room / total);
    }
    widths
}

/// Whether a cell is a change written with its sign and a percentage, and which way: `+62%` is up,
/// `−7%` is down. Anything else is not a change.
fn delta_sign(cell: &str) -> Option<bool> {
    let cell = cell.trim();
    if !cell.ends_with('%') {
        return None;
    }
    let rest = cell.trim_start_matches(['+', '-', '\u{2212}']);
    if rest.len() == cell.len() || !rest.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(cell.starts_with('+'))
}

/// The number a cell starts with, for ordering a column of figures as figures.
fn leading_number(cell: &str) -> Option<f64> {
    let trimmed = cell.trim().trim_start_matches(['$', '£', '€', '+']).replace('\u{2212}', "-");
    let end = trimmed
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == ','))
        .unwrap_or(trimmed.len());
    trimmed[..end].replace(',', "").parse().ok()
}

/// Places in the project as file chips that each open their file at their line, under a header with a
/// file mark. A file with a note has a row of its own, the note beside its chip.
fn files(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    items: &[rich::component::FileRef],
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let heading = match title.is_empty() {
        true => match items.len() {
            1 => "1 file".to_owned(),
            count => format!("{count} files"),
        },
        false => title.to_owned(),
    };
    let detail = match title.is_empty() {
        true => String::new(),
        false => match items.len() {
            1 => "1 file".to_owned(),
            count => format!("{count} files"),
        },
    };
    let mut pen =
        at.y + kit.header(Lead::Mark(Icon::File), &heading, &detail, at, width) + kit.z(GAP);
    let chip_height = kit.z(30.0);
    let gap = kit.z(8.0);
    let noted = items.iter().any(|item| !item.note.is_empty());
    let mut x = at.x + kit.z(2.0);
    for (index, item) in items.iter().enumerate() {
        let name = match item.path.rsplit_once(['/', '\\']) {
            Some((_, name)) => name.to_owned(),
            None => item.path.clone(),
        };
        let shown = match item.line {
            Some(line) => format!("{name}:{line}"),
            None => name,
        };
        let chip = FileChip::new(&shown).label(format!("Open {}", item.path));
        let size = chip.measure(kit.rux);
        let size = Vec2::new(size.x.min(width - kit.z(4.0)), size.y);
        if !noted && x > at.x + kit.z(2.0) && x + size.x > at.x + width {
            x = at.x + kit.z(2.0);
            pen += chip_height + gap;
        }
        if kit.draw {
            let rect = Rect::from_min_size(Pos2::new(x, pen), size);
            if in_scope(kit, key, ("file", index), |rux| chip.show(rux, rect).clicked()) {
                kit.acts.push(Act::OpenFile(item.path.clone(), item.line));
            }
        }
        if noted {
            if !item.note.is_empty() {
                let left = x + size.x + kit.z(12.0);
                kit.line(
                    SMALL,
                    &item.note,
                    theme.ink.i400,
                    left,
                    pen + chip_height / 2.0,
                    at.x + width - left,
                );
            }
            pen += chip_height + gap;
        } else {
            x += size.x + gap;
        }
    }
    match noted {
        true => pen - gap - at.y,
        false => pen + chip_height - at.y,
    }
}

/// A unified diff as the design's diff card: the file, how many lines changed, the lines numbered
/// with their signs in a well, and `Copy` and `Open in editor`.
fn diff(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    path: &str,
    text: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    let lines = diff_lines(text);
    let changed = lines.iter().filter(|line| line.sign != Sign::Same).count();
    let heading = match (path.is_empty(), title.is_empty()) {
        (false, _) => path.rsplit_once(['/', '\\']).map_or(path, |(_, name)| name).to_owned(),
        (true, false) => title.to_owned(),
        (true, true) => "Change".to_owned(),
    };
    let detail = match changed {
        1 => "1 line changed".to_owned(),
        count => format!("{count} lines changed"),
    };
    let open = (!path.is_empty()).then_some("Open in editor");
    let card = DiffCard::new(&heading, &detail, &lines).buttons(Some("Copy"), open, None);
    let height = card.height(kit.rux);
    if kit.draw {
        let rect = Rect::from_min_size(at, Vec2::new(width, height));
        let (_, outcome) = in_scope(kit, key, "diff", |rux| card.show(rux, rect));
        if outcome.ghost {
            kit.acts.push(Act::Copy(text.to_owned()));
        }
        if outcome.secondary {
            kit.acts.push(Act::OpenFile(path.to_owned(), first_line_of(text)));
        }
    }
    height
}

/// A unified diff's lines as a card's lines: numbered from each hunk's header in the new file, or the
/// old file for a line taken out, and signed. File headers are left out.
pub fn diff_lines(text: &str) -> Vec<Line> {
    let mut old = 0_u32;
    let mut new = 0_u32;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with("+++")
            || line.starts_with("---")
            || line.starts_with("diff ")
            || line.starts_with("index ")
        {
            continue;
        }
        if line.starts_with("@@") {
            let mut parts = line.split_whitespace().skip(1);
            let start = |part: Option<&str>| {
                part.and_then(|p| p[1..].split(',').next()?.parse::<u32>().ok()).unwrap_or(1)
            };
            old = start(parts.next());
            new = start(parts.next());
            continue;
        }
        let (sign, number, body) = match line.chars().next() {
            Some('+') => {
                new += 1;
                (Sign::Added, new - 1, &line[1..])
            }
            Some('-') => {
                old += 1;
                (Sign::Removed, old - 1, &line[1..])
            }
            Some(' ') => {
                old += 1;
                new += 1;
                (Sign::Same, new - 1, &line[1..])
            }
            _ => {
                old += 1;
                new += 1;
                (Sign::Same, new - 1, line)
            }
        };
        let mut drawn = Line::plain(body).sign(sign);
        if number > 0 {
            drawn = drawn.number(number.to_string());
        }
        out.push(drawn);
    }
    out
}

/// The line a diff's first hunk starts on in the new file, which is where opening it should go.
fn first_line_of(diff: &str) -> Option<u32> {
    let hunk = diff.lines().find(|line| line.starts_with("@@"))?;
    let plus = hunk.split_whitespace().find(|part| part.starts_with('+'))?;
    plus.trim_start_matches('+').split(',').next()?.parse().ok()
}

/// A Mermaid diagram in a well, laid out by the editor's own Mermaid reader and fitted to the width.
fn diagram(kit: &mut Kit<'_, '_>, source: &str, at: Pos2, width: f32) -> f32 {
    let look = kit.look;
    let base = unluminous_core::CharStyle {
        family: look.font_family.as_str().into(),
        size: look.font_size * 0.8,
        ..unluminous_core::CharStyle::default()
    };
    let theme = crate::services::mermaid_scene::theme();
    let ctx = kit.rux.ctx().clone();
    let metrics =
        crate::services::mermaid_scene::EguiMetrics::new(&ctx, egui::FontFamily::Proportional);
    let laid = kit.scenes.scene(source, &base, &metrics, &theme);
    let pad = kit.z(14.0);
    match laid {
        Ok(scene) => {
            let natural = Vec2::new(scene.size.width, scene.size.height);
            let room = width - pad * 2.0;
            let scale = (room / natural.x.max(1.0)).min(1.0).min(kit.z(420.0) / natural.y.max(1.0));
            let height = natural.y * scale + pad * 2.0;
            let rect = Rect::from_min_size(at, Vec2::new(width, height));
            kit.well(rect, WELL_RADIUS);
            if kit.draw {
                let origin = Pos2::new(rect.center().x - natural.x * scale / 2.0, rect.top() + pad);
                crate::components::diagram_view::paint(kit.rux.ui, &scene, origin, scale);
            }
            height
        }
        Err(problem) => {
            let theme = kit.rux.theme();
            kit.words(MONO.at(11.0), &problem.message(), theme.ink.i500, at, width)
        }
    }
}

/// A wrapped row of pill buttons. A primary one is the action's gradient; one that was pressed stays
/// down, carved into the pane, so the conversation shows which suggestion was taken.
fn pills(
    kit: &mut Kit<'_, '_>,
    key: &str,
    buttons: &[(String, Option<Action>, bool)],
    at: Pos2,
    width: f32,
) -> f32 {
    let pressed = kit.states.get(key).and_then(|state| state.pressed.clone());
    let gap = kit.z(8.0);
    let mut x = at.x;
    let mut y = at.y;
    let mut row: f32 = 0.0;
    for (index, (label, action, primary)) in buttons.iter().enumerate() {
        let was = pressed.as_deref() == Some(label.as_str());
        let mut button = PillButton::new(label);
        if *primary {
            button = button.primary();
        }
        if was {
            button = button.showing(false, true, false);
        }
        let mut size = button.measure(kit.rux);
        size.x = size.x.min(width);
        if x > at.x && x + size.x > at.x + width {
            x = at.x;
            y += row + gap;
            row = 0.0;
        }
        if kit.draw {
            let rect = Rect::from_min_size(Pos2::new(x, y), size);
            if in_scope(kit, key, ("pill", index), |rux| button.show(rux, rect).clicked()) {
                kit.state(key).pressed = Some(label.clone());
                if let Some(action) = action {
                    kit.acts.push(act_of(action));
                }
            }
        }
        x += size.x + gap;
        row = row.max(size.y);
    }
    y + row - at.y
}

/// What a press of a button with `action` asks the pane to do.
pub fn act_of(action: &Action) -> Act {
    match action {
        Action::Send(text) => Act::SendWords(text.clone()),
        Action::Fill(text) => Act::Fill(text.clone()),
        Action::Copy(text) => Act::Copy(text.clone()),
        Action::Open { path, line } => Act::OpenFile(path.clone(), *line),
    }
}

/// A checklist: each item a round well that holds a tick once it is done, and a track under them
/// filled as far as what is done.
fn checklist(
    kit: &mut Kit<'_, '_>,
    key: &str,
    items: &[rich::component::Check],
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let ticks = kit.states.get(key).map(|state| state.ticks.clone()).unwrap_or_default();
    let side = kit.z(28.0);
    let row = kit.z(34.0);
    let mut pen = at.y;
    let mut done = 0;
    for (index, item) in items.iter().enumerate() {
        let on = ticks.get(&index).copied().unwrap_or(item.done);
        done += usize::from(on);
        let label = plain(&item.label);
        if kit.draw {
            let rect = Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, row));
            let response = kit.rux.ui.interact(rect, kit.id(key, ("tick", index)), Sense::click());
            if response.hovered() {
                kit.rux.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let well = Rect::from_min_size(
                Pos2::new(at.x + kit.z(3.0), rect.center().y - side / 2.0),
                Vec2::splat(side),
            );
            match on {
                true => {
                    StatusWell::new(Status::Done).diameter(28.0).show(kit.rux, well);
                }
                false => chat_well(kit.rux, well, side / 2.0),
            }
            let ink = match (on, response.hovered()) {
                (_, true) => theme.chat.sky,
                (true, false) => theme.ink.i500,
                (false, false) => theme.ink.i900,
            };
            let left = well.right() + kit.z(12.0);
            kit.line(Style::sans(12.5), &label, ink, left, rect.center().y, at.x + width - left);
            let name = label.clone();
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, name.clone())
            });
            if response.clicked() {
                kit.state(key).ticks.insert(index, !on);
            }
        }
        pen += row + kit.z(4.0);
    }
    // The count, and a track filled as far as what is done.
    let words = format!("{done} of {}", items.len());
    let label = kit.width_of(SMALL, &words);
    let track = kit.z(22.0);
    pen += kit.z(6.0);
    if !items.is_empty() {
        kit.line(SMALL, &words, theme.ink.i400, at.x, pen + track / 2.0, label + 1.0);
        if kit.draw {
            let left = at.x + label + kit.z(14.0);
            let rect =
                Rect::from_min_max(Pos2::new(left, pen), Pos2::new(at.x + width, pen + track));
            Track::new(done as f32, items.len() as f32)
                .without_knob()
                .label(words.clone())
                .show(kit.rux, rect);
        }
    }
    pen + track - at.y
}

/// A slider the person drags: the design's track with its knob, and the value the pointer points at.
#[allow(clippy::too_many_arguments)]
fn slider(
    kit: &mut Kit<'_, '_>,
    id: Id,
    label: &str,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    rect: Rect,
) -> Option<f64> {
    let span = (max - min).max(f64::EPSILON);
    Track::new((value - min) as f32, span as f32).label(label.to_owned()).show(kit.rux, rect);
    let response = kit.rux.ui.interact(rect, id, Sense::click_and_drag());
    if response.hovered() || response.dragged() {
        kit.rux.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let name = format!("{label}: {}", format::plain(value));
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, name.clone()));
    if !(response.dragged() || response.clicked()) {
        return None;
    }
    let pointer = response.interact_pointer_pos()?;
    let fraction = ((pointer.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0) as f64;
    let mut next = min + span * fraction;
    if step > 0.0 {
        next = min + ((next - min) / step).round() * step;
    }
    let next = next.clamp(min, max);
    (next != value).then_some(next)
}

/// A form: each field a labelled control, and a pill that sends what was filled in as a message.
#[allow(clippy::too_many_arguments)]
fn form(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    fields: &[rich::component::Field],
    submit: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let label_height = kit.height_of(SMALL);
    let mut pen = at.y;
    for field in fields {
        let current = kit
            .states
            .get(key)
            .and_then(|state| state.values.get(&field.name).cloned())
            .unwrap_or_else(|| field.value.clone());
        kit.line(
            SMALL,
            &field.label,
            theme.ink.i500,
            at.x + kit.z(4.0),
            pen + label_height / 2.0,
            width,
        );
        pen += label_height + kit.z(8.0);
        match field.kind {
            FieldKind::Toggle => {
                let on = matches!(current.as_str(), "true" | "yes" | "on" | "1");
                let height = kit.z(46.0);
                if kit.draw {
                    let choices = vec!["Off".to_owned(), "On".to_owned()];
                    let rect = Rect::from_min_size(
                        Pos2::new(at.x, pen),
                        Vec2::new(width.min(kit.z(200.0)), height),
                    );
                    let picked = in_scope(kit, key, ("toggle", &field.name), |rux| {
                        PillSwitch::new(&choices, Some(usize::from(on)), "Choose").show(rux, rect)
                    });
                    if let Some(index) = picked {
                        kit.state(key).values.insert(field.name.clone(), (index == 1).to_string());
                    }
                }
                pen += height;
            }
            FieldKind::Slider => {
                let value: f64 = current.parse().unwrap_or(field.min);
                let height = kit.z(22.0);
                let readout = format::plain(value);
                let wide = kit.width_of(FIGURE, &readout);
                kit.line(
                    FIGURE,
                    &readout,
                    theme.ink.i900,
                    at.x + width - wide,
                    pen + height / 2.0,
                    wide + 1.0,
                );
                if kit.draw {
                    let rect = Rect::from_min_size(
                        Pos2::new(at.x, pen),
                        Vec2::new(width - wide - kit.z(14.0), height),
                    );
                    let id = kit.id(key, ("slider", &field.name));
                    if let Some(next) =
                        slider(kit, id, &field.label, value, field.min, field.max, field.step, rect)
                    {
                        kit.state(key).values.insert(field.name.clone(), format::plain(next));
                    }
                }
                pen += height;
            }
            FieldKind::Select => {
                let chosen = field.options.iter().position(|one| *one == current);
                // A few short choices are a pill switch, which shows them all; more are a menu.
                let fits = field.options.len() <= 4
                    && field.options.iter().all(|one| {
                        kit.width_of(Style::sans(12.0), one) + kit.z(24.0)
                            <= width / field.options.len() as f32
                    });
                let height = if fits { kit.z(46.0) } else { kit.z(32.0) };
                if kit.draw {
                    let rect = Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, height));
                    let picked = match fits {
                        true => in_scope(kit, key, ("switch", &field.name), |rux| {
                            PillSwitch::new(&field.options, chosen, "Choose").show(rux, rect)
                        }),
                        false => {
                            let zoom = kit.rux.zoom();
                            let mut open = kit
                                .state(key)
                                .selects
                                .get(&field.name)
                                .copied()
                                .unwrap_or_default();
                            let outcome = in_a_child(
                                kit.rux,
                                rect,
                                ("agent-chat-ui-select", key, &field.name),
                                |rux| {
                                    rux::components::Select::new(&field.options, chosen)
                                        .zoom(zoom)
                                        .label("Choose")
                                        .show(rux, rect, &mut open)
                                },
                            );
                            kit.state(key).selects.insert(field.name.clone(), open);
                            outcome.chosen
                        }
                    };
                    if let Some(value) = picked.and_then(|index| field.options.get(index)) {
                        kit.state(key).values.insert(field.name.clone(), value.clone());
                    }
                }
                pen += height;
            }
            _ => {
                let multiline = field.kind == FieldKind::Multiline;
                // As tall as `rux` measures its own field, so the words sit in the middle of it.
                let height = match multiline {
                    true => kit.z(72.0),
                    false => {
                        let mut probe = String::new();
                        let style = kit.style(Style::sans(12.5));
                        rux::components::TextInput::new(&mut probe)
                            .style(style)
                            .measure(kit.rux)
                            .y
                            .max(kit.z(32.0))
                    }
                };
                if kit.draw {
                    let rect = Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, height));
                    let style = kit.style(if field.kind == FieldKind::Number {
                        MONO.at(12.0)
                    } else {
                        Style::sans(12.5)
                    });
                    let mut text = current.clone();
                    let salt = ("agent-chat-ui-field", key.to_owned(), field.name.clone());
                    let changed = in_a_child(kit.rux, rect, salt.clone(), |rux| match multiline {
                        true => {
                            rux::components::TextArea::new(&mut text)
                                .id_salt(salt)
                                .show(rux, rect)
                                .changed
                        }
                        false => {
                            rux::components::TextInput::new(&mut text)
                                .id_salt(salt)
                                .style(style)
                                .show(rux, rect)
                                .changed
                        }
                    });
                    if changed {
                        kit.state(key).values.insert(field.name.clone(), text);
                    }
                }
                pen += height;
            }
        }
        pen += kit.z(16.0);
    }
    let sent = kit.states.get(key).and_then(|state| state.pressed.clone()).is_some();
    let label = if sent { "Sent" } else { submit };
    let button = match sent {
        true => PillButton::new(label).showing(false, true, false),
        false => PillButton::new(label).primary(),
    };
    let size = button.measure(kit.rux);
    if kit.draw {
        let rect = Rect::from_min_size(Pos2::new(at.x + width - size.x, pen), size);
        if in_scope(kit, key, "submit", |rux| button.show(rux, rect).clicked()) && !sent {
            let state = kit.states.get(key).cloned().unwrap_or_default();
            kit.state(key).pressed = Some(submit.to_owned());
            kit.acts.push(Act::SendWords(form_message(title, fields, &state)));
        }
    }
    pen + size.y - at.y
}

/// Draw a control that lays itself out with `egui` into a `Ui` of its own.
///
/// **A text box puts itself into its `Ui`'s layout**, and putting a rectangle into a layout moves that
/// layout's cursor to the rectangle's bottom. Drawn straight into the conversation's own `Ui`, the
/// form's last field moved the conversation's cursor back up to its bottom edge, so the next message
/// started inside the form and was drawn under it. A child `Ui` keeps that to itself.
///
/// `salt` names the child, and it is the field's own name rather than where it is: a field that moved
/// as the conversation scrolled would otherwise be a new field, and lose the keyboard.
fn in_a_child<R>(
    rux: &mut Rux<'_>,
    rect: Rect,
    salt: impl std::hash::Hash + std::fmt::Debug,
    add: impl FnOnce(&mut Rux<'_>) -> R,
) -> R {
    let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(salt));
    let mut inner = Rux { ui: &mut child, state: rux.state, chrome: rux.chrome };
    add(&mut inner)
}

/// What submitting a form sends: its title and each field's value, one a line.
pub fn form_message(title: &str, fields: &[rich::component::Field], state: &BlockState) -> String {
    let mut lines = vec![match title.is_empty() {
        true => "Here are the values:".to_owned(),
        false => format!("{title}:"),
    }];
    for field in fields {
        let value = state.values.get(&field.name).cloned().unwrap_or_else(|| field.value.clone());
        lines.push(format!(
            "- {}: {}",
            field.label,
            if value.is_empty() { "(left empty)" } else { &value }
        ));
    }
    lines.join("\n")
}

/// A calculator: a slider for each input, readouts for its outputs, and a chart worked out over a
/// range.
fn calculator(
    kit: &mut Kit<'_, '_>,
    key: &str,
    inputs: &[rich::component::Input],
    outputs: &[rich::component::Output],
    chart: Option<&rich::component::CalcChart>,
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let values: HashMap<String, f64> = inputs
        .iter()
        .map(|input| {
            let value = kit
                .states
                .get(key)
                .and_then(|state| state.inputs.get(&input.name).copied())
                .unwrap_or(input.value);
            (input.name.clone(), value)
        })
        .collect();
    let line = kit.height_of(FIGURE);
    let mut pen = at.y;
    for input in inputs {
        let value = values.get(&input.name).copied().unwrap_or(input.value);
        let reading =
            format::number(value, if input.unit == "$" { "money" } else { "number" }, &input.unit);
        let wide = kit.width_of(FIGURE, &reading);
        kit.line(
            FIGURE,
            &reading,
            theme.ink.i900,
            at.x + width - wide,
            pen + line / 2.0,
            wide + 1.0,
        );
        kit.line(
            Style::sans(12.5),
            &input.label,
            theme.ink.i700,
            at.x,
            pen + line / 2.0,
            width - wide - kit.z(12.0),
        );
        pen += line + kit.z(10.0);
        let height = kit.z(22.0);
        if kit.draw {
            let rect = Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, height));
            let id = kit.id(key, ("input", &input.name));
            if let Some(next) =
                slider(kit, id, &input.label, value, input.min, input.max, input.step, rect)
            {
                kit.state(key).inputs.insert(input.name.clone(), next);
            }
        }
        pen += height + kit.z(18.0);
    }
    let lookup = |name: &str| values.get(name).copied();
    if !outputs.is_empty() {
        let cells: Vec<Cell> = outputs
            .iter()
            .map(|output| {
                let worked = output.expr.as_ref().map(|expr| expr.eval(&lookup));
                Cell {
                    label: output.label.clone(),
                    value: match &worked {
                        Some(Ok(value)) => format::number(
                            *value,
                            if output.format.is_empty() { "number" } else { &output.format },
                            &output.unit,
                        ),
                        Some(Err(_)) | None => "\u{2014}".to_owned(),
                    },
                    delta: String::new(),
                    good: None,
                    note: match worked {
                        Some(Err(problem)) => problem,
                        _ => String::new(),
                    },
                    history: Vec::new(),
                }
            })
            .collect();
        pen += readouts(kit, &cells, Pos2::new(at.x, pen), width) + kit.z(18.0);
    }
    if let Some(chart) = chart {
        let (labels, series) = calculated(chart, &values);
        pen += chart_at(
            kit,
            &format!("{key}#chart"),
            "",
            chart.kind,
            &labels,
            &series,
            &chart.unit,
            Pos2::new(at.x, pen),
            width,
        ) + kit.z(18.0);
    }
    (pen - kit.z(18.0) - at.y).max(0.0)
}

/// Evaluate a calculator's chart at every value of its `x`. At most 60 points, so a range written as
/// `0` to `1000` in steps of one is sampled rather than drawn a thousand points wide.
pub fn calculated(
    chart: &rich::component::CalcChart,
    values: &HashMap<String, f64>,
) -> (Vec<String>, Vec<(String, Vec<f64>)>) {
    let lookup = |name: &str| values.get(name).copied();
    let from = chart.from.as_ref().and_then(|e| e.eval(&lookup).ok()).unwrap_or(0.0);
    let to = chart.to.as_ref().and_then(|e| e.eval(&lookup).ok()).unwrap_or(from);
    let mut step = chart.step.as_ref().and_then(|e| e.eval(&lookup).ok()).unwrap_or(1.0).abs();
    if step <= 0.0 || !step.is_finite() {
        step = 1.0;
    }
    let span = (to - from).abs();
    if span / step > 60.0 {
        step = span / 60.0;
    }
    // Each x worked out from its index rather than by adding the step again and again, so the last one
    // is the end of the range rather than a hair short of it.
    let mut xs = Vec::new();
    while xs.len() < 61 {
        let x = from + step * xs.len() as f64;
        if x > to + step * 1e-6 {
            break;
        }
        xs.push(if (x - to).abs() <= step * 1e-6 { to } else { x });
    }
    let labels = xs.iter().map(|x| format::plain((x * 100.0).round() / 100.0)).collect();
    let series = chart
        .series
        .iter()
        .map(|one| {
            let points = xs
                .iter()
                .map(|x| {
                    let here = |name: &str| {
                        if name == chart.x {
                            Some(*x)
                        } else {
                            values.get(name).copied()
                        }
                    };
                    one.expr.as_ref().and_then(|expr| expr.eval(&here).ok()).unwrap_or(0.0)
                })
                .collect();
            (one.name.clone(), points)
        })
        .collect();
    (labels, series)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_rolls_up_written_the_way_it_was_written() {
        assert_eq!(rolled("178 s", 0.5), "89 s");
        assert_eq!(rolled("$1,204.50", 0.5), "$602.25");
        assert_eq!(rolled("+42%", 0.0), "+0%");
        assert_eq!(rolled("ready", 0.3), "ready");
        assert_eq!(rolled("12", 1.0), "12");
    }

    #[test]
    fn a_value_is_split_from_a_short_unit_after_it() {
        assert_eq!(split_unit("178 s"), ("178", " s"));
        assert_eq!(split_unit("1.2 GB"), ("1.2", " GB"));
        assert_eq!(split_unit("$1,204"), ("$1,204", ""));
        assert_eq!(split_unit("two words"), ("two words", ""));
    }

    #[test]
    fn a_change_is_written_with_its_sign() {
        assert_eq!(change_of(128.0, 178.0).as_deref(), Some("+39%"));
        assert_eq!(change_of(41.0, 38.0).as_deref(), Some("\u{2212}7%"));
        assert_eq!(change_of(0.0, 3.0), None);
    }

    #[test]
    fn a_table_takes_room_from_its_widest_column_first() {
        let widths = column_widths(&[80.0, 40.0, 40.0, 400.0], 360.0, 56.0);
        assert_eq!(&widths[..3], &[80.0, 40.0, 40.0], "the narrow columns keep what they need");
        assert!((widths.iter().sum::<f32>() - 360.0).abs() < 0.5);
        let roomy = column_widths(&[80.0, 40.0], 300.0, 56.0);
        assert_eq!(roomy, vec![260.0, 40.0], "the first column takes the room to spare");
        let tight = column_widths(&[300.0, 300.0], 100.0, 56.0);
        assert!((tight.iter().sum::<f32>() - 100.0).abs() < 0.5);
    }

    #[test]
    fn a_cell_is_a_change_only_with_its_sign_and_a_percentage() {
        assert_eq!(delta_sign("+62%"), Some(true));
        assert_eq!(delta_sign("\u{2212}7%"), Some(false));
        assert_eq!(delta_sign("62%"), None);
        assert_eq!(delta_sign("131 s"), None);
    }

    #[test]
    fn a_diff_is_numbered_from_its_hunks() {
        let lines = diff_lines("--- a/x\n+++ b/x\n@@ -12,3 +12,3 @@\n [workspace]\n-kurbo = \"0.12\"\n+kurbo = \"0.11\"\n egui");
        let shown: Vec<(Option<String>, Sign, String)> =
            lines.iter().map(|line| (line.number.clone(), line.sign, line.text())).collect();
        assert_eq!(
            shown,
            vec![
                (Some("12".into()), Sign::Same, "[workspace]".into()),
                (Some("13".into()), Sign::Removed, "kurbo = \"0.12\"".into()),
                (Some("13".into()), Sign::Added, "kurbo = \"0.11\"".into()),
                (Some("14".into()), Sign::Same, "egui".into()),
            ]
        );
    }

    #[test]
    fn a_calculators_chart_is_sampled_and_bounded() {
        let value = serde_json::json!({"type": "calculator", "inputs": [{"name": "n", "value": 10}], "outputs": [{"label": "x", "expr": "n"}],
            "chart": {"kind": "line", "x": {"name": "i", "from": 0, "to": "n*100"}, "series": [{"name": "s", "expr": "i*2"}]}});
        let (component, problems) = Component::read(&value, true);
        assert!(problems.is_empty(), "{problems:?}");
        let Kind::Calculator { chart: Some(chart), .. } = component.kind else { panic!() };
        let values = HashMap::from([("n".to_owned(), 10.0)]);
        let (labels, series) = calculated(&chart, &values);
        assert!(labels.len() <= 61 && labels.len() > 50, "{}", labels.len());
        assert_eq!(series[0].1[0], 0.0);
        assert_eq!(*series[0].1.last().unwrap(), 2000.0);
    }

    #[test]
    fn a_form_sends_its_values_as_a_message() {
        let value = serde_json::json!({"type": "form", "title": "Run", "fields": [{"name": "name", "label": "Name", "value": "Dev"}, {"name": "cmd", "label": "Command"}]});
        let (component, _) = Component::read(&value, true);
        let Kind::Form { fields, .. } = component.kind else { panic!() };
        let mut state = BlockState::default();
        state.values.insert("cmd".into(), "npm run dev".into());
        assert_eq!(
            form_message("Run", &fields, &state),
            "Run:\n- Name: Dev\n- Command: npm run dev"
        );
    }

    #[test]
    fn a_diff_opens_at_its_first_hunk() {
        assert_eq!(first_line_of("@@ -84,3 +86,4 @@\n x"), Some(86));
        assert_eq!(first_line_of("+ just a line"), None);
    }
}
