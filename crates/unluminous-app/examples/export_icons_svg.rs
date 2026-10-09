//! Write every drawn mark in `theme::icon` out as an SVG file, one per mark and per icon set.
//!
//! The marks are egui painter calls rather than pictures, which is right for the window and leaves a
//! web design tool nothing to work from. This runs each mark through a real `egui::Context`, takes the
//! shapes it painted and writes them as SVG at the mark's own size: a 24 point cell with the mark
//! centred on 12,12, which is the cell `tests/icons.rs` draws its sheet in. A mark redesigned on the web
//! is then compared against exactly what the Rust draws, and carried back as painter calls.
//!
//! ```sh
//! cargo run -p unluminous-app --example export_icons_svg -- <output folder>
//! ```
//!
//! It writes `<folder>/material/<name>.svg` and `<folder>/classic/<name>.svg`. A mark that has one
//! drawing is the same in both folders. The ink is `#1E2530`; the colour wheel keeps its own hues.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use egui::epaint::{ColorMode, Shape};
use egui::{Color32, Pos2, Rect, Vec2};
use unluminous_app::theme::{icon, IconSet};

/// The side of one mark's cell, in points. The rail's own buttons are twenty four.
const CELL: f32 = 24.0;
/// The pixel density the marks are drawn at. The pixel grid helpers snap to whole pixels, so a high
/// density keeps every point within an eighth of a point of where the drawing asked for it.
const DENSITY: f32 = 8.0;
/// The ink every mark is drawn in.
const INK: Color32 = Color32::from_rgb(0x1E, 0x25, 0x30);

