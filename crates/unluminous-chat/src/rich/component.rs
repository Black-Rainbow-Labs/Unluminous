//! What a block means: the components, and reading one out of a JSON value.
//!
//! Two readings of the same value, chosen by `strict`:
//!
//! - **While the block is arriving** a missing field takes its empty value and an item that does not
//!   read is left out, so the component draws with whatever has come. Nothing is reported.
//! - **Once it has finished** a missing required field, a value of the wrong kind and a `type` this
//!   version has not got are errors, each with the path to it, and the pane draws a notice instead of
//!   the component. A key the component does not have is a **note**: ignored when drawing, reported by
//!   `validate`, so a model that adds a field costs nothing on the screen and is still told about it.
//!
//! Every field name is `snake_case`, a list is always a list, and every component takes an optional
//! `title`. The catalogue in `super::catalogue` is the reference for all of it; a field read here and
//! not described there fails a test.

use serde_json::{Map, Value};

use super::expr::Expr;

/// How deep one component may hold others. A card in a column in a tab is three; nothing an answer
/// needs is deeper, and a bound keeps a hostile block from costing a frame.
pub const DEEPEST: usize = 4;

/// One problem with a block, and where in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    /// A path like `series[1].values[3]`, or empty for the block as a whole.
    pub path: String,
    pub message: String,
    /// An error stops the component being drawn; a note does not.
    pub error: bool,
}

impl Problem {
    pub fn error(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self { path: path.into(), message: message.into(), error: true }
    }

    pub fn note(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self { path: path.into(), message: message.into(), error: false }
    }

    pub fn is_error(&self) -> bool {
        self.error
    }

    /// The problem as one line, with its path in front when it has one.
    pub fn line(&self) -> String {
        match self.path.is_empty() {
            true => self.message.clone(),
            false => format!("{}: {}", self.path, self.message),
        }
    }
}

/// One component: its title and what it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub title: String,
    pub kind: Kind,
}

/// How a callout, a badge or a delta is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Danger,
    Tip,
}

impl Tone {
    pub const NAMES: &'static [&'static str] =
        &["neutral", "info", "success", "warning", "danger", "tip"];

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "neutral" => Self::Neutral,
            "info" => Self::Info,
            "success" => Self::Success,
            "warning" => Self::Warning,
            "danger" | "error" => Self::Danger,
            "tip" => Self::Tip,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }
}

/// Where a step or a timeline item has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Progress {
    Done,
    Active,
    #[default]
    Todo,
}

impl Progress {
    pub const NAMES: &'static [&'static str] = &["done", "active", "todo"];
}

/// Which way a number went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Trend {
    Up,
    Down,
    #[default]
    Flat,
}

/// Which way up a chart is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartKind {
    #[default]
    Bar,
    Line,
    Area,
    Donut,
    /// What something was and what was added to it, as a ring of two with the total in the middle
    /// (`task-2235`). One series of two values.
    Dial,
}

impl ChartKind {
    pub const NAMES: &'static [&'static str] = &["bar", "line", "area", "donut", "dial"];

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }
}

/// What pressing something does. These four and nothing else; see the TDD §4.2.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Send the text as the person's next message.
    Send(String),
    /// Put the text in the composer to be edited.
    Fill(String),
    /// Open a file of the project, at a line when one is given.
    Open { path: String, line: Option<u32> },
    /// Copy the text.
    Copy(String),
}

