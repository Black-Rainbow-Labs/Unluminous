//! Where something is in a document: which story, which paragraph, and how far into it.
//!
//! A paragraph is addressed by a path of indices from the top of its story: the block, then for each table
//! it is inside the row, the cell and the block inside the cell, and for a content control the block
//! inside it. A position is a path and a byte offset into that paragraph's flat text
//! (`Paragraph::flat_text`). Paths compare in document order, so two positions compare as positions.

use std::cmp::Ordering;

use crate::model::{Block, Document, Paragraph};

/// Which run of blocks a position is in.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum StoryId {
    #[default]
    Body,
    /// A header or footer, by its relationship id.
    Header(String),
    Footnote(i64),
    Endnote(i64),
    Comment(i64),
    /// The text in a text box, by its drawing's `wp:docPr` id.
    TextBox(i64),
}

/// A paragraph's address inside its story.
pub type Path = Vec<u32>;

/// A place in the document.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Pos {
    pub story: StoryId,
    pub path: Path,
    pub offset: usize,
}

impl Pos {
    pub fn new(path: Path, offset: usize) -> Pos {
        Pos { story: StoryId::Body, path, offset }
    }

    /// The same paragraph at another offset.
    pub fn at(&self, offset: usize) -> Pos {
        Pos { offset, ..self.clone() }
    }
}

impl PartialOrd for Pos {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Pos {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.story, &self.path, self.offset).cmp(&(&other.story, &other.path, other.offset))
    }
}

/// A selection: where it started and where the caret is now. Both are in the same story.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: Pos,
    pub head: Pos,
}

impl Selection {
    pub fn caret(pos: Pos) -> Selection {
        Selection { anchor: pos.clone(), head: pos }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The earlier end and the later one.
    pub fn ordered(&self) -> (&Pos, &Pos) {
        match self.anchor <= self.head {
            true => (&self.anchor, &self.head),
            false => (&self.head, &self.anchor),
        }
    }
}

/// The blocks of a story.
pub fn story<'a>(document: &'a Document, id: &StoryId) -> Option<&'a Vec<Block>> {
    Some(match id {
        StoryId::Body => &document.body,
        StoryId::Header(rid) => &document.headers.iter().find(|h| &h.id == rid)?.story.blocks,
        StoryId::Footnote(n) => {
            &document.footnotes.as_ref()?.notes.iter().find(|s| s.id() == Some(*n))?.blocks
        }
        StoryId::Endnote(n) => {
            &document.endnotes.as_ref()?.notes.iter().find(|s| s.id() == Some(*n))?.blocks
        }
        StoryId::Comment(n) => {
            &document.comments.as_ref()?.notes.iter().find(|s| s.id() == Some(*n))?.blocks
        }
        StoryId::TextBox(n) => &document.textboxes.iter().find(|t| t.id == *n)?.blocks,
    })
}

/// The blocks of a story, to change, and marks the part it lives in changed.
pub fn story_mut<'a>(document: &'a mut Document, id: &StoryId) -> Option<&'a mut Vec<Block>> {
    Some(match id {
        StoryId::Body => &mut document.body,
        StoryId::Header(rid) => {
            let header = document.headers.iter_mut().find(|h| &h.id == rid)?;
            header.dirty = true;
            &mut header.story.blocks
        }
        StoryId::Footnote(n) => notes_mut(document.footnotes.as_mut()?, *n)?,
        StoryId::Endnote(n) => notes_mut(document.endnotes.as_mut()?, *n)?,
        StoryId::Comment(n) => notes_mut(document.comments.as_mut()?, *n)?,
        StoryId::TextBox(n) => {
            let textbox = document.textboxes.iter_mut().find(|t| t.id == *n)?;
            textbox.dirty = true;
            &mut textbox.blocks
        }
    })
}

fn notes_mut(part: &mut crate::model::NotesPart, id: i64) -> Option<&mut Vec<Block>> {
    part.dirty = true;
    Some(&mut part.notes.iter_mut().find(|s| s.id() == Some(id))?.blocks)
}

/// The block a path names, walking into tables and content controls.
pub fn block<'a>(blocks: &'a [Block], path: &[u32]) -> Option<&'a Block> {
    let (first, rest) = path.split_first()?;
    let block = blocks.get(*first as usize)?;
    if rest.is_empty() {
        return Some(block);
    }
    match block {
        Block::Table(table) => {
            let [row, cell, inner @ ..] = rest else { return None };
            let cell = table.rows.get(*row as usize)?.cells.get(*cell as usize)?;
            self::block(&cell.blocks, inner)
        }
        Block::Sdt(sdt) => self::block(&sdt.blocks, rest),
        _ => None,
    }
}

