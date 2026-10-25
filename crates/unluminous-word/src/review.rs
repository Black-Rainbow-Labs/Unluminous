//! Review (task-2359, TDD 5.4 C30 to C32, M17): formatting changes tracked as `w:rPrChange` and
//! `w:pPrChange`, the next and previous change, Lock Tracking with Word's password hash, the word count
//! with notes and text boxes, the language of the selection, the accessibility checker and the
//! Document Inspector.

use std::sync::Arc;

use sha2::{Digest, Sha512};

use crate::model::*;
use crate::position::{self, Pos, Selection, StoryId};
use crate::props;
use crate::session::{now_iso, Scope, Session};
use crate::xml::{El, Node};

/// How tracked changes are shown, Word's Display for Review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Markup {
    /// Changed lines marked with a bar in the margin, the text shown as it will be.
    Simple,
    /// Insertions underlined and deletions struck through, as now.
    #[default]
    All,
    /// The text as it will be, with no marks.
    None,
    /// The text as it was before the changes.
    Original,
}

impl Markup {
    pub fn parse(name: &str) -> Option<Markup> {
        Some(match name {
            "simple" => Markup::Simple,
            "all" => Markup::All,
            "none" | "final" | "no-markup" => Markup::None,
            "original" => Markup::Original,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Markup::Simple => "simple",
            Markup::All => "all",
            Markup::None => "none",
            Markup::Original => "original",
        }
    }
}

/// One problem the accessibility checker found: what, and where to go to fix it.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub kind: &'static str,
    pub message: String,
    pub at: Pos,
}

/// What the Document Inspector found, by kind, with how many of each.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inspection {
    pub comments: usize,
    pub revisions: usize,
    pub hidden: usize,
    /// The names in the document's properties: author, last saved by, and the like.
    pub personal: Vec<(String, String)>,
}

impl Session {
    /// Record, before a tracked formatting change, what the runs the selection covers looked like, as a
    /// `w:rPrChange` inside each changed run's properties. A run that already has one keeps it, since it
    /// holds the formatting from before the first change.
    pub fn track_run_formatting(document: &mut Document, from: &Pos, to: &Pos, author: &str) {
        let date = now_iso();
        for (path, length, _) in crate::edit::covered(document, from, to) {
            let start = if path == from.path { from.offset } else { 0 };
            let end = if path == to.path { to.offset } else { length };
            let id = document.next_id();
            let Some(p) = position::paragraph_mut(document, &from.story, &path) else { continue };
            let first = p.split_at(start);
            let last = p.split_at(end);
            for run in &mut p.runs[first..last] {
                if run.props.child("w:rPrChange").is_some()
                    || !matches!(run.content, Content::Text(_))
                {
                    continue;
                }
                let mut before = (*run.props).clone();
                before.name = "w:rPr".into();
                let change = El::new("w:rPrChange")
                    .with("w:id", id.to_string())
                    .with("w:author", author.to_owned())
                    .with("w:date", date.clone())
                    .with_child(before);
                let mut props = (*run.props).clone();
                props.children.push(Node::El(change));
                crate::order::sort(&mut props);
                run.props = Arc::new(props);
            }
        }
    }

    /// Record, before a tracked paragraph formatting change, each covered paragraph's properties as a
    /// `w:pPrChange`.
    pub fn track_paragraph_formatting(document: &mut Document, from: &Pos, to: &Pos, author: &str) {
        let date = now_iso();
        for (path, _, _) in crate::edit::covered(document, from, to) {
            let id = document.next_id();
            let Some(p) = position::paragraph_mut(document, &from.story, &path) else { continue };
            if p.props.child("w:pPrChange").is_some() {
                continue;
            }
            let mut before = p.props.clone();
            before.remove_children("w:rPr");
            before.remove_children("w:sectPr");
            let change = El::new("w:pPrChange")
                .with("w:id", id.to_string())
                .with("w:author", author.to_owned())
                .with("w:date", date.clone())
                .with_child(before);
            p.props.children.push(Node::El(change));
            crate::order::sort(&mut p.props);
        }
    }

