//! The one list of components: what each is for, its fields, and a complete example.
//!
//! Everything an agent is told about the library is made from this list, so nothing can describe a
//! component that does not exist or leave out one that does: the guide put in front of a
//! conversation ([`guide`]), the full reference `plugins run agent-chat components` prints
//! ([`reference`], [`describe`]), the fields `validate` checks a block against ([`field_names`]), and
//! the gallery conversation that shows every component at once ([`gallery`]).
//!
//! **Every example is a test.** Each must read with no problem at all, and must only grow as it is
//! cut short at every byte, which is the property progressive drawing rests on.

use super::component::Component;

/// One field of a component.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub name: &'static str,
    /// What it holds, written the way a person reads a signature: `text`, `number`, `[text]`.
    pub kind: &'static str,
    pub required: bool,
    pub about: &'static str,
}

/// One component.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub name: &'static str,
    pub group: &'static str,
    /// What it is for, in one sentence: the part of the guide that decides whether it gets used.
    pub when: &'static str,
    pub fields: &'static [Field],
    pub example: &'static str,
}

const fn field(name: &'static str, kind: &'static str, about: &'static str) -> Field {
    Field { name, kind, required: false, about }
}

const fn required(name: &'static str, kind: &'static str, about: &'static str) -> Field {
    Field { name, kind, required: true, about }
}

/// The actions a button can take. Written once and named in four places.
const ACTION: &str = "one of send (text sent as the person's next message), fill (text put in the composer), open (a project path, with line), copy (text)";