/// One mark: its file name and what draws it centred on a point.
type Mark = (&'static str, Box<dyn Fn(&egui::Painter, Pos2)>);

/// Every mark, in the order `tests/icons.rs` reads them, followed by the ones that take a parameter.
fn marks() -> Vec<Mark> {
    use unluminous_core::{Align, SymbolKind};
    let simple: Vec<(&'static str, fn(&egui::Painter, Pos2, Color32))> = vec![
        ("folder", icon::folder),
        ("file", icon::editing_area),
        ("file-page", icon::file_page),
        ("text-page", icon::text_page),
        ("branch", icon::branch),
        ("realm", icon::realm),
        ("photo", icon::photo),
        ("audio", icon::audio),
        ("video", icon::video),
        ("unknown", icon::unknown),
        ("chat", icon::chat),
        ("board", icon::board),
        ("database", icon::database),
        ("table", icon::table),
        ("terminal", icon::terminal),
        ("run", icon::run),
        ("bug", icon::bug),
        ("debug-run", icon::debug_run),
        ("stop", icon::stop),
        ("rerun", icon::rerun),
        ("resume", icon::resume),
        ("clear", icon::clear),
        ("magnifier", icon::magnifier),
        ("crosshair", icon::crosshair),
        ("whole-page", icon::whole_page),
        ("plus", icon::plus),
        ("cross", icon::cross),
        ("tick", icon::tick),
        ("wrench", icon::wrench),
        ("bin", icon::bin),
        ("clock", icon::clock),
        ("copy", icon::copy),
        ("comment", icon::comment),
        ("image", icon::image),
        ("font", icon::font),
        ("undo", icon::undo),
        ("key", icon::key),
        ("stack", icon::stack),
        ("diamond", icon::diamond),
        ("zoom-in", icon::zoom_in),
        ("zoom-out", icon::zoom_out),
        ("chevron-down", icon::chevron_down),
        ("chevron-up", icon::chevron_up),
        ("more", icon::more),
        ("collapse", icon::collapse),
        ("line-spacing", icon::line_spacing),
        ("color-wheel", icon::color_wheel),
        ("state-dot", icon::state_dot),
        ("breakpoint-badge", icon::breakpoint_badge),
    ];
    let mut all: Vec<Mark> = simple
        .into_iter()
        .map(|(name, draw)| {
            (
                name,
                Box::new(move |p: &egui::Painter, c: Pos2| draw(p, c, INK))
                    as Box<dyn Fn(&egui::Painter, Pos2)>,
            )
        })
        .collect();
    let with: Vec<Mark> = vec![
        ("disclosure-closed", Box::new(|p, c| icon::disclosure(p, c, false, INK))),
        ("disclosure-open", Box::new(|p, c| icon::disclosure(p, c, true, INK))),
        ("folder-mark-closed", Box::new(|p, c| icon::folder_mark(p, c, false, INK, 1.0))),
        ("folder-mark-open", Box::new(|p, c| icon::folder_mark(p, c, true, INK, 1.0))),
        ("file-mark-prose", Box::new(|p, c| icon::file_mark(p, c, true, INK, 1.0))),
        ("file-mark-code", Box::new(|p, c| icon::file_mark(p, c, false, INK, 1.0))),
        ("redo", Box::new(|p, c| icon::undo_redo(p, c, true, INK))),
        ("breakpoint", Box::new(|p, c| icon::breakpoint(p, c, true, INK))),
        ("breakpoint-off", Box::new(|p, c| icon::breakpoint(p, c, false, INK))),
        ("step-over", Box::new(|p, c| icon::step(p, c, icon::StepIcon::Over, INK))),
        ("step-into", Box::new(|p, c| icon::step(p, c, icon::StepIcon::Into, INK))),
        ("step-out", Box::new(|p, c| icon::step(p, c, icon::StepIcon::Out, INK))),
        ("symbol-function", Box::new(|p, c| icon::symbol_kind(p, c, SymbolKind::Function, INK))),
        ("symbol-type", Box::new(|p, c| icon::symbol_kind(p, c, SymbolKind::Type, INK))),
        ("symbol-constant", Box::new(|p, c| icon::symbol_kind(p, c, SymbolKind::Constant, INK))),
        ("symbol-variable", Box::new(|p, c| icon::symbol_kind(p, c, SymbolKind::Variable, INK))),
        ("symbol-module", Box::new(|p, c| icon::symbol_kind(p, c, SymbolKind::Module, INK))),
        (
            "align-left",
            Box::new(|p, c| {
                icon::alignment(p, Rect::from_center_size(c, Vec2::splat(12.0)), Align::Left, INK)
            }),
        ),
        (
            "align-centre",
            Box::new(|p, c| {
                icon::alignment(p, Rect::from_center_size(c, Vec2::splat(12.0)), Align::Center, INK)
            }),
        ),
        (
            "align-right",
            Box::new(|p, c| {
                icon::alignment(p, Rect::from_center_size(c, Vec2::splat(12.0)), Align::Right, INK)
            }),
        ),
        (
            "align-justify",
            Box::new(|p, c| {
                icon::alignment(
                    p,
                    Rect::from_center_size(c, Vec2::splat(12.0)),
                    Align::Justify,
                    INK,
                )
            }),
        ),
        (
            "view-raw",
            Box::new(|p, c| {
                icon::view_mode(
                    p,
                    Rect::from_center_size(c, Vec2::splat(18.0)),
                    unluminous_app::ViewMode::Raw,
                    INK,
                )
            }),
        ),
        (
            "view-side-by-side",
            Box::new(|p, c| {
                icon::view_mode(
                    p,
                    Rect::from_center_size(c, Vec2::splat(18.0)),
                    unluminous_app::ViewMode::SideBySide,
                    INK,
                )
            }),
        ),
        (
            "view-preview",
            Box::new(|p, c| {
                icon::view_mode(
                    p,
                    Rect::from_center_size(c, Vec2::splat(18.0)),
                    unluminous_app::ViewMode::Preview,
                    INK,
                )
            }),
        ),
    ];
    all.extend(with);
    all
}

/// Draw one mark in a frame of its own and return the shapes it painted, in points.
fn shapes_of(ctx: &egui::Context, set: IconSet, draw: &dyn Fn(&egui::Painter, Pos2)) -> Vec<Shape> {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(CELL))),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| {
        let ctx = ui.ctx();
        let mut theme = unluminous_app::theme::active();
        theme.icons = set;
        unluminous_app::theme::activate(theme);
        let painter = ctx.layer_painter(egui::LayerId::background());
        draw(&painter, Pos2::new(CELL / 2.0, CELL / 2.0));
    });
    let mut output = output;
    // Nothing uploads the font atlas here, so the delta egui hands back is cleared rather than dropped.
    output.textures_delta.clear();
    output.shapes.into_iter().map(|clipped| clipped.shape).collect()
}

/// A colour as SVG writes it, with its own opacity attribute when it is not opaque.
fn paint(attribute: &str, colour: Color32) -> String {
    if colour == Color32::TRANSPARENT {
        return format!(" {attribute}=\"none\"");
    }
    let [r, g, b, a] = colour.to_srgba_unmultiplied();
    let mut out = format!(" {attribute}=\"#{r:02x}{g:02x}{b:02x}\"");
    if a < 255 {
        let _ = write!(out, " {attribute}-opacity=\"{:.3}\"", f32::from(a) / 255.0);
    }
    out
}

/// A point list as SVG writes it.
fn points(list: &[Pos2]) -> String {
    list.iter().map(|p| format!("{:.3},{:.3}", p.x, p.y)).collect::<Vec<_>>().join(" ")
}