    /// Move the selection to the next (or previous) tracked change after the caret, wrapping round:
    /// an insertion, a deletion, or a formatting change. Answers whether there was one.
    pub fn go_to_change(&mut self, forward: bool) -> bool {
        let mut changes: Vec<(Pos, Pos)> = Vec::new();
        for (path, p) in position::paragraphs(&self.document.body) {
            let mut at = 0;
            let mut open: Option<(usize, u64)> = None;
            for run in &p.runs {
                let length = run.len();
                let wrap = run
                    .wraps
                    .iter()
                    .find(|w| {
                        matches!(w.el.name.as_str(), "w:ins" | "w:del" | "w:moveFrom" | "w:moveTo")
                    })
                    .map(|w| w.id);
                let changed = wrap.or_else(|| run.props.child("w:rPrChange").map(|_| u64::MAX));
                match (open, changed) {
                    (Some((_, a)), Some(b)) if a == b => {}
                    (Some((start, _)), next) => {
                        changes.push((Pos::new(path.clone(), start), Pos::new(path.clone(), at)));
                        open = next.map(|id| (at, id));
                    }
                    (None, Some(id)) => open = Some((at, id)),
                    (None, None) => {}
                }
                at += length;
            }
            if let Some((start, _)) = open {
                changes.push((Pos::new(path.clone(), start), Pos::new(path.clone(), at)));
            }
            if p.props.child("w:pPrChange").is_some()
                && !changes.iter().any(|(a, _)| a.path == path)
            {
                changes.push((Pos::new(path.clone(), 0), Pos::new(path.clone(), p.len())));
            }
        }
        if changes.is_empty() {
            return false;
        }
        let caret = self.selection.ordered().0.clone();
        let chosen = if forward {
            changes.iter().find(|(a, _)| *a > caret).or(changes.first())
        } else {
            changes.iter().rev().find(|(a, _)| *a < caret).or(changes.last())
        };
        if let Some((a, b)) = chosen.cloned() {
            self.select(a, b);
        }
        true
    }

    /// Lock Track Changes on with a password, as Word's Lock Tracking does: `w:documentProtection
    /// w:edit="trackedChanges"` with the SHA-512 hash Word writes (the password as UTF-16, a random salt,
    /// 100,000 rounds). An empty password unlocks.
    pub fn lock_tracking(&mut self, password: &str) {
        let protection =
            (!password.is_empty()).then(|| protection_element("trackedChanges", password));
        let unlocking = protection.is_none();
        self.permit = true;
        self.change(Scope::Whole, move |document, selection, _| {
            if let Some(settings) = document.settings.as_mut() {
                settings.root.remove_children("w:documentProtection");
                if let Some(el) = &protection {
                    settings
                        .root
                        .children
                        .insert(1.min(settings.root.children.len()), Node::El(el.clone()));
                    props::set_on_off(&mut settings.root, "w:trackRevisions", Some(true));
                }
                crate::page_setup::sort_settings(&mut settings.root);
                settings.dirty = true;
            }
            selection.clone()
        });
        self.permit = false;
        if !unlocking {
            self.tracking = true;
        }
        self.protection = (!unlocking).then(|| "trackedChanges".to_owned());
    }

    /// Restrict Editing: let this document be changed only as `kind` says (`readOnly`, `comments`,
    /// `trackedChanges` or `forms`), until `password` stops it, as Word's Restrict Editing pane does.
    pub fn restrict_editing(&mut self, kind: &str, password: &str) -> Result<(), String> {
        if !matches!(kind, "readOnly" | "comments" | "trackedChanges" | "forms") {
            return Err(format!("Restrict Editing is readOnly, comments, trackedChanges or forms, not {kind}"));
        }
        let protection = protection_element(kind, password);
        self.permit = true;
        let tracking = kind == "trackedChanges";
        self.change(Scope::Whole, move |document, selection, _| {
            if let Some(settings) = document.settings.as_mut() {
                settings.root.remove_children("w:documentProtection");
                settings.root.children.push(Node::El(protection.clone()));
                if tracking {
                    props::set_on_off(&mut settings.root, "w:trackRevisions", Some(true));
                }
                crate::page_setup::sort_settings(&mut settings.root);
                settings.dirty = true;
            }
            selection.clone()
        });
        self.permit = false;
        self.protection = Some(kind.to_owned());
        if tracking {
            self.tracking = true;
        }
        Ok(())
    }