/// Every component, in the order the guide lists them.
pub const ENTRIES: &[Entry] = &[
    Entry {
        name: "card",
        group: "Layout",
        when: "a titled panel around some text or other components; the default container",
        fields: &[
            field("subtitle", "text", "a quiet line under the title"),
            field("text", "markdown", "the body"),
            field("badge", "text", "a short label at the top right, like a status"),
            field("children", "[component]", "components inside the card"),
        ],
        example: r#"{"type": "card", "title": "unluminous 0.64.5", "subtitle": "Released 2026-10-07", "badge": "latest", "text": "Adds **browser sessions** that survive a restart and pinned elements on browser nodes."}"#,
    },
    Entry {
        name: "columns",
        group: "Layout",
        when: "two to four things compared side by side; they stack when the pane is narrow",
        fields: &[required("columns", "[component]", "one component a column, usually cards")],
        example: r#"{"type": "columns", "columns": [{"type": "card", "title": "ripgrep", "badge": "baseline", "text": "412 ms. Reads every file on every query."}, {"type": "card", "title": "Atrius index", "badge": "recommended", "text": "11 ms. A trigram index kept current by a watcher."}]}"#,
    },
    Entry {
        name: "tabs",
        group: "Layout",
        when: "alternatives the person picks between, such as one answer per platform",
        fields: &[required("tabs", "[{label, children}]", "each tab's label and its components")],
        example: r#"{"type": "tabs", "tabs": [{"label": "Windows", "children": [{"type": "callout", "tone": "info", "text": "Run `pwsh tools/release.ps1` from the main checkout."}]}, {"label": "macOS", "children": [{"type": "callout", "tone": "info", "text": "Run `bash tools/release.sh`."}]}]}"#,
    },
    Entry {
        name: "stack",
        group: "Layout",
        when: "several components kept together under one title, with no card around them",
        fields: &[required("children", "[component]", "the components, one under another")],
        example: r#"{"type": "stack", "children": [{"type": "callout", "tone": "success", "text": "All 483 pictures still match."}, {"type": "badges", "items": ["windows", "macos"]}]}"#,
    },
    Entry {
        name: "callout",
        group: "Explaining",
        when: "one thing the person must not miss: a warning, a result, a tip",
        fields: &[
            field("tone", "info | success | warning | danger | tip", "the colour and the mark; info when left out"),
            required("text", "markdown", "what it says"),
        ],
        example: r#"{"type": "callout", "tone": "warning", "title": "This rewrites history", "text": "The branch was pushed. A force push will be needed, and anyone who pulled it will have to reset."}"#,
    },
    Entry {
        name: "steps",
        group: "Explaining",
        when: "an ordered procedure, or progress through a plan",
        fields: &[required("items", "[{title, text, state}]", "state is done, active or todo; a plain string is a step with only a title")],
        example: r#"{"type": "steps", "title": "Moving the realm sidecar", "items": [{"title": "Read the old keys", "text": "`store::import` keeps them", "state": "done"}, {"title": "Write the new file", "text": "once, on the first save", "state": "active"}, {"title": "Drop the import", "state": "todo"}]}"#,
    },
    Entry {
        name: "timeline",
        group: "Explaining",
        when: "things that happen at times: a schedule, a history, a log of events",
        fields: &[required("items", "[{time, title, text, state}]", "time is any short text such as 14:05 or Tue")],
        example: r#"{"type": "timeline", "title": "Release day", "items": [{"time": "09:00", "title": "Freeze main", "state": "done"}, {"time": "11:30", "title": "Run the window suite", "text": "483 pictures, about 40 minutes", "state": "active"}, {"time": "14:00", "title": "Publish 0.65.0"}]}"#,
    },
    Entry {
        name: "keyvalue",
        group: "Explaining",
        when: "the properties of one thing: a build, a file, a configuration",
        fields: &[required("items", "[{key, value}]", "the rows, in order")],
        example: r#"{"type": "keyvalue", "title": "Build", "items": [{"key": "Version", "value": "0.64.5"}, {"key": "Target", "value": "x86_64-pc-windows-msvc"}, {"key": "Binary", "value": "36.2 MB"}]}"#,
    },
    Entry {
        name: "badges",
        group: "Explaining",
        when: "a few short labels: platforms, statuses, tags",
        fields: &[required("items", "[{label, tone}]", "a plain string is a neutral badge")],
        example: r#"{"type": "badges", "items": [{"label": "windows", "tone": "success"}, {"label": "macos", "tone": "success"}, {"label": "linux", "tone": "warning"}]}"#,
    },
    Entry {
        name: "diagram",
        group: "Explaining",
        when: "how parts connect: a flow, a sequence, a state machine. Mermaid source",
        fields: &[required("source", "text", "Mermaid: flowchart, sequenceDiagram, stateDiagram and the rest")],
        example: r#"{"type": "diagram", "title": "How a component reaches the pane", "source": "flowchart TD\n  A[The agent writes a ui fence] --> B[Repaired while it arrives]\n  B --> C[Read into a component]\n  C --> D[Drawn with rux]"}"#,
    },
    Entry {
        name: "stats",
        group: "Numbers",
        when: "two to six headline numbers, each with how it changed",
        fields: &[required(
            "items",
            "[{label, value, delta, trend, good, note, history}]",
            "trend is up, down or flat (read from a +/- delta when left out); good is up or down, which way is better; up when left out; history is the last few values, oldest first, drawn as a sparkline",
        )],
        example: r#"{"type": "stats", "items": [{"label": "Total", "value": "178 s", "delta": "+39%", "good": "down", "history": [126, 131, 129, 134, 151, 170, 178]}, {"label": "App", "value": "131 s", "delta": "+62%", "good": "down", "history": [81, 84, 83, 88, 104, 122, 131]}, {"label": "Core", "value": "38 s", "delta": "-7%", "good": "down", "history": [41, 41, 40, 40, 39, 38, 38]}]}"#,
    },
    Entry {
        name: "progress",
        group: "Numbers",
        when: "how far along something is: a suite, a migration, a quota",
        fields: &[required("items", "[{label, value, max}]", "max is 100 when left out")],
        example: r#"{"type": "progress", "items": [{"label": "Window suite", "value": 412, "max": 483}, {"label": "Disk C:", "value": 62}]}"#,
    },
    Entry {
        name: "chart",
        group: "Numbers",
        when: "numbers compared across categories or over time",
        fields: &[
            field("kind", "bar | line | area | donut | dial", "bar when left out; donut takes one series, its first value is the one the chart is about; dial takes one series of two values, what it was and what was added; bar with two series draws this one against the last as a marker"),
            required("labels", "[text]", "the categories, or the points along the x axis"),
            required("series", "[{name, values}]", "one list of numbers per series, as long as labels"),
            field("unit", "text", "written after every value, like s or MB; $ goes in front"),
            field("stacked", "true | false", "bars and areas on top of each other"),
        ],
        example: r#"{"type": "chart", "kind": "bar", "title": "Release build time by crate", "unit": "s", "labels": ["core", "app", "cli"], "series": [{"name": "last week", "values": [41, 81, 9]}, {"name": "this week", "values": [38, 131, 9]}]}"#,
    },
    Entry {
        name: "table",
        group: "Numbers",
        when: "rows of records with several columns; sortable by pressing a header",
        fields: &[
            required("columns", "[text] or [{label, align}]", "align is left, right or center; numbers read best right"),
            required("rows", "[[cell]]", "one list of cells a row, as long as columns"),
        ],
        example: r#"{"type": "table", "title": "Slowest test binaries", "columns": ["Binary", {"label": "Tests", "align": "right"}, {"label": "Time", "align": "right"}], "rows": [["plugins_board_and_chat", 142, "61 s"], ["canvas_realm", 97, "48 s"], ["editor_formatting", 64, "22 s"]]}"#,
    },
    Entry {
        name: "files",
        group: "Code",
        when: "places in the project the person should look at; each row opens the file",
        fields: &[required("items", "[{path, line, note}]", "path relative to the project; a plain string is a path")],
        example: r#"{"type": "files", "title": "Where the import runs", "items": [{"path": "crates/unluminous-app/src/services/realm/store.rs", "line": 212, "note": "store::import"}, {"path": "crates/unluminous-app/src/app/realm.rs", "line": 1840, "note": "the caller"}]}"#,
    },
    Entry {
        name: "diff",
        group: "Code",
        when: "a change to a file, shown before it is made or after",
        fields: &[
            field("path", "text", "the file, relative to the project; pressing it opens the file"),
            required("text", "text", "a unified diff: lines starting with +, - or a space, and @@ headers"),
        ],
        example: r#"{"type": "diff", "path": "Cargo.toml", "text": "@@ -84,3 +84,3 @@\n [workspace.dependencies]\n-kurbo = { version = \"0.12\", features = [\"serde\"] }\n+kurbo = \"0.12\"\n rux = { git = \"https://github.com/Black-Rainbow-Labs/black-rainbow-labs-rux.git\" }"}"#,
    },
    Entry {
        name: "actions",
        group: "Acting",
        when: "what the person can do next; end an answer with two or three when there are obvious next steps",
        fields: &[required(
            "items",
            "[{label, send | fill | open | copy, line, primary}]",
            "each button does exactly one of the four; a plain string is a next question that is sent when pressed; primary marks the one most people want",
        )],
        example: r#"{"type": "actions", "items": [{"label": "Make the change", "send": "Remove the serde feature from kurbo and rebuild", "primary": true}, {"label": "Open Cargo.toml", "open": "Cargo.toml", "line": 84}, "Show the dependency tree"]}"#,
    },
    Entry {
        name: "choices",
        group: "Acting",
        when: "a question with a few short answers; pressing one sends it",
        fields: &[
            field("question", "text", "the question, when the title is not it"),
            required("options", "[text]", "two to six answers"),
        ],
        example: r#"{"type": "choices", "title": "Which branch should this go on?", "options": ["task-2211", "main", "A new branch"]}"#,
    },
    Entry {
        name: "checklist",
        group: "Acting",
        when: "things the person will tick off themselves; the ticks are kept in the pane",
        fields: &[required("items", "[{label, done}]", "a plain string is an item not yet done")],
        example: r#"{"type": "checklist", "title": "Before the release", "items": [{"label": "Commit the task's own work", "done": true}, "Run the window suite with --no-fail-fast", "Copy notarize.env into the worktree"]}"#,
    },
    Entry {
        name: "form",
        group: "Acting",
        when: "several values you need from the person at once; submitting sends them as a message",
        fields: &[
            required(
                "fields",
                "[{name, label, kind, options, value, min, max, step}]",
                "kind is text, number, select, toggle, slider or multiline; options for select",
            ),
            field("submit", "text", "the button's label; Send when left out"),
        ],
        example: r#"{"type": "form", "title": "New run configuration", "submit": "Create it", "fields": [{"name": "name", "label": "Name", "value": "Dev server"}, {"name": "command", "label": "Command", "value": "npm run dev"}, {"name": "keep", "label": "Keep it after this session", "kind": "toggle", "value": true}]}"#,
    },
    Entry {
        name: "calculator",
        group: "Acting",
        when: "a small tool for the question: inputs the person moves and numbers worked out from them, optionally charted",
        fields: &[
            required("inputs", "[{name, label, value, min, max, step, unit}]", "each is a slider; name is what the expressions call it"),
            required("outputs", "[{label, expr, unit, format}]", "expr is arithmetic over the inputs; format is number, integer, money, percent or compact"),
            field("chart", "{kind, x: {name, from, to, step}, series: [{name, expr}], unit}", "each series evaluated at every x from from to to; from and to may be expressions"),
        ],
        example: r#"{"type": "calculator", "title": "Savings", "inputs": [{"name": "monthly", "label": "Each month", "value": 300, "min": 0, "max": 2000, "step": 50, "unit": "$"}, {"name": "rate", "label": "Yearly return", "value": 5, "min": 0, "max": 12, "step": 0.5, "unit": "%"}, {"name": "years", "label": "Years", "value": 20, "min": 1, "max": 40}], "outputs": [{"label": "After the last year", "expr": "fv(rate/100/12, years*12, monthly)", "format": "money", "unit": "$"}, {"label": "Paid in", "expr": "monthly*12*years", "format": "money", "unit": "$"}], "chart": {"kind": "area", "x": {"name": "y", "from": 0, "to": "years"}, "series": [{"name": "balance", "expr": "fv(rate/100/12, y*12, monthly)"}], "unit": "$"}}"#,
    },
];

