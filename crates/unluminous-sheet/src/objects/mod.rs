//! Pictures, shapes and the other objects a worksheet's drawing places over its cells, and sparklines.
//!
//! **An object keeps the XML it was read from.** Its `tree` is the element Excel wrote, and the fields of
//! [`DrawingObject`] are what a person can change. When the workbook is written, [`write::sync_drawings`]
//! compares the fields with what the tree says and patches only what differs, so an effect, a gradient or
//! a text style this version does not model is still there after a shape has been moved or recoloured.
//!
//! **The drawing part is written again only when an object changed** (`Sheet::drawing_changed`). A workbook
//! whose objects were not touched is saved with the drawing part as the bytes it was.
//!
//! Charts stay in the drawing part as the XML they are; the objects here are every other anchor, in the
//! order they are stacked, bottom first. Form controls, SmartArt, WordArt, icons, equations, OLE objects
//! and 3D models are kept as they are and located by their anchor.

pub mod geometry;
pub mod make;
pub mod props;
pub mod read;
pub mod sparkline;
pub mod write;

use crate::coord::CellRef;
use crate::model::Sheet;
use crate::xlsx::xmltree::Elem;
pub use geometry::{Rect, EMU_PER_PIXEL, EMU_PER_POINT};

/// The number an object is known by on its sheet: the `id` of its `cNvPr`.
pub type ObjectId = u32;

/// How an anchor places an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnchorKind {
    /// From one cell to another: the object grows and shrinks with the cells.
    #[default]
    TwoCell,
    /// From a cell, with a size of its own.
    OneCell,
    /// At a distance from the corner of the sheet.
    Absolute,
}

/// What happens to an object when the cells under it move or change size (Excel's Placement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// Move and size with cells.
    #[default]
    MoveAndSize,
    /// Move but do not size with cells.
    Move,
    /// Do not move or size with cells.
    Fixed,
}

impl Placement {
    pub fn attr(&self) -> &'static str {
        match self {
            Placement::MoveAndSize => "twoCell",
            Placement::Move => "oneCell",
            Placement::Fixed => "absolute",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "oneCell" => Placement::Move,
            "absolute" => Placement::Fixed,
            _ => Placement::MoveAndSize,
        }
    }
}

/// Where an object sits: the anchor element and the numbers in it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Anchor {
    pub kind: AnchorKind,
    pub placement: Placement,
    /// The top left cell and the offsets into it, in drawing units. Used by `TwoCell` and `OneCell`.
    pub from: (CellRef, i64, i64),
    /// The bottom right cell and the offsets into it. Used by `TwoCell`.
    pub to: (CellRef, i64, i64),
    /// The corner of an `Absolute` anchor.
    pub pos: (i64, i64),
    /// The size of a `OneCell` or `Absolute` anchor.
    pub ext: (i64, i64),
}

impl Anchor {
    /// The rectangle the anchor covers on a sheet.
    pub fn rect(&self, sheet: &Sheet) -> Rect {
        match self.kind {
            AnchorKind::TwoCell => {
                let (x0, y0) = geometry::point_of(sheet, self.from);
                let (x1, y1) = geometry::point_of(sheet, self.to);
                Rect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
            }
            AnchorKind::OneCell => {
                let (x, y) = geometry::point_of(sheet, self.from);
                Rect::new(x, y, self.ext.0, self.ext.1)
            }
            AnchorKind::Absolute => Rect::new(self.pos.0, self.pos.1, self.ext.0, self.ext.1),
        }
    }

