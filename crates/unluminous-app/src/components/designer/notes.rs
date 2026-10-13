//! The notes at the right of a design tab: the chosen element, a note being written, and the threads.
//!
//! The Notes column is raised cards with carved wells inside them, which is how the Agent-Chat tool blocks and the
//! Settings cards are built, so nothing here needed a surface of its own. A thread is a card like a chat message:
//! the number disc a pin wears, the element's tag in a carved chip, who said what with the words drawn through
//! `components::markdown_text` so a reply's code is coloured, and for the thread that is open a carved reply box
//! and the buttons that act on it. A note whose element has gone wears `Detached`.
//!
//! Every function takes a rectangle, draws, and reports an [`Act`]. The text boxes edit fields of the view in
//! place, which is the one thing the drawing changes. Heights are worked out before anything is drawn, from how
//! the words wrap, because the cards are laid one under another in a scrolling column.

use std::cell::RefCell;
use std::path::Path;

use egui::{Pos2, Rect, Sense, Vec2};
use rux::icon::Icon;
use rux::{Rux, Style};

use super::{kit, Act};
use crate::components::markdown_text::{self, Cache, CodeColors, Colors};
use crate::services::designer::notes::{time_ago, unix_now, NoteState};
use crate::services::designer::{Designer, Filter};
use crate::services::plugin_ui::Look;

/// The room between the cards, and inside them.
const GAP: f32 = 12.0;
const PAD_X: f32 = 16.0;
const PAD_Y: f32 = 14.0;
/// How tall the header is, with the title and the open and resolved switch.
const HEADER: f32 = 40.0;
/// A card's corner, and a carved block's inside it.
const CARD_RADIUS: f32 = 22.0;
const BLOCK_RADIUS: f32 = 16.0;
/// How tall a button is, a number disc, a tag chip, a reply box.
const BUTTON: f32 = 30.0;
const DISC: f32 = 24.0;
const REPLY: f32 = 40.0;
/// How tall the note being written is: its header, a text area, the list of what goes with it, and its buttons.
const COMPOSER: f32 = 318.0;
/// The type size of a note's words.
const WORDS: f32 = 12.5;

thread_local! {
    /// The markdown of every message drawn, kept so a frame that changed nothing lays nothing out again.
    static MARKDOWN: RefCell<Cache> = RefCell::new(Cache::default());
}

/// One message of a thread, ready to draw.
struct Said {
    author: String,
    body: String,
    ago: String,
}

/// A thread, copied out of the view so the drawing holds no borrow of the provider.
struct Thread {
    id: u32,
    tag: String,
    open: bool,
    detached: bool,
    active: bool,
    said: Vec<Said>,
}

/// The chosen element, as the card shows it.
struct Chosen {
    tag: String,
    selector: String,
    excerpt: String,
    size: String,
}

/// What the sidebar needs, copied out of the view.
struct Facts {
    filter: Filter,
    open: usize,
    resolved: usize,
    chosen: Option<Chosen>,
    composing: Option<(u32, String, Vec<String>)>,
    threads: Vec<Thread>,
    has_a_selection: bool,
}

/// How tall one thing in the column is, and what it is.
enum Block {
    Element(f32),
    Composer,
    Thread(usize, f32),
}

/// Copy what the sidebar needs out of the view, so the drawing holds no borrow of the provider.
fn facts_of(designer: &Designer, design: &Path) -> Option<Facts> {
    let view = designer.design_view(design)?;
    let (open, resolved) = view.notes.counts();
    let now = unix_now();
    let threads = view
        .listed()
        .into_iter()
        .map(|note| Thread {
            id: note.id,
            tag: note.anchor.tag.clone(),
            open: note.state == NoteState::Open,
            detached: view.detached.contains(&note.id),
            active: view.active == Some(note.id),
            said: note
                .messages
                .iter()
                .map(|message| Said {
                    author: message.author.clone(),
                    body: message.body.clone(),
                    ago: time_ago(&message.at, now),
                })
                .collect(),
        })
        .collect();
    let chosen = view.selection.as_ref().map(|element| Chosen {
        tag: element.tag.clone(),
        selector: element.selector.clone(),
        excerpt: element.excerpt.clone(),
        size: size_of(&element.styles),
    });
    let composing = view.composing.as_ref().map(|composer| {
        (view.notes.next, composer.element.tag.clone(), what_goes_with(designer, composer))
    });
    Some(Facts {
        filter: view.filter,
        open,
        resolved,
        chosen,
        composing,
        threads,
        has_a_selection: view.selection.is_some(),
    })
}