/// The fields `type_name` reads, besides `type` and `title`.
pub fn field_names(type_name: &str) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = ENTRIES
        .iter()
        .find(|entry| entry.name == type_name)
        .map(|entry| entry.fields.iter().map(|field| field.name).collect())
        .unwrap_or_default();
    // A chart's own fields, read by `calc_chart` and the stats' `good`, are inside lists and objects
    // and so are never checked as top level keys. The top level ones are all above.
    names.sort_unstable();
    names
}

/// The entry for one component.
pub fn entry(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.name == name)
}

/// One component written out in full: what it is for, its fields and its example.
pub fn describe(name: &str) -> Option<String> {
    let entry = entry(name)?;
    let mut out = format!("{} ({}): {}\n\nFields:\n", entry.name, entry.group, entry.when);
    out.push_str("  type      \"");
    out.push_str(entry.name);
    out.push_str("\" (required)\n  title     text, shown at the top\n");
    for field in entry.fields {
        out.push_str(&format!(
            "  {:<9} {}{}: {}\n",
            field.name,
            field.kind,
            if field.required { " (required)" } else { "" },
            field.about
        ));
    }
    out.push_str("\nExample:\n```ui\n");
    out.push_str(entry.example);
    out.push_str("\n```\n");
    Some(out)
}