    /// Move the anchor to cover a rectangle, keeping its kind.
    pub fn set_rect(&mut self, sheet: &Sheet, rect: Rect) {
        let rect = Rect::new(rect.x.max(0), rect.y.max(0), rect.w.max(0), rect.h.max(0));
        match self.kind {
            AnchorKind::TwoCell => {
                self.from = geometry::cell_at(sheet, rect.x, rect.y);
                self.to = geometry::cell_at(sheet, rect.right(), rect.bottom());
            }
            AnchorKind::OneCell => {
                self.from = geometry::cell_at(sheet, rect.x, rect.y);
                self.ext = (rect.w, rect.h);
            }
            AnchorKind::Absolute => {
                self.pos = (rect.x, rect.y);
                self.ext = (rect.w, rect.h);
            }
        }
    }
}

/// What an object is, as far as drawing and editing it go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Picture,
    /// A shape with a geometry from the preset list: rectangle, ellipse, arrow, and the rest.
    Shape,
    TextBox,
    /// A line or connector.
    Connector,
    Group,
    /// A button, check box, list box and the other form controls, kept as written.
    FormControl,
    SmartArt,
    /// Text shaped by a text warp.
    WordArt,
    /// A picture that carries an SVG alongside its raster stand in.
    Icon,
    Equation,
    OleObject,
    Model3D,
    /// A slicer, a timeline, ink and the rest: kept, drawn as a card.
    Other,
}

impl ObjectKind {
    /// The word a card or a list names the kind with.
    pub fn label(&self) -> &'static str {
        match self {
            ObjectKind::Picture => "Picture",
            ObjectKind::Shape => "Shape",
            ObjectKind::TextBox => "Text box",
            ObjectKind::Connector => "Line",
            ObjectKind::Group => "Group",
            ObjectKind::FormControl => "Form control",
            ObjectKind::SmartArt => "SmartArt graphic",
            ObjectKind::WordArt => "WordArt",
            ObjectKind::Icon => "Icon",
            ObjectKind::Equation => "Equation",
            ObjectKind::OleObject => "Embedded object",
            ObjectKind::Model3D => "3D model",
            ObjectKind::Other => "Object",
        }
    }

    /// Whether this version understands the object well enough to change its looks and text.
    pub fn is_editable_shape(&self) -> bool {
        matches!(self, ObjectKind::Shape | ObjectKind::TextBox | ObjectKind::Connector)
    }
}

/// A colour a shape names: an RGB value or a theme colour, with the changes the file applies to it.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeColor {
    pub base: ColorBase,
    /// The opacity in thousandths of a percent: 100,000 is opaque.
    pub alpha: u32,
    /// Changes the file applies after choosing the base (`lumMod`, `lumOff`, `shade`, `tint`), kept.
    pub mods: Vec<(String, i64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ColorBase {
    Rgb(u8, u8, u8),
    /// A theme colour by its name in the file: `accent1`, `tx1`, `bg1`, `dk2`.
    Scheme(String),
}

impl ShapeColor {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { base: ColorBase::Rgb(r, g, b), alpha: 100_000, mods: Vec::new() }
    }

    pub fn scheme(name: &str) -> Self {
        Self { base: ColorBase::Scheme(name.to_owned()), alpha: 100_000, mods: Vec::new() }
    }

    pub fn with_transparency(mut self, percent: f64) -> Self {
        self.alpha = ((100.0 - percent.clamp(0.0, 100.0)) * 1000.0).round() as u32;
        self
    }

    /// The transparency Excel's Format pane shows: 0 is opaque, 100 is clear.
    pub fn transparency(&self) -> f64 {
        100.0 - self.alpha as f64 / 1000.0
    }
}

/// What fills a shape.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Fill {
    /// Nothing is written, so the shape's style decides.
    #[default]
    Inherit,
    None,
    Solid(ShapeColor),
    /// A gradient, a pattern or a picture, kept as written.
    Other,
}

/// How a line is dashed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dash {
    #[default]
    Solid,
    Dot,
    Dash,
    DashDot,
    LongDash,
    LongDashDot,
    SquareDot,
}

