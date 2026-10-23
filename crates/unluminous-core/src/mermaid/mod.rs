//! Mermaid diagrams, read and laid out.
//!
//! `task-1660` asks that a `.mmd` file open as a drawn diagram and that a ` ```mermaid ` block in a
//! Markdown file be drawn in the preview. `tasks/unluminous-mermaid-plugin-tdd.md` records why that is
//! done here, in Rust, rather than by running Mermaid's own JavaScript: the three alternatives each
//! bring a browser, a DOM or a second rendering engine into a text editor, and what a diagram
//! actually needs on top of what Unluminous already has is arithmetic.
//!
//! So this module is the arithmetic. It reads the source, works out where everything goes, and hands
//! back a [`Scene`] — rectangles, circles, polygons, lines and pieces of text at absolute positions.
//! It has no user interface dependency, it measures through [`FontMetrics`] like the editor's layout
//! does, and its tests run with no window and no fonts.
//!
//! ## What is drawn
//!
//! Every diagram type in Mermaid's own documentation: thirty three of them since `task-2339` added
//! the thirteen that were named but not drawn (C4, architecture, Venn, Ishikawa, Wardley, Cynefin,
//! tree view, event modelling, swimlanes, railroad, ZenUML, agentflow and use case). Three of those
//! are another type's grammar under a different keyword, and are rewritten into that type rather than
//! drawn a second way: ZenUML into a sequence diagram, agentflow into a flowchart, and swimlanes into a
//! flowchart's nodes laid out in lanes. `tasks/task-2339-mermaid-every-type-tdd.md` is the design.
//!
//! [`Kind::NotDrawn`] stays for a type Mermaid adds later: such a keyword is named in a panel rather
//! than quietly mis-parsed as something else and drawn wrongly.
//!
//! ## The bar these are held to
//!
//! Not "identical to `mermaid.js`" — the curves are polylines, the fonts are Unluminous's and the colours
//! are Unluminous's. **Correct and readable**: the right nodes, the right edges in the right direction,
//! the right labels, nothing overlapping, and nothing running off the edge.
//!
//! **Three of those are asserted for every diagram type and the fourth is not**, which `task-1984`
//! found this paragraph claiming otherwise. `check::properties` runs the three that are true of any
//! scene whatever it draws: nothing outside the scene, every number finite, every label present.
//! Overlap is `check::no_overlapping_nodes`, asked for by the types that have a notion of a node box
//! at all — a pie chart's slices overlap the circle they are in by construction and a Gantt bar
//! overlaps its row, so a shared assertion would have to be switched off by more types than it was
//! switched on by. A type added later inherits the three and decides about the fourth.
//!
//! ## Nothing is fetched, and nothing is run
//!
//! There is no path through any of this that opens a socket or executes anything a document
//! contains. `click`, `href` and callbacks are read and ignored. That is a property of the design
//! rather than of a setting.

pub mod scene;
pub mod source;
pub mod text;
pub mod theme;

pub mod layered;
pub mod parts;
pub mod shapes;

#[cfg(test)]
pub mod check;

pub mod block;
pub mod class;
pub mod er;
pub mod flowchart;
pub mod gantt;
pub mod gitgraph;
pub mod journey;
pub mod kanban;
pub mod mindmap;
pub mod packet;
pub mod pie;
pub mod quadrant;
pub mod radar;
pub mod requirement;
pub mod sankey;
pub mod sequence;
pub mod state;
pub mod timeline;
pub mod treemap;
pub mod xychart;

pub mod agentflow;
pub mod architecture;
pub mod c4;
pub mod cynefin;
pub mod eventmodeling;
pub mod ishikawa;
pub mod railroad;
pub mod swimlane;
pub mod treeview;
pub mod usecase;
pub mod venn;
pub mod wardley;
pub mod zenuml;

use crate::metrics::FontMetrics;
use crate::style::CharStyle;

pub use scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Size, Stroke, TextStyle};
pub use source::Source;
pub use theme::Theme;