/// Every component written out in full.
pub fn reference() -> String {
    let mut out = String::from(FORMAT);
    out.push_str("\n\n");
    for entry in ENTRIES {
        out.push_str(&describe(entry.name).unwrap_or_default());
        out.push('\n');
    }
    out.push_str(RULES);
    out
}

/// How a component is written, in the words every reference starts with.
const FORMAT: &str = "Components are written in your answer as a fenced code block whose language is `ui`, holding one JSON object with a \"type\". Markdown before, between and after them is drawn as usual. A component appears while you are still writing it, so put the fields that matter most first. Every component takes an optional \"title\".";

/// The judgement the OpenAI page describes its model learning, written as instructions.
const RULES: &str = "How to choose:
1. Text is the default. Use a component when it makes the answer faster to read or lets the person act on it, and never for decoration.
2. One component that answers the question beats several that dress it up.
3. Numbers that are compared want a chart, stats or a table; an ordered procedure wants steps; events at times want a timeline; a decision wants choices; places in the code want files; a proposed edit wants a diff; next steps want actions.
4. Every value must be real: read it, measure it or compute it. Never invent data to fill a chart.
5. Buttons only send a message, fill the composer, open a file or copy text. They cannot run anything, so to do something when pressed, have the button send a message asking for it.
6. If you can run commands, check a component you have not used before: unluminous-cli plugins run agent-chat validate '<json>'. The full reference is unluminous-cli plugins run agent-chat components [name].";

