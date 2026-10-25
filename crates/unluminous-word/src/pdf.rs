//! The laid out pages as a PDF file.
//!
//! What is drawn on screen is what goes in the file: every text item at its own place, every fill, rule
//! and picture. Each TrueType face is embedded whole (`FontFile2`) as a CID font with `Identity-H`, so any
//! character the font has prints, and a `ToUnicode` map makes the text searchable and copyable. A face
//! that is part of a font collection cannot be embedded as one file and is replaced by Helvetica, which
//! every PDF reader has.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write as _;

use ab_glyph::{Font, FontRef};
use unluminous_core::CharStyle;

use crate::layout::{Item, Pages, RuleKind};

/// The pixels of a picture, as a PDF wants them.
pub enum Picture {
    /// A JPEG file's bytes, which a PDF holds as they are.
    Jpeg { width: u32, height: u32, bytes: Vec<u8> },
    /// Eight bit RGB, three bytes a pixel, row by row.
    Rgb { width: u32, height: u32, pixels: Vec<u8> },
}

/// A font file's bytes and the face inside it.
pub type FaceBytes = (std::sync::Arc<Vec<u8>>, u32);

/// What a PDF says about itself and how it is built (task-2359, TDD 5.6): its document information, an
/// outline from the headings, tagged structure, and PDF/A-2b.
#[derive(Debug, Clone, Default)]
pub struct PdfOptions {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    /// The document's language, such as `en-US`.
    pub lang: String,
    /// The bookmarks: each heading's level from 1, its words, its page from 0 and how far down it is.
    pub outline: Vec<(u8, String, usize, f32)>,
    /// The structure a screen reader follows, from [`structure`].
    pub structure: Option<Structure>,
    /// Write PDF/A-2b: every font embedded, XMP metadata, an sRGB output intent and a file id.
    pub pdfa: bool,
}

/// A tagged PDF's structure: its elements, and for each page which element each line and picture
/// belongs to.
#[derive(Debug, Clone, Default)]
pub struct Structure {
    /// Each element's role (`H1` to `H6`, `P`, `LI`, `TD`, `Figure`) and, for a figure, its alt text.
    pub elements: Vec<(String, Option<String>)>,
    /// For each page, its lines' tops and bottoms with the element they belong to.
    pub lines: Vec<Vec<(f32, f32, usize)>>,
    /// For each page, its pictures' rectangles (x, y, w, h) with their figure element.
    pub figures: Vec<Vec<((f32, f32, f32, f32), usize)>>,
}

/// The structure of laid out pages: a heading for each heading paragraph, a list item for each list
/// paragraph, a cell for each paragraph in a table, a paragraph for the rest, and a figure for each
/// picture, with its alt text. Headers, footers and notes are artifacts.
pub fn structure(document: &crate::model::Document, pages: &Pages) -> Structure {
    use crate::position::StoryId;
    let resolver = crate::styles::Resolver::new(document);
    let mut out = Structure::default();
    let mut element_of: HashMap<(usize, Vec<u32>), usize> = HashMap::new();
    for (index, page) in pages.pages.iter().enumerate() {
        let mut lines = Vec::new();
        for line in page.carets.iter().filter(|l| l.story == StoryId::Body) {
            let key = (index, line.path.clone());
            let element = match element_of.get(&key) {
                Some(e) => *e,
                None => {
                    let role = match crate::position::paragraph(document, &StoryId::Body, &line.path) {
                        Some(p) => {
                            let look = resolver.paragraph(&p.props, None);
                            if line.path.len() > 1 {
                                "TD".to_owned()
                            } else if let Some(level) = look.outline_level {
                                format!("H{}", (level + 1).min(6))
                            } else if look.numbering.is_some() {
                                "LI".to_owned()
                            } else {
                                "P".to_owned()
                            }
                        }
                        None => "P".to_owned(),
                    };
                    out.elements.push((role, None));
                    element_of.insert(key, out.elements.len() - 1);
                    out.elements.len() - 1
                }
            };
            lines.push((line.top, line.top + line.height, element));
        }
        let mut figures = Vec::new();
        for item in &page.items {
            if let Item::Object { rect, story, path, offset } = item {
                let alt = crate::position::paragraph(document, story, path).and_then(|p| {
                    let mut at = 0;
                    p.runs.iter().find_map(|run| {
                        let here = at == *offset;
                        at += run.len();
                        match (&run.content, here) {
                            (crate::model::Content::Object(el), true) => {
                                crate::read::descendant(el, "wp:docPr").and_then(|d| d.attr("descr")).map(str::to_owned)
                            }
                            _ => None,
                        }
                    })
                });
                out.elements.push(("Figure".into(), Some(alt.unwrap_or_default())));
                figures.push(((rect.x, rect.y, rect.w, rect.h), out.elements.len() - 1));
            }
        }
        out.lines.push(lines);
        out.figures.push(figures);
    }
    out
}