/// `62px / 800`, from an element's font size and weight when the page said them.
fn size_of(styles: &[(String, String)]) -> String {
    let find =
        |name: &str| styles.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str());
    match (find("font-size"), find("font-weight")) {
        (Some(size), Some(weight)) => format!("{size} / {weight}"),
        (Some(size), None) => size.to_owned(),
        _ => String::new(),
    }
}

/// The chips under a new note: everything that goes to the agent with it.
fn what_goes_with(
    designer: &Designer,
    composer: &crate::services::designer::Composer,
) -> Vec<String> {
    let mut goes = vec!["selector".to_owned(), "text excerpt".to_owned()];
    if designer.config.send_html && !composer.element.html.is_empty() {
        goes.push(format!("HTML, {} chars", composer.element.html.chars().count()));
    }
    let named = |wanted: &str, word: &str, goes: &mut Vec<String>| {
        if composer.element.styles.iter().any(|(name, _)| name == wanted) {
            goes.push(word.to_owned());
        }
    };
    named("font-family", "font", &mut goes);
    named("font-size", "size", &mut goes);
    named("color", "colour", &mut goes);
    named("padding", "padding", &mut goes);
    named("margin", "margin", &mut goes);
    goes
}

/// The colours markdown is drawn in here, from the theme the window is in.
fn colours() -> Colors {
    let theme = crate::theme::rux_theme();
    Colors {
        text: theme.ink.i700,
        strong: theme.ink.i900,
        code: theme.ink.i900,
        link: theme.chat.sky,
        quiet: theme.ink.i400,
        rule: theme.ink.i500,
    }
}

/// How tall some markdown is when laid out to `width`.
fn markdown_height(look: &Look<'_>, key: &str, body: &str, width: f32, scale: f32) -> f32 {
    MARKDOWN.with_borrow_mut(|cache| {
        cache
            .rendered(
                key,
                body,
                look.renderer,
                &look.font_family,
                WORDS * scale,
                colours(),
                width,
                look.highlighter,
            )
            .height()
    })
}

/// Draw some markdown into `area`.
fn draw_markdown(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    key: &str,
    body: &str,
    area: Rect,
    scale: f32,
) {
    MARKDOWN.with_borrow_mut(|cache| {
        let made = cache.rendered(
            key,
            body,
            look.renderer,
            &look.font_family,
            WORDS * scale,
            colours(),
            area.width(),
            look.highlighter,
        );
        let code = CodeColors {
            panel: crate::theme::rux_theme().chat.well,
            chip: egui::Color32::TRANSPARENT,
            radius: 6,
        };
        markdown_text::show_with(ui, area, made, look.renderer, 0.0, Some(code));
    });
}

/// Draw the notes into `rect` and report what was pressed.
///
/// @param designer - the provider
/// @param ui - where to draw
/// @param look - the window's look
/// @param rect - the sidebar
/// @param design - the design, relative to the project
pub fn show(
    designer: &mut Designer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    design: &Path,
) -> Vec<Act> {
    let scale = look.scale();
    let Some(facts) = facts_of(designer, design) else { return Vec::new() };
    let mut acts = Vec::new();
    let header = Rect::from_min_size(rect.min, Vec2::new(rect.width(), HEADER * scale));
    kit::layer(ui, scale, "designer-notes-header", header, |rux| {
        the_header(rux, header, design, &facts, scale, &mut acts)
    });
    let column =
        Rect::from_min_max(Pos2::new(rect.left(), header.bottom() + 6.0 * scale), rect.max);
    if column.height() > 40.0 * scale {
        acts.extend(the_column(designer, ui, look, column, design, &facts, scale));
    }
    acts
}

