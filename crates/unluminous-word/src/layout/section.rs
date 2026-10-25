//! A section's page: its size, margins, columns, how it starts, and which headers and footers it uses.

use crate::props::{number, twips};
use crate::xml::El;

/// How a section starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    NextPage,
    Continuous,
    EvenPage,
    OddPage,
    NextColumn,
}

/// One column's width and the gap after it, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    pub width: f32,
    pub space: f32,
}

/// A section's page, read from its `w:sectPr`.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub width: f32,
    pub height: f32,
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    pub header: f32,
    pub footer: f32,
    pub gutter: f32,
    pub start: Start,
    pub columns: Vec<Column>,
    pub title_page: bool,
    /// `(type, relationship id)` of each `w:headerReference`: `default`, `first` or `even`.
    pub headers: Vec<(String, String)>,
    pub footers: Vec<(String, String)>,
    /// `w:pgNumType w:start`, when the section restarts its page numbers.
    pub page_number_start: Option<u32>,
    /// `w:pgNumType w:fmt`.
    pub page_number_format: String,
    pub vertical_center: bool,
    /// `w:vAlign`: `top`, `center`, `both` or `bottom`.
    pub vertical_align: String,
    /// `w:pgBorders`: the line width in points, its colour, its distance in points, and whether the
    /// distance is from the text rather than from the page's edge.
    pub page_border: Option<(f32, crate::props::Rgb, f32, bool)>,
    /// `w:lnNumType`: count by, the first number, restart (`newPage`, `newSection`, `continuous`) and the
    /// distance from the text in points.
    pub line_numbers: Option<(u32, u32, String, f32)>,
    /// `w:cols w:sep`: a line between the columns.
    pub separator: bool,
}

impl Geometry {
    /// The page `sectPr` describes. A section with no `w:pgSz` is US Letter with Word's normal margins,
    /// which is what Word assumes too.
    pub fn of(sect: &El) -> Geometry {
        let size = |attr: &str, fallback: f64| -> f32 {
            twips(
                sect.child("w:pgSz")
                    .and_then(|el| el.attr(attr))
                    .and_then(number)
                    .unwrap_or(fallback),
            )
        };
        let margin = |attr: &str, fallback: f64| -> f32 {
            twips(
                sect.child("w:pgMar")
                    .and_then(|el| el.attr(attr))
                    .and_then(number)
                    .unwrap_or(fallback)
                    .abs(),
            )
        };
        let mut geometry = Geometry {
            width: size("w:w", 12_240.0),
            height: size("w:h", 15_840.0),
            top: margin("w:top", 1440.0),
            bottom: margin("w:bottom", 1440.0),
            left: margin("w:left", 1440.0),
            right: margin("w:right", 1440.0),
            header: margin("w:header", 720.0),
            footer: margin("w:footer", 720.0),
            gutter: margin("w:gutter", 0.0),
            start: match sect.val_of("w:type") {
                Some("continuous") => Start::Continuous,
                Some("evenPage") => Start::EvenPage,
                Some("oddPage") => Start::OddPage,
                Some("nextColumn") => Start::NextColumn,
                _ => Start::NextPage,
            },
            columns: Vec::new(),
            title_page: crate::props::on_off(sect, "w:titlePg") == Some(true),
            headers: references(sect, "w:headerReference"),
            footers: references(sect, "w:footerReference"),
            page_number_start: sect
                .child("w:pgNumType")
                .and_then(|el| el.attr("w:start"))
                .and_then(|start| start.parse().ok()),
            page_number_format: sect
                .child("w:pgNumType")
                .and_then(|el| el.attr("w:fmt"))
                .unwrap_or("decimal")
                .to_owned(),
            vertical_center: sect.val_of("w:vAlign") == Some("center"),
            vertical_align: sect.val_of("w:vAlign").unwrap_or("top").to_owned(),
            page_border: sect.child("w:pgBorders").and_then(|b| {
                let top = b.child("w:top").or_else(|| b.child("w:left"))?;
                if top.attr("w:val").is_some_and(|v| v == "nil" || v == "none") {
                    return None;
                }
                let width = top.attr("w:sz").and_then(number).map_or(0.5, |s| s as f32 / 8.0);
                let colour = top
                    .attr("w:color")
                    .and_then(crate::props::Rgb::parse)
                    .unwrap_or(crate::props::Rgb::BLACK);
                let space = top.attr("w:space").and_then(number).map_or(24.0, |s| s as f32);
                Some((width, colour, space, b.attr("w:offsetFrom") == Some("text")))
            }),
            line_numbers: sect.child("w:lnNumType").map(|l| {
                let read = |name: &str| l.attr(name).and_then(number);
                (
                    read("w:countBy").map_or(1, |n| n as u32).max(1),
                    read("w:start").map_or(1, |n| n as u32 + 1),
                    l.attr("w:restart").unwrap_or("newPage").to_owned(),
                    read("w:distance").map_or(18.0, twips),
                )
            }),
            separator: sect
                .child("w:cols")
                .is_some_and(|c| crate::props::on_off_attr(c.attr("w:sep"))),
        };
        geometry.columns = columns(sect, geometry.text_width());
        geometry
    }

    /// The width between the margins.
    pub fn text_width(&self) -> f32 {
        (self.width - self.left - self.right - self.gutter).max(36.0)
    }

    /// Whether this page is landscape.
    pub fn landscape(&self) -> bool {
        self.width > self.height
    }
}

fn references(sect: &El, name: &str) -> Vec<(String, String)> {
    sect.children_named(name)
        .filter_map(|el| {
            Some((el.attr("w:type").unwrap_or("default").to_owned(), el.attr("r:id")?.to_owned()))
        })
        .collect()
}

/// The columns of a section, which share the text width with a gap between them.
fn columns(sect: &El, text_width: f32) -> Vec<Column> {
    let Some(cols) = sect.child("w:cols") else {
        return vec![Column { width: text_width, space: 0.0 }];
    };
    let explicit: Vec<Column> = cols
        .children_named("w:col")
        .map(|col| Column {
            width: twips(col.attr("w:w").and_then(number).unwrap_or(0.0)),
            space: twips(col.attr("w:space").and_then(number).unwrap_or(0.0)),
        })
        .collect();
    let equal = cols.attr("w:equalWidth").is_none_or(|v| !matches!(v, "0" | "false" | "off"));
    if !explicit.is_empty() && !equal {
        return explicit;
    }
    let count = cols.attr("w:num").and_then(number).unwrap_or(1.0).clamp(1.0, 45.0) as usize;
    let space = twips(cols.attr("w:space").and_then(number).unwrap_or(720.0));
    let width = ((text_width - space * (count as f32 - 1.0)) / count as f32).max(18.0);
    (0..count).map(|_| Column { width, space }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_section_with_nothing_said_is_letter_with_inch_margins() {
        let geometry = Geometry::of(&El::new("w:sectPr"));
        assert_eq!((geometry.width, geometry.height), (612.0, 792.0));
        assert_eq!(geometry.text_width(), 468.0);
        assert_eq!(geometry.columns.len(), 1);
    }

    #[test]
    fn two_equal_columns_share_the_width_with_the_gap() {
        let sect = El::new("w:sectPr")
            .with_child(El::new("w:cols").with("w:num", "2").with("w:space", "720"));
        let geometry = Geometry::of(&sect);
        assert_eq!(geometry.columns.len(), 2);
        assert_eq!(geometry.columns[0].width, (468.0 - 36.0) / 2.0);
    }
}