/// The compact reference put in front of a conversation: one line a component, the rules, and two
/// examples. About fourteen hundred tokens.
pub fn guide() -> String {
    let mut out = String::from(
        "Your answers are drawn in Unluminous's chat pane, which can draw interactive components as well as markdown. ",
    );
    out.push_str(FORMAT);
    out.push_str("\n\nThe components, with their fields (* is required):\n");
    for entry in ENTRIES {
        let fields: Vec<String> = entry
            .fields
            .iter()
            .map(|field| {
                format!("{}{}: {}", field.name, if field.required { "*" } else { "" }, field.kind)
            })
            .collect();
        out.push_str(&format!("- {}: {}. {}\n", entry.name, entry.when, fields.join("; ")));
    }
    out.push_str(&format!("Button actions are {ACTION}.\n\n"));
    out.push_str(RULES);
    out.push_str("\n\nTwo examples:\n```ui\n");
    out.push_str(entry("chart").map(|e| e.example).unwrap_or_default());
    out.push_str("\n```\n```ui\n");
    out.push_str(entry("actions").map(|e| e.example).unwrap_or_default());
    out.push_str("\n```");
    out
}

/// A conversation that shows every component, as pairs of question and answer.
///
/// This is the example set: `plugins run agent-chat gallery` opens it, and the screenshot tests draw
/// it. Each answer is what a good answer to its question looks like, so it doubles as a demonstration
/// of the rules above as much as of the components.
pub fn gallery() -> Vec<(&'static str, String)> {
    let block =
        |name: &str| format!("```ui\n{}\n```", entry(name).map(|e| e.example).unwrap_or_default());
    vec![
        (
            "Why did the release build get slower this week?",
            format!(
                "Three crates got slower, and **unluminous-app** is most of it: the new `rux` dependency pulled in a second copy of `kurbo` with its `serde` feature on.\n\n{}\n\n{}\n\nTurning that feature off should take most of it back.\n\n{}",
                block("stats"),
                block("chart"),
                block("actions")
            ),
        ),
        (
            "Where does the release build's time go?",
            concat!(
                "This week's 178 seconds against last week's 128:

",
                "```ui
{\"type\": \"chart\", \"kind\": \"dial\", \"title\": \"Release build\", \"unit\": \"s\", \"labels\": [\"Last week\", \"Added this week\"], \"series\": [{\"name\": \"seconds\", \"values\": [128, 50]}]}
```

",
                "Most of it is the app crate:

",
                "```ui
{\"type\": \"chart\", \"kind\": \"donut\", \"title\": \"Where the time goes\", \"unit\": \"s\", \"labels\": [\"unluminous-app\", \"unluminous-core\", \"unluminous-cli\"], \"series\": [{\"name\": \"seconds\", \"values\": [131, 38, 9]}]}
```

",
                "It rose when rux was added:

",
                "```ui
{\"type\": \"chart\", \"kind\": \"area\", \"title\": \"Release build · 14 days\", \"unit\": \"s\", \"labels\": [\"26 Sep\", \"27 Sep\", \"28 Sep\", \"29 Sep\", \"30 Sep\", \"1 Oct\", \"2 Oct\", \"3 Oct\", \"4 Oct\", \"5 Oct\", \"6 Oct\", \"7 Oct\", \"8 Oct\", \"9 Oct\"], \"series\": [{\"name\": \"seconds\", \"values\": [119, 121, 118, 124, 122, 126, 125, 131, 129, 134, 151, 170, 176, 178]}]}
```"
            )
            .to_owned(),
        ),
        (
            "Is it safe to rebase task-2211 onto main now?",
            format!("{}\n\n{}\n\n{}", block("callout"), block("choices"), block("diff")),
        ),
        (
            "What's left before the release?",
            format!("{}\n\n{}\n\n{}", block("checklist"), block("progress"), block("timeline")),
        ),
        (
            "Walk me through moving the realm sidecar.",
            format!("{}\n\n{}\n\n{}", block("steps"), block("files"), block("diagram")),
        ),
        (
            "Compare the search engines we could use for Find in Files.",
            format!("{}\n\n{}", block("columns"), block("table")),
        ),
        (
            "Tell me about the build we just shipped.",
            format!("{}\n\n{}\n\n{}\n\n{}", block("card"), block("keyvalue"), block("badges"), block("stack")),
        ),
        ("How do I cut a release on each machine?", block("tabs")),
        ("Set up a run configuration for the dev server.", block("form")),
        (
            "If I put 300 a month away, what does it come to?",
            format!("It depends on the return and how long you leave it. Move the sliders:\n\n{}", block("calculator")),
        ),
    ]
}

/// Every type the gallery shows, which a test holds against [`TYPES`].
pub fn gallery_types() -> Vec<String> {
    let mut out = Vec::new();
    for (_, answer) in gallery() {
        for segment in super::segments(&answer) {
            if let super::Segment::Block { source, finished } = segment {
                if let Ok(component) = super::read_block(&source, finished) {
                    collect_types(&component, &mut out);
                }
            }
        }
    }
    out
}

fn collect_types(component: &Component, out: &mut Vec<String>) {
    use super::component::Kind;
    let name = component.type_name().to_owned();
    if !out.contains(&name) {
        out.push(name);
    }
    let children: Vec<&Component> = match &component.kind {
        Kind::Card { children, .. } | Kind::Stack { children } => children.iter().collect(),
        Kind::Columns { columns } => columns.iter().collect(),
        Kind::Tabs { tabs } => tabs.iter().flat_map(|tab| tab.children.iter()).collect(),
        _ => Vec::new(),
    };
    for child in children {
        collect_types(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich::component::TYPES;
    use crate::rich::{read_block, repair::repair, validate};

    #[test]
    fn the_catalogue_lists_every_type_and_nothing_else() {
        let names: Vec<&str> = ENTRIES.iter().map(|entry| entry.name).collect();
        assert_eq!(names, TYPES);
    }

    #[test]
    fn every_example_reads_with_no_problem_at_all() {
        for entry in ENTRIES {
            let problems = validate(entry.example);
            assert!(problems.is_empty(), "{}: {:#?}", entry.name, problems);
            let component = read_block(entry.example, true).expect(entry.name);
            assert_eq!(component.type_name(), entry.name);
        }
    }

    /// The property progressive drawing rests on: cut short at any byte, an example reads, and what it
    /// reads never holds more than what a longer cut holds.
    #[test]
    fn a_component_only_grows_as_its_text_arrives() {
        for entry in ENTRIES {
            let mut largest = 0;
            for cut in 0..=entry.example.len() {
                if !entry.example.is_char_boundary(cut) {
                    continue;
                }
                let prefix = &entry.example[..cut];
                let repaired = repair(prefix);
                if !repaired.is_empty() {
                    serde_json::from_str::<serde_json::Value>(&repaired).unwrap_or_else(|e| {
                        panic!(
                            "{} cut at {cut}: {prefix:?} repaired to {repaired:?}: {e}",
                            entry.name
                        )
                    });
                }
                let component = read_block(prefix, false).expect("a block arriving always reads");
                let size = component.size();
                assert!(
                    size >= largest,
                    "{} shrank from {largest} to {size} at byte {cut}: {prefix:?}",
                    entry.name
                );
                largest = size;
            }
        }
    }

    #[test]
    fn the_guide_names_every_component_and_stays_short() {
        let guide = guide();
        for entry in ENTRIES {
            assert!(guide.contains(&format!("- {}:", entry.name)), "{} missing", entry.name);
        }
        // Roughly four characters a token. The guide goes in front of every conversation, so it is
        // held to a budget rather than allowed to grow with each component.
        assert!(guide.len() < 9_000, "the guide is {} characters", guide.len());
    }

    #[test]
    fn the_gallery_shows_every_component() {
        let shown = gallery_types();
        for name in TYPES {
            assert!(shown.iter().any(|one| one == name), "the gallery does not show {name}");
        }
    }

    #[test]
    fn describing_a_component_gives_its_fields_and_its_example() {
        let text = describe("calculator").unwrap();
        assert!(text.contains("outputs"));
        assert!(text.contains("```ui"));
        assert!(describe("map").is_none());
    }
}