/// `Notes` at the left and the switch between open and resolved at the right.
fn the_header(
    rux: &mut Rux<'_>,
    rect: Rect,
    design: &Path,
    facts: &Facts,
    scale: f32,
    acts: &mut Vec<Act>,
) {
    let middle = rect.center().y;
    kit::heading(
        rux,
        Pos2::new(rect.left() + 4.0 * scale, middle),
        "Notes",
        16.0,
        rect.width() * 0.3,
    );
    let choices = [format!("Open {}", facts.open), format!("Resolved {}", facts.resolved)];
    let switch = Rect::from_min_size(
        Pos2::new(rect.right() - 196.0 * scale, middle - 19.0 * scale),
        Vec2::new(196.0 * scale, 38.0 * scale),
    );
    let chosen = Some(if facts.filter == Filter::Open { 0 } else { 1 });
    if let Some(picked) = kit::pill_switch(rux, switch, &choices, chosen, "Notes filter", true) {
        let filter = if picked == 0 { Filter::Open } else { Filter::Resolved };
        acts.push(Act::SetFilter(design.to_path_buf(), filter));
    }
}

/// The scrolling column: the chosen element, the note being written, and the threads, one under another.
fn the_column(
    designer: &mut Designer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    design: &Path,
    facts: &Facts,
    scale: f32,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let width = rect.width() - 10.0 * scale;
    let mut blocks = Vec::new();
    if let Some(chosen) = facts.chosen.as_ref().filter(|_| facts.composing.is_none()) {
        blocks.push(Block::Element(element_height(ui, chosen, width, scale)));
    }
    if facts.composing.is_some() {
        blocks.push(Block::Composer);
    }
    for (at, thread) in facts.threads.iter().enumerate() {
        blocks.push(Block::Thread(
            at,
            thread_height(ui, look, design, thread, width, scale, facts.has_a_selection),
        ));
    }
    if blocks.is_empty() {
        return nothing_to_list(ui, rect, facts, scale);
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt(("designer-column", design))
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let top = ui.cursor().min;
            let mut y = top.y;
            for block in &blocks {
                let height = match block {
                    Block::Element(height) | Block::Thread(_, height) => *height,
                    Block::Composer => COMPOSER * scale,
                };
                let card = Rect::from_min_size(Pos2::new(rect.left(), y), Vec2::new(width, height));
                match block {
                    Block::Element(_) => acts.extend(the_element(ui, card, design, facts, scale)),
                    Block::Composer => {
                        acts.extend(the_composer(designer, ui, card, design, facts, scale))
                    }
                    Block::Thread(at, _) => acts.extend(one_thread(
                        designer,
                        ui,
                        look,
                        card,
                        design,
                        &facts.threads[*at],
                        facts,
                        scale,
                    )),
                }
                y += height + GAP * scale;
            }
            ui.allocate_space(Vec2::new(rect.width(), (y - top.y).max(0.0)));
        });
    acts
}

/// What the column says when there is nothing in it.
fn nothing_to_list(ui: &mut egui::Ui, rect: Rect, facts: &Facts, scale: f32) -> Vec<Act> {
    let words = match facts.filter {
        Filter::Open => {
            "No open notes. Choose Comment, then click an element in the page to make one."
        }
        Filter::Resolved => "No resolved notes.",
    };
    kit::layer(ui, scale, "designer-notes-empty", rect, |rux| {
        kit::paragraph(
            rux,
            Pos2::new(rect.left() + 4.0 * scale, rect.top() + 8.0 * scale),
            rect.width() - 8.0 * scale,
            words,
            12.0,
        );
    });
    Vec::new()
}

// ------------------------------------------------------------------------------- the chosen element

/// How tall the chosen element's card is: its labels, its tag, its selector as it wraps, its excerpt and its buttons.
fn element_height(ui: &egui::Ui, chosen: &Chosen, width: f32, scale: f32) -> f32 {
    let inner = (width - 2.0 * PAD_X * scale - 24.0 * scale).max(40.0);
    let style = Style::mono(10.5 * scale).leading(1.4);
    let selector = rux::text::wrapped(
        ui.painter(),
        style,
        &chosen.selector,
        crate::theme::color::text(),
        inner,
    );
    let excerpt = if chosen.excerpt.is_empty() { 0.0 } else { 24.0 };
    (PAD_Y + 18.0 + 8.0 + 26.0 + 10.0 + 18.0 + excerpt + 12.0 + BUTTON + PAD_Y) * scale
        + selector.size().y
}