pub fn block_mut<'a>(blocks: &'a mut [Block], path: &[u32]) -> Option<&'a mut Block> {
    let (first, rest) = path.split_first()?;
    let block = blocks.get_mut(*first as usize)?;
    if rest.is_empty() {
        return Some(block);
    }
    match block {
        Block::Table(table) => {
            let [row, cell, inner @ ..] = rest else { return None };
            let cell = table.rows.get_mut(*row as usize)?.cells.get_mut(*cell as usize)?;
            block_mut(&mut cell.blocks, inner)
        }
        Block::Sdt(sdt) => block_mut(&mut sdt.blocks, rest),
        _ => None,
    }
}

/// The list of blocks the last index of `path` indexes into, and that index.
pub fn container_mut<'a>(
    blocks: &'a mut Vec<Block>,
    path: &[u32],
) -> Option<(&'a mut Vec<Block>, usize)> {
    match path {
        [] => None,
        [only] => Some((blocks, *only as usize)),
        [first, rest @ ..] => match blocks.get_mut(*first as usize)? {
            Block::Table(table) => {
                let [row, cell, inner @ ..] = rest else { return None };
                let cell = table.rows.get_mut(*row as usize)?.cells.get_mut(*cell as usize)?;
                container_mut(&mut cell.blocks, inner)
            }
            Block::Sdt(sdt) => container_mut(&mut sdt.blocks, rest),
            _ => None,
        },
    }
}

/// The paragraph a path names.
pub fn paragraph<'a>(
    document: &'a Document,
    story: &StoryId,
    path: &[u32],
) -> Option<&'a Paragraph> {
    block(self::story(document, story)?, path)?.as_paragraph()
}

pub fn paragraph_mut<'a>(
    document: &'a mut Document,
    story: &StoryId,
    path: &[u32],
) -> Option<&'a mut Paragraph> {
    block_mut(story_mut(document, story)?, path)?.as_paragraph_mut()
}

/// Every paragraph of a list of blocks, with its path, in document order.
pub fn paragraphs(blocks: &[Block]) -> Vec<(Path, &Paragraph)> {
    let mut out = Vec::new();
    collect(blocks, &mut Vec::new(), &mut out);
    out
}

fn collect<'a>(blocks: &'a [Block], prefix: &mut Path, out: &mut Vec<(Path, &'a Paragraph)>) {
    for (index, block) in blocks.iter().enumerate() {
        prefix.push(index as u32);
        match block {
            Block::Paragraph(paragraph) => out.push((prefix.clone(), paragraph)),
            Block::Table(table) => {
                for (r, row) in table.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        prefix.extend([r as u32, c as u32]);
                        collect(&cell.blocks, prefix, out);
                        prefix.truncate(prefix.len() - 2);
                    }
                }
            }
            Block::Sdt(sdt) => collect(&sdt.blocks, prefix, out),
            Block::Kept(_) => {}
        }
        prefix.pop();
    }
}

/// The path of the paragraph after `path` in the story, in document order.
pub fn next_paragraph(blocks: &[Block], path: &[u32]) -> Option<Path> {
    let all = paragraphs(blocks);
    let at = all.iter().position(|(p, _)| p.as_slice() == path)?;
    all.get(at + 1).map(|(p, _)| p.clone())
}

pub fn previous_paragraph(blocks: &[Block], path: &[u32]) -> Option<Path> {
    let all = paragraphs(blocks);
    let at = all.iter().position(|(p, _)| p.as_slice() == path)?;
    at.checked_sub(1).map(|before| all[before].0.clone())
}

/// The innermost table cell a paragraph is in, as `(table path, row, cell)`.
pub fn cell_of(blocks: &[Block], path: &[u32]) -> Option<(Path, u32, u32)> {
    let mut found = None;
    let mut at = 0;
    let mut current = blocks;
    while at < path.len() {
        match current.get(path[at] as usize)? {
            Block::Table(table) if at + 2 < path.len() => {
                let (row, cell) = (path[at + 1], path[at + 2]);
                found = Some((path[..=at].to_vec(), row, cell));
                current = &table.rows.get(row as usize)?.cells.get(cell as usize)?.blocks;
                at += 3;
            }
            Block::Sdt(sdt) if at + 1 < path.len() => {
                current = &sdt.blocks;
                at += 1;
            }
            _ => break,
        }
    }
    found
}