    /// Stop Restrict Editing with its password.
    pub fn stop_protection(&mut self, password: &str) -> Result<(), String> {
        if self.protection.is_none() {
            return Err("this document is not protected".into());
        }
        if !self.protection_password_matches(password) {
            return Err("the password is not right".into());
        }
        self.permit = true;
        self.change(Scope::Whole, |document, selection, _| {
            if let Some(settings) = document.settings.as_mut() {
                settings.root.remove_children("w:documentProtection");
                settings.dirty = true;
            }
            selection.clone()
        });
        self.permit = false;
        self.protection = None;
        Ok(())
    }

    /// Password to Modify: a person who opens the document without `password` can read it and not
    /// change it. An empty password takes it away.
    pub fn set_password_to_modify(&mut self, password: &str) {
        let element = (!password.is_empty()).then(|| {
            let mut el = protection_element("readOnly", password);
            el.name = "w:writeProtection".into();
            el.remove_attr("w:edit");
            el.remove_attr("w:enforcement");
            el
        });
        self.permit = true;
        self.change(Scope::Whole, move |document, selection, _| {
            if let Some(settings) = document.settings.as_mut() {
                settings.root.remove_children("w:writeProtection");
                if let Some(el) = &element {
                    settings.root.children.push(Node::El(el.clone()));
                }
                crate::page_setup::sort_settings(&mut settings.root);
                settings.dirty = true;
            }
            selection.clone()
        });
        self.permit = false;
    }

    /// Open a document that has a password to modify for changing, with that password.
    pub fn unlock_for_editing(&mut self, password: &str) -> Result<(), String> {
        let Some(w) = self.document.settings.as_ref().and_then(|s| s.root.child("w:writeProtection")) else {
            self.read_only = false;
            return Ok(());
        };
        let (Some(salt), Some(hash)) = (w.attr("w:salt").or(w.attr("w:saltValue")), w.attr("w:hash").or(w.attr("w:hashValue"))) else {
            self.read_only = false;
            return Ok(());
        };
        let spins = w.attr("w:cryptSpinCount").or(w.attr("w:spinCount")).and_then(|v| v.parse().ok()).unwrap_or(100_000);
        let salt = base64_decode(salt).ok_or("the document's password is not readable")?;
        if base64(&password_hash(password, &salt, spins)) != hash {
            return Err("the password is not right".into());
        }
        self.read_only = false;
        Ok(())
    }

    /// Whether Lock Tracking is on, so tracking cannot be turned off without the password.
    pub fn tracking_locked(&self) -> bool {
        self.document
            .settings
            .as_ref()
            .and_then(|s| s.root.child("w:documentProtection"))
            .is_some_and(|p| {
                p.attr("w:edit") == Some("trackedChanges")
                    && props::on_off_attr(p.attr("w:enforcement"))
            })
    }

