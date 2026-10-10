//! Signature help: which callable the caret is inside the brackets of, and which of its parameters is
//! being typed. `task-2231` §6.8.
//!
//! The language server's answer is used when the file's server answers. Without one, the structural
//! tier reads it: the call's open bracket is found backwards from the caret, the commas at its own
//! depth say which parameter, and the name in front of the bracket is looked up in the tab's own
//! definitions, then the project's. That is a guess where one name has two definitions, and it is the
//! same guess Go to Definition makes there.
//!
//! The line opens when `(` or `,` is typed inside a call and after a callable row is accepted, follows
//! the caret while it stays between the brackets, and closes when the caret leaves them or on `Escape`.
//! Nothing here waits for the server: a frame shows the structural line until the answer arrives.

use std::ops::Range;

use atrius_index::outline::Definition as Shaped;

use super::UnluminousApp;

/// How far back from the caret an open bracket is looked for. A call's arguments longer than this
/// are rare, and a bound keeps the walk off the keystroke budget.
const LOOK_BACK: usize = 4096;

/// A callable's signature as one line, and which parameter is being typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// The whole line: `draw(ui: &mut Ui, area: Rect) -> Response`.
    pub label: String,
    /// Each parameter's bytes within `label`.
    pub parameters: Vec<Range<usize>>,
    /// Which parameter the caret is in, counting from zero.
    pub active: Option<usize>,
    /// The callable's documentation, when it has any.
    pub doc: Option<String>,
    /// True when a language server answered, false when the structural tier read it.
    pub from_server: bool,
}

/// A call the caret is inside the brackets of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The byte of the open bracket.
    pub open: usize,
    /// The callable's name, the identifier in front of the bracket.
    pub name: String,
    /// True when it is reached through a value with `.`, so a method's receiver is not counted.
    pub through_a_value: bool,
    /// Which argument the caret is in, counting the commas at the call's own depth.
    pub argument: usize,
}

/// The call whose brackets hold a byte, read backwards: the nearest `(` the walk does not close again,
/// with an identifier in front of it. `None` outside every call, and for a bracket with no name in
/// front, which is a grouping rather than a call.
///
/// @param text - the text
/// @param caret - the byte
pub fn call_at(text: &str, caret: usize) -> Option<Call> {
    let bytes = text.as_bytes();
    let floor = caret.saturating_sub(LOOK_BACK);
    let mut depth = 0usize;
    let mut commas = 0usize;
    let mut at = caret.min(bytes.len());
    while at > floor {
        at -= 1;
        match bytes[at] {
            b')' | b']' | b'}' => depth += 1,
            b'[' | b'{' if depth == 0 => return None,
            b'(' | b'[' | b'{' if depth > 0 => depth -= 1,
            b'(' => return named_call(text, at, commas),
            b',' if depth == 0 => commas += 1,
            b';' if depth == 0 => return None,
            _ => {}
        }
    }
    None
}

/// The call an open bracket starts, when an identifier stands in front of it.
///
/// @param text - the text
/// @param open - the open bracket's byte
/// @param argument - which argument the caret is in
fn named_call(text: &str, open: usize, argument: usize) -> Option<Call> {
    let before = text[..open].trim_end_matches([' ', '\t']);
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '$')
        .last()
        .map(|(i, _)| i)?;
    let name = &before[start..];
    if name.is_empty() || name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    let through_a_value = before[..start].ends_with('.');
    Some(Call { open, name: name.to_owned(), through_a_value, argument })
}

/// True for a parameter that is the method's receiver rather than one written between its brackets.
///
/// @param name - the parameter's name as the structure read it
fn is_receiver(name: &str) -> bool {
    let bare = name.trim_start_matches('&').trim_start();
    let bare = bare.strip_prefix("mut ").unwrap_or(bare).trim();
    let bare = bare.strip_prefix('\'').map_or(bare, |b| b.split_whitespace().last().unwrap_or(b));
    matches!(bare, "self" | "this" | "cls")
}

/// A definition's signature as one line with each parameter's bytes, the receiver left out.
///
/// @param d - the definition
/// @param argument - which argument the caret is in
pub fn structural_signature(d: &Shaped, argument: usize) -> Signature {
    let mut label = format!("{}(", d.name);
    let mut parameters = Vec::new();
    for parameter in d.parameters.iter().filter(|p| !is_receiver(&p.name)) {
        if !parameters.is_empty() {
            label.push_str(", ");
        }
        let start = label.len();
        label.push_str(&parameter.name);
        if let Some(ty) = &parameter.type_text {
            label.push_str(": ");
            label.push_str(ty);
        }
        parameters.push(start..label.len());
    }
    label.push(')');
    if let Some(returns) = &d.returns {
        label.push_str(" -> ");
        label.push_str(returns);
    }
    let active = (argument < parameters.len()).then_some(argument);
    Signature { label, parameters, active, doc: d.doc.clone(), from_server: false }
}