impl Dash {
    pub fn attr(&self) -> &'static str {
        match self {
            Dash::Solid => "solid",
            Dash::Dot => "sysDot",
            Dash::Dash => "dash",
            Dash::DashDot => "dashDot",
            Dash::LongDash => "lgDash",
            Dash::LongDashDot => "lgDashDot",
            Dash::SquareDot => "sysDash",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "sysDot" | "dot" => Dash::Dot,
            "dash" => Dash::Dash,
            "dashDot" | "sysDashDot" => Dash::DashDot,
            "lgDash" => Dash::LongDash,
            "lgDashDot" | "lgDashDotDot" | "sysDashDotDot" => Dash::LongDashDot,
            "sysDash" => Dash::SquareDot,
            _ => Dash::Solid,
        }
    }
}

/// The end of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnd {
    #[default]
    None,
    Arrow,
    Triangle,
    Stealth,
    Diamond,
    Oval,
}

impl LineEnd {
    pub fn attr(&self) -> &'static str {
        match self {
            LineEnd::None => "none",
            LineEnd::Arrow => "arrow",
            LineEnd::Triangle => "triangle",
            LineEnd::Stealth => "stealth",
            LineEnd::Diamond => "diamond",
            LineEnd::Oval => "oval",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "arrow" => LineEnd::Arrow,
            "triangle" => LineEnd::Triangle,
            "stealth" => LineEnd::Stealth,
            "diamond" => LineEnd::Diamond,
            "oval" => LineEnd::Oval,
            _ => LineEnd::None,
        }
    }
}

/// What outlines a shape or draws a line.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Stroke {
    #[default]
    Inherit,
    None,
    Solid(ShapeColor),
}

/// A shape's outline.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    pub stroke: Stroke,
    /// The width in drawing units; `None` leaves the style's.
    pub width: Option<i64>,
    pub dash: Dash,
    pub head: LineEnd,
    pub tail: LineEnd,
}

/// How text sits across a shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    #[default]
    Left,
    Centre,
    Right,
    Justify,
}

impl HAlign {
    pub fn attr(&self) -> &'static str {
        match self {
            HAlign::Left => "l",
            HAlign::Centre => "ctr",
            HAlign::Right => "r",
            HAlign::Justify => "just",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "ctr" => HAlign::Centre,
            "r" => HAlign::Right,
            "just" | "dist" => HAlign::Justify,
            _ => HAlign::Left,
        }
    }
}

/// How text sits down a shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

impl VAlign {
    pub fn attr(&self) -> &'static str {
        match self {
            VAlign::Top => "t",
            VAlign::Middle => "ctr",
            VAlign::Bottom => "b",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "ctr" => VAlign::Middle,
            "b" => VAlign::Bottom,
            _ => VAlign::Top,
        }
    }
}

/// A run of text with one format.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    /// The size in points; `None` leaves the style's (11).
    pub size: Option<f64>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Option<ShapeColor>,
    pub font: Option<String>,
}

impl TextRun {
    pub fn plain(text: &str) -> Self {
        Self { text: text.to_owned(), size: None, bold: false, italic: false, underline: false, color: None, font: None }
    }
}

/// A paragraph of runs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextParagraph {
    pub align: HAlign,
    pub runs: Vec<TextRun>,
}

/// The text inside a shape.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextBody {
    pub paragraphs: Vec<TextParagraph>,
    pub valign: VAlign,
    /// Whether lines wrap at the shape's edge.
    pub wrap: bool,
}

impl TextBody {
    /// The text with a line break between paragraphs.
    pub fn plain_text(&self) -> String {
        self.paragraphs
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// What a shape looks like.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeData {
    /// The preset geometry's name: `rect`, `roundRect`, `ellipse`, `line`, `rightArrow`.
    pub preset: String,
    /// The first adjust handle's value, such as how round a rounded rectangle's corners are.
    pub adjust: Option<i64>,
    pub fill: Fill,
    pub line: Line,
    pub text: Option<TextBody>,
    /// The fill the shape's style gives it where `fill` says `Inherit`.
    pub style_fill: Option<ShapeColor>,
    /// The outline colour the style gives where the line says `Inherit`.
    pub style_line: Option<ShapeColor>,
    /// The text colour the style gives.
    pub style_text: Option<ShapeColor>,
}

/// How much of each edge of a picture is cut away, in thousandths of a percent of its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Crop {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Crop {
    pub fn is_none(&self) -> bool {
        *self == Crop::default()
    }
}