/// One button.
#[derive(Debug, Clone, PartialEq)]
pub struct Button {
    pub label: String,
    pub action: Option<Action>,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub label: String,
    pub children: Vec<Component>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub time: String,
    pub title: String,
    pub text: String,
    pub state: Progress,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Badge {
    pub label: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stat {
    pub label: String,
    pub value: String,
    pub delta: String,
    pub trend: Trend,
    /// Whether going up is good. A build time going up is bad, a pass rate going up is good.
    pub up_is_good: bool,
    pub note: String,
    /// The last few values, oldest first, drawn as a sparkline under the number (`task-2235`).
    pub history: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub label: String,
    pub value: f64,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chart {
    pub kind: ChartKind,
    pub labels: Vec<String>,
    pub series: Vec<Series>,
    pub unit: String,
    pub stacked: bool,
}

/// How a column's cells are lined up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Right,
    Centre,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub label: String,
    pub align: Align,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileRef {
    pub path: String,
    pub line: Option<u32>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub label: String,
    pub done: bool,
}

/// What kind of control a form field is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldKind {
    #[default]
    Text,
    Number,
    Select,
    Toggle,
    Slider,
    Multiline,
}

impl FieldKind {
    pub const NAMES: &'static [&'static str] =
        &["text", "number", "select", "toggle", "slider", "multiline"];
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    pub options: Vec<String>,
    /// The starting value, written as text whatever the kind: `"true"` for a toggle that starts on.
    pub value: String,
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    pub name: String,
    pub label: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub label: String,
    /// `None` while the expression is still being written.
    pub expr: Option<Expr>,
    pub unit: String,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalcSeries {
    pub name: String,
    pub expr: Option<Expr>,
}

/// A chart a calculator draws by evaluating each series at every value of `x`.
#[derive(Debug, Clone, PartialEq)]
pub struct CalcChart {
    pub kind: ChartKind,
    pub x: String,
    pub from: Option<Expr>,
    pub to: Option<Expr>,
    pub step: Option<Expr>,
    pub series: Vec<CalcSeries>,
    pub unit: String,
}

/// What a component is.
///
/// The calculator is far larger than the other kinds. It is left inline rather than boxed: a reply holds
/// a handful of components, each read once per change to its text, so the size costs nothing measurable.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Kind {
    /// A block whose `type` has not arrived yet.
    Pending,
    Card {
        subtitle: String,
        text: String,
        badge: String,
        children: Vec<Component>,
    },
    Columns {
        columns: Vec<Component>,
    },
    Tabs {
        tabs: Vec<Tab>,
    },
    Stack {
        children: Vec<Component>,
    },
    Callout {
        tone: Tone,
        text: String,
    },
    /// `steps` and `timeline` are one drawing; a timeline shows each item's time.
    Steps {
        items: Vec<Step>,
        timeline: bool,
    },
    KeyValue {
        items: Vec<(String, String)>,
    },
    Badges {
        items: Vec<Badge>,
    },
    Stats {
        items: Vec<Stat>,
    },
    Progress {
        items: Vec<Bar>,
    },
    Chart(Chart),
    Table {
        columns: Vec<Column>,
        rows: Vec<Vec<String>>,
    },
    Files {
        items: Vec<FileRef>,
    },
    Diff {
        path: String,
        text: String,
    },
    Diagram {
        source: String,
    },
    Actions {
        items: Vec<Button>,
    },
    Choices {
        question: String,
        options: Vec<String>,
    },
    Checklist {
        items: Vec<Check>,
    },
    Form {
        fields: Vec<Field>,
        submit: String,
    },
    Calculator {
        inputs: Vec<Input>,
        outputs: Vec<Output>,
        chart: Option<CalcChart>,
    },
}

/// Every `type` this version reads, in the order the catalogue lists them.
pub const TYPES: &[&str] = &[
    "card",
    "columns",
    "tabs",
    "stack",
    "callout",
    "steps",
    "timeline",
    "keyvalue",
    "badges",
    "diagram",
    "stats",
    "progress",
    "chart",
    "table",
    "files",
    "diff",
    "actions",
    "choices",
    "checklist",
    "form",
    "calculator",
];

impl Component {
    /// A component whose `type` has not arrived yet.
    pub fn pending() -> Self {
        Self { title: String::new(), kind: Kind::Pending }
    }

    /// The `type` it was written as.
    pub fn type_name(&self) -> &'static str {
        match &self.kind {
            Kind::Pending => "pending",
            Kind::Card { .. } => "card",
            Kind::Columns { .. } => "columns",
            Kind::Tabs { .. } => "tabs",
            Kind::Stack { .. } => "stack",
            Kind::Callout { .. } => "callout",
            Kind::Steps { timeline: false, .. } => "steps",
            Kind::Steps { timeline: true, .. } => "timeline",
            Kind::KeyValue { .. } => "keyvalue",
            Kind::Badges { .. } => "badges",
            Kind::Stats { .. } => "stats",
            Kind::Progress { .. } => "progress",
            Kind::Chart(_) => "chart",
            Kind::Table { .. } => "table",
            Kind::Files { .. } => "files",
            Kind::Diff { .. } => "diff",
            Kind::Diagram { .. } => "diagram",
            Kind::Actions { .. } => "actions",
            Kind::Choices { .. } => "choices",
            Kind::Checklist { .. } => "checklist",
            Kind::Form { .. } => "form",
            Kind::Calculator { .. } => "calculator",
        }
    }

