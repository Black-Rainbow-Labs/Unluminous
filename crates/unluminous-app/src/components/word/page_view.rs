//! The pages: white sheets one under another on the tab's ground, what is laid out on each, the
//! selection behind the text and the caret over it, and the pointer.
//!
//! Only the pages on the screen are drawn. A glyph is drawn from the window's own atlas at the size it is
//! seen at, through `WordFonts::drawn`, which is the size the layout measured it at times the zoom.

use egui::{Color32, Mesh, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use unluminous_word::layout::{CaretLine, Item, RuleKind};
use unluminous_word::position::{Pos, Selection, StoryId};

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::fonts::WordFonts;
use crate::services::word::Doc;

/// Space between two pages, and above the first, in screen points.
pub const GAP: f32 = 18.0;

/// Where each page's top left corner is, in screen points relative to the top of the scrolled content,
/// and how tall all of it is.
pub fn page_tops(doc: &Doc, width: f32) -> (Vec<Pos2>, f32) {
    let zoom = doc.view.zoom;
    let mut y = GAP;
    let mut tops = Vec::with_capacity(doc.pages.pages.len());
    for page in &doc.pages.pages {
        let w = page.width * zoom;
        let x = ((width - w) / 2.0).max(GAP);
        tops.push(Pos2::new(x, y));
        y += page.height * zoom + GAP;
    }
    (tops, y)
}

/// The page and the point on it under a screen position.
pub fn page_point(doc: &Doc, area: Rect, at: Pos2) -> Option<(usize, f32, f32)> {
    let (tops, _) = page_tops(doc, area.width());
    let zoom = doc.view.zoom;
    let local = Vec2::new(at.x - area.left(), at.y - area.top() + doc.view.scroll);
    let mut best = None;
    for (index, (top, page)) in tops.iter().zip(&doc.pages.pages).enumerate() {
        let bottom = top.y + page.height * zoom + GAP / 2.0;
        if local.y <= bottom || index + 1 == tops.len() {
            best = Some((index, (local.x - top.x) / zoom, (local.y - top.y) / zoom));
            break;
        }
    }
    best
}

/// The stories a click may land in: the body, and a header or footer only when the caret is in one,
/// which is how Word keeps a click on the page from wandering into the header.
fn clickable(selection: &Selection) -> Vec<StoryId> {
    match &selection.head.story {
        StoryId::Body => vec![StoryId::Body],
        other => vec![other.clone()],
    }
}

/// Draw the pages and take the pointer.
pub fn show(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, area: Rect, requests: &mut Vec<Request>) {
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0.0, look.ground(look.palette.board_page));
    let response = ui.interact(area, ui.id().with(("word-pages", &doc.key)), Sense::click_and_drag());
    scroll_and_zoom(ui, doc, area, &response);
    pointer(ui, doc, area, &response, requests);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    let (tops, total) = page_tops(doc, area.width());
    doc.view.scroll = doc.view.scroll.clamp(0.0, (total - area.height()).max(0.0));
    if doc.view.reveal {
        reveal(doc, area, &tops);
        doc.view.reveal = false;
    }
    let fonts = WordFonts::new(look.renderer);
    let zoom = doc.view.zoom;
    let selection = doc.session.selection.clone();
    let showing: Vec<(usize, Pos2, Rect)> = tops
        .iter()
        .zip(&doc.pages.pages)
        .enumerate()
        .map(|(index, (top, page))| {
            let origin = area.min + Vec2::new(top.x, top.y - doc.view.scroll);
            (index, origin, Rect::from_min_size(origin, Vec2::new(page.width * zoom, page.height * zoom)))
        })
        .filter(|(_, _, rect)| rect.bottom() >= area.top() && rect.top() <= area.bottom())
        .collect();
    for (index, origin, rect) in showing {
        let page = &doc.pages.pages[index];
        // The sheet, with a soft edge under it.
        painter.rect_filled(rect.translate(Vec2::new(0.0, 2.0)).expand(1.0), 2.0, Color32::from_black_alpha(70));
        let paper = page.background.map_or(Color32::WHITE, |c| Color32::from_rgb(c.0, c.1, c.2));
        painter.rect_filled(rect, 0.0, paper);
        let to_screen = |x: f32, y: f32| origin + Vec2::new(x * zoom, y * zoom);
        paint_selection(&painter, page, &selection, zoom, origin);
        if doc.view.marks {
            paint_marks(&painter, page, to_screen);
        }
        paint_items(ui, &painter, &fonts, doc, index, origin, zoom);
    }
    if look.has_the_keyboard {
        paint_caret(ui, &painter, doc, area, &tops);
    }
}