/// One egui shape as SVG elements.
fn svg_of(shape: &Shape, out: &mut String) {
    match shape {
        Shape::Noop | Shape::Text(_) | Shape::Callback(_) => {}
        Shape::Vec(shapes) => shapes.iter().for_each(|s| svg_of(s, out)),
        Shape::Circle(c) => {
            let _ = writeln!(
                out,
                "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\"{}{} stroke-width=\"{:.3}\"/>",
                c.center.x,
                c.center.y,
                c.radius,
                paint("fill", c.fill),
                paint("stroke", c.stroke.color),
                c.stroke.width
            );
        }
        Shape::Ellipse(e) => {
            let _ = writeln!(
                out,
                "<ellipse cx=\"{:.3}\" cy=\"{:.3}\" rx=\"{:.3}\" ry=\"{:.3}\"{}{} stroke-width=\"{:.3}\"/>",
                e.center.x, e.center.y, e.radius.x, e.radius.y, paint("fill", e.fill), paint("stroke", e.stroke.color), e.stroke.width
            );
        }
        Shape::LineSegment { points: [a, b], stroke } => {
            let _ = writeln!(
                out,
                "<line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\"{} stroke-width=\"{:.3}\" stroke-linecap=\"round\"/>",
                a.x, a.y, b.x, b.y, paint("stroke", stroke.color), stroke.width
            );
        }
        Shape::Path(p) => {
            let stroke = match p.stroke.color {
                ColorMode::Solid(colour) => colour,
                _ => INK,
            };
            let element = if p.closed { "polygon" } else { "polyline" };
            let _ = writeln!(
                out,
                "<{element} points=\"{}\"{}{} stroke-width=\"{:.3}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>",
                points(&p.points),
                paint("fill", p.fill),
                paint("stroke", if p.stroke.width > 0.0 { stroke } else { Color32::TRANSPARENT }),
                p.stroke.width
            );
        }
        Shape::Rect(r) => {
            let radius = f32::from(r.corner_radius.nw);
            let _ = writeln!(
                out,
                "<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" rx=\"{:.3}\"{}{} stroke-width=\"{:.3}\"/>",
                r.rect.min.x, r.rect.min.y, r.rect.width(), r.rect.height(), radius, paint("fill", r.fill), paint("stroke", r.stroke.color), r.stroke.width
            );
        }
        Shape::Mesh(mesh) => {
            for triangle in mesh.indices.chunks(3) {
                let [a, b, c] =
                    [triangle[0], triangle[1], triangle[2]].map(|i| &mesh.vertices[i as usize]);
                let _ = writeln!(
                    out,
                    "<polygon points=\"{}\"{}/>",
                    points(&[a.pos, b.pos, c.pos]),
                    paint("fill", a.color)
                );
            }
        }
        Shape::QuadraticBezier(q) => {
            let [a, b, c] = q.points;
            let _ = writeln!(
                out,
                "<path d=\"M{:.3} {:.3} Q{:.3} {:.3} {:.3} {:.3}{}\"{}{} stroke-width=\"{:.3}\" stroke-linecap=\"round\"/>",
                a.x, a.y, b.x, b.y, c.x, c.y, if q.closed { " Z" } else { "" }, paint("fill", q.fill),
                paint("stroke", match q.stroke.color { ColorMode::Solid(c) => c, _ => INK }), q.stroke.width
            );
        }
        Shape::CubicBezier(q) => {
            let [a, b, c, d] = q.points;
            let _ = writeln!(
                out,
                "<path d=\"M{:.3} {:.3} C{:.3} {:.3} {:.3} {:.3} {:.3} {:.3}{}\"{}{} stroke-width=\"{:.3}\" stroke-linecap=\"round\"/>",
                a.x, a.y, b.x, b.y, c.x, c.y, d.x, d.y, if q.closed { " Z" } else { "" }, paint("fill", q.fill),
                paint("stroke", match q.stroke.color { ColorMode::Solid(c) => c, _ => INK }), q.stroke.width
            );
        }
    }
}

/// Write one set's marks into a folder of their own.
fn write_set(ctx: &egui::Context, set: IconSet, folder: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(folder)?;
    let mut count = 0;
    for (name, draw) in marks() {
        let mut body = String::new();
        for shape in shapes_of(ctx, set, draw.as_ref()) {
            svg_of(&shape, &mut body);
        }
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {CELL} {CELL}\" width=\"{CELL}\" height=\"{CELL}\">\n<!-- unluminous theme::icon, {} set: {name} -->\n{body}</svg>\n",
            set.name()
        );
        std::fs::write(folder.join(format!("{name}.svg")), svg)?;
        count += 1;
    }
    Ok(count)
}

fn main() -> std::io::Result<()> {
    let folder = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("_agent_output/icons-svg"));
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(DENSITY);
    for set in [IconSet::Material, IconSet::Classic] {
        let count = write_set(&ctx, set, &folder.join(set.name()))?;
        println!("{}: {count} marks", set.name());
    }
    Ok(())
}