    /// How many things it holds, which is what "a component only grows" is measured in.
    pub fn size(&self) -> usize {
        let children = |list: &[Component]| list.iter().map(Component::size).sum::<usize>();
        1 + match &self.kind {
            Kind::Pending => 0,
            Kind::Card { children: list, .. } | Kind::Stack { children: list } => children(list),
            Kind::Columns { columns } => children(columns),
            Kind::Tabs { tabs } => tabs.iter().map(|tab| 1 + children(&tab.children)).sum(),
            Kind::Callout { .. } | Kind::Diff { .. } | Kind::Diagram { .. } => 0,
            Kind::Steps { items, .. } => items.len(),
            Kind::KeyValue { items } => items.len(),
            Kind::Badges { items } => items.len(),
            Kind::Stats { items } => items.len(),
            Kind::Progress { items } => items.len(),
            Kind::Chart(chart) => {
                chart.labels.len() + chart.series.iter().map(|s| 1 + s.values.len()).sum::<usize>()
            }
            Kind::Table { columns, rows } => columns.len() + rows.len(),
            Kind::Files { items } => items.len(),
            Kind::Actions { items } => items.len(),
            Kind::Choices { options, .. } => options.len(),
            Kind::Checklist { items } => items.len(),
            Kind::Form { fields, .. } => fields.len(),
            Kind::Calculator { inputs, outputs, chart } => {
                inputs.len() + outputs.len() + chart.as_ref().map_or(0, |c| 1 + c.series.len())
            }
        }
    }

    /// Read a component out of `value`, with every problem found on the way.
    pub fn read(value: &Value, strict: bool) -> (Component, Vec<Problem>) {
        let mut reader = Reader { strict, problems: Vec::new() };
        let component = reader.component(value, "", 0).unwrap_or_else(Component::pending);
        (component, reader.problems)
    }
}

/// Reads a value into a component, collecting problems with their paths.
struct Reader {
    strict: bool,
    problems: Vec<Problem>,
}

/// A path with one more step on it.
fn join(path: &str, key: &str) -> String {
    match path.is_empty() {
        true => key.to_owned(),
        false => format!("{path}.{key}"),
    }
}

/// A path with an index on it.
fn index(path: &str, at: usize) -> String {
    format!("{path}[{at}]")
}

/// A JSON value written as text: a string as it is, a number without a needless `.0`.
fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(super::format::plain(number.as_f64().unwrap_or(0.0))),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

impl Reader {
    fn error(&mut self, path: &str, message: impl Into<String>) {
        if self.strict {
            self.problems.push(Problem::error(path, message));
        }
    }

    fn note(&mut self, path: &str, message: impl Into<String>) {
        if self.strict {
            self.problems.push(Problem::note(path, message));
        }
    }

    /// Read one component at `path`, or `None` when it is not one at all.
    fn component(&mut self, value: &Value, path: &str, depth: usize) -> Option<Component> {
        let Some(object) = value.as_object() else {
            self.error(path, "expected an object with a \"type\"");
            return None;
        };
        if depth > DEEPEST {
            self.error(path, format!("components are nested more than {DEEPEST} deep"));
            return None;
        }
        let type_name = match object.get("type") {
            Some(Value::String(name)) => name.clone(),
            None if !self.strict => return Some(Component::pending()),
            _ => {
                self.error(&join(path, "type"), format!("expected one of {}", TYPES.join(", ")));
                return None;
            }
        };
        let title = self.text(object, path, "title", false);
        let kind = match type_name.as_str() {
            "card" => self.card(object, path, depth),
            "columns" => {
                Kind::Columns { columns: self.children(object, path, "columns", depth, true) }
            }
            "tabs" => self.tabs(object, path, depth),
            "stack" => {
                Kind::Stack { children: self.children(object, path, "children", depth, true) }
            }
            "callout" => self.callout(object, path),
            "steps" => self.steps(object, path, false),
            "timeline" => self.steps(object, path, true),
            "keyvalue" => self.keyvalue(object, path),
            "badges" => self.badges(object, path),
            "stats" => self.stats(object, path),
            "progress" => self.progress(object, path),
            "chart" => Kind::Chart(self.chart(object, path)),
            "table" => self.table(object, path),
            "files" => self.files(object, path),
            "diff" => Kind::Diff {
                path: self.text(object, path, "path", false),
                text: self.text(object, path, "text", true),
            },
            "diagram" => Kind::Diagram { source: self.text(object, path, "source", true) },
            "actions" => self.actions(object, path),
            "choices" => self.choices(object, path),
            "checklist" => self.checklist(object, path),
            "form" => self.form(object, path),
            "calculator" => self.calculator(object, path),
            other => {
                // A type still arriving is a prefix of a real one, which is not a problem yet.
                if !self.strict && TYPES.iter().any(|known| known.starts_with(other)) {
                    return Some(Component::pending());
                }
                self.error(
                    &join(path, "type"),
                    format!(
                        "\"{other}\" is not a component; the components are {}",
                        TYPES.join(", ")
                    ),
                );
                return None;
            }
        };
        self.unknown_keys(object, path, &type_name);
        Some(Component { title, kind })
    }