/// The chosen element: its tag and size, its selector in a carved block, and the buttons that walk the tree.
fn the_element(
    ui: &mut egui::Ui,
    card: Rect,
    design: &Path,
    facts: &Facts,
    scale: f32,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(chosen) = &facts.chosen else { return acts };
    kit::layer(ui, scale, "designer-notes-element", card, |rux| {
        let theme = rux.theme();
        kit::card_surface(rux, card, CARD_RADIUS * scale);
        let left = card.left() + PAD_X * scale;
        let right = card.right() - PAD_X * scale;
        let mut y = card.top() + PAD_Y * scale;
        kit::quiet(rux, Pos2::new(left, y + 9.0 * scale), "Chosen element", 11.0, 160.0 * scale);
        let clear = Rect::from_min_max(
            Pos2::new(right - 90.0 * scale, y),
            Pos2::new(right, y + 18.0 * scale),
        );
        if kit::button(rux, clear, "Esc to clear", None, kit::Look::Ghost) {
            acts.push(Act::ClearSelection(design.to_path_buf()));
        }
        y += 26.0 * scale;
        let (word, rest) = chosen
            .tag
            .split_once('.')
            .map_or((chosen.tag.as_str(), String::new()), |(tag, rest)| (tag, format!(".{rest}")));
        let tag = Rect::from_center_size(
            Pos2::new(left + 12.0 * scale, y + 13.0 * scale),
            Vec2::new(24.0 * scale, 22.0 * scale),
        );
        kit::well_surface(rux, tag, 8.0 * scale);
        let galley = rux.text(rux::Style::mono(11.0 * scale).semibold(), word, theme.chat.sky);
        rux::text::draw_centred(rux.painter(), tag, galley, theme.chat.sky);
        let name_room = right - tag.right() - 12.0 * scale - 90.0 * scale;
        kit::heading(
            rux,
            Pos2::new(tag.right() + 8.0 * scale, y + 13.0 * scale),
            &rest,
            13.5,
            name_room,
        );
        if !chosen.size.is_empty() {
            let galley = rux.text(rux::Style::mono(11.0 * scale), &chosen.size, theme.ink.i400);
            let at = Pos2::new(right - galley.size().x, y + 13.0 * scale - galley.size().y / 2.0);
            rux.painter().galley(at, galley, theme.ink.i400);
        }
        y += 36.0 * scale;
        let inner = right - left - 24.0 * scale;
        let style = rux.zs(Style::mono(10.5).leading(1.4));
        let selector =
            rux::text::wrapped(rux.painter(), style, &chosen.selector, theme.ink.i500, inner);
        let block = Rect::from_min_size(
            Pos2::new(left, y),
            Vec2::new(right - left, selector.size().y + 20.0 * scale),
        );
        kit::well_surface(rux, block, 14.0 * scale);
        rux.painter().galley(
            Pos2::new(left + 12.0 * scale, y + 10.0 * scale),
            selector,
            theme.ink.i500,
        );
        y = block.bottom() + 10.0 * scale;
        if !chosen.excerpt.is_empty() {
            let quote = format!("\u{201C}{}\u{201D}", chosen.excerpt);
            kit::quiet(
                rux,
                Pos2::new(left + 2.0 * scale, y + 9.0 * scale),
                &quote,
                11.5,
                right - left,
            );
            y += 24.0 * scale;
        }
        y += 12.0 * scale;
        let each = (right - left - 12.0 * scale) / 3.0;
        for (index, (label, mark, act)) in [
            ("Parent", Icon::ChevLeft, Act::Walk(design.to_path_buf(), true)),
            ("Child", Icon::ChevRight, Act::Walk(design.to_path_buf(), false)),
            ("Add note", Icon::Plus, Act::StartNote(design.to_path_buf())),
        ]
        .into_iter()
        .enumerate()
        {
            let at = Rect::from_min_size(
                Pos2::new(left + index as f32 * (each + 6.0 * scale), y),
                Vec2::new(each, BUTTON * scale),
            );
            if kit::button(rux, at, label, Some(mark), kit::Look::Chip) {
                acts.push(act);
            }
        }
    });
    acts
}