/// A source with more lines than this is refused rather than laid out.
///
/// Not because the arithmetic would be wrong, but because a preview is worked out again on every
/// keystroke and a diagram nobody could read is not worth stopping the window for. Ten thousand
/// lines is far past any diagram a person writes and far short of anything that takes a noticeable
/// time.
const LINE_LIMIT: usize = 10_000;

/// Which diagram a source is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Flowchart,
    Sequence,
    Class,
    State,
    Er,
    Requirement,
    Pie,
    Gantt,
    Journey,
    GitGraph,
    Mindmap,
    Timeline,
    Quadrant,
    XyChart,
    Sankey,
    Block,
    Packet,
    Kanban,
    Radar,
    Treemap,
    C4,
    Architecture,
    Venn,
    Ishikawa,
    Wardley,
    Cynefin,
    TreeView,
    EventModeling,
    Swimlane,
    Railroad,
    ZenUml,
    Agentflow,
    UseCase,
    /// A Mermaid diagram type this version of Unluminous does not draw. It carries the name to say so
    /// with, which is the whole reason it is a variant rather than an error.
    NotDrawn(&'static str),
}

impl Kind {
    /// What this kind is called when it has to be named to a reader.
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Flowchart => "flowchart",
            Kind::Sequence => "sequence",
            Kind::Class => "class",
            Kind::State => "state",
            Kind::Er => "entity relationship",
            Kind::Requirement => "requirement",
            Kind::Pie => "pie",
            Kind::Gantt => "gantt",
            Kind::Journey => "user journey",
            Kind::GitGraph => "git graph",
            Kind::Mindmap => "mindmap",
            Kind::Timeline => "timeline",
            Kind::Quadrant => "quadrant",
            Kind::XyChart => "xy chart",
            Kind::Sankey => "sankey",
            Kind::Block => "block",
            Kind::Packet => "packet",
            Kind::Kanban => "kanban",
            Kind::Radar => "radar",
            Kind::Treemap => "treemap",
            Kind::C4 => "C4",
            Kind::Architecture => "architecture",
            Kind::Venn => "Venn",
            Kind::Ishikawa => "Ishikawa",
            Kind::Wardley => "Wardley",
            Kind::Cynefin => "Cynefin",
            Kind::TreeView => "tree view",
            Kind::EventModeling => "event modelling",
            Kind::Swimlane => "swimlane",
            Kind::Railroad => "railroad",
            Kind::ZenUml => "ZenUML",
            Kind::Agentflow => "agentflow",
            Kind::UseCase => "use case",
            Kind::NotDrawn(name) => name,
        }
    }

    /// True when Unluminous can draw this one.
    pub fn is_drawn(&self) -> bool {
        !matches!(self, Kind::NotDrawn(_))
    }
}