    /// Note every key `type_name` does not read.
    fn unknown_keys(&mut self, object: &Map<String, Value>, path: &str, type_name: &str) {
        if !self.strict {
            return;
        }
        let known = super::catalogue::field_names(type_name);
        for key in object.keys() {
            if key != "type" && key != "title" && !known.contains(&key.as_str()) {
                self.note(
                    &join(path, key),
                    format!("{type_name} has no field \"{key}\"; it is ignored"),
                );
            }
        }
    }

    /// A text field, which may be a string or a number. Missing is the empty string.
    fn text(
        &mut self,
        object: &Map<String, Value>,
        path: &str,
        key: &str,
        required: bool,
    ) -> String {
        match object.get(key) {
            None | Some(Value::Null) => {
                if required {
                    self.error(&join(path, key), "is required");
                }
                String::new()
            }
            Some(value) => text_of(value).unwrap_or_else(|| {
                self.error(&join(path, key), "expected text");
                String::new()
            }),
        }
    }

    fn number(&mut self, object: &Map<String, Value>, path: &str, key: &str, fallback: f64) -> f64 {
        match object.get(key) {
            None | Some(Value::Null) => fallback,
            Some(Value::Number(number)) => number.as_f64().unwrap_or(fallback),
            Some(Value::String(text)) => text.trim().parse().unwrap_or_else(|_| {
                self.error(&join(path, key), "expected a number");
                fallback
            }),
            Some(_) => {
                self.error(&join(path, key), "expected a number");
                fallback
            }
        }
    }

    fn flag(&mut self, object: &Map<String, Value>, path: &str, key: &str) -> bool {
        match object.get(key) {
            None | Some(Value::Null) => false,
            Some(Value::Bool(flag)) => *flag,
            Some(_) => {
                self.error(&join(path, key), "expected true or false");
                false
            }
        }
    }

    /// One of `names`, by name.
    fn choice(
        &mut self,
        object: &Map<String, Value>,
        path: &str,
        key: &str,
        names: &[&str],
    ) -> Option<String> {
        let text = self.text(object, path, key, false);
        if text.is_empty() {
            return None;
        }
        if !(names.contains(&text.as_str()) || key == "tone" && text == "error") {
            self.error(&join(path, key), format!("expected one of {}", names.join(", ")));
            return None;
        }
        Some(text)
    }