// -------------------------------------------------------------------------------- the note being written

/// The note being written: its number, the element's tag, a carved text area, what goes with it, and its buttons.
fn the_composer(
    designer: &mut Designer,
    ui: &mut egui::Ui,
    card: Rect,
    design: &Path,
    facts: &Facts,
    scale: f32,
) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some((number, tag, goes)) = &facts.composing else { return acts };
    let Some(view) = designer.design_view_mut(design) else { return acts };
    let Some(composer) = view.composing.as_mut() else { return acts };
    let can_add = !composer.text.trim().is_empty();
    let text = &mut composer.text;
    kit::layer(ui, scale, "designer-notes-composer", card, |rux| {
        kit::card_surface(rux, card, 26.0 * scale);
        let left = card.left() + PAD_X * scale;
        let right = card.right() - PAD_X * scale;
        let mut y = card.top() + PAD_Y * scale;
        kit::number_disc(
            rux,
            Pos2::new(left + DISC * scale / 2.0, y + 13.0 * scale),
            DISC,
            *number,
            true,
        );
        let after = left + (DISC + 8.0) * scale;
        let width =
            kit::heading(rux, Pos2::new(after, y + 13.0 * scale), "New note", 13.5, 100.0 * scale);
        kit::tag_chip(
            rux,
            Pos2::new(after + width + 10.0 * scale, y + 13.0 * scale),
            tag,
            right - after - width - 54.0 * scale,
        );
        let close = Rect::from_center_size(
            Pos2::new(right - 13.0 * scale, y + 13.0 * scale),
            Vec2::splat(26.0 * scale),
        );
        if kit::chip_button(rux, close, Icon::X, 26.0, "Close note") {
            acts.push(Act::CancelNote(design.to_path_buf()));
        }
        y += 40.0 * scale;
        let field = Rect::from_min_size(Pos2::new(left, y), Vec2::new(right - left, 104.0 * scale));
        kit::carved_field(
            rux,
            field,
            18.0 * scale,
            text,
            "What should change?",
            "New note",
            "composer",
            true,
            13.0,
        );
        y = field.bottom() + 12.0 * scale;
        kit::quiet(
            rux,
            Pos2::new(left + 2.0 * scale, y + 7.0 * scale),
            "Goes with the note",
            11.0,
            right - left,
        );
        y += 20.0 * scale;
        what_goes(rux, goes, left, right, y, scale);
        the_composer_buttons(
            rux,
            left - 8.0 * scale,
            right + 8.0 * scale,
            card.bottom() - (PAD_Y + 38.0) * scale,
            can_add,
            design,
            scale,
            &mut acts,
        );
    });
    acts
}

/// The chips that say what goes with the note, wrapped to the card's width. Answers where they end.
fn what_goes(
    rux: &mut Rux<'_>,
    goes: &[String],
    left: f32,
    right: f32,
    top: f32,
    scale: f32,
) -> f32 {
    let (mut x, mut y) = (left, top);
    for word in goes {
        let style = rux.zs(Style::sans(11.0));
        let ink = rux.theme().ink.i500;
        let galley = rux.text(style, word, ink);
        let width = galley.size().x + 22.0 * scale;
        if x + width > right {
            x = left;
            y += 30.0 * scale;
        }
        let chip = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 24.0 * scale));
        kit::well_surface(rux, chip, chip.height() / 2.0);
        rux::text::draw_centred(rux.painter(), chip, galley, ink);
        x = chip.right() + 6.0 * scale;
    }
    y + 24.0 * scale
}