/// The diagram a keyword names.
///
/// Matched without regard to case. Mermaid's own keywords are camel case, but somebody typing
/// `sequencediagram` means the same thing and there is nothing to be gained by refusing it.
pub fn kind_of(keyword: &str) -> Option<Kind> {
    let word = keyword.trim().to_ascii_lowercase();
    let kind = match word.as_str() {
        "flowchart" | "flowchart-v2" | "graph" => Kind::Flowchart,
        "sequencediagram" => Kind::Sequence,
        "classdiagram" | "classdiagram-v2" => Kind::Class,
        "statediagram" | "statediagram-v2" => Kind::State,
        "erdiagram" => Kind::Er,
        "requirementdiagram" | "requirement" => Kind::Requirement,
        "pie" | "piechart" => Kind::Pie,
        "gantt" => Kind::Gantt,
        "journey" | "userjourney" => Kind::Journey,
        "gitgraph" => Kind::GitGraph,
        "mindmap" => Kind::Mindmap,
        "timeline" => Kind::Timeline,
        "quadrantchart" | "quadrant" => Kind::Quadrant,
        "xychart" | "xychart-beta" => Kind::XyChart,
        "sankey" | "sankey-beta" => Kind::Sankey,
        "block" | "block-beta" => Kind::Block,
        "packet" | "packet-beta" => Kind::Packet,
        "kanban" => Kind::Kanban,
        "radar" | "radar-beta" => Kind::Radar,
        "treemap" | "treemap-beta" => Kind::Treemap,
        "c4context" | "c4container" | "c4component" | "c4dynamic" | "c4deployment"
        | "c4diagram" => Kind::C4,
        "architecture" | "architecture-beta" => Kind::Architecture,
        "venn" | "venn-beta" => Kind::Venn,
        "ishikawa" | "ishikawa-beta" | "fishbone" => Kind::Ishikawa,
        "wardley" | "wardley-beta" => Kind::Wardley,
        "cynefin" | "cynefin-beta" => Kind::Cynefin,
        "treeview" | "treeview-beta" => Kind::TreeView,
        "eventmodeling" | "eventmodelling" => Kind::EventModeling,
        "swimlane" | "swimlane-beta" | "swimlanes" => Kind::Swimlane,
        "railroad" | "railroad-beta" | "railroad-ebnf-beta" | "railroad-abnf-beta"
        | "railroad-peg-beta" => Kind::Railroad,
        "zenuml" => Kind::ZenUml,
        "agentflow" | "agentflow-beta" => Kind::Agentflow,
        "usecase" | "usecase-beta" => Kind::UseCase,
        _ => return None,
    };
    Some(kind)
}

/// Why a diagram could not be drawn.
///
/// Carries the line it went wrong on, and the line itself, because Mermaid's own error box says only
/// that there was a syntax error and knowing *which line* is what a person writing one wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The line in the source, counting from one. `None` when the trouble is with the whole thing.
    pub line: Option<usize>,
    /// The line itself, so the panel can show it without the caller going back to the source.
    pub text: String,
    pub reason: String,
    /// True when this is a diagram type Unluminous does not draw yet, rather than a fault in the source.
    /// The two are shown differently: one is the author's business and the other is Unluminous's.
    pub unsupported: bool,
}

impl Problem {
    /// A fault in the source, at a line.
    pub fn at(line: &source::Line, reason: impl Into<String>) -> Self {
        Self {
            line: Some(line.number),
            text: line.text.clone(),
            reason: reason.into(),
            unsupported: false,
        }
    }

    /// A fault with the whole source rather than with one line of it.
    pub fn whole(reason: impl Into<String>) -> Self {
        Self { line: None, text: String::new(), reason: reason.into(), unsupported: false }
    }

    /// A diagram type this version does not draw.
    pub fn not_drawn(name: &str) -> Self {
        Self {
            line: None,
            text: String::new(),
            reason: format!("Unluminous does not draw a {name} diagram yet."),
            unsupported: true,
        }
    }

    /// The whole message, with the line number in front of it when there is one.
    pub fn message(&self) -> String {
        match self.line {
            Some(number) => format!("Line {number}: {}", self.reason),
            None => self.reason.clone(),
        }
    }
}

/// What a renderer is given: a way to measure, a font to measure in, and the colours.
///
/// There is deliberately **no width**. Every diagram works out its own natural size and whoever
/// draws it fits that to the pane, which is what `services::picture` already does with a picture.
/// Passing a width in would mean a diagram that reflowed as a splitter moved and a screenshot test
/// that depended on the size of the window.
pub struct Options<'a> {
    pub metrics: &'a dyn FontMetrics,
    /// The family and the size ordinary label text is set in. Everything else is worked out from it,
    /// so a diagram follows the editor's font exactly as the Markdown preview does.
    pub base: CharStyle,
    pub theme: Theme,
}

impl<'a> Options<'a> {
    /// The usual options for `metrics`: Unluminous's palette, and a plain fourteen point face.
    pub fn new(metrics: &'a dyn FontMetrics) -> Self {
        Self {
            metrics,
            base: CharStyle { size: 14.0, ..CharStyle::default() },
            theme: Theme::default(),
        }
    }