    /// A list field. A missing list is empty; required means a finished block must have one item.
    fn list<'v>(
        &mut self,
        object: &'v Map<String, Value>,
        path: &str,
        key: &str,
        required: bool,
    ) -> &'v [Value] {
        match object.get(key) {
            Some(Value::Array(items)) => {
                if required && items.is_empty() {
                    self.error(&join(path, key), "must have at least one item");
                }
                items
            }
            None | Some(Value::Null) => {
                if required {
                    self.error(&join(path, key), "is required");
                }
                &[]
            }
            Some(_) => {
                self.error(&join(path, key), "expected a list");
                &[]
            }
        }
    }

    /// The object at one place in a list, or a problem.
    fn item<'v>(&mut self, value: &'v Value, path: &str) -> Option<&'v Map<String, Value>> {
        match value.as_object() {
            Some(object) => Some(object),
            None => {
                self.error(path, "expected an object");
                None
            }
        }
    }

    fn children(
        &mut self,
        object: &Map<String, Value>,
        path: &str,
        key: &str,
        depth: usize,
        required: bool,
    ) -> Vec<Component> {
        let list = self.list(object, path, key, required).to_vec();
        list.iter()
            .enumerate()
            .filter_map(|(at, value)| {
                self.component(value, &index(&join(path, key), at), depth + 1)
            })
            .collect()
    }

    fn card(&mut self, object: &Map<String, Value>, path: &str, depth: usize) -> Kind {
        let children = self.children(object, path, "children", depth, false);
        let text = self.text(object, path, "text", false);
        if self.strict && text.is_empty() && children.is_empty() && object.get("subtitle").is_none()
        {
            self.error(path, "a card needs a text, a subtitle or children");
        }
        Kind::Card {
            subtitle: self.text(object, path, "subtitle", false),
            text,
            badge: self.text(object, path, "badge", false),
            children,
        }
    }

    fn tabs(&mut self, object: &Map<String, Value>, path: &str, depth: usize) -> Kind {
        let mut tabs = Vec::new();
        for (at, value) in self.list(object, path, "tabs", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "tabs"), at);
            let Some(tab) = self.item(value, &here) else { continue };
            tabs.push(Tab {
                label: self.text(tab, &here, "label", true),
                children: self.children(tab, &here, "children", depth, true),
            });
        }
        Kind::Tabs { tabs }
    }

    fn callout(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let tone = self
            .choice(object, path, "tone", Tone::NAMES)
            .and_then(|name| Tone::from_name(&name))
            .unwrap_or(Tone::Info);
        Kind::Callout { tone, text: self.text(object, path, "text", true) }
    }

    fn steps(&mut self, object: &Map<String, Value>, path: &str, timeline: bool) -> Kind {
        let mut items = Vec::new();
        for (at, value) in self.list(object, path, "items", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "items"), at);
            // A plain string is a step with only a title, which is how a model writes a quick list.
            if let Value::String(title) = value {
                items.push(Step {
                    time: String::new(),
                    title: title.clone(),
                    text: String::new(),
                    state: Progress::Todo,
                });
                continue;
            }
            let Some(item) = self.item(value, &here) else { continue };
            let state = match self.choice(item, &here, "state", Progress::NAMES).as_deref() {
                Some("done") => Progress::Done,
                Some("active") => Progress::Active,
                _ => Progress::Todo,
            };
            items.push(Step {
                time: self.text(item, &here, "time", false),
                title: self.text(item, &here, "title", true),
                text: self.text(item, &here, "text", false),
                state,
            });
        }
        Kind::Steps { items, timeline }
    }

    fn keyvalue(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let mut items = Vec::new();
        for (at, value) in self.list(object, path, "items", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "items"), at);
            let Some(item) = self.item(value, &here) else { continue };
            items
                .push((self.text(item, &here, "key", true), self.text(item, &here, "value", true)));
        }
        Kind::KeyValue { items }
    }

    fn tone_of(&mut self, item: &Map<String, Value>, here: &str) -> Tone {
        self.choice(item, here, "tone", Tone::NAMES)
            .and_then(|name| Tone::from_name(&name))
            .unwrap_or_default()
    }

    fn badges(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let mut items = Vec::new();
        for (at, value) in self.list(object, path, "items", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "items"), at);
            if let Value::String(label) = value {
                items.push(Badge { label: label.clone(), tone: Tone::Neutral });
                continue;
            }
            let Some(item) = self.item(value, &here) else { continue };
            let tone = self.tone_of(item, &here);
            items.push(Badge { label: self.text(item, &here, "label", true), tone });
        }
        Kind::Badges { items }
    }

    fn stats(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let mut items = Vec::new();
        for (at, value) in self.list(object, path, "items", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "items"), at);
            let Some(item) = self.item(value, &here) else { continue };
            let delta = self.text(item, &here, "delta", false);
            let trend = match self.choice(item, &here, "trend", &["up", "down", "flat"]).as_deref()
            {
                Some("up") => Trend::Up,
                Some("down") => Trend::Down,
                Some(_) => Trend::Flat,
                // Unsaid, a delta written with its sign says which way it went.
                None if delta.starts_with('+') => Trend::Up,
                None if delta.starts_with('-') || delta.starts_with('\u{2212}') => Trend::Down,
                None => Trend::Flat,
            };
            let up_is_good = match item.get("good") {
                Some(Value::String(way)) => way != "down",
                _ => true,
            };
            items.push(Stat {
                label: self.text(item, &here, "label", true),
                value: self.text(item, &here, "value", true),
                delta,
                trend,
                up_is_good,
                note: self.text(item, &here, "note", false),
                history: self
                    .list(item, &here, "history", false)
                    .iter()
                    .filter_map(Value::as_f64)
                    .filter(|number| number.is_finite())
                    .collect(),
            });
        }
        Kind::Stats { items }
    }

    fn progress(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let mut items = Vec::new();
        for (at, value) in self.list(object, path, "items", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "items"), at);
            let Some(item) = self.item(value, &here) else { continue };
            let max = self.number(item, &here, "max", 100.0);
            items.push(Bar {
                label: self.text(item, &here, "label", true),
                value: self.number(item, &here, "value", 0.0),
                max: if max > 0.0 { max } else { 100.0 },
            });
        }
        Kind::Progress { items }
    }

    fn chart_kind(&mut self, object: &Map<String, Value>, path: &str) -> ChartKind {
        match self.choice(object, path, "kind", ChartKind::NAMES).as_deref() {
            Some("line") => ChartKind::Line,
            Some("area") => ChartKind::Area,
            Some("donut") | Some("pie") => ChartKind::Donut,
            Some("dial") | Some("gauge") => ChartKind::Dial,
            _ => ChartKind::Bar,
        }
    }

    fn chart(&mut self, object: &Map<String, Value>, path: &str) -> Chart {
        let kind = self.chart_kind(object, path);
        let labels: Vec<String> =
            self.list(object, path, "labels", true).iter().filter_map(text_of).collect();
        let mut series = Vec::new();
        for (at, value) in self.list(object, path, "series", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "series"), at);
            let Some(item) = self.item(value, &here) else { continue };
            let mut values = Vec::new();
            for (at, value) in self.list(item, &here, "values", true).to_vec().iter().enumerate() {
                match value.as_f64() {
                    Some(number) if number.is_finite() => values.push(number),
                    _ => self.error(&index(&join(&here, "values"), at), "expected a number"),
                }
            }
            if self.strict && !labels.is_empty() && values.len() != labels.len() {
                self.error(
                    &join(&here, "values"),
                    format!("has {} values for {} labels", values.len(), labels.len()),
                );
            }
            series.push(Series { name: self.text(item, &here, "name", false), values });
        }
        Chart {
            kind,
            labels,
            series,
            unit: self.text(object, path, "unit", false),
            stacked: self.flag(object, path, "stacked"),
        }
    }

    fn table(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let mut columns = Vec::new();
        for (at, value) in self.list(object, path, "columns", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "columns"), at);
            if let Some(label) = text_of(value) {
                columns.push(Column { label, align: Align::Left });
                continue;
            }
            let Some(item) = self.item(value, &here) else { continue };
            let align = match self
                .choice(item, &here, "align", &["left", "right", "center", "centre"])
                .as_deref()
            {
                Some("right") => Align::Right,
                Some("center") | Some("centre") => Align::Centre,
                _ => Align::Left,
            };
            columns.push(Column { label: self.text(item, &here, "label", true), align });
        }
        let mut rows = Vec::new();
        for (at, value) in self.list(object, path, "rows", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "rows"), at);
            match value.as_array() {
                Some(cells) => {
                    if self.strict && cells.len() != columns.len() {
                        self.error(
                            &here,
                            format!("has {} cells for {} columns", cells.len(), columns.len()),
                        );
                    }
                    rows.push(cells.iter().map(|cell| text_of(cell).unwrap_or_default()).collect());
                }
                None => self.error(&here, "expected a list of cells"),
            }
        }
        Kind::Table { columns, rows }
    }

    fn files(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let mut items = Vec::new();
        for (at, value) in self.list(object, path, "items", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "items"), at);
            if let Value::String(file) = value {
                items.push(FileRef { path: file.clone(), line: None, note: String::new() });
                continue;
            }
            let Some(item) = self.item(value, &here) else { continue };
            let line = self.number(item, &here, "line", 0.0);
            items.push(FileRef {
                path: self.text(item, &here, "path", true),
                line: (line >= 1.0).then_some(line as u32),
                note: self.text(item, &here, "note", false),
            });
        }
        Kind::Files { items }
    }

    /// One action off an object that names exactly one of `send`, `fill`, `open` and `copy`.
    fn action(&mut self, item: &Map<String, Value>, here: &str) -> Option<Action> {
        let named: Vec<&str> = ["send", "fill", "open", "copy"]
            .into_iter()
            .filter(|key| item.contains_key(*key))
            .collect();
        if named.len() > 1 {
            self.error(
                here,
                format!("names {} actions; a button does one thing", named.join(" and ")),
            );
        }
        let key = named.first()?;
        let text = self.text(item, here, key, true);
        Some(match *key {
            "send" => Action::Send(text),
            "fill" => Action::Fill(text),
            "copy" => Action::Copy(text),
            _ => {
                let line = self.number(item, here, "line", 0.0);
                Action::Open { path: text, line: (line >= 1.0).then_some(line as u32) }
            }
        })
    }

    fn actions(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let mut items = Vec::new();
        for (at, value) in self.list(object, path, "items", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "items"), at);
            // A plain string is a next question: pressing it sends it.
            if let Value::String(label) = value {
                items.push(Button {
                    label: label.clone(),
                    action: Some(Action::Send(label.clone())),
                    primary: false,
                });
                continue;
            }
            let Some(item) = self.item(value, &here) else { continue };
            let action = self.action(item, &here);
            if self.strict && action.is_none() {
                self.error(&here, "needs one of send, fill, open or copy");
            }
            items.push(Button {
                label: self.text(item, &here, "label", true),
                action,
                primary: self.flag(item, &here, "primary"),
            });
        }
        Kind::Actions { items }
    }

    fn choices(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let options = self.list(object, path, "options", true).iter().filter_map(text_of).collect();
        Kind::Choices { question: self.text(object, path, "question", false), options }
    }

    fn checklist(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let mut items = Vec::new();
        for (at, value) in self.list(object, path, "items", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "items"), at);
            if let Value::String(label) = value {
                items.push(Check { label: label.clone(), done: false });
                continue;
            }
            let Some(item) = self.item(value, &here) else { continue };
            items.push(Check {
                label: self.text(item, &here, "label", true),
                done: self.flag(item, &here, "done"),
            });
        }
        Kind::Checklist { items }
    }

    fn form(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let mut fields = Vec::new();
        for (at, value) in self.list(object, path, "fields", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "fields"), at);
            let Some(item) = self.item(value, &here) else { continue };
            let kind = match self.choice(item, &here, "kind", FieldKind::NAMES).as_deref() {
                Some("number") => FieldKind::Number,
                Some("select") => FieldKind::Select,
                Some("toggle") => FieldKind::Toggle,
                Some("slider") => FieldKind::Slider,
                Some("multiline") => FieldKind::Multiline,
                _ => FieldKind::Text,
            };
            let options: Vec<String> = self
                .list(item, &here, "options", kind == FieldKind::Select)
                .iter()
                .filter_map(text_of)
                .collect();
            let name = self.text(item, &here, "name", true);
            let label = self.text(item, &here, "label", false);
            fields.push(Field {
                label: if label.is_empty() { name.clone() } else { label },
                name,
                kind,
                options,
                value: self.text(item, &here, "value", false),
                min: self.number(item, &here, "min", 0.0),
                max: self.number(item, &here, "max", 100.0),
                step: self.number(item, &here, "step", 1.0),
            });
        }
        let submit = self.text(object, path, "submit", false);
        Kind::Form { fields, submit: if submit.is_empty() { "Send".to_owned() } else { submit } }
    }

    /// An expression field, parsed now so a mistake in it is reported with its path.
    fn expression(&mut self, object: &Map<String, Value>, path: &str, key: &str) -> Option<Expr> {
        let source = match object.get(key) {
            Some(Value::Number(number)) => number.to_string(),
            _ => self.text(object, path, key, true),
        };
        if source.is_empty() {
            return None;
        }
        match Expr::parse(&source) {
            Ok(expr) => Some(expr),
            Err(problem) => {
                self.error(&join(path, key), problem);
                None
            }
        }
    }

    fn calculator(&mut self, object: &Map<String, Value>, path: &str) -> Kind {
        let mut inputs = Vec::new();
        for (at, value) in self.list(object, path, "inputs", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "inputs"), at);
            let Some(item) = self.item(value, &here) else { continue };
            let name = self.text(item, &here, "name", true);
            if !name.is_empty() && !super::expr::is_a_name(&name) {
                self.error(
                    &join(&here, "name"),
                    "a name is letters, digits and _, starting with a letter",
                );
            }
            let label = self.text(item, &here, "label", false);
            let min = self.number(item, &here, "min", 0.0);
            let max = self.number(item, &here, "max", 100.0);
            let value = self.number(item, &here, "value", min);
            inputs.push(Input {
                label: if label.is_empty() { name.clone() } else { label },
                name,
                value,
                min,
                max: if max > min { max } else { min + 1.0 },
                step: self.number(item, &here, "step", 1.0),
                unit: self.text(item, &here, "unit", false),
            });
        }
        let mut outputs = Vec::new();
        for (at, value) in self.list(object, path, "outputs", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "outputs"), at);
            let Some(item) = self.item(value, &here) else { continue };
            let expr = self.expression(item, &here, "expr");
            outputs.push(Output {
                label: self.text(item, &here, "label", true),
                expr,
                unit: self.text(item, &here, "unit", false),
                format: self
                    .choice(item, &here, "format", super::format::FORMATS)
                    .unwrap_or_default(),
            });
        }
        let chart = match object.get("chart") {
            Some(Value::Object(chart)) => self.calc_chart(chart, &join(path, "chart")),
            None | Some(Value::Null) => None,
            Some(_) => {
                self.error(&join(path, "chart"), "expected an object");
                None
            }
        };
        Kind::Calculator { inputs, outputs, chart }
    }

    fn calc_chart(&mut self, object: &Map<String, Value>, path: &str) -> Option<CalcChart> {
        let kind = self.chart_kind(object, path);
        let x = match object.get("x") {
            Some(Value::Object(x)) => x.clone(),
            _ => {
                self.error(&join(path, "x"), "is required: {\"name\", \"from\", \"to\"}");
                return None;
            }
        };
        let here = join(path, "x");
        let name = self.text(&x, &here, "name", true);
        let from = self.expression(&x, &here, "from");
        let to = self.expression(&x, &here, "to");
        let step = match x.get("step") {
            None | Some(Value::Null) => None,
            _ => self.expression(&x, &here, "step"),
        };
        let mut series = Vec::new();
        for (at, value) in self.list(object, path, "series", true).to_vec().iter().enumerate() {
            let here = index(&join(path, "series"), at);
            let Some(item) = self.item(value, &here) else { continue };
            let expr = self.expression(item, &here, "expr");
            series.push(CalcSeries { name: self.text(item, &here, "name", false), expr });
        }
        Some(CalcChart {
            kind,
            x: name,
            from,
            to,
            step,
            series,
            unit: self.text(object, path, "unit", false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_finished_block_names_the_path_to_every_problem() {
        let (_, problems) = Component::read(
            &json!({"type": "chart", "labels": ["a", "b"], "series": [{"name": "x", "values": [1, "two"]}], "colour": "red"}),
            true,
        );
        let lines: Vec<String> = problems.iter().map(Problem::line).collect();
        assert!(lines.iter().any(|l| l == "series[0].values[1]: expected a number"), "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("series[0].values: has 1 values")), "{lines:?}");
        let note =
            problems.iter().find(|p| p.path == "colour").expect("a note about the extra key");
        assert!(!note.is_error(), "an extra key is a note, not an error");
    }

    #[test]
    fn a_block_still_arriving_reads_without_complaint() {
        let (component, problems) =
            Component::read(&json!({"type": "table", "columns": ["a"]}), false);
        assert!(problems.is_empty());
        assert_eq!(component.type_name(), "table");
        let (pending, _) = Component::read(&json!({"title": "x"}), false);
        assert_eq!(pending.kind, Kind::Pending);
        let (prefix, _) = Component::read(&json!({"type": "che"}), false);
        assert_eq!(prefix.kind, Kind::Pending, "a type still being written is not an error yet");
    }

    #[test]
    fn a_type_that_is_not_a_component_says_which_ones_are() {
        let (_, problems) = Component::read(&json!({"type": "map"}), true);
        assert!(problems[0].message.contains("calculator"), "{problems:?}");
    }

    #[test]
    fn plain_strings_are_the_short_forms_of_items() {
        let (component, problems) =
            Component::read(&json!({"type": "actions", "items": ["Show me the test"]}), true);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            component.kind,
            Kind::Actions {
                items: vec![Button {
                    label: "Show me the test".into(),
                    action: Some(Action::Send("Show me the test".into())),
                    primary: false
                }]
            }
        );
    }

    #[test]
    fn a_button_does_one_thing() {
        let (_, problems) = Component::read(
            &json!({"type": "actions", "items": [{"label": "x", "send": "a", "copy": "b"}]}),
            true,
        );
        assert!(problems.iter().any(|p| p.message.contains("send and copy")), "{problems:?}");
    }

    #[test]
    fn nesting_is_bounded() {
        let mut value = json!({"type": "callout", "text": "deep"});
        for _ in 0..8 {
            value = json!({"type": "stack", "children": [value]});
        }
        let (_, problems) = Component::read(&value, true);
        assert!(problems.iter().any(|p| p.message.contains("nested")), "{problems:?}");
    }

    #[test]
    fn a_stat_with_a_signed_delta_knows_which_way_it_went() {
        let (component, _) = Component::read(
            &json!({"type": "stats", "items": [{"label": "a", "value": 3, "delta": "+4%"}, {"label": "b", "value": "1", "delta": "-2"}]}),
            true,
        );
        let Kind::Stats { items } = component.kind else { panic!() };
        assert_eq!(items[0].trend, Trend::Up);
        assert_eq!(items[0].value, "3");
        assert_eq!(items[1].trend, Trend::Down);
    }
}