/// `Cancel`, `Add note` and `Add and ask agent` along the foot of the composer.
#[allow(clippy::too_many_arguments)]
fn the_composer_buttons(
    rux: &mut Rux<'_>,
    left: f32,
    right: f32,
    top: f32,
    can_add: bool,
    design: &Path,
    scale: f32,
    acts: &mut Vec<Act>,
) {
    let height = 38.0 * scale;
    let ask = kit::button_width(rux, "Add and ask agent", true);
    let ask_at = Rect::from_min_size(Pos2::new(right - ask, top), Vec2::new(ask, height));
    if kit::button(rux, ask_at, "Add and ask agent", Some(Icon::Spark), kit::Look::Primary)
        && can_add
    {
        acts.push(Act::AddNote(design.to_path_buf(), true));
    }
    let add = kit::button_width(rux, "Add note", false);
    let add_at = Rect::from_min_size(
        Pos2::new(ask_at.left() - 8.0 * scale - add, top),
        Vec2::new(add, height),
    );
    if kit::button(rux, add_at, "Add note", None, kit::Look::Chip) && can_add {
        acts.push(Act::AddNote(design.to_path_buf(), false));
    }
    let cancel = Rect::from_min_size(Pos2::new(left, top), Vec2::new(60.0 * scale, height));
    if kit::button(rux, cancel, "Cancel", None, kit::Look::Ghost) {
        acts.push(Act::CancelNote(design.to_path_buf()));
    }
}

// ---------------------------------------------------------------------------------------- the threads

/// The key a message's markdown is kept under.
fn key_of(design: &Path, thread: u32, message: usize) -> String {
    format!("{}#{thread}.{message}", design.display())
}

/// The messages that are said in words, which is every one but the designer's own notes about what it did.
fn is_a_system_line(said: &Said) -> bool {
    said.author == "designer"
}

/// How tall a thread's card is, from how its words wrap.
fn thread_height(
    ui: &egui::Ui,
    look: &Look<'_>,
    design: &Path,
    thread: &Thread,
    width: f32,
    scale: f32,
    has_a_selection: bool,
) -> f32 {
    let _ = ui;
    let inner = (width - 2.0 * PAD_X * scale).max(40.0);
    let shown = if thread.active { thread.said.len() } else { 1.min(thread.said.len()) };
    let mut height = (PAD_Y + DISC + 12.0) * scale;
    for (at, said) in thread.said.iter().take(shown).enumerate() {
        if is_a_system_line(said) {
            height += 32.0 * scale;
        } else if at == 0 {
            height +=
                markdown_height(look, &key_of(design, thread.id, at), &said.body, inner, scale)
                    + 28.0 * scale;
        } else {
            let inside = inner - 24.0 * scale;
            height +=
                markdown_height(look, &key_of(design, thread.id, at), &said.body, inside, scale)
                    + 54.0 * scale;
        }
    }
    if !thread.active && thread.said.len() > 1 {
        height += 20.0 * scale;
    }
    if thread.active && thread.open {
        height += (REPLY + 10.0) * scale;
    }
    if thread.active {
        height += (BUTTON + 12.0) * scale;
        if has_a_selection || thread.detached {
            height += (BUTTON + 6.0) * scale;
        }
    }
    height + PAD_Y * scale
}

/// One thread's card.
#[allow(clippy::too_many_arguments)]
fn one_thread(
    designer: &mut Designer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    card: Rect,
    design: &Path,
    thread: &Thread,
    facts: &Facts,
    scale: f32,
) -> Vec<Act> {
    let mut acts = Vec::new();
    if !thread.active {
        let response =
            ui.interact(card, ui.id().with(("designer-thread", thread.id)), Sense::click());
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let named = format!("Note {}", thread.id);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone())
        });
        if response.clicked() {
            acts.push(Act::OpenNote(design.to_path_buf(), thread.id));
        }
    }
    let reply = match thread.active && thread.open {
        true => designer.design_view_mut(design).map(|view| &mut view.reply),
        false => None,
    };
    kit::layer(ui, scale, ("designer-thread-card", thread.id), card, |rux| {
        let chat = rux.theme().chat;
        kit::card_surface(rux, card, CARD_RADIUS * scale);
        if thread.active {
            let ring = egui::Stroke::new(1.5, chat.sky.gamma_multiply(0.4));
            rux.painter().rect_stroke(
                card.expand(0.75),
                CARD_RADIUS * scale,
                ring,
                egui::StrokeKind::Outside,
            );
        }
        let bottom = the_words(rux, look, card, design, thread, scale);
        if thread.active {
            the_buttons(
                rux,
                card,
                bottom,
                thread,
                reply,
                design,
                facts.has_a_selection,
                scale,
                &mut acts,
            );
        }
    });
    acts
}

