//! Making new objects: what an image is, and the XML Excel writes for a new picture, shape, line or group.

use super::props;
use super::*;
use crate::xlsx::xmltree::{self, Elem};

const XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// What the first bytes of an image say about it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageInfo {
    /// The file extension the part is stored with.
    pub extension: &'static str,
    pub content_type: &'static str,
    pub width: u32,
    pub height: u32,
    /// Dots an inch, 96 when the image does not say.
    pub dpi: f64,
}

impl ImageInfo {
    /// The size the image is inserted at, in drawing units.
    pub fn natural_size(&self) -> (i64, i64) {
        let unit = 914_400.0 / self.dpi;
        ((self.width as f64 * unit).round() as i64, (self.height as f64 * unit).round() as i64)
    }
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn png(b: &[u8]) -> Option<ImageInfo> {
    let (width, height) = (be32(b, 16)?, be32(b, 20)?);
    let mut dpi = 96.0;
    let mut at = 8;
    while at + 12 <= b.len() {
        let len = be32(b, at)? as usize;
        if &b[at + 4..at + 8] == b"pHYs" && b.get(at + 16) == Some(&1) {
            let per_metre = be32(b, at + 8)? as f64;
            if per_metre > 0.0 {
                dpi = per_metre * 0.0254;
            }
        }
        at += 12 + len;
    }
    Some(ImageInfo { extension: "png", content_type: "image/png", width, height, dpi: dpi.round().max(1.0) })
}

fn jpeg(b: &[u8]) -> Option<ImageInfo> {
    let mut at = 2;
    let mut dpi = 96.0;
    while at + 4 < b.len() {
        if b[at] != 0xFF {
            at += 1;
            continue;
        }
        let marker = b[at + 1];
        let len = u16::from_be_bytes([b[at + 2], b[at + 3]]) as usize;
        if marker == 0xE0 && b.get(at + 4..at + 9) == Some(b"JFIF\0") {
            let units = *b.get(at + 11)?;
            let x = u16::from_be_bytes([*b.get(at + 12)?, *b.get(at + 13)?]) as f64;
            if x > 0.0 {
                dpi = match units {
                    1 => x,
                    2 => x * 2.54,
                    _ => 96.0,
                };
            }
        }
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let height = u16::from_be_bytes([*b.get(at + 5)?, *b.get(at + 6)?]) as u32;
            let width = u16::from_be_bytes([*b.get(at + 7)?, *b.get(at + 8)?]) as u32;
            return Some(ImageInfo { extension: "jpeg", content_type: "image/jpeg", width, height, dpi: dpi.round().max(1.0) });
        }
        at += 2 + len;
    }
    None
}

fn gif(b: &[u8]) -> Option<ImageInfo> {
    let w = u16::from_le_bytes([*b.get(6)?, *b.get(7)?]) as u32;
    let h = u16::from_le_bytes([*b.get(8)?, *b.get(9)?]) as u32;
    Some(ImageInfo { extension: "gif", content_type: "image/gif", width: w, height: h, dpi: 96.0 })
}

fn bmp(b: &[u8]) -> Option<ImageInfo> {
    let le32 = |at: usize| Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?));
    let (w, h) = (le32(18)?, le32(22)?);
    let per_metre = le32(38).unwrap_or(0) as f64;
    let dpi = if per_metre > 0.0 { (per_metre * 0.0254).round() } else { 96.0 };
    Some(ImageInfo { extension: "bmp", content_type: "image/bmp", width: w.unsigned_abs(), height: h.unsigned_abs(), dpi })
}

fn svg(b: &[u8]) -> Option<ImageInfo> {
    let text = std::str::from_utf8(b).ok()?;
    let root = text.find("<svg")?;
    let tag = &text[root..root + text[root..].find('>')?];
    let attr = |name: &str| {
        let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        let value = &tag[at..at + tag[at..].find('"')?];
        value.trim_end_matches("px").trim().parse::<f64>().ok()
    };
    let (w, h) = match (attr("width"), attr("height")) {
        (Some(w), Some(h)) => (w, h),
        _ => {
            let at = tag.find("viewBox=\"")? + 9;
            let nums: Vec<f64> = tag[at..at + tag[at..].find('"')?]
                .split(|c: char| c == ' ' || c == ',')
                .filter_map(|n| n.parse().ok())
                .collect();
            (*nums.get(2)?, *nums.get(3)?)
        }
    };
    Some(ImageInfo { extension: "svg", content_type: "image/svg+xml", width: w.round() as u32, height: h.round() as u32, dpi: 96.0 })
}