    /// A style like the base one, at a size and a weight.
    pub fn style(&self, scale: f32, bold: bool) -> CharStyle {
        CharStyle { size: self.base.size * scale, bold, ..self.base.clone() }
    }
}

/// Read `text` and work out where everything in it goes.
///
/// The one way in. Everything else in this module is reached through here, so a caller never has to
/// know which of twenty parsers a source belongs to.
pub fn render(text: &str, options: &Options) -> Result<Scene, Problem> {
    let source = Source::read(text).ok_or_else(|| {
        Problem::whole(
            "There is no diagram here: the first line should name one, such as `flowchart TD`.",
        )
    })?;
    if source.lines.len() > LINE_LIMIT {
        return Err(Problem::whole(format!(
            "This diagram has more than {LINE_LIMIT} lines, which is more than Unluminous draws."
        )));
    }
    let Some(kind) = kind_of(&source.keyword) else {
        return Err(Problem::whole(format!(
            "`{}` does not name a Mermaid diagram. The first line should be something like `flowchart TD` or `sequenceDiagram`.",
            source.keyword
        )));
    };
    match kind {
        Kind::Flowchart => flowchart::render(&source, options),
        Kind::Sequence => sequence::render(&source, options),
        Kind::Class => class::render(&source, options),
        Kind::State => state::render(&source, options),
        Kind::Er => er::render(&source, options),
        Kind::Requirement => requirement::render(&source, options),
        Kind::Pie => pie::render(&source, options),
        Kind::Gantt => gantt::render(&source, options),
        Kind::Journey => journey::render(&source, options),
        Kind::GitGraph => gitgraph::render(&source, options),
        Kind::Mindmap => mindmap::render(&source, options),
        Kind::Timeline => timeline::render(&source, options),
        Kind::Quadrant => quadrant::render(&source, options),
        Kind::XyChart => xychart::render(&source, options),
        Kind::Sankey => sankey::render(&source, options),
        Kind::Block => block::render(&source, options),
        Kind::Packet => packet::render(&source, options),
        Kind::Kanban => kanban::render(&source, options),
        Kind::Radar => radar::render(&source, options),
        Kind::Treemap => treemap::render(&source, options),
        Kind::C4 => c4::render(&source, options),
        Kind::Architecture => architecture::render(&source, options),
        Kind::Venn => venn::render(&source, options),
        Kind::Ishikawa => ishikawa::render(&source, options),
        Kind::Wardley => wardley::render(&source, options),
        Kind::Cynefin => cynefin::render(&source, options),
        Kind::TreeView => treeview::render(&source, options),
        Kind::EventModeling => eventmodeling::render(&source, options),
        Kind::Swimlane => swimlane::render(&source, options),
        Kind::Railroad => railroad::render(&source, options),
        Kind::ZenUml => zenuml::render(&source, options),
        Kind::Agentflow => agentflow::render(&source, options),
        Kind::UseCase => usecase::render(&source, options),
        Kind::NotDrawn(name) => Err(Problem::not_drawn(name)),
    }
}