/// Paint one page's items: fills first, then pictures, text and rules.
fn paint_items(ui: &egui::Ui, painter: &egui::Painter, fonts: &WordFonts, doc: &mut Doc, index: usize, origin: Pos2, zoom: f32) {
    let renderer = fonts.renderer;
    let page = &doc.pages.pages[index];
    let to_screen = |x: f32, y: f32| origin + Vec2::new(x * zoom, y * zoom);
    let mut pictures: Vec<(Rect, String)> = Vec::new();
    for item in &page.items {
        match item {
            Item::Fill { rect, color } => {
                let r = Rect::from_min_size(to_screen(rect.x, rect.y), Vec2::new(rect.w * zoom, rect.h * zoom));
                painter.rect_filled(r, 0.0, Color32::from_rgb(color.0, color.1, color.2));
            }
            Item::Picture { rect, part } => {
                pictures.push((Rect::from_min_size(to_screen(rect.x, rect.y), Vec2::new(rect.w * zoom, rect.h * zoom)), part.clone()));
            }
            Item::Placeholder { rect, label } => {
                let r = Rect::from_min_size(to_screen(rect.x, rect.y), Vec2::new(rect.w * zoom, rect.h * zoom));
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
                painter.text(r.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), Color32::GRAY);
            }
            _ => {}
        }
    }
    for (rect, part) in pictures {
        let texture = picture_texture(ui, doc, &part);
        match texture {
            Some(texture) => {
                painter.image(texture.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
            }
        }
    }
    let page = &doc.pages.pages[index];
    // Every glyph on the page is collected before the atlas texture is taken, and the pass is repeated when
    // the atlas filled and moved glyphs already collected. `editor_view::paint_text` explains why.
    let mut placed: Vec<(Rect, Rect, Color32)> = Vec::new();
    for _ in 0..3 {
        let generation = renderer.generation();
        placed.clear();
        for item in &page.items {
            let Item::Text(text) = item else { continue };
            let mut style = fonts.drawn(&text.style);
            style.size *= zoom;
            let color = Color32::from_rgb(text.style.color.r, text.style.color.g, text.style.color.b);
            for (character, x) in text.text.chars().zip(&text.xs) {
                if character.is_whitespace() {
                    continue;
                }
                let Some(glyph) = renderer.glyph(character, &style) else { continue };
                let at = to_screen(*x, text.baseline) + glyph.offset;
                let at = renderer.crispness().snap(at);
                placed.push((Rect::from_min_size(at, glyph.size), glyph.uv, color));
            }
        }
        if renderer.generation() == generation {
            break;
        }
    }
    let texture = renderer.texture(ui.ctx());
    let mut mesh = Mesh::with_texture(texture);
    for (rect, uv, color) in placed {
        mesh.add_rect_with_uv(rect, uv, color);
    }
    if !mesh.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
    for item in &page.items {
        if let Item::Rule { x0, y0, x1, y1, width, color, kind } = item {
            let color = Color32::from_rgb(color.0, color.1, color.2);
            let stroke = Stroke::new((width * zoom).max(0.6), color);
            let (a, b) = (to_screen(*x0, *y0), to_screen(*x1, *y1));
            match kind {
                RuleKind::Solid => {
                    painter.line_segment([a, b], stroke);
                }
                RuleKind::Double => {
                    painter.line_segment([a, b], stroke);
                    let gap = Vec2::new(0.0, (width * 2.0 * zoom).max(2.0));
                    painter.line_segment([a + gap, b + gap], stroke);
                }
                RuleKind::Dotted | RuleKind::Dashed => {
                    let (on, off) = if *kind == RuleKind::Dotted { (1.0, 1.5) } else { (4.0, 2.5) };
                    painter.extend(Shape::dashed_line(&[a, b], stroke, on * zoom.max(1.0), off * zoom.max(1.0)));
                }
                RuleKind::Wave => {
                    let length = (b.x - a.x).max(0.0);
                    let step = 2.0 * zoom.max(1.0);
                    let points: Vec<Pos2> = (0..=((length / step) as usize))
                        .map(|n| Pos2::new(a.x + n as f32 * step, a.y + if n % 2 == 0 { -step / 2.0 } else { step / 2.0 }))
                        .collect();
                    painter.add(Shape::line(points, stroke));
                }
            }
        }
    }
}

