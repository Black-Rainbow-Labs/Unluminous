//! The components an agent writes into an answer, drawn as instruments.
//!
//! `tasks/task-2211-agent-chat-intelligent-ui-tdd.md` is the design and §5.0 is the look: an answer's
//! components read as machined modules on a desk. A component is a raised **plate** with a lit top
//! edge; data sits on a recessed **screen**; a control is a **key cap** that sinks when pressed; colour
//! is **light**, an LED or glowing data, and never a filled accent or a stripe. Every one of those is a
//! `rux` control or primitive, and this file only composes them.
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

use egui::{Color32, Id, Pos2, Rect, Vec2};
use rux::components::{
    instrument::{self, timing, Instrument, READING, SILK},
    Chart, ChartKind as RuxChartKind, Checkbox, Column as RuxColumn, Fader, Item, Key, Meter,
    Series as RuxSeries, Stage, Table, Timeline,
};
use rux::{Rux, Style};
use unluminous_chat::rich::component::{
    Action, Align, ChartKind, Component, FieldKind, Kind, Progress as StepState, Tone, Trend,
};
use unluminous_chat::rich::{self, format, Problem};

use super::Act;
use crate::services::plugin_ui::Look;

/// How far a plate's contents sit from its edge.
const PAD: f32 = 14.0;
/// A plate's corners.
const RADIUS: f32 = 14.0;
/// Between the title and what is under it, and between the parts of a component.
const GAP: f32 = 10.0;
/// Between two components side by side, or one under another inside a card.
const BETWEEN: f32 = 9.0;
/// The title of a component.
const TITLE: Style = Style::sans(12.5).semibold().tracking(-0.005).leading(1.35);
/// Plain words a component sets itself rather than through markdown.
const WORDS: Style = Style::sans(12.5).leading(1.45);
/// A quiet line under a title.
const SUBTITLE: Style = Style::sans(11.5).leading(1.4);
/// A path or a figure.
const MONO: Style = Style::mono(11.5);

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
}

impl Kit<'_, '_> {
    fn z(&self, points: f32) -> f32 {
        self.rux.z(points)
    }

    fn style(&self, style: Style) -> Style {
        self.rux.zs(style)
    }

    fn id(&self, key: &str, part: impl std::hash::Hash + std::fmt::Debug) -> Id {
        Id::new(("agent-chat-ui", key)).with(part)
    }

