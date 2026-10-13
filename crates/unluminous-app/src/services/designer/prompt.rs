//! The words sent to Agent-Chat when a design is handed to the agent: one note, every open note, or a request
//! that is not a note.
//!
//! Every message says which design, which element, what was asked, what the element looks like now, and the
//! two commands the agent answers with: `reply` and `resolve`. The agent is the one in the Agent-Chat pane,
//! so these arrive as an ordinary message and are read back in the conversation like any other.
//!
//! The commands carry `--design <path>` so they work whether or not the design is showing, because an agent
//! is often working while a person has the tab open on something else.

use super::notes::Note;
use super::page::Element;

/// How much of an element's markup goes with a request that asks for it.
const HTML_SENT: usize = 800;

/// The start of every request: which design, and where it is open.
fn heading(design: &str) -> String {
    format!("Design: {design} (open in Unluminous's Designer)\n")
}

/// The element a request is about, as the lines under its heading.
///
/// @param element - what the page said about it
/// @param send_html - whether the markup goes too
fn about_an_element(element: &Element, send_html: bool) -> String {
    let mut out = String::new();
    if !element.excerpt.is_empty() {
        out.push_str(&format!("The element now: \"{}\"\n", element.excerpt));
    }
    if send_html && !element.html.is_empty() {
        let html: String = element.html.chars().take(HTML_SENT).collect();
        out.push_str(&html);
        out.push('\n');
    }
    if !element.styles.is_empty() {
        let computed: Vec<String> = element
            .styles
            .iter()
            .filter(|(_, value)| !value.is_empty())
            .map(|(name, value)| format!("{name} {value}"))
            .collect();
        out.push_str(&format!("Computed: {}\n", computed.join("; ")));
    }
    out
}

/// A note and its thread, indented under the line that names where it is.
fn the_note(note: &Note) -> String {
    let mut out =
        format!("Note {} on {}, selector `{}`:\n", note.id, note.anchor.tag, note.anchor.selector);
    match note.messages.as_slice() {
        [only] => out.push_str(&format!("  \"{}\"\n", only.body)),
        many => {
            for message in many {
                out.push_str(&format!("  {}: {}\n", message.author, message.body));
            }
        }
    }
    out
}

/// The lines that tell the agent how to answer a note.
fn how_to_answer(design: &str, id: u32) -> String {
    format!(
        "  unluminous-cli plugins run designer reply --design {design} {id} <what you changed>\n\
         \x20 unluminous-cli plugins run designer resolve --design {design} {id}\n"
    )
}

/// The request for one note.
///
/// @param design - the design's path, relative to the project
/// @param note - the note
/// @param element - what the page last said about the note's element, when it said
/// @param send_html - whether the element's markup goes too
pub fn for_a_note(design: &str, note: &Note, element: Option<&Element>, send_html: bool) -> String {
    let mut out = heading(design);
    out.push_str(&the_note(note));
    if let Some(element) = element {
        out.push_str(&about_an_element(element, send_html));
    } else if !note.anchor.excerpt.is_empty() {
        out.push_str(&format!("The element: \"{}\"\n", note.anchor.excerpt));
    }
    out.push_str("\nChange the design file to do what the note asks. Then reply to the note and resolve it:\n");
    out.push_str(&how_to_answer(design, note.id));
    out
}

/// The request for every open note in one message.
///
/// @param design - the design's path, relative to the project
/// @param notes - each open note, with what the page last said about its element
/// @param send_html - whether each element's markup goes too
pub fn for_every_open_note(
    design: &str,
    notes: &[(&Note, Option<&Element>)],
    send_html: bool,
) -> String {
    let mut out = heading(design);
    out.push_str(&format!("{} open notes:\n\n", notes.len()));
    for (note, element) in notes {
        out.push_str(&the_note(note));
        if let Some(element) = element {
            out.push_str(&about_an_element(element, send_html));
        }
        out.push('\n');
    }
    out.push_str(
        "Change the design file to do what the notes ask. Then reply to each note once and resolve it, \
         using its number:\n",
    );
    out.push_str(&format!(
        "  unluminous-cli plugins run designer reply --design {design} <note> <what you changed>\n\
         \x20 unluminous-cli plugins run designer resolve --design {design} <note>\n"
    ));
    out
}

/// A request that is not a note: about the chosen element when there is one, and about the whole design when
/// there is not.
///
/// @param design - the design's path, relative to the project
/// @param element - the chosen element
/// @param words - what the person asked
/// @param send_html - whether the element's markup goes too
pub fn free(design: &str, element: Option<&Element>, words: &str, send_html: bool) -> String {
    let mut out = heading(design);
    match element {
        Some(element) => {
            out.push_str(&format!(
                "About the element {}, selector `{}`.\n",
                element.tag, element.selector
            ));
            out.push_str(&about_an_element(element, send_html));
        }
        None => out.push_str("This is about the whole design.\n"),
    }
    out.push_str(&format!("\nRequest: {}\n", words.trim()));
    out.push_str("\nChange the design file to do this. The open notes on it can be read with:\n");
    out.push_str(&format!(
        "  unluminous-cli plugins run designer comments --design {design} open\n"
    ));
    out
}