/// A picture's image and crop.
#[derive(Debug, Clone, PartialEq)]
pub struct PictureData {
    /// The image part, such as `xl/media/image1.png`, whose bytes are in `Kept::parts`.
    pub part: String,
    pub crop: Crop,
    /// The SVG part of an icon or a picture inserted as SVG, drawn from `part` here.
    pub svg: Option<String>,
}

/// One object in a drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawingObject {
    pub id: ObjectId,
    pub name: String,
    /// The alternative text.
    pub descr: String,
    pub hidden: bool,
    pub kind: ObjectKind,
    pub anchor: Anchor,
    /// For an object inside a group: its place in the group's own coordinates.
    pub frame: Rect,
    /// Degrees clockwise.
    pub rotation: f64,
    /// The size of the frame before it is turned, in drawing units.
    pub frame_size: (i64, i64),
    pub flip_h: bool,
    pub flip_v: bool,
    /// Whether resizing keeps the proportions (Excel's Lock aspect ratio).
    pub lock_aspect: bool,
    /// Whether the object is locked while the sheet is protected.
    pub locked: bool,
    /// Whether the object prints.
    pub prints: bool,
    pub picture: Option<PictureData>,
    pub shape: Option<ShapeData>,
    pub children: Vec<DrawingObject>,
    /// The stored picture of an object kept as written, which is what is drawn for it.
    pub fallback: Option<String>,
    /// For a group: the child coordinate space (`chOff` and `chExt`).
    pub child_space: Rect,
    /// The element the object was read from, or made from.
    pub tree: Elem,
    /// Whether `tree` is the whole `mc:AlternateContent` (the anchor is inside it) rather than the content.
    pub wrapped: bool,
}

impl DrawingObject {
    /// The object's own frame before it is turned about its centre, which is what the anchor covers
    /// (Excel's Left, Top, Width and Height).
    pub fn frame_rect(&self, sheet: &Sheet) -> Rect {
        self.anchor.rect(sheet)
    }

    /// The box round the object as it is drawn, turned.
    pub fn bounds(&self, sheet: &Sheet) -> Rect {
        let frame = self.frame_rect(sheet);
        if self.rotation.rem_euclid(360.0) == 0.0 {
            frame
        } else {
            frame.rotated_bounds(self.rotation)
        }
    }

    /// Put the object's frame at a rectangle.
    pub fn set_frame(&mut self, sheet: &Sheet, frame: Rect) {
        self.frame_size = (frame.w, frame.h);
        self.anchor.set_rect(sheet, frame);
    }

    /// Whether the object is a picture or shape this version can draw from its own data.
    pub fn is_drawn_from_data(&self) -> bool {
        self.picture.is_some() || self.shape.is_some() || !self.children.is_empty()
    }
}

/// The objects of a sheet, bottom first, and whether they changed since the file was read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawing {
    pub objects: Vec<DrawingObject>,
    /// Pixels a character of the default font is wide (7 for Calibri, 8 for Aptos Narrow), which positions depend on; 0 until known.
    pub char_px: f64,
    /// The drawing part is written again from `objects` when this is set.
    pub changed: bool,
}

impl Drawing {
    pub fn find(&self, id: ObjectId) -> Option<&DrawingObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    pub fn find_mut(&mut self, id: ObjectId) -> Option<&mut DrawingObject> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    pub fn index_of(&self, id: ObjectId) -> Option<usize> {
        self.objects.iter().position(|o| o.id == id)
    }
}