/// The texture for a picture part, decoded once.
fn picture_texture(ui: &egui::Ui, doc: &mut Doc, part: &str) -> Option<egui::TextureHandle> {
    if let Some(found) = doc.view.pictures.get(part) {
        return found.clone();
    }
    let texture = doc
        .session
        .document
        .package
        .part(part)
        .and_then(|bytes| crate::services::picture::decode_bytes(bytes).ok())
        .map(|image| ui.ctx().load_texture(format!("word-picture-{}-{part}", doc.key), image, egui::TextureOptions::LINEAR));
    doc.view.pictures.insert(part.to_owned(), texture.clone());
    texture
}

/// The selection, as a band behind each line it covers.
fn paint_selection(painter: &egui::Painter, page: &unluminous_word::layout::Page, selection: &Selection, zoom: f32, origin: Pos2) {
    if selection.is_empty() {
        return;
    }
    let (from, to) = selection.ordered();
    let colour = Color32::from_rgb(0xB4, 0xD5, 0xFE);
    for line in &page.carets {
        if line.story != from.story {
            continue;
        }
        let Some((x0, x1)) = covered(line, from, to) else { continue };
        let rect = Rect::from_min_max(origin + Vec2::new(x0 * zoom, line.top * zoom), origin + Vec2::new(x1.max(x0 + 3.0) * zoom, (line.top + line.height) * zoom));
        painter.rect_filled(rect, 0.0, colour);
    }
}

/// How much of a line a selection covers, as the x where it starts and ends.
pub fn covered(line: &CaretLine, from: &Pos, to: &Pos) -> Option<(f32, f32)> {
    let path = &line.path;
    if path < &from.path || path > &to.path {
        return None;
    }
    let first = line.stops.first()?.1;
    let last = line.stops.last()?.1;
    let start = if *path == from.path {
        if from.offset > line.end && !line.last {
            return None;
        }
        if from.offset <= line.start { first } else { line.x_of(from.offset) }
    } else {
        first
    };
    let end = if *path == to.path {
        if to.offset < line.start || (to.offset == line.start && to.offset != from.offset && line.start != line.end && *path != from.path) {
            return None;
        }
        if to.offset >= line.end { last } else { line.x_of(to.offset) }
    } else {
        // A paragraph wholly inside the selection shows its mark selected too, as a little more width.
        last + if line.last { 4.0 } else { 0.0 }
    };
    (end > start || (*path != to.path)).then_some((start, end))
}

/// Paragraph marks at the end of each paragraph, when formatting marks are shown.
fn paint_marks(painter: &egui::Painter, page: &unluminous_word::layout::Page, to_screen: impl Fn(f32, f32) -> Pos2) {
    for line in page.carets.iter().filter(|l| l.last) {
        let x = line.stops.last().map_or(0.0, |s| s.1);
        painter.text(to_screen(x + 1.0, line.baseline), egui::Align2::LEFT_BOTTOM, "¶", egui::FontId::proportional(10.0), Color32::from_rgb(0x7A, 0x8C, 0xC8));
    }
}

/// The caret, and the place an input method's window should open.
fn paint_caret(ui: &egui::Ui, painter: &egui::Painter, doc: &Doc, area: Rect, tops: &[Pos2]) {
    let Some((index, line)) = doc.pages.find(&doc.session.selection.head) else { return };
    let zoom = doc.view.zoom;
    let origin = area.min + Vec2::new(tops[index].x, tops[index].y - doc.view.scroll);
    let x = line.x_of(doc.session.selection.head.offset);
    let top = origin + Vec2::new(x * zoom, line.top * zoom);
    let bottom = origin + Vec2::new(x * zoom, (line.top + line.height) * zoom);
    if area.contains(top) || area.contains(bottom) {
        painter.line_segment([top, bottom], Stroke::new(1.5, Color32::BLACK));
        let rect = Rect::from_min_max(top, bottom + Vec2::new(1.0, 0.0));
        ui.ctx().output_mut(|output| {
            output.ime = Some(egui::output::IMEOutput { purpose: egui::IMEPurpose::Normal, rect: area, cursor_rect: rect, should_interrupt_composition: false })
        });
    }
}

/// Scroll the caret onto the screen.
fn reveal(doc: &mut Doc, area: Rect, tops: &[Pos2]) {
    let Some((index, line)) = doc.pages.find(&doc.session.selection.head) else { return };
    let zoom = doc.view.zoom;
    let top = tops[index].y + line.top * zoom;
    let bottom = top + line.height * zoom;
    let margin = 40.0;
    if top < doc.view.scroll + margin {
        doc.view.scroll = (top - margin).max(0.0);
    } else if bottom > doc.view.scroll + area.height() - margin {
        doc.view.scroll = bottom - area.height() + margin;
    }
}