/// Which diagram `text` holds, without laying any of it out.
///
/// Cheap: it reads as far as the first real line and stops. The window asks this to decide whether a
/// fence is a diagram at all before it goes to the trouble of drawing one.
pub fn kind(text: &str) -> Option<Kind> {
    Source::read(text).and_then(|source| kind_of(&source.keyword))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::FixedMetrics;

    #[test]
    fn every_keyword_mermaid_ships_is_recognised() {
        // The whole list from Mermaid's own navigation, so a type that is added to Unluminous later shows
        // up here as a change from NotDrawn rather than as a new line.
        let all = [
            "flowchart",
            "graph",
            "sequenceDiagram",
            "classDiagram",
            "stateDiagram",
            "stateDiagram-v2",
            "erDiagram",
            "journey",
            "gantt",
            "pie",
            "quadrantChart",
            "requirementDiagram",
            "gitGraph",
            "c4Diagram",
            "mindmap",
            "timeline",
            "zenuml",
            "sankey-beta",
            "xychart-beta",
            "block-beta",
            "packet-beta",
            "kanban",
            "architecture",
            "radar-beta",
            "eventModeling",
            "treemap",
            "venn",
            "ishikawa",
            "wardley",
            "cynefin",
            "treeView",
            "swimlanes",
            "C4Context",
            "C4Container",
            "C4Component",
            "C4Dynamic",
            "C4Deployment",
            "architecture-beta",
            "venn-beta",
            "ishikawa-beta",
            "wardley-beta",
            "cynefin-beta",
            "treeView-beta",
            "swimlane-beta",
            "railroad-beta",
            "railroad-ebnf-beta",
            "railroad-abnf-beta",
            "railroad-peg-beta",
            "agentflow-beta",
            "usecase-beta",
        ];
        for keyword in all {
            assert!(kind_of(keyword).is_some(), "{keyword} should be recognised");
        }
    }

    #[test]
    fn every_type_mermaid_documents_is_drawn() {
        // `task-2339`: the thirteen that used to be named in a panel are drawn now, so nothing in
        // Mermaid's own list is left over.
        for keyword in [
            "c4Diagram",
            "C4Context",
            "zenuml",
            "architecture-beta",
            "swimlane-beta",
            "eventModeling",
            "venn-beta",
            "ishikawa-beta",
            "wardley-beta",
            "cynefin-beta",
            "treeView-beta",
            "railroad-ebnf-beta",
            "agentflow-beta",
            "usecase-beta",
        ] {
            let kind = kind_of(keyword).expect("recognised");
            assert!(kind.is_drawn(), "{keyword} is drawn");
            assert!(!kind.name().is_empty(), "{keyword} has a name to say");
        }
    }

    #[test]
    fn a_keyword_that_is_not_mermaid_at_all_is_nothing() {
        assert_eq!(kind_of("fn"), None);
        assert_eq!(kind_of("# heading"), None);
    }

    #[test]
    fn the_case_of_a_keyword_does_not_matter() {
        assert_eq!(kind_of("SequenceDiagram"), Some(Kind::Sequence));
        assert_eq!(kind_of("sequencediagram"), Some(Kind::Sequence));
        assert_eq!(kind_of("FlowChart"), Some(Kind::Flowchart));
    }

    #[test]
    fn a_source_with_no_diagram_in_it_says_so_rather_than_drawing_nothing() {
        let metrics = FixedMetrics::default();
        let problem = render("", &Options::new(&metrics)).expect_err("nothing to draw");
        assert!(problem.reason.contains("no diagram"));
        assert!(!problem.unsupported, "an empty source is not an unsupported type");
    }

    #[test]
    fn a_type_unluminous_does_not_draw_is_a_problem_that_names_it() {
        // No keyword reaches this today; the problem is still the one a type Mermaid adds later is
        // given, so it is checked directly.
        let problem = Problem::not_drawn("Wardley");
        assert!(problem.unsupported, "this is Unluminous's limitation, not the author's fault");
        assert!(problem.reason.contains("Wardley"), "it says which type: {}", problem.reason);
    }

    #[test]
    fn an_unrecognised_first_word_is_told_what_one_looks_like() {
        let metrics = FixedMetrics::default();
        let problem = render("banana split\n", &Options::new(&metrics)).expect_err("not a diagram");
        assert!(problem.reason.contains("banana"), "it quotes what was written");
        assert!(problem.reason.contains("flowchart TD"), "it says what one looks like");
        assert!(!problem.unsupported);
    }

    #[test]
    fn a_problem_says_which_line_went_wrong() {
        let line = source::Line { number: 7, text: "A ~~> B".to_owned(), indent: 0 };
        let problem = Problem::at(&line, "that is not a link");
        assert_eq!(problem.message(), "Line 7: that is not a link");
        assert_eq!(problem.text, "A ~~> B");
    }
}