/// What an image is, or `None` when the bytes are not a picture this version knows (PNG, JPEG, GIF, BMP, SVG).
pub fn image_info(bytes: &[u8]) -> Option<ImageInfo> {
    let info = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        jpeg(bytes)
    } else if bytes.starts_with(b"GIF8") {
        gif(bytes)
    } else if bytes.starts_with(b"BM") {
        bmp(bytes)
    } else {
        svg(bytes)
    }?;
    (info.width > 0 && info.height > 0).then_some(info)
}

fn el(xml: &str) -> Elem {
    xmltree::parse(xml).unwrap_or_default()
}

/// The content element of a new picture. `embed` is the relationship id of its image.
pub fn picture_tree(id: ObjectId, name: &str, embed: &str, svg_embed: Option<&str>, size: (i64, i64)) -> Elem {
    let blip = match svg_embed {
        Some(s) => format!(
            "<a:blip xmlns:r=\"{R}\" r:embed=\"{embed}\"><a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"{s}\"/></a:ext></a:extLst></a:blip>"
        ),
        None => format!("<a:blip xmlns:r=\"{R}\" r:embed=\"{embed}\"/>"),
    };
    el(&format!(
        "<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill>{blip}<a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>",
        size.0, size.1
    ))
    .with_namespaces(&[("xmlns:xdr", XDR), ("xmlns:a", A)])
}

/// The kinds of shape Insert > Shapes offers that this version makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewShape {
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Line,
    Arrow,
    TextBox,
}

impl NewShape {
    /// The name Excel gives the first one of the kind.
    pub fn base_name(&self) -> &'static str {
        match self {
            NewShape::Rectangle => "Rectangle",
            NewShape::RoundedRectangle => "Rectangle: Rounded Corners",
            NewShape::Ellipse => "Oval",
            NewShape::Line => "Straight Connector",
            NewShape::Arrow => "Straight Arrow Connector",
            NewShape::TextBox => "TextBox",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        Some(match word.to_ascii_lowercase().replace([' ', '-', '_'], "").as_str() {
            "rectangle" | "rect" => NewShape::Rectangle,
            "roundedrectangle" | "roundrect" => NewShape::RoundedRectangle,
            "ellipse" | "oval" | "circle" => NewShape::Ellipse,
            "line" => NewShape::Line,
            "arrow" => NewShape::Arrow,
            "textbox" | "text" => NewShape::TextBox,
            _ => return None,
        })
    }
}

const STYLE_FILLED: &str = "<xdr:style><a:lnRef idx=\"2\"><a:schemeClr val=\"accent1\"><a:shade val=\"15000\"/></a:schemeClr></a:lnRef><a:fillRef idx=\"1\"><a:schemeClr val=\"accent1\"/></a:fillRef><a:effectRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"lt1\"/></a:fontRef></xdr:style>";
const STYLE_TEXT_BOX: &str = "<xdr:style><a:lnRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:lnRef><a:fillRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:fillRef><a:effectRef idx=\"0\"><a:scrgbClr r=\"0\" g=\"0\" b=\"0\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"dk1\"/></a:fontRef></xdr:style>";
const STYLE_LINE: &str = "<xdr:style><a:lnRef idx=\"1\"><a:schemeClr val=\"accent1\"/></a:lnRef><a:fillRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:fillRef><a:effectRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"tx1\"/></a:fontRef></xdr:style>";