/// The number, the tag, when it was said, and the messages. Answers where the last of them ends.
fn the_words(
    rux: &mut Rux<'_>,
    look: &Look<'_>,
    card: Rect,
    design: &Path,
    thread: &Thread,
    scale: f32,
) -> f32 {
    let theme = rux.theme();
    let left = card.left() + PAD_X * scale;
    let right = card.right() - PAD_X * scale;
    let inner = right - left;
    let top = card.top() + PAD_Y * scale;
    kit::number_disc(
        rux,
        Pos2::new(left + DISC * scale / 2.0, top + DISC * scale / 2.0),
        DISC,
        thread.id,
        false,
    );
    let ago = thread.said.first().map(|said| said.ago.as_str()).unwrap_or_default();
    let ago_width = kit::quiet(
        rux,
        Pos2::new(right - 80.0 * scale, top + DISC * scale / 2.0),
        "",
        11.0,
        80.0 * scale,
    );
    let _ = ago_width;
    let galley = rux.text(rux::Style::sans(11.0 * scale), ago, theme.ink.i400);
    let ago_room = galley.size().x;
    rux.painter().galley(
        Pos2::new(right - ago_room, top + DISC * scale / 2.0 - galley.size().y / 2.0),
        galley,
        theme.ink.i400,
    );
    let after = left + (DISC + 8.0) * scale;
    let detached = if thread.detached { 74.0 * scale } else { 0.0 };
    kit::tag_chip(
        rux,
        Pos2::new(after, top + DISC * scale / 2.0),
        &thread.tag,
        right - after - ago_room - detached - 12.0 * scale,
    );
    if thread.detached {
        let pill = Rect::from_center_size(
            Pos2::new(right - ago_room - 42.0 * scale, top + DISC * scale / 2.0),
            Vec2::new(66.0 * scale, 20.0 * scale),
        );
        rux.chrome.surface(pill, pill.height() / 2.0, theme.chat.well, theme.chat.carved_sm());
        let galley =
            rux.text(rux::Style::sans(10.5 * scale).medium(), "Detached", theme.chat.waiting);
        rux::text::draw_centred(rux.painter(), pill, galley, theme.chat.waiting);
    }
    let mut y = top + (DISC + 12.0) * scale;
    let shown = if thread.active { thread.said.len() } else { 1.min(thread.said.len()) };
    for (at, said) in thread.said.iter().take(shown).enumerate() {
        y = one_message(rux, look, design, thread, at, said, left, inner, y, scale);
    }
    if !thread.active && thread.said.len() > 1 {
        let replies = thread.said.len() - 1;
        let more = format!("{replies} {}", if replies == 1 { "reply" } else { "replies" });
        kit::quiet(rux, Pos2::new(left, y + 6.0 * scale), &more, 11.0, inner);
        y += 20.0 * scale;
    }
    y
}