/// One sample of every diagram type Unluminous draws, and the words each one has to end up saying.
///
/// This is the list the cross-cutting tests below walk, and it is deliberately a constant in the
/// module rather than a local in one test: adding a diagram type means adding a line here, and every
/// property is then asserted for it without anybody having to remember to do so.
#[cfg(test)]
pub const SAMPLES: &[(&str, &str, &[&str])] = &[
    (
        "flowchart",
        "flowchart TD\n Start([Begin]) --> Check{Ready?}\n Check -->|yes| Ship[Ship it]\n Check -->|no| Fix[Fix it]\n Fix --> Check\n",
        &["Begin", "Ready?", "Ship it", "yes"],
    ),
    (
        "sequence",
        "sequenceDiagram\n actor Alice\n participant Bob\n Alice ->>+ Bob: Ask\n Bob -->>- Alice: Answer\n Note over Alice,Bob: done\n",
        &["Alice", "Bob", "Ask", "Answer"],
    ),
    (
        "class",
        "classDiagram\n class Animal {\n <<abstract>>\n +String name\n +move()\n }\n Animal <|-- Dog\n Animal <|-- Cat\n",
        &["Animal", "Dog", "Cat", "+String name"],
    ),
    (
        "state",
        "stateDiagram-v2\n [*] --> Idle\n Idle --> Running : start\n Running --> [*]\n",
        &["Idle", "Running", "start"],
    ),
    (
        "er",
        "erDiagram\n CUSTOMER ||--o{ ORDER : places\n ORDER {\n int number PK\n date placed\n }\n",
        &["CUSTOMER", "ORDER", "places", "number"],
    ),
    (
        "requirement",
        "requirementDiagram\n requirement top {\n id: 1\n text: it works\n }\n element test {\n type: simulation\n }\n test - verifies -> top\n",
        &["top", "test", "verifies"],
    ),
    ("pie", "pie title Pets\n \"Dogs\" : 386\n \"Cats\" : 85\n", &["Pets", "Dogs", "Cats"]),
    (
        "gantt",
        "gantt\n title Plan\n dateFormat YYYY-MM-DD\n section Design\n Sketch : d1, 2024-01-01, 5d\n Review : after d1, 3d\n",
        &["Plan", "Sketch", "Review"],
    ),
    (
        "journey",
        "journey\n title My day\n section Morning\n Wake up: 3: Me\n Make tea: 5: Me, Cat\n",
        &["My day", "Wake up", "Make tea", "Cat"],
    ),
    (
        "gitGraph",
        "gitGraph\n commit id: \"one\"\n branch develop\n commit id: \"two\"\n checkout main\n merge develop tag: \"v1\"\n",
        &["main", "develop", "one", "two", "v1"],
    ),
    (
        "mindmap",
        "mindmap\nroot((Unluminous))\n  Editing\n    Undo\n  Panes\n    Terminal\n",
        &["Unluminous", "Editing", "Undo", "Panes", "Terminal"],
    ),
    (
        "timeline",
        "timeline\n title History\n section Early\n 2002 : LinkedIn\n 2004 : Facebook : Google\n",
        &["History", "Early", "2002", "LinkedIn", "Google"],
    ),
    (
        "quadrantChart",
        "quadrantChart\n title Reach\n x-axis Low --> High\n y-axis Few --> Many\n quadrant-1 Expand\n A: [0.3, 0.6]\n",
        &["Reach", "Expand", "A"],
    ),
    (
        "xychart",
        "xychart-beta\n title Sales\n x-axis [jan, feb, mar]\n y-axis \"Revenue\" 0 --> 100\n bar [30, 45, 60]\n line [20, 35, 50]\n",
        &["Sales", "Revenue", "jan", "mar"],
    ),
    (
        "sankey",
        "sankey-beta\n Coal,Electricity,25\n Gas,Electricity,15\n Electricity,Homes,30\n",
        &["Coal", "Gas", "Electricity", "Homes"],
    ),
    (
        "block",
        "block-beta\n columns 3\n a[\"Front\"]:3\n block:services\n  api[\"API\"]\n end\n space\n db[(\"Store\")]\n a --> db\n",
        &["Front", "API", "Store"],
    ),
    (
        "packet",
        "packet-beta\n title TCP\n 0-15: \"Source Port\"\n 16-31: \"Destination Port\"\n 32-63: \"Sequence Number\"\n",
        &["TCP", "Source Port", "Sequence Number"],
    ),
    (
        "kanban",
        "kanban\n todo[Todo]\n  a[Write it]@{ assigned: \"Jason\", priority: \"High\" }\n doing[Doing]\n  b[Test it]\n",
        &["Todo", "Doing", "Write it", "Jason", "Test it"],
    ),
    (
        "radar",
        "radar-beta\n title Scores\n axis Speed, Accuracy, Quality\n curve A[\"Team A\"]{4, 3, 5}\n max 5\n",
        &["Scores", "Speed", "Quality", "Team A"],
    ),
    (
        "treemap",
        "treemap-beta\n\"Editing\"\n  \"Undo\": 30\n  \"Search\": 12\n\"Panes\": 22\n",
        &["Editing", "Undo", "Panes"],
    ),
    (
        "C4",
        "C4Container\n Person(customer, \"Customer\", \"Uses the bank\")\n Container_Boundary(c1, \"Internet Banking\") {\n  Container(spa, \"Single-Page App\", \"Angular\", \"The banking pages\")\n  ContainerDb(db, \"Database\", \"SQL\")\n }\n Rel(customer, spa, \"Uses\", \"HTTPS\")\n Rel(spa, db, \"Reads\")\n",
        &["Customer", "Internet Banking", "Single-Page App", "[Angular]", "Database", "Uses", "[HTTPS]"],
    ),
    (
        "architecture",
        "architecture-beta\n group api(cloud)[API]\n service db(database)[Database] in api\n service server(server)[Server] in api\n db:L -- R:server\n",
        &["API", "Database", "Server"],
    ),
    (
        "venn",
        "venn-beta\n title Overlap\n set A[\"Alpha\"]\n set B[\"Beta\"]\n union A,B[\"Both\"]\n",
        &["Overlap", "Alpha", "Beta", "Both"],
    ),
    (
        "ishikawa",
        "ishikawa-beta\n Blurry Photo\n Process\n  Out of focus\n User\n  Shaky hands\n",
        &["Blurry Photo", "Process", "Out of focus", "Shaky hands"],
    ),
    (
        "wardley",
        "wardley-beta\n title Tea\n anchor Business [0.95, 0.63]\n component Cup of Tea [0.79, 0.61]\n component Kettle [0.43, 0.35]\n Business -> Cup of Tea\n Cup of Tea -> Kettle\n evolve Kettle 0.62\n",
        &["Tea", "Business", "Cup of Tea", "Kettle"],
    ),
    (
        "cynefin",
        "cynefin-beta\n title Response\n complex\n  \"Investigate\"\n clear\n  \"Restart service\"\n complex --> complicated : \"Pattern\"\n",
        &["Response", "Complex", "Investigate", "Restart service", "Pattern"],
    ),
    (
        "treeView",
        "treeView-beta\n src/\n  index.js ## entry point\n package.json\n",
        &["src/", "index.js", "entry point", "package.json"],
    ),
    (
        "eventmodeling",
        "eventmodeling\n tf 01 ui CartUI\n tf 02 cmd AddItem { description: string }\n tf 03 evt ItemAdded\n",
        &["CartUI", "AddItem", "ItemAdded"],
    ),
    (
        "swimlane",
        "swimlane-beta LR\n subgraph Customer\n  order[Place order]\n end\n subgraph Store\n  pack[Pack items]\n end\n order -->|paid| pack\n",
        &["Customer", "Store", "Place order", "Pack items", "paid"],
    ),
    (
        "railroad",
        "railroad-ebnf-beta\n title Numbers\n number = digit+ ;\n digit = \"0\" | \"1\" ;\n",
        &["Numbers", "number", "digit", "0", "1"],
    ),
    (
        "zenuml",
        "zenuml\n title Demo\n Alice->Bob: Hello\n Bob.answer() {\n  return fine\n }\n",
        &["Demo", "Alice", "Bob", "Hello", "answer()", "fine"],
    ),
    (
        "agentflow",
        "agentflow-beta TB\n flow writer[\"Writer\"]\n  draft[\"Draft\"]@{ shape: task }\n  tool[\"spell_check\"]@{ shape: tool }\n  draft --> tool\n end\n",
        &["Writer", "Draft", "spell_check"],
    ),
    (
        "usecase",
        "usecase-beta\n actor Customer\n systemBoundary Shop\n  Checkout(\"Check out\")\n end\n Customer --> Checkout\n",
        &["Customer", "Shop", "Check out"],
    ),
];

