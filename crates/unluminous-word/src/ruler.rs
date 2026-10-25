//! What the ruler changes (task-2359, TDD 5.5): a paragraph's left, right and first line indents, and its
//! tab stops, added, moved and taken away. Positions are points from the text column's left edge, as
//! `ParaLook` holds them.
//!
//! Each is one [`Session`] change over the paragraphs the selection covers, as a drag on Word's ruler is.

use crate::props::TabKind;
use crate::session::Session;
use crate::xml::{El, Node};

fn twips(points: f32) -> String {
    ((points * 20.0).round() as i64).to_string()
}

fn kind_name(kind: TabKind) -> &'static str {
    match kind {
        TabKind::Left => "left",
        TabKind::Center => "center",
        TabKind::Right => "right",
        TabKind::Decimal => "decimal",
        TabKind::Bar => "bar",
        TabKind::Clear => "clear",
    }
}

impl Session {
    /// The left and right indents, and the first line's indent from the left one (negative for a hanging
    /// indent), in points.
    pub fn set_indents(&mut self, left: f32, right: f32, first: f32) {
        self.format_paragraphs(move |ppr| {
            let ind = ppr.child_or_insert("w:ind");
            for name in ["w:start", "w:end", "w:firstLine", "w:hanging", "w:startChars", "w:endChars", "w:firstLineChars", "w:hangingChars"] {
                ind.remove_attr(name);
            }
            ind.set_attr("w:left", twips(left.max(-1584.0)));
            ind.set_attr("w:right", twips(right));
            if first > 0.0 {
                ind.set_attr("w:firstLine", twips(first));
            } else if first < 0.0 {
                ind.set_attr("w:hanging", twips(-first));
            }
        });
    }

    /// A tab stop of `kind` at `position` points.
    pub fn add_tab_stop(&mut self, position: f32, kind: TabKind) {
        let pos = twips(position);
        self.format_paragraphs(move |ppr| {
            let tabs = ppr.child_or_insert("w:tabs");
            tabs.children.retain(|n| !matches!(n, Node::El(e) if e.attr("w:pos") == Some(pos.as_str())));
            tabs.children.push(Node::El(El::new("w:tab").with("w:val", kind_name(kind)).with("w:pos", pos.clone())));
            sort_tabs(tabs);
        });
    }

    /// Take away the tab stop at `position` points: the paragraph's own, and one its style gives with a
    /// `clear` stop, as Word writes it.
    pub fn remove_tab_stop(&mut self, position: f32) {
        let from_style = {
            let resolver = crate::styles::Resolver::new(&self.document);
            let head = &self.selection.head;
            crate::position::paragraph(&self.document, &head.story, &head.path).is_some_and(|p| {
                let mut bare = p.props.clone();
                bare.remove_children("w:tabs");
                resolver.paragraph(&bare, None).tabs.iter().any(|t| (t.position - position).abs() < 0.5)
            })
        };
        let pos = twips(position);
        self.format_paragraphs(move |ppr| {
            let tabs = ppr.child_or_insert("w:tabs");
            tabs.children.retain(|n| !matches!(n, Node::El(e) if e.attr("w:pos") == Some(pos.as_str())));
            if from_style {
                tabs.children.push(Node::El(El::new("w:tab").with("w:val", "clear").with("w:pos", pos.clone())));
                sort_tabs(tabs);
            }
            if tabs.children.is_empty() {
                ppr.remove_children("w:tabs");
            }
        });
    }

    /// Move the tab stop at `from` to `to` points, keeping its kind and leader.
    pub fn move_tab_stop(&mut self, from: f32, to: f32) {
        let look = self.paragraph_at_caret();
        let Some(stop) = look.tabs.iter().find(|t| (t.position - from).abs() < 0.5).cloned() else { return };
        let (from_pos, to_pos) = (twips(from), twips(to));
        let leader = stop.leader;
        self.format_paragraphs(move |ppr| {
            let tabs = ppr.child_or_insert("w:tabs");
            tabs.children.retain(|n| !matches!(n, Node::El(e) if e.attr("w:pos") == Some(from_pos.as_str()) || e.attr("w:pos") == Some(to_pos.as_str())));
            let mut tab = El::new("w:tab").with("w:val", kind_name(stop.kind)).with("w:pos", to_pos.clone());
            if let Some(leader) = leader {
                let name = match leader {
                    '.' => "dot",
                    '-' => "hyphen",
                    '_' => "underscore",
                    _ => "middleDot",
                };
                tab.set_attr("w:leader", name);
            }
            tabs.children.push(Node::El(tab));
            sort_tabs(tabs);
        });
    }
}

/// Word writes a paragraph's tab stops in order of position.
fn sort_tabs(tabs: &mut El) {
    let position = |n: &Node| match n {
        Node::El(e) => e.attr("w:pos").and_then(|p| p.parse::<i64>().ok()).unwrap_or(0),
        _ => 0,
    };
    tabs.children.sort_by_key(position);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indents_and_tab_stops_change_from_the_ruler() {
        let mut session = Session::new(crate::new::blank());
        session.type_text("A paragraph");
        session.set_indents(36.0, 18.0, -18.0);
        let look = session.paragraph_at_caret();
        assert_eq!((look.indent_left, look.indent_right, look.indent_first), (36.0, 18.0, -18.0));
        session.add_tab_stop(144.0, TabKind::Right);
        session.add_tab_stop(72.0, TabKind::Left);
        let positions: Vec<f32> = session.paragraph_at_caret().tabs.iter().map(|t| t.position).collect();
        assert_eq!(positions, vec![72.0, 144.0]);
        session.move_tab_stop(144.0, 216.0);
        let look = session.paragraph_at_caret();
        assert_eq!(look.tabs[1].position, 216.0);
        assert_eq!(look.tabs[1].kind, TabKind::Right);
        session.remove_tab_stop(72.0);
        assert_eq!(session.paragraph_at_caret().tabs.len(), 1);
    }
}