/// The element a text item at `baseline` on `page` belongs to.
fn element_at(structure: &Structure, page: usize, baseline: f32) -> Option<usize> {
    structure.lines.get(page)?.iter().find(|(top, bottom, _)| baseline >= *top - 0.5 && baseline <= *bottom + 0.5).map(|l| l.2)
}

/// The figure element of a picture drawn at `rect` on `page`.
fn figure_at(structure: &Structure, page: usize, rect: &crate::layout::Rect) -> Option<usize> {
    structure
        .figures
        .get(page)?
        .iter()
        .find(|((x, y, w, h), _)| (x - rect.x).abs() < 0.5 && (y - rect.y).abs() < 0.5 && (w - rect.w).abs() < 0.5 && (h - rect.h).abs() < 0.5)
        .map(|f| f.1)
}

struct EmbeddedFont {
    name: String,
    bytes: Option<std::sync::Arc<Vec<u8>>>,
    index: u32,
    /// Glyph ids used, with the character each stands for and its advance in thousandths of an em.
    used: HashMap<u16, (char, f32)>,
}

/// Write the pages as a PDF. `face` answers the font file a style is drawn with, and `picture` the
/// pixels of a package part.
pub fn write(
    pages: &Pages,
    title: &str,
    face: &dyn Fn(&CharStyle) -> Option<FaceBytes>,
    picture: &dyn Fn(&str) -> Option<Picture>,
) -> Vec<u8> {
    write_with(pages, &PdfOptions { title: title.to_owned(), ..Default::default() }, face, picture)
}