#[cfg(test)]
mod every_type {
    use super::*;
    use crate::metrics::FixedMetrics;

    fn options() -> Options<'static> {
        Options::new(Box::leak(Box::new(FixedMetrics::default())))
    }

    #[test]
    fn there_is_a_sample_for_every_type_unluminous_draws() {
        // The guard on the whole of the rest of this module: a diagram type added to `kind_of`
        // without a sample here would otherwise be tested by nothing at all.
        let drawn: Vec<&'static str> = [
            "flowchart",
            "sequenceDiagram",
            "classDiagram",
            "stateDiagram-v2",
            "erDiagram",
            "requirementDiagram",
            "pie",
            "gantt",
            "journey",
            "gitGraph",
            "mindmap",
            "timeline",
            "quadrantChart",
            "xychart-beta",
            "sankey-beta",
            "block-beta",
            "packet-beta",
            "kanban",
            "radar-beta",
            "treemap-beta",
            "C4Container",
            "architecture-beta",
            "venn-beta",
            "ishikawa-beta",
            "wardley-beta",
            "cynefin-beta",
            "treeView-beta",
            "eventmodeling",
            "swimlane-beta",
            "railroad-ebnf-beta",
            "zenuml",
            "agentflow-beta",
            "usecase-beta",
        ]
        .into_iter()
        .filter(|keyword| kind_of(keyword).is_some_and(|kind| kind.is_drawn()))
        .collect();
        assert_eq!(drawn.len(), 33, "thirty three types are drawn");
        assert_eq!(SAMPLES.len(), 33, "and every one of them has a sample");
    }

    #[test]
    fn every_diagram_type_keeps_every_property() {
        for (name, source, wanted) in SAMPLES {
            let scene = match render(source, &options()) {
                Ok(scene) => scene,
                Err(problem) => panic!("{name} should draw: {}", problem.message()),
            };
            check::properties(&scene, wanted);
            assert!(
                scene.size.width > 0.0 && scene.size.height > 0.0,
                "{name} produced a scene with no size"
            );
            assert!(!scene.items.is_empty(), "{name} drew nothing");
        }
    }

    #[test]
    fn every_diagram_type_lays_out_the_same_way_twice() {
        // Everything the screenshot tests do rests on this one.
        for (name, source, _) in SAMPLES {
            let first = render(source, &options());
            let second = render(source, &options());
            assert!(first == second, "{name} gave two different pictures for one source");
        }
    }

    #[test]
    fn every_diagram_type_survives_being_cut_off_part_way_through() {
        // Which is what a preview sees on nearly every keystroke while one is being typed. Any of
        // these may refuse to draw; none of them may panic, and none may hand back a scene with a
        // number in it that is not a number.
        for (name, source, _) in SAMPLES {
            for length in 0..source.len() {
                if !source.is_char_boundary(length) {
                    continue;
                }
                if let Ok(scene) = render(&source[..length], &options()) {
                    assert!(
                        scene.size.width.is_finite() && scene.size.height.is_finite(),
                        "{name} cut off at {length} gave a size that is not a number"
                    );
                }
            }
            // And a line at a time, which is what a paste looks like.
            let lines: Vec<&str> = source.lines().collect();
            for count in 0..lines.len() {
                let _ = render(&lines[..count].join("\n"), &options());
            }
        }
    }
}