/// The content element of a new shape.
pub fn shape_tree(kind: NewShape, id: ObjectId, name: &str, size: (i64, i64)) -> Elem {
    let xfrm = format!("<a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>", size.0, size.1);
    let xml = match kind {
        NewShape::Line | NewShape::Arrow => {
            let (prst, ln) = if kind == NewShape::Line {
                ("line", "")
            } else {
                ("straightConnector1", "<a:ln><a:tailEnd type=\"triangle\"/></a:ln>")
            };
            format!("<xdr:cxnSp macro=\"\"><xdr:nvCxnSpPr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvCxnSpPr/></xdr:nvCxnSpPr><xdr:spPr>{xfrm}<a:prstGeom prst=\"{prst}\"><a:avLst/></a:prstGeom>{ln}</xdr:spPr>{STYLE_LINE}</xdr:cxnSp>")
        }
        NewShape::TextBox => format!(
            "<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvSpPr txBox=\"1\"/></xdr:nvSpPr><xdr:spPr>{xfrm}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val=\"lt1\"/></a:solidFill><a:ln w=\"9525\" cmpd=\"sng\"><a:solidFill><a:schemeClr val=\"lt1\"><a:shade val=\"50000\"/></a:schemeClr></a:solidFill></a:ln></xdr:spPr>{STYLE_TEXT_BOX}<xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\" vert=\"horz\" rtlCol=\"0\" anchor=\"t\"/><a:lstStyle/><a:p><a:endParaRPr lang=\"en-US\" sz=\"1100\"/></a:p></xdr:txBody></xdr:sp>"
        ),
        _ => {
            let (prst, av) = match kind {
                NewShape::RoundedRectangle => ("roundRect", ""),
                NewShape::Ellipse => ("ellipse", ""),
                _ => ("rect", ""),
            };
            format!("<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr>{xfrm}<a:prstGeom prst=\"{prst}\"><a:avLst>{av}</a:avLst></a:prstGeom></xdr:spPr>{STYLE_FILLED}<xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\" rtlCol=\"0\" anchor=\"ctr\"/><a:lstStyle/><a:p><a:pPr algn=\"ctr\"/><a:endParaRPr lang=\"en-US\" sz=\"1100\"/></a:p></xdr:txBody></xdr:sp>")
        }
    };
    el(&xml).with_namespaces(&[("xmlns:xdr", XDR), ("xmlns:a", A)])
}

/// The content element of a new group, holding the content elements of its members.
pub fn group_tree(id: ObjectId, name: &str, frame: Rect, members: Vec<Elem>) -> Elem {
    let mut g = el(&format!(
        "<xdr:grpSp><xdr:nvGrpSpPr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvGrpSpPr/></xdr:nvGrpSpPr><xdr:grpSpPr><a:xfrm><a:off x=\"{0}\" y=\"{1}\"/><a:ext cx=\"{2}\" cy=\"{3}\"/><a:chOff x=\"{0}\" y=\"{1}\"/><a:chExt cx=\"{2}\" cy=\"{3}\"/></a:xfrm></xdr:grpSpPr></xdr:grpSp>",
        frame.x, frame.y, frame.w, frame.h
    ))
    .with_namespaces(&[("xmlns:xdr", XDR), ("xmlns:a", A)]);
    for m in members {
        g.push(m);
    }
    g
}

/// Write a transform into a content element.
pub fn set_frame_in_tree(tree: &mut Elem, frame: Rect) {
    let t = props::Xfrm { off: (frame.x, frame.y), ext: (frame.w, frame.h), ..Default::default() };
    props::write_xfrm(tree, &t, None);
}

impl Elem {
    /// Declare namespaces on this element, unless it has them.
    pub fn with_namespaces(mut self, list: &[(&str, &str)]) -> Self {
        for (n, v) in list {
            if self.attr(n).is_none() {
                self.attrs.push(((*n).to_owned(), (*v).to_owned()));
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_told_apart_and_measured() {
        let mut png = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        png.extend_from_slice(&[0, 0, 0, 40, 0, 0, 0, 20]);
        png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        let info = image_info(&png).unwrap();
        assert_eq!((info.extension, info.width, info.height), ("png", 40, 20));
        assert_eq!(info.natural_size(), (381_000, 190_500));
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 12\"></svg>";
        assert_eq!(image_info(svg).map(|i| (i.width, i.height)), Some((24, 12)));
        assert!(image_info(b"not a picture").is_none());
    }

    #[test]
    fn a_new_shape_reads_back_as_the_kind_it_was_made_as() {
        let tree = shape_tree(NewShape::RoundedRectangle, 5, "Rect", (100, 50));
        let shape = props::read_shape(&tree).unwrap();
        assert_eq!(shape.preset, "roundRect");
        assert_eq!(shape.style_fill, Some(ShapeColor::scheme("accent1")));
        let arrow = props::read_shape(&shape_tree(NewShape::Arrow, 6, "A", (10, 10))).unwrap();
        assert_eq!(arrow.line.tail, LineEnd::Triangle);
    }
}