    /// Whether `password` is the one a document's protection was set with.
    pub fn protection_password_matches(&self, password: &str) -> bool {
        let Some(p) =
            self.document.settings.as_ref().and_then(|s| s.root.child("w:documentProtection"))
        else {
            return true;
        };
        let (Some(salt), Some(hash)) = (
            p.attr("w:salt").or(p.attr("w:saltValue")),
            p.attr("w:hash").or(p.attr("w:hashValue")),
        ) else {
            return true;
        };
        let spins = p
            .attr("w:cryptSpinCount")
            .or(p.attr("w:spinCount"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(100_000);
        let Some(salt) = base64_decode(salt) else { return false };
        base64(&password_hash(password, &salt, spins)) == hash
    }

    /// Words, characters and paragraphs counting the footnotes, endnotes and text boxes too, as Word's
    /// Include textboxes, footnotes and endnotes box does.
    pub fn word_count_with_notes(&self) -> (usize, usize) {
        let mut words = 0;
        let mut characters = 0;
        let mut stories = vec![StoryId::Body];
        for (part, make) in [
            (&self.document.footnotes, StoryId::Footnote as fn(i64) -> StoryId),
            (&self.document.endnotes, StoryId::Endnote),
        ] {
            if let Some(part) = part {
                stories
                    .extend(part.notes.iter().filter_map(Story::id).filter(|id| *id > 0).map(make));
            }
        }
        stories.extend(self.document.textboxes.iter().map(|t| StoryId::TextBox(t.id)));
        for story in stories {
            let Some(blocks) = position::story(&self.document, &story) else { continue };
            for (_, p) in position::paragraphs(blocks) {
                let text = crate::session::clean_text(&p.visible_text());
                words += text.split_whitespace().count();
                characters += text.chars().filter(|c| !c.is_whitespace()).count();
            }
        }
        (words, characters)
    }

    /// Mark the selection as written in a language (`en-US`, `fr-FR`, `de-DE`), `w:lang`. Spelling
    /// checks only English (United States), so text in another language is left alone.
    /// The document's language as a tag, such as `en-US`: the default run properties' `w:lang`.
    pub fn language_tag(&self) -> String {
        self.document
            .styles
            .as_ref()
            .and_then(|s| s.root.child("w:docDefaults")?.child("w:rPrDefault")?.child("w:rPr")?.child("w:lang")?.attr("w:val").map(str::to_owned))
            .unwrap_or_else(|| "en-US".into())
    }

    /// The language of the text at the caret, as the status area names it: the run's own `w:lang`, its
    /// style's, or the document's default, with the common tags spelled out.
    pub fn language_at_caret(&self) -> String {
        let head = &self.selection.head;
        let resolver = crate::styles::Resolver::new(&self.document);
        let mut tag: Option<String> = None;
        if let Some(paragraph) = crate::position::paragraph(&self.document, &head.story, &head.path) {
            let (props, _) = crate::edit::props_at(paragraph, head.offset);
            tag = props.child("w:lang").and_then(|l| l.attr("w:val")).map(str::to_owned);
            if tag.is_none() {
                if let Some(style) = resolver.paragraph_style_id(&paragraph.props) {
                    tag = resolver
                        .chain(style)
                        .into_iter()
                        .rev()
                        .find_map(|s| s.child("w:rPr")?.child("w:lang")?.attr("w:val").map(str::to_owned));
                }
            }
        }
        let tag = tag.or_else(|| {
            let styles = self.document.styles.as_ref()?;
            let defaults = styles.root.child("w:docDefaults")?.child("w:rPrDefault")?.child("w:rPr")?;
            defaults.child("w:lang")?.attr("w:val").map(str::to_owned)
        });
        let tag = tag.unwrap_or_else(|| "en-US".into());
        match tag.as_str() {
            "en-US" => "English (United States)".into(),
            "en-GB" => "English (United Kingdom)".into(),
            "en-CA" => "English (Canada)".into(),
            "en-AU" => "English (Australia)".into(),
            "fr-FR" => "French (France)".into(),
            "de-DE" => "German (Germany)".into(),
            "es-ES" => "Spanish (Spain)".into(),
            "it-IT" => "Italian (Italy)".into(),
            "ja-JP" => "Japanese".into(),
            "zh-CN" => "Chinese (Simplified)".into(),
            other => other.to_owned(),
        }
    }

    pub fn set_language(&mut self, tag: &str) {
        let tag = tag.to_owned();
        self.format(move |rpr| {
            rpr.put(El::new("w:lang").with("w:val", tag.clone()));
        });
    }

    /// The accessibility checker: pictures with no alt text, a heading that skips a level, a table with
    /// no header row, and text whose colour is hard to read on the page.
    pub fn accessibility(&self) -> Vec<Issue> {
        let resolver = crate::styles::Resolver::new(&self.document);
        let page = self
            .document
            .settings
            .as_ref()
            .and_then(|_| self.document.background.as_ref())
            .and_then(|b| b.attr("w:color"))
            .and_then(props::Rgb::parse)
            .unwrap_or(props::Rgb(255, 255, 255));
        let mut out = Vec::new();
        let mut last_level: Option<u8> = None;
        for (path, p) in position::paragraphs(&self.document.body) {
            let look = resolver.paragraph(&p.props, None);
            if let Some(level) = look.outline_level {
                if let Some(previous) = last_level {
                    if level > previous + 1 {
                        out.push(Issue {
                            kind: "heading",
                            message: format!(
                                "Heading {} follows Heading {} with no Heading {} between them",
                                level + 1,
                                previous + 1,
                                previous + 2
                            ),
                            at: Pos::new(path.clone(), 0),
                        });
                    }
                }
                last_level = Some(level);
            }
            let mut offset = 0;
            for run in &p.runs {
                if let Some(picture) = run.content.picture() {
                    let mut described = false;
                    picture.walk(&mut |node| {
                        if node.name == "wp:docPr"
                            && node.attr("descr").is_some_and(|d| !d.trim().is_empty())
                        {
                            described = true;
                        }
                        if node.name == "v:shape"
                            && node.attr("alt").is_some_and(|d| !d.trim().is_empty())
                        {
                            described = true;
                        }
                    });
                    if !described {
                        out.push(Issue {
                            kind: "alt-text",
                            message: "A picture has no alt text".into(),
                            at: Pos::new(path.clone(), offset),
                        });
                    }
                }
                if matches!(&run.content, Content::Text(t) if !t.trim().is_empty()) {
                    let colour = resolver
                        .run(&p.props, &run.props, None)
                        .color
                        .unwrap_or(props::Rgb(0, 0, 0));
                    let ratio = contrast(colour, page);
                    if ratio < 4.5
                        && !out.iter().any(|i: &Issue| i.kind == "contrast" && i.at.path == path)
                    {
                        out.push(Issue {
                            kind: "contrast",
                            message: format!("Text in #{} on the page has a contrast of {ratio:.1} to 1, under 4.5", colour.hex()),
                            at: Pos::new(path.clone(), offset),
                        });
                    }
                }
                offset += run.len();
            }
        }
        for (index, block) in self.document.body.iter().enumerate() {
            if let Block::Table(t) = block {
                let header = t
                    .rows
                    .first()
                    .is_some_and(|r| props::on_off(&r.props, "w:tblHeader") == Some(true));
                if !header && t.rows.len() > 1 {
                    out.push(Issue {
                        kind: "table-header",
                        message: "A table has no header row".into(),
                        at: Pos::new(vec![index as u32, 0, 0, 0], 0),
                    });
                }
            }
        }
        out
    }

    /// The Document Inspector: comments, tracked changes, hidden text and the personal names in the
    /// document's properties.
    pub fn inspect(&self) -> Inspection {
        let resolver = crate::styles::Resolver::new(&self.document);
        let mut found =
            Inspection {
                comments: self.document.comments.as_ref().map_or(0, |c| {
                    c.notes.iter().filter(|n| n.id().is_some_and(|i| i >= 0)).count()
                }),
                ..Inspection::default()
            };
        for (_, p) in position::paragraphs(&self.document.body) {
            for run in &p.runs {
                if run.is_inserted() || run.is_deleted() || run.props.child("w:rPrChange").is_some()
                {
                    found.revisions += 1;
                }
                if matches!(run.content, Content::Text(_))
                    && resolver.run(&p.props, &run.props, None).hidden
                {
                    found.hidden += 1;
                }
            }
        }
        for name in
            ["dc:creator", "cp:lastModifiedBy", "cp:keywords", "dc:description", "dc:subject"]
        {
            if let Some(value) =
                crate::new::property(&self.document, name).filter(|v| !v.trim().is_empty())
            {
                found.personal.push((name.to_owned(), value));
            }
        }
        found
    }

    /// Remove what the Document Inspector found of one kind: `comments`, `revisions` (accepted),
    /// `hidden` text, or `personal` names. Answers how many were removed.
    pub fn remove_inspected(&mut self, kind: &str) -> Result<usize, String> {
        match kind {
            "comments" => Ok(self.delete_all_comments()),
            "revisions" => Ok(self.resolve_revisions(true, true)),
            "hidden" => {
                let mut count = 0;
                self.change(Scope::Whole, |document, selection, _| {
                    let resolver_document = document.clone();
                    let resolver = crate::styles::Resolver::new(&resolver_document);
                    let paths: Vec<Vec<u32>> =
                        position::paragraphs(&document.body).into_iter().map(|(p, _)| p).collect();
                    for path in paths {
                        if let Some(p) = position::paragraph_mut(document, &StoryId::Body, &path) {
                            let props = p.props.clone();
                            let before = p.runs.len();
                            p.runs.retain(|r| {
                                !(matches!(r.content, Content::Text(_))
                                    && resolver.run(&props, &r.props, None).hidden)
                            });
                            count += before - p.runs.len();
                            let length = p.len();
                            for mark in &mut p.marks {
                                mark.offset = mark.offset.min(length);
                            }
                        }
                    }
                    let length =
                        position::paragraph(document, &selection.head.story, &selection.head.path)
                            .map_or(0, Paragraph::len);
                    Selection::caret(selection.head.at(selection.head.offset.min(length)))
                });
                Ok(count)
            }
            "personal" => {
                let names = self.inspect().personal;
                for (name, _) in &names {
                    crate::new::set_property(&mut self.document, name, "");
                }
                self.modified = true;
                Ok(names.len())
            }
            other => Err(format!(
                "the inspector removes comments, revisions, hidden or personal, not {other}"
            )),
        }
    }
}

/// `w:documentProtection` for an editing restriction (`readOnly`, `comments`, `trackedChanges`,
/// `forms`) with the password hash Word writes.
pub fn protection_element(edit: &str, password: &str) -> El {
    let mut salt = [0u8; 16];
    if getrandom::fill(&mut salt).is_err() {
        // No random numbers: a salt from the clock, which is still unique to this moment.
        salt = std::array::from_fn(|i| {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos());
            (nanos.wrapping_mul(2654435761).wrapping_add(i as u32 * 40503) >> (i % 4 * 8)) as u8
        });
    }
    let hash = password_hash(password, &salt, 100_000);
    El::new("w:documentProtection")
        .with("w:edit", edit)
        .with("w:enforcement", "1")
        .with("w:cryptProviderType", "rsaAES")
        .with("w:cryptAlgorithmClass", "hash")
        .with("w:cryptAlgorithmType", "typeAny")
        .with("w:cryptAlgorithmSid", "14")
        .with("w:cryptSpinCount", "100000")
        .with("w:hash", base64(&hash))
        .with("w:salt", base64(&salt))
}

/// The ECMA-376 password hash: SHA-512 of the salt and the password as UTF-16, then `spins` rounds of
/// SHA-512 of the hash and the round's number.
pub fn password_hash(password: &str, salt: &[u8], spins: u32) -> Vec<u8> {
    let utf16: Vec<u8> = password.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut hash = Sha512::new().chain_update(salt).chain_update(&utf16).finalize().to_vec();
    for round in 0..spins {
        hash =
            Sha512::new().chain_update(&hash).chain_update(round.to_le_bytes()).finalize().to_vec();
    }
    hash
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a' + 26),
            b'0'..=b'9' => u32::from(c - b'0' + 52),
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let clean: Vec<u8> = text.bytes().filter(|b| *b != b'=' && !b.is_ascii_whitespace()).collect();
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= value(c)? << (18 - 6 * i);
        }
        out.push((acc >> 16) as u8);
        if chunk.len() > 2 {
            out.push((acc >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(acc as u8);
        }
    }
    Some(out)
}

/// The WCAG contrast ratio of two colours.
fn contrast(a: props::Rgb, b: props::Rgb) -> f32 {
    let luminance = |c: props::Rgb| {
        let channel = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.039_28 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.0) + 0.7152 * channel(c.1) + 0.0722 * channel(c.2)
    };
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restrict_editing_refuses_what_it_forbids_and_its_password_stops_it() {
        let mut session = Session::new(crate::new::blank());
        session.type_text("Some text");
        session.restrict_editing("readOnly", "s3cret").unwrap();
        session.type_text(" more");
        assert!(session.refused.is_some(), "typing is refused");
        let text = crate::export::to_text(&session.document);
        assert!(!text.contains("more"), "{text}");
        assert!(session.stop_protection("wrong").is_err());
        session.stop_protection("s3cret").unwrap();
        session.type_text(" more");
        assert!(crate::export::to_text(&session.document).contains("more"));
        session.restrict_editing("comments", "x").unwrap();
        session.add_comment("allowed");
        assert_eq!(session.document.comments.as_ref().map(|c| c.notes.len()), Some(1), "a comment is allowed");
        session.type_text("not allowed");
        assert!(!crate::export::to_text(&session.document).contains("not allowed"));
        // Password to modify: read only until the password is given, after reading the saved file again.
        let mut session = Session::new(crate::new::blank());
        session.set_password_to_modify("pw");
        let saved = crate::write::save(&session.document).unwrap();
        let mut again = Session::new(crate::read::open(&saved).unwrap());
        assert!(again.read_only);
        again.type_text("x");
        assert!(again.refused.is_some());
        assert!(again.unlock_for_editing("nope").is_err());
        again.unlock_for_editing("pw").unwrap();
        again.type_text("x");
        assert!(crate::export::to_text(&again.document).contains('x'));
    }

