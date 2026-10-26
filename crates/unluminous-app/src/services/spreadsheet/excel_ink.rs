//! The colours Excel itself draws a sheet's furniture in: icon sets, trace arrows, the error triangle, a
//! note's paper, page breaks. They belong to the workbook's page, the way a syntax token's colour belongs to
//! a language plugin, so they are named here once rather than chosen in a component. Each value was read off
//! Excel 16 drawing the same thing.

use egui::Color32;

/// An icon in a set of fewer than two icons, which has no low or high end.
pub const ICON_NEUTRAL: Color32 = Color32::from_rgb(0x9B, 0x9B, 0x9B);
/// The low third of an icon set.
pub const ICON_LOW: Color32 = Color32::from_rgb(0xE0, 0x4B, 0x4B);
/// The middle third of an icon set.
pub const ICON_MIDDLE: Color32 = Color32::from_rgb(0xE8, 0xB8, 0x2E);
/// The high third of an icon set.
pub const ICON_HIGH: Color32 = Color32::from_rgb(0x3C, 0xB3, 0x71);
/// A bar of a signal icon that is not lit.
pub const ICON_UNLIT: Color32 = Color32::from_gray(0xB0);
/// The rim round a symbol or sign icon.
pub const ICON_RIM: Color32 = Color32::from_gray(0x40);
/// A trace arrow that leads to an error.
pub const ARROW_ERROR: Color32 = Color32::from_rgb(0xD0, 0x30, 0x30);
/// A trace arrow between precedents and dependents.
pub const ARROW: Color32 = Color32::from_rgb(0x2A, 0x5F, 0xC9);
/// The triangle in the corner of a cell Excel's error checking flags.
pub const ERROR_TRIANGLE: Color32 = Color32::from_rgb(0x21, 0x9E, 0x5A);
/// The line from a note to its cell.
pub const NOTE_LEADER: Color32 = Color32::from_gray(0x80);
/// A note's paper.
pub const NOTE_PAPER: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xE1);
/// A note's edge.
pub const NOTE_EDGE: Color32 = Color32::from_gray(0x60);
/// The words on a note.
pub const NOTE_INK: Color32 = Color32::from_rgb(0, 0, 0);
/// The grip at a note's corner.
pub const NOTE_GRIP: Color32 = Color32::from_gray(0x90);
/// Page Break Preview's page lines and page numbers.
pub const PAGE_BREAK: Color32 = Color32::from_rgb(0x1F, 0x6F, 0xD0);
/// The shadow under a sheet of paper in Page Layout view.
pub const PAPER_SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 50);
/// A sparkline's negative points when the group names no colour.
pub const SPARK_NEGATIVE: Color32 = Color32::from_rgb(0xD0, 0, 0);
/// A shape filled with a gradient or pattern this version does not draw.
pub const UNDRAWN_FILL: Color32 = Color32::from_rgba_premultiplied(45, 45, 45, 90);
/// The theme's dark text colour, `tx1` and `dk1`, when the workbook's theme does not say.
pub const THEME_DARK: Color32 = Color32::from_rgb(0, 0, 0);
/// The theme's light background colour, `bg1` and `lt1`, when the workbook's theme does not say.
pub const THEME_LIGHT: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
/// A sheet of paper in Page Layout view and print preview, which is white on any theme.
pub const PAPER: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
/// The edge of a sheet of paper in Page Layout view.
pub const PAPER_EDGE: Color32 = Color32::from_gray(0xC0);
/// The shadow under a page in print preview.
pub const PREVIEW_SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 80);