/// The wheel scrolls and, with Ctrl or Cmd held, zooms around the pointer.
fn scroll_and_zoom(ui: &egui::Ui, doc: &mut Doc, area: Rect, response: &egui::Response) {
    if !response.hovered() && !response.contains_pointer() {
        return;
    }
    let (delta, zoom_delta, command) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.command));
    if command || (zoom_delta - 1.0).abs() > 0.001 {
        let factor = if (zoom_delta - 1.0).abs() > 0.001 { zoom_delta } else { (delta.y / 400.0).exp() };
        if (factor - 1.0).abs() > 0.0005 {
            let pointer_y = ui.input(|i| i.pointer.hover_pos()).map_or(area.center().y, |p| p.y) - area.top();
            let before = doc.view.zoom;
            doc.view.zoom = (doc.view.zoom * factor).clamp(0.25, 5.0);
            let ratio = doc.view.zoom / before;
            doc.view.scroll = (doc.view.scroll + pointer_y) * ratio - pointer_y;
            ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        }
        return;
    }
    if delta.y != 0.0 {
        doc.view.scroll -= delta.y;
        ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
    }
}

/// Presses, drags, double and triple clicks on the pages.
fn pointer(ui: &egui::Ui, doc: &mut Doc, area: Rect, response: &egui::Response, requests: &mut Vec<Request>) {
    let shift = ui.input(|i| i.modifiers.shift);
    if response.drag_started() || response.clicked() || response.is_pointer_button_down_on() && ui.input(|i| i.pointer.any_pressed()) {
        requests.push(Request::TakeTheKeyboard(true));
        ui.ctx().memory_mut(|m| m.stop_text_input());
    }
    let pressed = ui.input(|i| i.pointer.primary_pressed()) && response.hovered();
    let Some(at) = ui.input(|i| i.pointer.interact_pos()) else { return };
    let stories = clickable(&doc.session.selection);
    let hit = |doc: &Doc| page_point(doc, area, at).and_then(|(page, x, y)| doc.pages.hit(page, x, y, &stories));
    if pressed {
        doc.view.slash = None;
        let Some(pos) = hit(doc) else { return };
        let now = ui.input(|i| i.time);
        let (last, place, count) = doc.view.clicks;
        let count = if now - last < 0.45 && place.distance(at) < 6.0 { count % 3 + 1 } else { 1 };
        doc.view.clicks = (now, at, count);
        doc.view.goal_x = None;
        match count {
            2 => select_word(doc, &pos),
            3 => select_paragraph(doc, &pos),
            _ if shift => {
                let anchor = doc.session.selection.anchor.clone();
                doc.session.select(anchor, pos);
            }
            _ => doc.session.select(pos.clone(), pos),
        }
        doc.view.dragging = true;
        return;
    }
    if doc.view.dragging && response.dragged() {
        if let Some(pos) = hit(doc) {
            let anchor = doc.session.selection.anchor.clone();
            if pos.story == anchor.story {
                doc.session.select(anchor, pos);
            }
        }
        // Dragging past the edge scrolls.
        if at.y > area.bottom() - 10.0 {
            doc.view.scroll += 12.0;
        } else if at.y < area.top() + 10.0 {
            doc.view.scroll -= 12.0;
        }
    }
    if ui.input(|i| i.pointer.primary_released()) {
        doc.view.dragging = false;
    }
}

/// Double click: the word under the pointer.
pub fn select_word(doc: &mut Doc, pos: &Pos) {
    let Some(paragraph) = unluminous_word::position::paragraph(&doc.session.document, &pos.story, &pos.path) else { return };
    let text = paragraph.flat_text();
    let offset = pos.offset.min(text.len());
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
    let start = text[..offset].char_indices().rev().take_while(|(_, c)| is_word(*c)).last().map_or(offset, |(i, _)| i);
    let end = text[offset..].char_indices().find(|(_, c)| !is_word(*c)).map_or(text.len(), |(i, _)| offset + i);
    // A word takes the spaces after it, as Word's double click does.
    let end = end + text[end..].chars().take_while(|c| *c == ' ').count();
    doc.session.select(pos.at(start), pos.at(end));
}

/// Triple click: the paragraph.
pub fn select_paragraph(doc: &mut Doc, pos: &Pos) {
    let len = unluminous_word::position::paragraph(&doc.session.document, &pos.story, &pos.path).map_or(0, |p| p.len());
    doc.session.select(pos.at(0), pos.at(len));
}