/// [`write`], with the document information, outline, structure and PDF/A `options` ask for.
pub fn write_with(
    pages: &Pages,
    options: &PdfOptions,
    face: &dyn Fn(&CharStyle) -> Option<FaceBytes>,
    picture: &dyn Fn(&str) -> Option<Picture>,
) -> Vec<u8> {
    // Each page's marked content: which element each MCID belongs to, in order.
    let mut marked: Vec<Vec<usize>> = Vec::new();
    let mut fonts: Vec<EmbeddedFont> = Vec::new();
    let mut font_of: HashMap<(String, bool, bool), usize> = HashMap::new();
    let mut images: Vec<(String, Picture)> = Vec::new();
    let mut image_of: HashMap<String, usize> = HashMap::new();
    let mut contents: Vec<String> = Vec::new();
    for (page_index, page) in pages.pages.iter().enumerate() {
        let mut c = String::new();
        let mut mcids: Vec<usize> = Vec::new();
        let h = page.height;
        if let Some(bg) = page.background {
            let _ =
                writeln!(c, "{} {} {} rg 0 0 {} {} re f", f(bg.0), f(bg.1), f(bg.2), page.width, h);
        }
        for item in &page.items {
            // Tagged: a line's text and a picture are marked content of their element; the rest is an
            // artifact a screen reader skips.
            let element = options.structure.as_ref().and_then(|s| match item {
                Item::Text(text) => element_at(s, page_index, text.baseline),
                Item::Picture { rect, .. } => figure_at(s, page_index, rect),
                _ => None,
            });
            let tagged = options.structure.is_some();
            if let (true, Some(element)) = (tagged, element) {
                let role = &options.structure.as_ref().expect("tagged").elements[element].0;
                let _ = write!(c, "/{role} <</MCID {}>> BDC\n", mcids.len());
                mcids.push(element);
            } else if tagged {
                c.push_str("/Artifact BMC\n");
            }
            match item {
                Item::Fill { rect, color } => {
                    let _ = writeln!(
                        c,
                        "{} {} {} rg {:.2} {:.2} {:.2} {:.2} re f",
                        f(color.0),
                        f(color.1),
                        f(color.2),
                        rect.x,
                        h - rect.y - rect.h,
                        rect.w,
                        rect.h
                    );
                }
                Item::Rule { x0, y0, x1, y1, width, color, kind } => {
                    let dash = match kind {
                        RuleKind::Dotted => "[1 1] 0 d",
                        RuleKind::Dashed => "[3 2] 0 d",
                        _ => "[] 0 d",
                    };
                    let _ = writeln!(
                        c,
                        "{} {} {} RG {:.2} w {dash} {:.2} {:.2} m {:.2} {:.2} l S",
                        f(color.0),
                        f(color.1),
                        f(color.2),
                        width.max(0.1),
                        x0,
                        h - y0,
                        x1,
                        h - y1
                    );
                    if *kind == RuleKind::Double {
                        let gap = width * 2.0;
                        let _ = writeln!(
                            c,
                            "{:.2} {:.2} m {:.2} {:.2} l S",
                            x0,
                            h - y0 - gap,
                            x1,
                            h - y1 - gap
                        );
                    }
                }
                Item::Picture { rect, part, crop } => {
                    let index = match image_of.get(part) {
                        Some(index) => Some(*index),
                        None => picture(part).map(|p| {
                            images.push((part.clone(), p));
                            image_of.insert(part.clone(), images.len() - 1);
                            images.len() - 1
                        }),
                    };
                    if let Some(index) = index {
                        // A cropped picture is drawn whole, larger than its box, and clipped to the box.
                        let (l, t, r, b) = crop.unwrap_or((0.0, 0.0, 0.0, 0.0));
                        let full_w = rect.w / (1.0 - l - r).max(0.01);
                        let full_h = rect.h / (1.0 - t - b).max(0.01);
                        let _ = writeln!(
                            c,
                            "q {:.2} {:.2} {:.2} {:.2} re W n {:.2} 0 0 {:.2} {:.2} {:.2} cm /Im{index} Do Q",
                            rect.x,
                            h - rect.y - rect.h,
                            rect.w,
                            rect.h,
                            full_w,
                            full_h,
                            rect.x - full_w * l,
                            h - rect.y - rect.h - full_h * b
                        );
                    }
                }
                Item::Shape { rect, preset, fill, outline, flip } => {
                    let points = crate::layout::drawing::outline_points(preset, *rect, *flip);
                    let open = points.len() == 2;
                    let mut path = String::new();
                    for (n, (x, y)) in points.iter().enumerate() {
                        let op = if n == 0 { "m" } else { "l" };
                        let _ = write!(path, "{x:.2} {:.2} {op} ", h - y);
                    }
                    if let (Some(color), false) = (fill, open) {
                        let _ = writeln!(
                            c,
                            "{} {} {} rg {path}h f",
                            f(color.0),
                            f(color.1),
                            f(color.2)
                        );
                    }
                    if let Some((color, width)) = outline {
                        let close = if open { "" } else { "h " };
                        let _ = writeln!(
                            c,
                            "{} {} {} RG {width:.2} w [] 0 d {path}{close}S",
                            f(color.0),
                            f(color.1),
                            f(color.2)
                        );
                    }
                }
                Item::Object { .. } | Item::TableEdge { .. } => {}
                Item::Placeholder { rect, .. } => {
                    let _ = writeln!(
                        c,
                        "0.6 0.6 0.6 RG 0.5 w [] 0 d {:.2} {:.2} {:.2} {:.2} re S",
                        rect.x,
                        h - rect.y - rect.h,
                        rect.w,
                        rect.h
                    );
                }
                Item::Text(text) => {
                    let key = (text.style.family.to_string(), text.style.bold, text.style.italic);
                    let index = *font_of.entry(key).or_insert_with(|| {
                        let found = face(&text.style);
                        let collection = found
                            .as_ref()
                            .is_some_and(|(bytes, index)| *index > 0 || bytes.starts_with(b"ttcf"));
                        let (bytes, index) = match found {
                            Some((bytes, index))
                                if !collection && FontRef::try_from_slice(&bytes).is_ok() =>
                            {
                                (Some(bytes), index)
                            }
                            _ => (None, 0),
                        };
                        fonts.push(EmbeddedFont {
                            name: format!("F{}", fonts.len()),
                            bytes,
                            index,
                            used: HashMap::new(),
                        });
                        fonts.len() - 1
                    });
                    let font = &mut fonts[index];
                    let color = text.style.color;
                    let _ = write!(
                        c,
                        "BT /{} {:.2} Tf {} {} {} rg ",
                        font.name,
                        text.style.size,
                        f(color.r),
                        f(color.g),
                        f(color.b)
                    );
                    match &font.bytes {
                        Some(bytes) => {
                            let parsed = FontRef::try_from_slice_and_index(bytes, font.index)
                                .expect("checked above");
                            let units = parsed.units_per_em().unwrap_or(1000.0);
                            for (character, x) in text.text.chars().zip(&text.xs) {
                                if character.is_control() {
                                    continue;
                                }
                                let glyph = parsed.glyph_id(character);
                                let advance = parsed.h_advance_unscaled(glyph) / units * 1000.0;
                                font.used.insert(glyph.0, (character, advance));
                                let _ = write!(
                                    c,
                                    "1 0 0 1 {:.2} {:.2} Tm <{:04X}> Tj ",
                                    x,
                                    h - text.baseline,
                                    glyph.0
                                );
                            }
                        }
                        None => {
                            for (character, x) in text.text.chars().zip(&text.xs) {
                                let byte = if (character as u32) < 256 && !character.is_control() {
                                    character as u32 as u8
                                } else {
                                    b'?'
                                };
                                let _ = write!(
                                    c,
                                    "1 0 0 1 {:.2} {:.2} Tm <{:02X}> Tj ",
                                    x,
                                    h - text.baseline,
                                    byte
                                );
                            }
                        }
                    }
                    c.push_str("ET\n");
                }
            }
            if tagged {
                c.push_str("EMC\n");
            }
        }
        contents.push(c);
        marked.push(mcids);
    }
    assemble(pages, options, &contents, &fonts, &images, &marked)
}