    #[test]
    fn a_tracked_bold_records_the_formatting_before_it_and_rejects_back() {
        let mut session = Session::new(crate::new::blank());
        session.type_text("plain words");
        session.set_tracking(true);
        session.select(Pos::new(vec![0], 0), Pos::new(vec![0], 5));
        session.toggle("w:b");
        let p = position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap();
        let changed = p
            .runs
            .iter()
            .find(|r| r.props.child("w:rPrChange").is_some())
            .expect("a tracked change");
        assert!(changed.props.child("w:b").is_some());
        assert!(session.go_to_change(true));
        assert_eq!(session.selection.ordered().1.offset, 5);
        session.select_all();
        session.resolve_revisions(false, true);
        let p = position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap();
        assert!(p
            .runs
            .iter()
            .all(|r| r.props.child("w:b").is_none() && r.props.child("w:rPrChange").is_none()));
    }

    #[test]
    fn the_password_hash_checks_and_locks_tracking() {
        let mut session = Session::new(crate::new::blank());
        session.lock_tracking("secret");
        assert!(session.tracking_locked());
        assert!(session.tracking);
        assert!(session.protection_password_matches("secret"));
        assert!(!session.protection_password_matches("wrong"));
        assert_eq!(base64_decode(&base64(b"hello world")).unwrap(), b"hello world");
    }

    #[test]
    fn the_checker_and_the_inspector_find_what_they_look_for() {
        let mut session = Session::new(crate::new::blank());
        session.type_text("Title");
        session.apply_style("Heading1").unwrap();
        session.enter();
        session.type_text("Skipped");
        session.apply_style("Heading3").unwrap();
        session.enter();
        session.format(|rpr| rpr.put(El::new("w:color").with("w:val", "DDDDDD")));
        session.type_text("Faint text");
        let issues = session.accessibility();
        assert!(issues.iter().any(|i| i.kind == "heading"), "{issues:?}");
        assert!(issues.iter().any(|i| i.kind == "contrast"), "{issues:?}");
        crate::new::set_property(&mut session.document, "dc:creator", "Someone");
        assert!(session.inspect().personal.iter().any(|(_, v)| v == "Someone"));
        assert_eq!(session.remove_inspected("personal").unwrap(), 1);
        assert!(session.inspect().personal.is_empty());
        assert!((contrast(props::Rgb(0, 0, 0), props::Rgb(255, 255, 255)) - 21.0).abs() < 0.1);
    }
}