    fn state(&mut self, key: &str) -> &mut BlockState {
        self.states.entry(key.to_owned()).or_default()
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

    /// Markdown, through the same renderer the bubbles use. Answers its height.
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

    /// A component's title, when it has one, and the gap under it.
    fn title(&mut self, title: &str, at: Pos2, width: f32, right: Option<&str>) -> f32 {
        if title.is_empty() {
            return 0.0;
        }
        let theme = self.rux.theme();
        let right_width = match right {
            Some(words) if !words.is_empty() => {
                self.rux.measure(self.style(SILK), words).x + self.z(10.0)
            }
            _ => 0.0,
        };
        let height = self.words(TITLE, title, theme.ink.i900, at, width - right_width);
        if let (true, Some(words)) = (self.draw, right) {
            if !words.is_empty() {
                let style = self.style(SILK);
                let galley = rux::text::layout(self.rux.painter(), style, words, theme.ink.i400);
                let first_line = self.rux.measure(self.style(TITLE), "Ag").y;
                let x = at.x + width - galley.size().x;
                rux::text::draw_left_capitals(
                    self.rux.painter(),
                    Pos2::new(x, at.y + first_line / 2.0),
                    galley,
                    style,
                    theme.ink.i400,
                );
            }
        }
        height + self.z(GAP)
    }

    /// Lay out `body` inside a plate: measured first, then the plate drawn, then the body drawn in it.
    fn plated(
        &mut self,
        at: Pos2,
        width: f32,
        padding: f32,
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
            instrument::plate(self.rux, rect, self.z(RADIUS), 0.0);
            body(self, inner, inner_width);
        }
        height
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

/// One component, on a plate of its own when `plated`. `stretch` is the least height it may have, so a
/// row of plates side by side can be one height.
fn component_at(
    kit: &mut Kit<'_, '_>,
    component: &Component,
    key: &str,
    at: Pos2,
    width: f32,
    plated: bool,
    stretch: f32,
) -> f32 {
    let title = component.title.as_str();
    match &component.kind {
        Kind::Pending => pending(kit, key, at, width),
        Kind::Card { subtitle, text, badge, children } => {
            kit.plated(at, width, PAD, stretch, &mut |kit, at, width| {
                card(kit, key, title, subtitle, text, badge, children, at, width)
            })
        }
        Kind::Columns { columns } => columns_at(kit, key, title, columns, at, width),
        Kind::Tabs { tabs } => tabs_at(kit, key, title, tabs, at, width),
        Kind::Stack { children } => {
            let theme = kit.rux.theme();
            let mut pen = at.y;
            if !title.is_empty() {
                pen += kit.words(
                    TITLE,
                    title,
                    theme.ink.i900,
                    Pos2::new(at.x + kit.z(4.0), pen),
                    width,
                ) + kit.z(8.0);
            }
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
        _ if plated => {
            let padding = match &component.kind {
                Kind::Table { .. } | Kind::Diff { .. } => 10.0,
                Kind::Files { .. } => 6.0,
                Kind::Actions { .. } => 0.0,
                Kind::Badges { .. } if title.is_empty() => 0.0,
                _ => PAD,
            };
            if padding == 0.0 {
                return leaf(kit, component, key, at, width);
            }
            kit.plated(at, width, padding, stretch, &mut |kit, at, width| {
                leaf(kit, component, key, at, width)
            })
        }
        _ => leaf(kit, component, key, at, width),
    }
}

/// A component that holds no others.
fn leaf(kit: &mut Kit<'_, '_>, component: &Component, key: &str, at: Pos2, width: f32) -> f32 {
    let title = component.title.as_str();
    match &component.kind {
        Kind::Callout { tone, text } => callout(kit, key, title, *tone, text, at, width),
        Kind::Steps { items, timeline } => {
            let top = kit.title(title, at, width, None);
            top + steps(kit, key, items, *timeline, Pos2::new(at.x, at.y + top), width)
        }
        Kind::KeyValue { items } => {
            let top = kit.title(title, at, width, None);
            top + keyvalue(kit, items, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Badges { items } => {
            let top = kit.title(title, at, width, None);
            let items: Vec<(String, Tone)> =
                items.iter().map(|b| (b.label.clone(), b.tone)).collect();
            top + badges(kit, &items, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Stats { items } => {
            let top = kit.title(title, at, width, None);
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
                })
                .collect();
            top + readouts(kit, key, &cells, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Progress { items } => {
            let top = kit.title(title, at, width, None);
            top + progress(kit, key, items, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Chart(chart) => {
            let top = kit.title(title, at, width, Some(&chart.unit));
            let series: Vec<(String, Vec<f64>)> =
                chart.series.iter().map(|s| (s.name.clone(), s.values.clone())).collect();
            top + chart_at(
                kit,
                key,
                chart.kind,
                &chart.labels,
                &series,
                &chart.unit,
                chart.stacked,
                Pos2::new(at.x, at.y + top),
                width,
            )
        }
        Kind::Table { columns, rows } => {
            let top = kit.title(title, Pos2::new(at.x + kit.z(2.0), at.y), width, None);
            top + table(kit, key, columns, rows, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Files { items } => files(kit, key, title, items, at, width),
        Kind::Diff { path, text } => diff(kit, key, title, path, text, at, width),
        Kind::Diagram { source } => {
            let top = kit.title(title, at, width, None);
            top + diagram(kit, source, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Actions { items } => {
            let top = kit.title(title, Pos2::new(at.x + kit.z(4.0), at.y), width, None);
            let buttons: Vec<(String, Option<Action>, bool)> =
                items.iter().map(|b| (b.label.clone(), b.action.clone(), b.primary)).collect();
            top + keys(kit, key, &buttons, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Choices { question, options } => {
            let heading = if question.is_empty() { title } else { question.as_str() };
            let top = kit.title(heading, at, width, None);
            let buttons: Vec<(String, Option<Action>, bool)> =
                options.iter().map(|o| (o.clone(), Some(Action::Send(o.clone())), false)).collect();
            top + keys(kit, key, &buttons, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Checklist { items } => {
            let top = kit.title(title, at, width, None);
            top + checklist(kit, key, items, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Form { fields, submit } => {
            let top = kit.title(title, at, width, None);
            top + form(kit, key, title, fields, submit, Pos2::new(at.x, at.y + top), width)
        }
        Kind::Calculator { inputs, outputs, chart } => {
            let top = kit.title(title, at, width, None);
            top + calculator(
                kit,
                key,
                inputs,
                outputs,
                chart.as_ref(),
                Pos2::new(at.x, at.y + top),
                width,
            )
        }
        // The containers are handled by `component_at`; drawn bare here they are their children.
        Kind::Card { .. }
        | Kind::Columns { .. }
        | Kind::Tabs { .. }
        | Kind::Stack { .. }
        | Kind::Pending => component_at(kit, component, key, at, width, false, 0.0),
    }
}

/// A component whose `type` has not arrived yet: one short plate with three lamps lighting in turn.
fn pending(kit: &mut Kit<'_, '_>, key: &str, at: Pos2, width: f32) -> f32 {
    let height = kit.z(42.0);
    if kit.draw {
        let rect = Rect::from_min_size(at, Vec2::new(width, height));
        instrument::plate(kit.rux, rect, kit.z(RADIUS), 0.0);
        let theme = kit.rux.theme();
        let off = Instrument::of(theme).led_off;
        let now = kit.rux.ctx().input(|input| input.time) as f32;
        for lamp in 0..3 {
            let phase = ((now * 2.2 - lamp as f32 * 0.33).rem_euclid(1.0) * std::f32::consts::TAU)
                .cos()
                * 0.5
                + 0.5;
            let centre = Pos2::new(
                rect.left() + kit.z(PAD + 3.0) + lamp as f32 * kit.z(12.0),
                rect.center().y,
            );
            instrument::led(kit.rux.painter(), centre, kit.z(3.0), theme.accent.blue, off, phase);
        }
        let style = kit.style(SILK);
        let galley = rux::text::layout(kit.rux.painter(), style, "Assembling", theme.ink.i400);
        rux::text::draw_left_capitals(
            kit.rux.painter(),
            Pos2::new(rect.left() + kit.z(PAD + 40.0), rect.center().y),
            galley,
            style,
            theme.ink.i400,
        );
        kit.rux.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        let _ = key;
    }
    height
}

/// A block that did not read: a plate with an amber lamp, what is wrong, and the source on request.
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
    kit.plated(at, width, PAD, 0.0, &mut |kit, at, width| {
        let theme = kit.rux.theme();
        let indent = kit.z(16.0);
        let first_line = kit.rux.measure(kit.style(TITLE), "Ag").y;
        if kit.draw {
            let off = Instrument::of(theme).led_off;
            instrument::led(
                kit.rux.painter(),
                Pos2::new(at.x + kit.z(3.0), at.y + first_line / 2.0),
                kit.z(3.0),
                theme.accent.amber,
                off,
                1.0,
            );
        }
        let mut pen = at.y;
        pen += kit.words(
            TITLE,
            "This component could not be drawn",
            theme.ink.i900,
            Pos2::new(at.x + indent, pen),
            width - indent,
        );
        for line in &lines {
            pen += kit.z(2.0)
                + kit.words(
                    MONO.at(11.0),
                    line,
                    theme.ink.i500,
                    Pos2::new(at.x + indent, pen),
                    width - indent,
                );
        }
        pen += kit.z(GAP);
        let label = if showing { "Hide the source" } else { "Show the source" };
        let size = Key::new(label).compact().measure(kit.rux);
        if kit.draw {
            let rect = Rect::from_min_size(Pos2::new(at.x + indent, pen), size);
            if Key::new(label).compact().id(kit.id(&key, "source")).show(kit.rux, rect).clicked() {
                kit.state(&key).source = !showing;
            }
        }
        pen += size.y;
        if showing {
            pen += kit.z(GAP);
            let style = kit.style(MONO.at(11.0));
            let galley = rux::text::wrapped(
                kit.rux.painter(),
                style,
                &source,
                theme.ink.i700,
                width - kit.z(20.0),
            );
            let rect = Rect::from_min_size(
                Pos2::new(at.x, pen),
                Vec2::new(width, galley.size().y + kit.z(16.0)),
            );
            if kit.draw {
                instrument::screen(kit.rux, rect, kit.z(9.0), false);
                kit.rux.painter().galley(
                    rect.min + Vec2::splat(kit.z(8.0)) + Vec2::new(kit.z(2.0), 0.0),
                    galley,
                    theme.ink.i700,
                );
            }
            pen += rect.height();
        }
        pen - at.y
    })
}

/// A card: its title and subtitle, a badge at the top right, its words, and its children laid bare.
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
            let first_line = kit.rux.measure(kit.style(TITLE), "Ag").y;
            pill(
                kit,
                badge,
                Tone::Success,
                Pos2::new(at.x + width - badge_width + kit.z(10.0), pen + first_line / 2.0),
            );
        }
        pen += height;
    }
    if !subtitle.is_empty() {
        pen +=
            kit.z(2.0) + kit.words(SUBTITLE, subtitle, theme.ink.i400, Pos2::new(at.x, pen), width);
    }
    if !text.is_empty() {
        if pen > at.y {
            pen += kit.z(8.0);
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

/// How wide a pill with an LED and a silkscreen label is.
fn pill_width(kit: &Kit<'_, '_>, label: &str) -> f32 {
    kit.rux.measure(kit.style(SILK), label).x + kit.z(26.0)
}

/// A small recessed pill with an LED and a silkscreen label, centred on `left_centre.y`.
fn pill(kit: &mut Kit<'_, '_>, label: &str, tone: Tone, left_centre: Pos2) {
    if !kit.draw {
        return;
    }
    let theme = kit.rux.theme();
    let colours = Instrument::of(theme);
    let rect = Rect::from_min_size(
        Pos2::new(left_centre.x, left_centre.y - kit.z(10.0)),
        Vec2::new(pill_width(kit, label), kit.z(20.0)),
    );
    instrument::screen(kit.rux, rect, kit.z(10.0), false);
    let (colour, lit) = tone_light(theme, tone);
    instrument::led(
        kit.rux.painter(),
        Pos2::new(rect.left() + kit.z(10.0), rect.center().y),
        kit.z(2.5),
        colour,
        colours.led_off,
        lit,
    );
    let style = kit.style(SILK);
    let galley = rux::text::layout(kit.rux.painter(), style, label, theme.ink.i700);
    rux::text::draw_left_capitals(
        kit.rux.painter(),
        Pos2::new(rect.left() + kit.z(18.0), rect.center().y),
        galley,
        style,
        theme.ink.i700,
    );
}

/// The light a tone is shown by, and how lit it is. Neutral is a dark lamp.
fn tone_light(theme: &rux::Theme, tone: Tone) -> (Color32, f32) {
    match tone {
        Tone::Neutral => (theme.ink.i300, 0.0),
        Tone::Info => (theme.accent.blue, 1.0),
        Tone::Success => (theme.accent.mint, 1.0),
        Tone::Warning => (theme.accent.amber, 1.0),
        Tone::Danger => (theme.accent.coral, 1.0),
        Tone::Tip => (theme.accent.violet, 1.0),
    }
}

/// Columns side by side, one plate each and all one height, or one under another when the pane is too
/// narrow for them.
fn columns_at(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    columns: &[Component],
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let mut pen = at.y;
    if !title.is_empty() {
        pen += kit.words(TITLE, title, theme.ink.i900, Pos2::new(at.x + kit.z(4.0), pen), width)
            + kit.z(8.0);
    }
    let count = columns.len().max(1);
    let gap = kit.z(12.0);
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

/// A row of keys, one a tab, with the chosen tab down and lit, over that tab's components.
fn tabs_at(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    tabs: &[rich::component::Tab],
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let mut pen = at.y;
    if !title.is_empty() {
        pen += kit.words(TITLE, title, theme.ink.i900, Pos2::new(at.x + kit.z(4.0), pen), width)
            + kit.z(8.0);
    }
    let chosen = kit.states.get(key).map_or(0, |state| state.tab).min(tabs.len().saturating_sub(1));
    let mut x = at.x;
    let mut row_height: f32 = 0.0;
    for (index, tab) in tabs.iter().enumerate() {
        // Measured with its lamp, which is what it is drawn with; measured without, the label was cut.
        let size =
            Key::new(&tab.label).compact().led(theme.accent.blue, index == chosen).measure(kit.rux);
        if x > at.x && x + size.x > at.x + width {
            x = at.x;
            pen += row_height + kit.z(8.0);
        }
        if kit.draw {
            let rect = Rect::from_min_size(Pos2::new(x, pen), size);
            let pressed = Key::new(&tab.label)
                .compact()
                .led(theme.accent.blue, index == chosen)
                .down(index == chosen)
                .id(kit.id(key, ("tab", index)))
                .show(kit.rux, rect)
                .clicked();
            if pressed {
                kit.state(key).tab = index;
            }
        }
        x += size.x + kit.z(8.0);
        row_height = row_height.max(size.y);
    }
    pen += row_height + kit.z(BETWEEN + 2.0);
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
        pen -= kit.z(BETWEEN);
    }
    pen - at.y
}

/// A callout: the tone's lamp beside the title, the words under it, and no stripe.
///
/// The lamp flares once as the callout arrives, which is the one moment the eye should be drawn to it.
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
    let indent = kit.z(18.0);
    let first_line = kit.rux.measure(kit.style(TITLE), "Ag").y;
    if kit.draw {
        let (colour, _) = tone_light(theme, if tone == Tone::Neutral { Tone::Info } else { tone });
        let off = Instrument::of(theme).led_off;
        let flare = instrument::appearing(kit.rux, kit.id(key, "flare"), 0.0, timing::RISE * 2.0);
        // Up to a flash and settling back, rather than simply coming on.
        let lit = instrument::ease_out((flare * 1.6).min(1.0))
            * (1.0 + 0.6 * (1.0 - flare) * flare * 4.0).min(1.6);
        instrument::led(
            kit.rux.painter(),
            Pos2::new(at.x + kit.z(4.0), at.y + first_line / 2.0),
            kit.z(3.5),
            colour,
            off,
            lit.min(1.0),
        );
        if lit > 1.0 {
            kit.rux.painter().circle_filled(
                Pos2::new(at.x + kit.z(4.0), at.y + first_line / 2.0),
                kit.z(14.0),
                colour.gamma_multiply((lit - 1.0) * 0.12),
            );
        }
    }
    let mut pen = at.y;
    if !title.is_empty() {
        pen +=
            kit.words(TITLE, title, theme.ink.i900, Pos2::new(at.x + indent, pen), width - indent)
                + kit.z(3.0);
    }
    if !text.is_empty() {
        pen += kit.markdown(
            &format!("{key}#text"),
            text,
            Pos2::new(at.x + indent, pen),
            width - indent,
        );
    }
    (pen - at.y).max(first_line)
}

/// Steps and timelines: a groove with a lamp for each item, numbered when there is no time.
fn steps(
    kit: &mut Kit<'_, '_>,
    key: &str,
    items: &[rich::component::Step],
    timeline: bool,
    at: Pos2,
    width: f32,
) -> f32 {
    let numbers: Vec<String> = (1..=items.len()).map(|n| n.to_string()).collect();
    let titles: Vec<String> = items.iter().map(|step| plain(&step.title)).collect();
    let texts: Vec<String> = items.iter().map(|step| plain(&step.text)).collect();
    let rows: Vec<Item<'_>> = items
        .iter()
        .enumerate()
        .map(|(index, step)| Item {
            time: match timeline {
                true => step.time.as_str(),
                false => numbers[index].as_str(),
            },
            title: &titles[index],
            text: &texts[index],
            stage: match step.state {
                StepState::Done => Stage::Done,
                StepState::Active => Stage::Active,
                StepState::Todo => Stage::Todo,
            },
        })
        .collect();
    let line = Timeline::new(kit.id(key, "timeline"), &rows);
    let height = line.height_at(kit.rux, width);
    if kit.draw {
        line.show(kit.rux, Rect::from_min_size(at, Vec2::new(width, height)));
    }
    height
}

/// Rows of a key printed on the plate and a value beside it.
fn keyvalue(kit: &mut Kit<'_, '_>, items: &[(String, String)], at: Pos2, width: f32) -> f32 {
    let theme = kit.rux.theme();
    let silk = kit.style(SILK);
    let column = items
        .iter()
        .map(|(key, _)| kit.rux.measure(silk, key).x)
        .fold(0.0_f32, f32::max)
        .min(width * 0.4)
        + kit.z(16.0);
    let first_line = kit.rux.measure(kit.style(WORDS), "Ag").y;
    let mut pen = at.y;
    for (index, (key, value)) in items.iter().enumerate() {
        if index > 0 {
            pen += kit.z(6.0);
        }
        if kit.draw {
            let galley = rux::text::elided(
                kit.rux.painter(),
                silk,
                key,
                theme.ink.i400,
                column - kit.z(12.0),
            );
            rux::text::draw_left_capitals(
                kit.rux.painter(),
                Pos2::new(at.x, pen + first_line / 2.0),
                galley,
                silk,
                theme.ink.i400,
            );
        }
        let code = value.contains('`');
        let value = plain(value);
        let style =
            if code || rux::components::is_figure(&value) || value.contains(['_', '/', '\\']) {
                MONO
            } else {
                WORDS
            };
        let height =
            kit.words(style, &value, theme.ink.i700, Pos2::new(at.x + column, pen), width - column);
        pen += height.max(first_line);
    }
    pen - at.y
}

/// A wrapped row of pills.
fn badges(kit: &mut Kit<'_, '_>, items: &[(String, Tone)], at: Pos2, width: f32) -> f32 {
    let row = kit.z(20.0);
    let gap = kit.z(6.0);
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

/// One readout on a screen: a stat, or a calculator's output.
struct Cell {
    label: String,
    value: String,
    delta: String,
    /// Whether the change is good news, bad news, or neither.
    good: Option<bool>,
    note: String,
}

/// A grid of readouts. Each number rolls up to its value the first time it is drawn.
fn readouts(kit: &mut Kit<'_, '_>, key: &str, cells: &[Cell], at: Pos2, width: f32) -> f32 {
    if cells.is_empty() {
        return 0.0;
    }
    let theme = kit.rux.theme();
    let colours = Instrument::of(theme);
    let gap = kit.z(8.0);
    let columns = (((width + gap) / (kit.z(96.0) + gap)).floor() as usize).clamp(1, cells.len());
    let each = (width - gap * (columns as f32 - 1.0)) / columns as f32;
    let reading = kit.style(READING);
    let silk = kit.style(SILK);
    let small = kit.style(Style::sans(11.0));
    let pad = kit.z(10.0);
    let height_of = |kit: &Kit<'_, '_>, cell: &Cell| {
        let mut h =
            pad * 2.0 + kit.rux.measure(silk, "A").y + kit.z(3.0) + kit.rux.measure(reading, "0").y;
        if !cell.delta.is_empty() {
            h += kit.z(2.0) + kit.rux.measure(small, "0").y;
        }
        if !cell.note.is_empty() {
            h += kit.z(2.0) + kit.rux.measure(small, "0").y;
        }
        h
    };
    let mut pen = at.y;
    for (row, chunk) in cells.chunks(columns).enumerate() {
        let height = chunk.iter().map(|cell| height_of(kit, cell)).fold(0.0_f32, f32::max);
        if kit.draw {
            for (index, cell) in chunk.iter().enumerate() {
                let rect = Rect::from_min_size(
                    Pos2::new(at.x + index as f32 * (each + gap), pen),
                    Vec2::new(each, height),
                );
                instrument::screen(kit.rux, rect, kit.z(9.0), true);
                let painter = kit.rux.painter().clone();
                let mut y = rect.top() + pad;
                let galley = rux::text::elided(
                    &painter,
                    silk,
                    &cell.label,
                    theme.ink.i400,
                    each - pad * 2.0,
                );
                y += galley.size().y;
                painter.galley(
                    Pos2::new(rect.left() + pad, rect.top() + pad),
                    galley,
                    theme.ink.i400,
                );
                y += kit.z(3.0);
                let roll = instrument::ease_out(instrument::appearing(
                    kit.rux,
                    kit.id(key, ("roll", row, index)),
                    (row * columns + index) as f32 * timing::STAGGER * 2.0,
                    timing::ROLL,
                ));
                let shown = rolled(&cell.value, roll);
                let galley =
                    rux::text::elided(&painter, reading, &shown, theme.ink.i900, each - pad * 2.0);
                let reading_height = galley.size().y;
                painter.galley(Pos2::new(rect.left() + pad, y), galley, theme.ink.i900);
                y += reading_height;
                if !cell.delta.is_empty() {
                    y += kit.z(2.0);
                    let colour = match cell.good {
                        Some(true) => theme.accent.mint,
                        Some(false) => theme.accent.coral,
                        None => theme.ink.i400,
                    };
                    let line = kit.rux.measure(small, "0").y;
                    instrument::led(
                        &painter,
                        Pos2::new(rect.left() + pad + kit.z(2.5), y + line / 2.0),
                        kit.z(2.5),
                        colour,
                        colours.led_off,
                        if cell.good.is_some() { 1.0 } else { 0.0 },
                    );
                    let tint = match cell.good {
                        Some(_) => lighten(colour),
                        None => theme.ink.i500,
                    };
                    let galley = rux::text::elided(
                        &painter,
                        small,
                        &cell.delta,
                        tint,
                        each - pad * 2.0 - kit.z(10.0),
                    );
                    painter.galley(Pos2::new(rect.left() + pad + kit.z(10.0), y), galley, tint);
                    y += line;
                }
                if !cell.note.is_empty() {
                    y += kit.z(2.0);
                    let galley = rux::text::elided(
                        &painter,
                        small,
                        &cell.note,
                        theme.ink.i400,
                        each - pad * 2.0,
                    );
                    painter.galley(Pos2::new(rect.left() + pad, y), galley, theme.ink.i400);
                }
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

/// A colour a third of the way to white, for words in a coloured light's tint.
fn lighten(colour: Color32) -> Color32 {
    let up = |c: u8| (c as f32 + (255.0 - c as f32) * 0.35) as u8;
    Color32::from_rgb(up(colour.r()), up(colour.g()), up(colour.b()))
}

/// `value` with its first number replaced by `fraction` of itself, written the same way.
///
/// `178 s` at a half is `89 s`; `$1,204.50` at a half is `$602.25`. Text with no number in it is
/// unchanged. This is what makes a readout roll up as it appears, the way an instrument's display
/// counts up when it is switched on.
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

/// Labelled meters.
fn progress(
    kit: &mut Kit<'_, '_>,
    key: &str,
    items: &[rich::component::Bar],
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let style = kit.style(WORDS);
    let mono = kit.style(MONO.at(11.0));
    let line = kit.rux.measure(style, "Ag").y;
    let meter = Meter::height(kit.rux);
    let mut pen = at.y;
    for (index, bar) in items.iter().enumerate() {
        if index > 0 {
            pen += kit.z(GAP);
        }
        let fraction = (bar.value / bar.max).clamp(0.0, 1.0) as f32;
        let figure = match (bar.max - 100.0).abs() < f64::EPSILON {
            true => format!("{}%", format::plain(bar.value)),
            false => format!("{} / {}", format::plain(bar.value), format::plain(bar.max)),
        };
        if kit.draw {
            let painter = kit.rux.painter().clone();
            let figure_galley = rux::text::layout(&painter, mono, &figure, theme.ink.i500);
            let room = width - figure_galley.size().x - kit.z(10.0);
            let words = rux::text::elided(&painter, style, &bar.label, theme.ink.i700, room);
            rux::text::draw_left_capitals(
                &painter,
                Pos2::new(at.x, pen + line / 2.0),
                words,
                style,
                theme.ink.i700,
            );
            rux::text::draw_left_capitals(
                &painter,
                Pos2::new(at.x + width - figure_galley.size().x, pen + line / 2.0),
                figure_galley,
                mono,
                theme.ink.i500,
            );
            let colour = match fraction >= 1.0 {
                true => theme.accent.mint,
                false => theme.accent.blue,
            };
            Meter::new(fraction).colour(colour).id(kit.id(key, ("meter", index))).show(
                kit.rux,
                Rect::from_min_size(
                    Pos2::new(at.x, pen + line + kit.z(5.0)),
                    Vec2::new(width, meter),
                ),
            );
        }
        pen += line + kit.z(5.0) + meter;
    }
    pen - at.y
}

/// A chart from labels and named series, drawn on a screen by `rux`.
#[allow(clippy::too_many_arguments)]
fn chart_at(
    kit: &mut Kit<'_, '_>,
    key: &str,
    kind: ChartKind,
    labels: &[String],
    series: &[(String, Vec<f64>)],
    unit: &str,
    stacked: bool,
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let colours = Instrument::of(theme);
    let unit = unit.to_owned();
    let format = move |value: f64| {
        let largest = value.abs();
        let text = match largest >= 10_000.0 {
            true => format::number(value, "compact", ""),
            false => format::number(value, "number", ""),
        };
        match format::number(1.0, "number", &unit).starts_with(|c: char| !c.is_ascii_digit()) {
            true => format!("{}{text}", unit),
            false => text,
        }
    };
    // A comparison reads best as a quiet grey against the colour of now: the first of two series is the
    // older one, by the convention every example in the catalogue keeps.
    let paint = |index: usize| match (series.len(), index) {
        (2, 0) if kind != ChartKind::Donut => colours.quiet,
        (2, 1) if kind != ChartKind::Donut => colours.series(0),
        _ => colours.series(index),
    };
    let rux_kind = match kind {
        ChartKind::Bar => RuxChartKind::Bar,
        ChartKind::Line => RuxChartKind::Line,
        ChartKind::Area => RuxChartKind::Area,
        ChartKind::Donut => RuxChartKind::Donut,
    };
    let mut chart = Chart::new(kit.id(key, "chart"), rux_kind, labels, &format).stacked(stacked);
    for (index, (name, values)) in series.iter().enumerate() {
        chart = chart.series(RuxSeries { name, values, colour: paint(index) });
    }
    let height = chart.height_at(kit.rux, width);
    if kit.draw {
        chart.show(kit.rux, Rect::from_min_size(at, Vec2::new(width, height)));
    }
    height
}

/// A table, ordered the way the person last pressed its headers.
fn table(
    kit: &mut Kit<'_, '_>,
    key: &str,
    columns: &[rich::component::Column],
    rows: &[Vec<String>],
    at: Pos2,
    width: f32,
) -> f32 {
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
    let heads: Vec<RuxColumn<'_>> = columns
        .iter()
        .map(|column| RuxColumn {
            label: &column.label,
            align: match column.align {
                Align::Left => rux::components::Align::Left,
                Align::Right => rux::components::Align::Right,
                Align::Centre => rux::components::Align::Centre,
            },
        })
        .collect();
    let view = Table::new(kit.id(key, "table"), &heads, &ordered).sorted(sort);
    let height = view.height_at(kit.rux, width);
    if kit.draw {
        let outcome = view.show(kit.rux, Rect::from_min_size(at, Vec2::new(width, height)));
        if let Some(column) = outcome.sort {
            let state = kit.state(key);
            state.sort = match state.sort {
                Some((by, true)) if by == column => Some((column, false)),
                Some((by, false)) if by == column => None,
                _ => Some((column, true)),
            };
        }
    }
    height
}

/// The number a cell starts with, for ordering a column of figures as figures.
fn leading_number(cell: &str) -> Option<f64> {
    let trimmed = cell.trim().trim_start_matches(['$', '£', '€']);
    let end = trimmed
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == ','))
        .unwrap_or(trimmed.len());
    trimmed[..end].replace(',', "").parse().ok()
}

/// Places in the project, each a row that opens its file at its line.
fn files(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    items: &[rich::component::FileRef],
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let row = kit.z(32.0);
    let mut pen = at.y;
    if !title.is_empty() {
        pen += kit.z(6.0)
            + kit.title(
                title,
                Pos2::new(at.x + kit.z(8.0), pen + kit.z(6.0)),
                width - kit.z(16.0),
                None,
            )
            - kit.z(4.0);
    }
    let mono = kit.style(MONO);
    let note_style = kit.style(Style::sans(11.5));
    for (index, item) in items.iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, row));
        if kit.draw {
            let response =
                kit.rux.ui.interact(rect, kit.id(key, ("file", index)), egui::Sense::click());
            let hovered = response.hovered();
            if hovered {
                kit.rux.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                kit.rux.painter().rect_filled(
                    rect.shrink(kit.z(2.0)),
                    kit.z(9.0),
                    theme.interaction.hover,
                );
            }
            let painter = kit.rux.painter().clone();
            // A jack on a patch bay: a ring that lights under the pointer.
            let jack = Pos2::new(rect.left() + kit.z(14.0), rect.center().y);
            let ring = match hovered {
                true => theme.accent.blue,
                false => theme.ink.i300,
            };
            if hovered {
                painter.circle_filled(jack, kit.z(9.0), theme.accent.blue.gamma_multiply(0.12));
            }
            painter.circle_stroke(jack, kit.z(4.5), egui::Stroke::new(kit.z(1.5), ring));
            let (folder, name) = match item.path.rsplit_once(['/', '\\']) {
                Some((folder, name)) => (format!("{folder}/"), name.to_owned()),
                None => (String::new(), item.path.clone()),
            };
            let mut x = rect.left() + kit.z(28.0);
            let line = match item.line {
                Some(line) => format!(":{line}"),
                None => String::new(),
            };
            let note_width = match item.note.is_empty() {
                true => 0.0,
                false => (width * 0.4).min(kit.rux.measure(note_style, &item.note).x + kit.z(12.0)),
            };
            let room = rect.right() - kit.z(10.0) - note_width - x;
            let name_galley = rux::text::layout(&painter, mono, &name, theme.ink.i900);
            let line_galley = rux::text::layout(&painter, mono, &line, theme.ink.i300);
            let folder_room = (room - name_galley.size().x - line_galley.size().x).max(0.0);
            if folder_room > kit.z(20.0) && !folder.is_empty() {
                let folder_galley =
                    rux::text::elided(&painter, mono, &folder, theme.ink.i400, folder_room);
                let wide = folder_galley.size().x;
                rux::text::draw_left_capitals(
                    &painter,
                    Pos2::new(x, rect.center().y),
                    folder_galley,
                    mono,
                    theme.ink.i400,
                );
                x += wide;
            }
            let wide = name_galley.size().x;
            rux::text::draw_left_capitals(
                &painter,
                Pos2::new(x, rect.center().y),
                name_galley,
                mono,
                theme.ink.i900,
            );
            x += wide;
            rux::text::draw_left_capitals(
                &painter,
                Pos2::new(x, rect.center().y),
                line_galley,
                mono,
                theme.ink.i300,
            );
            if !item.note.is_empty() {
                let galley = rux::text::elided(
                    &painter,
                    note_style,
                    &item.note,
                    theme.ink.i500,
                    note_width - kit.z(4.0),
                );
                let left = rect.right() - kit.z(10.0) - galley.size().x;
                rux::text::draw_left_capitals(
                    &painter,
                    Pos2::new(left, rect.center().y),
                    galley,
                    note_style,
                    theme.ink.i500,
                );
            }
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!("Open {}", item.path),
                )
            });
            if response.clicked() {
                kit.acts.push(Act::OpenFile(item.path.clone(), item.line));
            }
        }
        pen += row;
    }
    pen - at.y
}

/// A unified diff on a screen: added lines lit mint, removed lines coral, the file openable.
fn diff(
    kit: &mut Kit<'_, '_>,
    key: &str,
    title: &str,
    path: &str,
    text: &str,
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let mono = kit.style(MONO.at(11.0));
    let mut pen = at.y;
    let added = text.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count();
    let removed = text.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count();
    let heading = if path.is_empty() { title } else { path };
    let head_height = kit.rux.measure(mono, "Ag").y + kit.z(6.0);
    if kit.draw {
        let painter = kit.rux.painter().clone();
        let counts = format!("+{added}  \u{2212}{removed}");
        let counts_galley = rux::text::layout(&painter, mono, &counts, theme.ink.i400);
        let room = width - counts_galley.size().x - kit.z(12.0);
        let galley = rux::text::elided(&painter, mono, heading, theme.ink.i900, room);
        let head = Rect::from_min_size(
            Pos2::new(at.x + kit.z(2.0), pen),
            Vec2::new(galley.size().x, head_height),
        );
        if !path.is_empty() {
            let response = kit.rux.ui.interact(head, kit.id(key, "path"), egui::Sense::click());
            if response.hovered() {
                kit.rux.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                kit.acts.push(Act::OpenFile(path.to_owned(), first_line_of(text)));
            }
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Open {path}"))
            });
        }
        rux::text::draw_left_capitals(
            &painter,
            Pos2::new(head.left(), head.center().y),
            galley,
            mono,
            theme.ink.i900,
        );
        let mut x = at.x + width - counts_galley.size().x - kit.z(2.0);
        for (words, colour) in [
            (format!("+{added}"), theme.accent.mint),
            (format!("\u{2212}{removed}"), theme.accent.coral),
        ] {
            let galley = rux::text::layout(&painter, mono, &words, lighten(colour));
            let wide = galley.size().x;
            rux::text::draw_left_capitals(
                &painter,
                Pos2::new(x, head.center().y),
                galley,
                mono,
                lighten(colour),
            );
            x += wide + kit.z(8.0);
        }
    }
    pen += head_height + kit.z(6.0);
    let line_height = kit.z(17.0);
    let lines: Vec<&str> = text.lines().collect();
    let screen_height = line_height * lines.len() as f32 + kit.z(14.0);
    if kit.draw {
        let rect = Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, screen_height));
        instrument::screen(kit.rux, rect, kit.z(9.0), false);
        let painter = kit.rux.painter().clone();
        for (index, line) in lines.iter().enumerate() {
            let top = rect.top() + kit.z(7.0) + index as f32 * line_height;
            let band = Rect::from_min_size(
                Pos2::new(rect.left() + kit.z(3.0), top),
                Vec2::new(rect.width() - kit.z(6.0), line_height),
            );
            let (ink, wash) = match line.chars().next() {
                Some('+') if !line.starts_with("+++") => {
                    (lighten(theme.accent.mint), Some(theme.accent.mint))
                }
                Some('-') if !line.starts_with("---") => {
                    (lighten(theme.accent.coral), Some(theme.accent.coral))
                }
                Some('@') => (theme.ink.i300, None),
                _ => (theme.ink.i500, None),
            };
            if let Some(wash) = wash {
                band_fade(&painter, band, wash.gamma_multiply(0.13));
            }
            let shown = match line.chars().next() {
                Some('-') if !line.starts_with("---") => format!("\u{2212}{}", &line[1..]),
                _ => (*line).to_owned(),
            };
            let galley = rux::text::elided(&painter, mono, &shown, ink, band.width() - kit.z(16.0));
            rux::text::draw_left_capitals(
                &painter,
                Pos2::new(band.left() + kit.z(9.0), band.center().y),
                galley,
                mono,
                ink,
            );
        }
    }
    pen += screen_height;
    pen - at.y
}

/// The line a diff's first hunk starts on in the new file, which is where opening it should go.
fn first_line_of(diff: &str) -> Option<u32> {
    let hunk = diff.lines().find(|line| line.starts_with("@@"))?;
    let plus = hunk.split_whitespace().find(|part| part.starts_with('+'))?;
    plus.trim_start_matches('+').split(',').next()?.parse().ok()
}

/// A band whose colour fades out to the right, the light a lit line throws along a screen.
fn band_fade(painter: &egui::Painter, rect: Rect, colour: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), colour);
    mesh.colored_vertex(rect.left_bottom(), colour);
    mesh.colored_vertex(rect.right_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(rect.right_bottom(), Color32::TRANSPARENT);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// A Mermaid diagram on a screen, laid out by the editor's own Mermaid reader and fitted to the width.
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
    let pad = kit.z(12.0);
    match laid {
        Ok(scene) => {
            let natural = Vec2::new(scene.size.width, scene.size.height);
            let room = width - pad * 2.0;
            let scale = (room / natural.x.max(1.0)).min(1.0).min(kit.z(420.0) / natural.y.max(1.0));
            let height = natural.y * scale + pad * 2.0;
            if kit.draw {
                let rect = Rect::from_min_size(at, Vec2::new(width, height));
                instrument::screen(kit.rux, rect, kit.z(9.0), true);
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

/// A wrapped row of key caps. A primary key has its lamp lit; a key that was pressed stays down with a
/// mint lamp, so the conversation shows which of its suggestions was taken.
fn keys(
    kit: &mut Kit<'_, '_>,
    key: &str,
    buttons: &[(String, Option<Action>, bool)],
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let pressed = kit.states.get(key).and_then(|state| state.pressed.clone());
    let gap = kit.z(8.0);
    let mut x = at.x;
    let mut y = at.y;
    let mut row: f32 = 0.0;
    for (index, (label, action, primary)) in buttons.iter().enumerate() {
        let was = pressed.as_deref() == Some(label.as_str());
        let lamp = match (was, primary) {
            (true, _) => Some(theme.accent.mint),
            (false, true) => Some(theme.accent.blue),
            _ => None,
        };
        let mut button = Key::new(label).id(kit.id(key, ("key", index))).down(was);
        if let Some(colour) = lamp {
            button = button.led(colour, true);
        }
        let mut size = button.measure(kit.rux);
        size.x = size.x.min(width);
        if x > at.x && x + size.x > at.x + width {
            x = at.x;
            y += row + gap;
            row = 0.0;
        }
        if kit.draw && button.show(kit.rux, Rect::from_min_size(Pos2::new(x, y), size)).clicked() {
            kit.state(key).pressed = Some(label.clone());
            if let Some(action) = action {
                kit.acts.push(act_of(action));
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

/// Checkboxes, and a run of segments under them counting what is done.
fn checklist(
    kit: &mut Kit<'_, '_>,
    key: &str,
    items: &[rich::component::Check],
    at: Pos2,
    width: f32,
) -> f32 {
    let theme = kit.rux.theme();
    let ticks = kit.states.get(key).map(|state| state.ticks.clone()).unwrap_or_default();
    let mut pen = at.y;
    let mut done = 0;
    for (index, item) in items.iter().enumerate() {
        let on = ticks.get(&index).copied().unwrap_or(item.done);
        done += usize::from(on);
        let label = plain(&item.label);
        let check = Checkbox::new(on, &label).id(kit.id(key, ("tick", index)));
        let height = check.height_at(kit.rux, width);
        if kit.draw
            && check
                .show(kit.rux, Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, height)))
                .clicked()
        {
            kit.state(key).ticks.insert(index, !on);
        }
        pen += height + kit.z(8.0);
    }
    // The count, and one segment an item lit for each that is done.
    let silk = kit.style(SILK);
    let words = format!("{done} of {}", items.len());
    let label = kit.rux.measure(silk, &words);
    let row = kit.z(8.0).max(label.y);
    if kit.draw && !items.is_empty() {
        let painter = kit.rux.painter().clone();
        let galley = rux::text::layout(&painter, silk, &words, theme.ink.i400);
        rux::text::draw_left_capitals(
            &painter,
            Pos2::new(at.x, pen + row / 2.0),
            galley,
            silk,
            theme.ink.i400,
        );
        let left = at.x + label.x + kit.z(12.0);
        let gap = kit.z(3.0);
        let count = items.len();
        let each = ((at.x + width - left) - gap * (count as f32 - 1.0)) / count as f32;
        // Unlit segments are holes in the plate, the screen's own colour, so the count reads as a meter.
        let off = Instrument::of(theme).screen;
        for segment in 0..count {
            let cell = Rect::from_min_size(
                Pos2::new(left + segment as f32 * (each + gap), pen + row / 2.0 - kit.z(3.0)),
                Vec2::new(each, kit.z(6.0)),
            );
            let lit = segment < done;
            if lit {
                painter.rect_filled(
                    cell.expand(kit.z(1.5)),
                    kit.z(3.0),
                    theme.accent.mint.gamma_multiply(0.15),
                );
            }
            painter.rect_filled(cell, kit.z(2.0), if lit { theme.accent.mint } else { off });
        }
    }
    pen + row - at.y
}

/// A form: each field a labelled control, and a key that sends what was filled in as a message.
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
    let silk = kit.style(SILK);
    let label_height = kit.rux.measure(silk, "A").y;
    let mut pen = at.y;
    for field in fields {
        let current = kit
            .states
            .get(key)
            .and_then(|state| state.values.get(&field.name).cloned())
            .unwrap_or_else(|| field.value.clone());
        match field.kind {
            FieldKind::Toggle => {
                let on = matches!(current.as_str(), "true" | "yes" | "on" | "1");
                let check = Checkbox::new(on, &field.label)
                    .colour(theme.accent.blue)
                    .settles(false)
                    .id(kit.id(key, ("toggle", &field.name)));
                let height = check.height_at(kit.rux, width);
                if kit.draw
                    && check
                        .show(
                            kit.rux,
                            Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, height)),
                        )
                        .clicked()
                {
                    kit.state(key).values.insert(field.name.clone(), (!on).to_string());
                }
                pen += height + kit.z(GAP);
                continue;
            }
            _ => {}
        }
        if kit.draw {
            let galley = rux::text::layout(kit.rux.painter(), silk, &field.label, theme.ink.i400);
            kit.rux.painter().galley(Pos2::new(at.x, pen), galley, theme.ink.i400);
        }
        pen += label_height + kit.z(5.0);
        match field.kind {
            FieldKind::Slider => {
                let value: f64 = current.parse().unwrap_or(field.min);
                let height = Fader::height(kit.rux);
                if kit.draw {
                    let fader = Fader::new(
                        kit.id(key, ("fader", &field.name)),
                        value,
                        field.min,
                        field.max,
                    )
                    .step(field.step)
                    .label(field.label.clone());
                    let readout = format::plain(value);
                    let mono = kit.style(MONO);
                    let galley =
                        rux::text::layout(kit.rux.painter(), mono, &readout, theme.ink.i900);
                    let wide = galley.size().x + kit.z(12.0);
                    kit.rux.painter().galley(
                        Pos2::new(at.x + width - galley.size().x, pen + kit.z(4.0)),
                        galley,
                        theme.ink.i900,
                    );
                    if let Some(next) = fader
                        .show(
                            kit.rux,
                            Rect::from_min_size(
                                Pos2::new(at.x, pen),
                                Vec2::new(width - wide, height),
                            ),
                        )
                        .changed
                    {
                        kit.state(key).values.insert(field.name.clone(), format::plain(next));
                    }
                }
                pen += height;
            }
            FieldKind::Select => {
                let height = kit.z(32.0);
                if kit.draw {
                    let chosen = field.options.iter().position(|one| *one == current);
                    let rect = Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, height));
                    let zoom = kit.rux.zoom();
                    let mut open =
                        kit.state(key).selects.get(&field.name).copied().unwrap_or_default();
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
                    let state = kit.state(key);
                    state.selects.insert(field.name.clone(), open);
                    if let Some(index) = outcome.chosen {
                        if let Some(value) = field.options.get(index) {
                            state.values.insert(field.name.clone(), value.clone());
                        }
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
        pen += kit.z(GAP);
    }
    let sent = kit.states.get(key).and_then(|state| state.pressed.clone()).is_some();
    let label = if sent { "Sent" } else { submit };
    let button = Key::new(label)
        .led(if sent { theme.accent.mint } else { theme.accent.blue }, true)
        .down(sent)
        .id(kit.id(key, "submit"));
    let size = button.measure(kit.rux);
    if kit.draw
        && button.show(kit.rux, Rect::from_min_size(Pos2::new(at.x, pen), size)).clicked()
        && !sent
    {
        let state = kit.states.get(key).cloned().unwrap_or_default();
        kit.state(key).pressed = Some(submit.to_owned());
        kit.acts.push(Act::SendWords(form_message(title, fields, &state)));
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

/// A calculator: faders for its inputs, readouts for its outputs, and a chart worked out over a range.
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
    let silk = kit.style(SILK);
    let mono = kit.style(MONO.at(12.0));
    let label_height = kit.rux.measure(silk, "A").y;
    let mut pen = at.y;
    for input in inputs {
        let value = values.get(&input.name).copied().unwrap_or(input.value);
        if kit.draw {
            let painter = kit.rux.painter().clone();
            let galley = rux::text::layout(&painter, silk, &input.label, theme.ink.i400);
            painter.galley(Pos2::new(at.x, pen), galley, theme.ink.i400);
            let reading = format::number(
                value,
                if input.unit == "$" { "money" } else { "number" },
                &input.unit,
            );
            let galley = rux::text::layout(&painter, mono, &reading, theme.ink.i900);
            painter.galley(
                Pos2::new(at.x + width - galley.size().x, pen + label_height - galley.size().y),
                galley,
                theme.ink.i900,
            );
        }
        pen += label_height + kit.z(4.0);
        let height = Fader::height(kit.rux);
        if kit.draw {
            let fader =
                Fader::new(kit.id(key, ("input", &input.name)), value, input.min, input.max)
                    .step(input.step)
                    .label(input.label.clone());
            if let Some(next) = fader
                .show(kit.rux, Rect::from_min_size(Pos2::new(at.x, pen), Vec2::new(width, height)))
                .changed
            {
                kit.state(key).inputs.insert(input.name.clone(), next);
            }
        }
        pen += height + kit.z(GAP - 2.0);
    }
    let lookup = |name: &str| values.get(name).copied();
    if !outputs.is_empty() {
        pen += kit.z(2.0);
        let cells: Vec<Cell> = outputs
            .iter()
            .map(|output| Cell {
                label: output.label.clone(),
                value: match output.expr.as_ref().map(|expr| expr.eval(&lookup)) {
                    Some(Ok(value)) => format::number(
                        value,
                        if output.format.is_empty() { "number" } else { &output.format },
                        &output.unit,
                    ),
                    Some(Err(_)) | None => "\u{2014}".to_owned(),
                },
                delta: String::new(),
                good: None,
                note: match output.expr.as_ref().map(|expr| expr.eval(&lookup)) {
                    Some(Err(problem)) => problem,
                    _ => String::new(),
                },
            })
            .collect();
        pen +=
            readouts(kit, &format!("{key}#out"), &cells, Pos2::new(at.x, pen), width) + kit.z(GAP);
    }
    if let Some(chart) = chart {
        let (labels, series) = calculated(chart, &values);
        let kind = chart.kind;
        pen += chart_at(
            kit,
            &format!("{key}#chart"),
            kind,
            &labels,
            &series,
            &chart.unit,
            false,
            Pos2::new(at.x, pen),
            width,
        ) + kit.z(GAP);
    }
    (pen - kit.z(GAP) - at.y).max(0.0)
}

/// Evaluate a calculator's chart at every value of its `x`. At most 60 points, so a range written as
/// `0` to `1000` in steps of one is sampled rather than drawn a thousand bars wide.
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