fn f(channel: u8) -> String {
    format!("{:.3}", f32::from(channel) / 255.0)
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
    let _ = encoder.write_all(bytes);
    encoder.finish().unwrap_or_default()
}

/// Lay the objects out, with the cross reference table.
fn assemble(
    pages: &Pages,
    options: &PdfOptions,
    contents: &[String],
    fonts: &[EmbeddedFont],
    images: &[(String, Picture)],
    marked: &[Vec<usize>],
) -> Vec<u8> {
    let title = options.title.as_str();
    let mut out: Vec<u8> = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    // Object numbers: 1 catalog, 2 pages, 3 info, then fonts, images, page objects and contents.
    let mut next = 4usize;
    let mut font_ids = Vec::new();
    for font in fonts {
        font_ids.push(next);
        next += if font.bytes.is_some() { 5 } else { 1 };
    }
    let image_ids: Vec<usize> = images
        .iter()
        .map(|_| {
            next += 1;
            next - 1
        })
        .collect();
    let page_ids: Vec<usize> = pages
        .pages
        .iter()
        .map(|_| {
            next += 2;
            next - 2
        })
        .collect();
    let mut objects: Vec<(usize, Vec<u8>)> = Vec::new();
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    let mut catalog = String::from("<< /Type /Catalog /Pages 2 0 R");
    // The outline: a bookmark for each heading, nested by level.
    if !options.outline.is_empty() {
        let root = next;
        next += 1;
        let ids: Vec<usize> = options.outline.iter().map(|_| { next += 1; next - 1 }).collect();
        let parent_of: Vec<Option<usize>> = (0..options.outline.len())
            .map(|i| (0..i).rev().find(|j| options.outline[*j].0 < options.outline[i].0))
            .collect();
        let children = |parent: Option<usize>| -> Vec<usize> {
            (0..options.outline.len()).filter(|i| parent_of[*i] == parent).collect()
        };
        let top = children(None);
        objects.push((root, format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>", ids[top[0]], ids[*top.last().expect("one")], top.len()).into_bytes()));
        for i in 0..options.outline.len() {
            let (_, words, page, y) = &options.outline[i];
            let siblings = children(parent_of[i]);
            let at = siblings.iter().position(|s| *s == i).expect("among its siblings");
            let mine = children(Some(i));
            let mut item = format!("<< /Title ({}) /Parent {} 0 R", pdf_string(words), parent_of[i].map_or(root, |p| ids[p]));
            if at > 0 {
                item.push_str(&format!(" /Prev {} 0 R", ids[siblings[at - 1]]));
            }
            if let Some(next_sibling) = siblings.get(at + 1) {
                item.push_str(&format!(" /Next {} 0 R", ids[*next_sibling]));
            }
            if !mine.is_empty() {
                item.push_str(&format!(" /First {} 0 R /Last {} 0 R /Count {}", ids[mine[0]], ids[*mine.last().expect("one")], mine.len()));
            }
            let page_id = page_ids.get(*page).copied().unwrap_or(page_ids[0]);
            let height = pages.pages.get(*page).map_or(792.0, |p| p.height);
            item.push_str(&format!(" /Dest [{page_id} 0 R /XYZ 0 {:.2} 0] >>", height - y));
            objects.push((ids[i], item.into_bytes()));
        }
        catalog.push_str(&format!(" /Outlines {root} 0 R /PageMode /UseOutlines"));
    }
    // The structure tree: an element for each heading, paragraph, list item, cell and figure, each
    // pointing at its marked content on its page, and the parent tree that leads back from them.
    let mut struct_parents: Vec<Option<usize>> = vec![None; pages.pages.len()];
    if let Some(structure) = &options.structure {
        let root = next;
        let parent_tree = next + 1;
        next += 2;
        let element_ids: Vec<usize> = structure.elements.iter().map(|_| { next += 1; next - 1 }).collect();
        let mut kids_of: Vec<Vec<(usize, usize)>> = vec![Vec::new(); structure.elements.len()];
        for (page, mcids) in marked.iter().enumerate() {
            for (mcid, element) in mcids.iter().enumerate() {
                kids_of[*element].push((page, mcid));
            }
        }
        for (index, (role, alt)) in structure.elements.iter().enumerate() {
            let kids: Vec<String> = kids_of[index].iter().map(|(page, mcid)| format!("<< /Type /MCR /Pg {} 0 R /MCID {mcid} >>", page_ids[*page])).collect();
            let alt = alt.as_ref().map(|a| format!(" /Alt ({})", pdf_string(a))).unwrap_or_default();
            objects.push((element_ids[index], format!("<< /Type /StructElem /S /{role} /P {root} 0 R /K [{}]{alt} >>", kids.join(" ")).into_bytes()));
        }
        let used: Vec<String> = (0..structure.elements.len()).filter(|i| !kids_of[*i].is_empty()).map(|i| format!("{} 0 R", element_ids[i])).collect();
        objects.push((root, format!("<< /Type /StructTreeRoot /K [{}] /ParentTree {parent_tree} 0 R >>", used.join(" ")).into_bytes()));
        let nums: Vec<String> = marked
            .iter()
            .enumerate()
            .map(|(page, mcids)| {
                struct_parents[page] = Some(page);
                let refs: Vec<String> = mcids.iter().map(|e| format!("{} 0 R", element_ids[*e])).collect();
                format!("{page} [{}]", refs.join(" "))
            })
            .collect();
        objects.push((parent_tree, format!("<< /Nums [{}] >>", nums.join(" ")).into_bytes()));
        catalog.push_str(&format!(" /StructTreeRoot {root} 0 R /MarkInfo << /Marked true >>"));
    }
    if !options.lang.is_empty() {
        catalog.push_str(&format!(" /Lang ({})", pdf_string(&options.lang)));
    }
    catalog.push_str(" /ViewerPreferences << /DisplayDocTitle true >>");
    // PDF/A-2b: the metadata in XMP and an sRGB output intent.
    let id = format!("{:032X}", {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        title.hash(&mut hasher);
        contents.len().hash(&mut hasher);
        for content in contents {
            content.len().hash(&mut hasher);
        }
        u128::from(hasher.finish()) << 64 | u128::from(hasher.finish().rotate_left(17))
    });
    if options.pdfa {
        let metadata = next;
        let intent = next + 1;
        let profile = next + 2;
        next += 3;
        let now = crate::session::now_iso();
        let bom = '\u{feff}';
        let xmp = format!(
            "<?xpacket begin=\"{bom}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\"><pdfaid:part>2</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance><dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title><dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator><xmp:CreateDate>{now}</xmp:CreateDate><xmp:CreatorTool>Unluminous</xmp:CreatorTool><pdf:Producer>Unluminous</pdf:Producer></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>",
            xml_text(title),
            xml_text(&options.author),
        );
        let mut stream = format!("<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n", xmp.len()).into_bytes();
        stream.extend(xmp.as_bytes());
        stream.extend(b"\nendstream");
        objects.push((metadata, stream));
        let icc = srgb_profile();
        let mut stream = format!("<< /N 3 /Length {} >>\nstream\n", icc.len()).into_bytes();
        stream.extend(&icc);
        stream.extend(b"\nendstream");
        objects.push((profile, stream));
        objects.push((intent, format!("<< /Type /OutputIntent /S /GTS_PDFA1 /OutputConditionIdentifier (sRGB IEC61966-2.1) /Info (sRGB IEC61966-2.1) /DestOutputProfile {profile} 0 R >>").into_bytes()));
        catalog.push_str(&format!(" /Metadata {metadata} 0 R /OutputIntents [{intent} 0 R]"));
    }
    catalog.push_str(" >>");
    objects.push((1, catalog.into_bytes()));
    objects.push((
        2,
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), page_ids.len())
            .into_bytes(),
    ));
    let mut info = format!("<< /Title ({}) /Producer (Unluminous)", pdf_string(title));
    for (key, value) in [("Author", &options.author), ("Subject", &options.subject), ("Keywords", &options.keywords)] {
        if !value.is_empty() {
            info.push_str(&format!(" /{key} ({})", pdf_string(value)));
        }
    }
    info.push_str(" >>");
    objects.push((3, info.into_bytes()));
    for (font, id) in fonts.iter().zip(&font_ids) {
        match &font.bytes {
            None => objects.push((*id, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec())),
            Some(bytes) => {
                let parsed = FontRef::try_from_slice_and_index(bytes, font.index).expect("checked");
                let units = parsed.units_per_em().unwrap_or(1000.0);
                let scale = |v: f32| (v / units * 1000.0).round() as i64;
                let base = format!("Unluminous{}+{}", id, font.name);
                let (descendant, descriptor, file, cmap) = (id + 1, id + 2, id + 3, id + 4);
                objects.push((*id, format!("<< /Type /Font /Subtype /Type0 /BaseFont /{base} /Encoding /Identity-H /DescendantFonts [{descendant} 0 R] /ToUnicode {cmap} 0 R >>").into_bytes()));
                let mut widths: Vec<(&u16, &(char, f32))> = font.used.iter().collect();
                widths.sort_by_key(|(gid, _)| **gid);
                let w: String = widths.iter().map(|(gid, (_, adv))| format!("{gid} [{}]", adv.round() as i64)).collect::<Vec<_>>().join(" ");
                objects.push((descendant, format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{base} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /W [{w}] /CIDToGIDMap /Identity >>").into_bytes()));
                objects.push((descriptor, format!(
                    "<< /Type /FontDescriptor /FontName /{base} /Flags 32 /FontBBox [-500 {} 1500 {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 {file} 0 R >>",
                    scale(parsed.descent_unscaled()), scale(parsed.ascent_unscaled()), scale(parsed.ascent_unscaled()), scale(parsed.descent_unscaled()), scale(parsed.ascent_unscaled() * 0.7)
                ).into_bytes()));
                let packed = deflate(bytes);
                let mut stream = format!("<< /Length {} /Length1 {} /Filter /FlateDecode >>\nstream\n", packed.len(), bytes.len()).into_bytes();
                stream.extend(&packed);
                stream.extend(b"\nendstream");
                objects.push((file, stream));
                let mut map = String::from("/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Adobe-Identity-UCS def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange\n");
                for chunk in widths.chunks(100) {
                    let _ = writeln!(map, "{} beginbfchar", chunk.len());
                    for (gid, (character, _)) in chunk {
                        let mut units = [0u16; 2];
                        let encoded: String = character.encode_utf16(&mut units).iter().map(|u| format!("{u:04X}")).collect();
                        let _ = writeln!(map, "<{gid:04X}> <{encoded}>");
                    }
                    map.push_str("endbfchar\n");
                }
                map.push_str("endcmap CMapName currentdict /CMap defineresource pop end end");
                let packed = deflate(map.as_bytes());
                let mut stream = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", packed.len()).into_bytes();
                stream.extend(&packed);
                stream.extend(b"\nendstream");
                objects.push((cmap, stream));
            }
        }
    }
    for ((_, picture), id) in images.iter().zip(&image_ids) {
        let mut stream = match picture {
            Picture::Jpeg { width, height, bytes } => {
                let mut s = format!("<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", bytes.len()).into_bytes();
                s.extend(bytes);
                s
            }
            Picture::Rgb { width, height, pixels } => {
                let packed = deflate(pixels);
                let mut s = format!("<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", packed.len()).into_bytes();
                s.extend(packed);
                s
            }
        };
        stream.extend(b"\nendstream");
        objects.push((*id, stream));
    }
    let font_resources: String = fonts
        .iter()
        .zip(&font_ids)
        .map(|(font, id)| format!("/{} {id} 0 R", font.name))
        .collect::<Vec<_>>()
        .join(" ");
    let image_resources: String = image_ids
        .iter()
        .enumerate()
        .map(|(i, id)| format!("/Im{i} {id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    for (index, ((page, id), content)) in pages.pages.iter().zip(&page_ids).zip(contents).enumerate() {
        let parents = struct_parents[index].map(|n| format!(" /StructParents {n} /Tabs /S")).unwrap_or_default();
        objects.push((*id, format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /Font << {font_resources} >> /XObject << {image_resources} >> >> /Contents {} 0 R{parents} >>",
            page.width, page.height, id + 1
        ).into_bytes()));
        let packed = deflate(content.as_bytes());
        let mut stream =
            format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", packed.len()).into_bytes();
        stream.extend(packed);
        stream.extend(b"\nendstream");
        objects.push((id + 1, stream));
    }
    objects.sort_by_key(|(id, _)| *id);
    offsets.resize(next, 0);
    for (id, body) in &objects {
        offsets[*id] = out.len();
        out.extend(format!("{id} 0 obj\n").as_bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {next}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets.iter().skip(1) {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!("trailer\n<< /Size {next} /Root 1 0 R /Info 3 0 R /ID [<{id}> <{id}>] >>\nstartxref\n{xref}\n%%EOF\n")
            .as_bytes(),
    );
    out
}

/// Text for XML, its markup characters escaped.
fn xml_text(text: &str) -> String {
    let mut out = String::new();
    crate::xml::escape_text(text, &mut out);
    out
}

/// A small sRGB ICC profile (version 2, display class, RGB to XYZ), for a PDF/A output intent: the D50
/// white point, the sRGB primaries adapted to D50, and a 2.2 gamma for each channel.
fn srgb_profile() -> Vec<u8> {
    fn s15(value: f64) -> [u8; 4] {
        ((value * 65536.0).round() as i32).to_be_bytes()
    }
    let xyz = |x: f64, y: f64, z: f64| -> Vec<u8> {
        let mut tag = b"XYZ \0\0\0\0".to_vec();
        tag.extend(s15(x));
        tag.extend(s15(y));
        tag.extend(s15(z));
        tag
    };
    let curve = || -> Vec<u8> {
        let mut tag = b"curv\0\0\0\0".to_vec();
        tag.extend(1u32.to_be_bytes());
        tag.extend(((2.2f64 * 256.0).round() as u16).to_be_bytes());
        tag.extend([0, 0]);
        tag
    };
    let text = |words: &str| -> Vec<u8> {
        let mut tag = b"text\0\0\0\0".to_vec();
        tag.extend(words.as_bytes());
        tag.push(0);
        while tag.len() % 4 != 0 {
            tag.push(0);
        }
        tag
    };
    let description = |words: &str| -> Vec<u8> {
        let mut tag = b"desc\0\0\0\0".to_vec();
        tag.extend(((words.len() + 1) as u32).to_be_bytes());
        tag.extend(words.as_bytes());
        tag.push(0);
        tag.extend([0u8; 4]);
        tag.extend([0u8; 4]);
        tag.extend([0u8; 2]);
        tag.push(0);
        tag.extend([0u8; 67]);
        while tag.len() % 4 != 0 {
            tag.push(0);
        }
        tag
    };
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", description("sRGB IEC61966-2.1")),
        (b"cprt", text("No copyright, use freely")),
        (b"wtpt", xyz(0.9642, 1.0, 0.8249)),
        (b"rXYZ", xyz(0.4360, 0.2225, 0.0139)),
        (b"gXYZ", xyz(0.3851, 0.7169, 0.0971)),
        (b"bXYZ", xyz(0.1431, 0.0606, 0.7141)),
        (b"rTRC", curve()),
        (b"gTRC", curve()),
        (b"bTRC", curve()),
    ];
    let table_size = 4 + 12 * tags.len();
    let mut offset = 128 + table_size;
    let mut table = (tags.len() as u32).to_be_bytes().to_vec();
    let mut data: Vec<u8> = Vec::new();
    for (signature, body) in &tags {
        table.extend(*signature);
        table.extend((offset as u32).to_be_bytes());
        table.extend((body.len() as u32).to_be_bytes());
        data.extend(body);
        offset += body.len();
    }
    let size = 128 + table.len() + data.len();
    let mut header = Vec::with_capacity(128);
    header.extend((size as u32).to_be_bytes());
    header.extend([0u8; 4]);
    header.extend([2, 0x10, 0, 0]);
    header.extend(b"mntr");
    header.extend(b"RGB ");
    header.extend(b"XYZ ");
    header.extend([0u8; 12]);
    header.extend(b"acsp");
    header.extend(b"MSFT");
    header.extend([0u8; 4]);
    header.extend([0u8; 4]);
    header.extend([0u8; 4]);
    header.extend([0u8; 8]);
    header.extend([0u8; 4]);
    header.extend(s15(0.9642));
    header.extend(s15(1.0));
    header.extend(s15(0.8249));
    header.extend([0u8; 4]);
    header.resize(128, 0);
    let mut out = header;
    out.extend(table);
    out.extend(data);
    out
}

fn pdf_string(text: &str) -> String {
    text.chars()
        .filter(|c| (*c as u32) < 128 && !c.is_control())
        .map(|c| match c {
            '(' => "\\(".to_owned(),
            ')' => "\\)".to_owned(),
            '\\' => "\\\\".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pdf_carries_its_outline_its_structure_and_pdfa_metadata() {
        let mut session = crate::session::Session::new(crate::new::blank());
        session.type_text("A heading
A paragraph under it");
        session.select(crate::position::Pos::new(vec![0], 0), crate::position::Pos::new(vec![0], 0));
        session.apply_style("Heading1").unwrap();
        let pages = crate::layout::lay_out(&session.document, &unluminous_core::FixedMetrics::default(), Default::default());
        let options = PdfOptions {
            title: "Report".into(),
            author: "Jason".into(),
            lang: "en-US".into(),
            outline: vec![(1, "A heading".into(), 0, 72.0)],
            structure: Some(structure(&session.document, &pages)),
            pdfa: true,
            ..Default::default()
        };
        let pdf = write_with(&pages, &options, &|_| None, &|_| None);
        let text = String::from_utf8_lossy(&pdf);
        for wanted in ["/Outlines", "/Title (A heading)", "/StructTreeRoot", "/S /H1", "/S /P", "/MCID 0", "/MarkInfo", "/Lang (en-US)", "pdfaid:part", "/OutputIntents", "/ID [<", "/Author (Jason)"] {
            assert!(text.contains(wanted), "{wanted}");
        }
        // Every marked content sequence is closed.
        assert_eq!(text.matches(" BDC").count() + text.matches(" BMC").count(), text.matches("EMC").count());
    }

    #[test]
    fn a_document_becomes_a_pdf_with_a_page_for_each_page() {
        let mut session = crate::session::Session::new(crate::new::blank());
        session.type_text("Hello (world)\u{c}second page");
        let pages = crate::layout::lay_out(
            &session.document,
            &unluminous_core::FixedMetrics::default(),
            Default::default(),
        );
        let pdf = write(&pages, "A (title)", &|_| None, &|_| None);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.7"));
        assert!(text.contains("/Count 2"));
        assert!(text.contains("/Helvetica"));
        assert!(text.trim_end().ends_with("%%EOF"));
        // Every object the cross reference table names starts where it says.
        let xref_at: usize =
            text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        let table = &text[xref_at..];
        for (line, id) in table.lines().skip(3).take_while(|l| l.ends_with(" n ")).zip(1..) {
            let offset: usize = line[..10].parse().unwrap();
            assert!(pdf[offset..].starts_with(format!("{id} 0 obj").as_bytes()), "object {id}");
        }
    }
}