/// One message: the person's first words plainly, a reply in a carved block with who said it, and a line of
/// the designer's own as a quiet note. Answers where it ends.
#[allow(clippy::too_many_arguments)]
fn one_message(
    rux: &mut Rux<'_>,
    look: &Look<'_>,
    design: &Path,
    thread: &Thread,
    at: usize,
    said: &Said,
    left: f32,
    inner: f32,
    y: f32,
    scale: f32,
) -> f32 {
    let theme = rux.theme();
    if is_a_system_line(said) {
        let pill = rux::components::NotePill::new(&said.body);
        let size = pill.measure(rux);
        pill.show(rux, Rect::from_min_size(Pos2::new(left, y + 2.0 * scale), size));
        return y + 32.0 * scale;
    }
    let key = key_of(design, thread.id, at);
    if at == 0 {
        kit::quiet(rux, Pos2::new(left, y + 6.0 * scale), &said.author, 11.0, inner);
        let words = markdown_height(look, &key, &said.body, inner, scale);
        let area = Rect::from_min_size(Pos2::new(left, y + 16.0 * scale), Vec2::new(inner, words));
        draw_markdown(rux.ui, look, &key, &said.body, area, scale);
        return area.bottom() + 12.0 * scale;
    }
    let pad = 12.0 * scale;
    let words = markdown_height(look, &key, &said.body, inner - 2.0 * pad, scale);
    let block = Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner, words + 42.0 * scale));
    kit::well_surface(rux, block, BLOCK_RADIUS * scale);
    let avatar = Rect::from_center_size(
        Pos2::new(block.left() + pad + 10.0 * scale, block.top() + 18.0 * scale),
        Vec2::splat(20.0 * scale),
    );
    rux::components::chip_disc(rux, avatar);
    rux.mark(rux::icon::Mark::new(Icon::Spark, 11.0 * scale), avatar.center(), theme.chat.sky);
    let width = kit::heading(
        rux,
        Pos2::new(avatar.right() + 8.0 * scale, avatar.center().y),
        &said.author,
        12.0,
        120.0 * scale,
    );
    kit::quiet(
        rux,
        Pos2::new(avatar.right() + 16.0 * scale + width, avatar.center().y),
        &said.ago,
        11.0,
        90.0 * scale,
    );
    let area = Rect::from_min_size(
        Pos2::new(block.left() + pad, block.top() + 34.0 * scale),
        Vec2::new(inner - 2.0 * pad, words),
    );
    draw_markdown(rux.ui, look, &key, &said.body, area, scale);
    block.bottom() + 12.0 * scale
}

/// The carved reply box and the buttons of the thread that is open.
#[allow(clippy::too_many_arguments)]
fn the_buttons(
    rux: &mut Rux<'_>,
    card: Rect,
    from: f32,
    thread: &Thread,
    reply: Option<&mut String>,
    design: &Path,
    has_a_selection: bool,
    scale: f32,
    acts: &mut Vec<Act>,
) {
    let left = card.left() + PAD_X * scale;
    let right = card.right() - PAD_X * scale;
    let owner = design.to_path_buf();
    let mut y = from;
    if let Some(reply) = reply {
        let field = Rect::from_min_size(Pos2::new(left, y), Vec2::new(right - left, REPLY * scale));
        kit::carved_field(
            rux,
            field,
            BLOCK_RADIUS * scale,
            reply,
            "Reply",
            "Reply to note",
            ("reply", thread.id),
            false,
            12.5,
        );
        y = field.bottom() + 10.0 * scale;
    }
    let x = std::cell::Cell::new(left);
    let row = std::cell::Cell::new(y);
    let place = |rux: &mut Rux<'_>, label: &str, mark: Option<Icon>, look: kit::Look| -> bool {
        let width = kit::button_width(rux, label, mark.is_some());
        let button =
            Rect::from_min_size(Pos2::new(x.get(), row.get()), Vec2::new(width, BUTTON * scale));
        x.set(button.right() + 6.0 * scale);
        kit::button(rux, button, label, mark, look)
    };
    if thread.open {
        if place(rux, "Ask agent", Some(Icon::Spark), kit::Look::Chip) {
            acts.push(Act::Ask(owner.clone(), thread.id));
        }
        if place(rux, "Resolve", Some(Icon::Tick), kit::Look::Chip) {
            acts.push(Act::Resolve(owner.clone(), thread.id));
        }
        if place(rux, "Reply", None, kit::Look::Ghost) {
            acts.push(Act::Reply(owner.clone(), thread.id));
        }
    } else if place(rux, "Reopen", Some(Icon::Rerun), kit::Look::Chip) {
        acts.push(Act::Reopen(owner.clone(), thread.id));
    }
    if has_a_selection || thread.detached {
        x.set(left);
        row.set(row.get() + (BUTTON + 6.0) * scale);
        if place(rux, "Move to selection", None, kit::Look::Ghost) {
            acts.push(Act::MoveToSelection(owner.clone(), thread.id));
        }
        if place(rux, "Delete", None, kit::Look::Ghost) {
            acts.push(Act::Delete(owner.clone(), thread.id));
        }
    } else {
        // Delete is always reachable, so it shares the first row when there is room.
        if place(rux, "Delete", None, kit::Look::Ghost) {
            acts.push(Act::Delete(owner, thread.id));
        }
    }
}