impl UnluminousApp {
    /// Opens the line when the caret is inside a call's brackets, and asks the server.
    pub(crate) fn open_the_signature(&mut self) {
        let head = self.document().selection().head;
        let text = self.document().text().to_string();
        if call_at(&text, head).is_some() {
            self.signature_open = true;
            self.ask_for_signature_help();
        }
    }

    /// Works the line for a frame: opens it when `(` or `,` was just typed inside a call, closes it once
    /// the caret has left the brackets, and asks the server again when the caret or the text moved.
    ///
    /// @param typed - whether a character reached the document this frame
    pub(crate) fn keep_the_signature_fresh(&mut self, typed: bool) {
        let head = self.document().selection().head;
        if typed && !self.signature_open && head > 0 {
            // `(` and `,` are one byte each, so only a byte before the caret that starts a character
            // can be one; the byte before the caret can be the middle of a letter such as `é`.
            let text = self.document().text();
            let before = match text.is_char_boundary(head - 1) {
                true => text.byte_slice(head - 1..head),
                false => String::new(),
            };
            if matches!(before.as_str(), "(" | ",") && self.completion_applies_here() {
                self.open_the_signature();
            }
            return;
        }
        if !self.signature_open {
            return;
        }
        let text = self.document().text().to_string();
        if call_at(&text, head).is_none() {
            self.signature_open = false;
            return;
        }
        self.ask_for_signature_help();
    }

    /// True while the signature line is open.
    pub fn signature_is_open(&self) -> bool {
        self.signature_open
    }

    /// Closes the line, which is what `Escape` does while no completion list is open.
    pub(crate) fn close_the_signature(&mut self) {
        self.signature_open = false;
    }

    /// The signature the caret is inside, when the line is open: the server's answer when it has one
    /// for this place, otherwise the structural reading.
    pub(crate) fn signature_now(&mut self) -> Option<Signature> {
        if !self.signature_open {
            return None;
        }
        self.signature_at_the_caret()
    }

    /// The signature the caret is inside, whether or not the line is open. What `editor signature`
    /// answers and what the line draws.
    pub fn signature_at_the_caret(&mut self) -> Option<Signature> {
        if let Some((true, Some(help))) = self.server_signature() {
            return Some(Signature {
                label: help.label,
                parameters: help.parameters,
                active: help.active,
                doc: help.doc,
                from_server: true,
            });
        }
        let head = self.document().selection().head;
        let text = self.document().text().to_string();
        let call = call_at(&text, head)?;
        let found = self.callable_named(&call.name)?;
        Some(structural_signature(&found, call.argument))
    }

    /// The callable a name in front of a bracket means: the tab's own definition, else the project's.
    ///
    /// @param name - the name
    fn callable_named(&mut self, name: &str) -> Option<Shaped> {
        let index = self.files.active_index();
        let own = self.tab_structure(index).and_then(|structure| {
            structure
                .definitions
                .iter()
                .find(|d| d.name == name && d.symbol_kind.is_callable())
                .cloned()
        });
        if own.is_some() {
            return own;
        }
        let symbols = self.project_symbols.as_ref()?;
        let lower = name.to_lowercase();
        symbols
            .read(|table| {
                table
                    .named(&lower)
                    .iter()
                    .find(|d| d.definition.name == name && d.definition.symbol_kind.is_callable())
                    .map(|d| d.definition.clone())
            })
            .flatten()
    }

    /// Draws the line above the caret, when it is open and the pane recorded where the caret is.
    ///
    /// @param ui - the window's `Ui`
    pub(crate) fn show_the_signature(&mut self, ui: &mut egui::Ui) {
        let Some(anchor) = self.caret_anchor else { return };
        let Some(signature) = self.signature_now() else { return };
        crate::components::signature::show(ui, &signature, anchor.caret, anchor.pane);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_call_is_read_backwards_with_the_argument_the_caret_is_in() {
        let text = "let x = draw(ui, area, ";
        let call = call_at(text, text.len()).unwrap();
        assert_eq!(call.name, "draw");
        assert_eq!(call.argument, 2);
        assert!(!call.through_a_value);
    }

    #[test]
    fn a_nested_call_is_skipped_and_a_method_is_through_a_value() {
        let text = "self.paint(rect(a, b), ";
        let call = call_at(text, text.len()).unwrap();
        assert_eq!(call.name, "paint");
        assert_eq!(call.argument, 1);
        assert!(call.through_a_value);
    }

    #[test]
    fn outside_every_call_there_is_none() {
        assert_eq!(call_at("let x = (a + b", 14), None);
        assert_eq!(call_at("draw(a); b", 10), None);
        assert_eq!(call_at("let v = [a, ", 12), None);
    }

    #[test]
    fn a_methods_receiver_is_left_out_of_the_line() {
        assert!(is_receiver("&self"));
        assert!(is_receiver("&mut self"));
        assert!(is_receiver("&'a self"));
        assert!(!is_receiver("selfish"));
        assert!(!is_receiver("mut x"));
    }
}
