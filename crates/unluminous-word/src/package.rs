//! The parts of the file, their content types and their relationships (Open Packaging Conventions).
//!
//! Every part is held as the bytes it was read as. A part Unluminous regenerates replaces its bytes when
//! the document is written; every other part is written back untouched, in the order it was read.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::xml::{self, El};

pub const CONTENT_TYPES: &str = "[Content_Types].xml";

/// Relationship types, by the last part of their URI where Word's and the strict schema's differ only in
/// the front.
pub mod kind {
    pub const OFFICE_DOCUMENT: &str = "officeDocument";
    pub const STYLES: &str = "styles";
    pub const NUMBERING: &str = "numbering";
    pub const SETTINGS: &str = "settings";
    pub const FOOTNOTES: &str = "footnotes";
    pub const ENDNOTES: &str = "endnotes";
    pub const COMMENTS: &str = "comments";
    pub const HEADER: &str = "header";
    pub const FOOTER: &str = "footer";
    pub const IMAGE: &str = "image";
    pub const HYPERLINK: &str = "hyperlink";
    pub const CORE: &str = "core-properties";
    pub const THEME: &str = "theme";
    pub const FONT_TABLE: &str = "fontTable";
}

/// The full relationship type URI Word writes for `kind`.
pub fn kind_uri(kind: &str) -> String {
    match kind {
        kind::CORE => "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties".into(),
        other => format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{other}"),
    }
}

/// One part.
#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    pub data: Arc<Vec<u8>>,
}

/// One relationship.
#[derive(Debug, Clone, PartialEq)]
pub struct Rel {
    pub id: String,
    /// The full type URI.
    pub kind: String,
    pub target: String,
    pub external: bool,
}

impl Rel {
    /// Whether this relationship's type ends in `/<kind>`.
    pub fn is(&self, kind: &str) -> bool {
        self.kind.rsplit('/').next() == Some(kind)
    }
}

/// The relationships of one part.
#[derive(Debug, Clone, Default)]
pub struct Rels {
    pub rels: Vec<Rel>,
    pub dirty: bool,
}

impl Rels {
    pub fn by_id(&self, id: &str) -> Option<&Rel> {
        self.rels.iter().find(|rel| rel.id == id)
    }

    pub fn of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Rel> + 'a {
        self.rels.iter().filter(move |rel| rel.is(kind))
    }

    /// Add a relationship and answer its id: `rIdU<n>`, which no id Word makes (`rId<n>`) can equal.
    pub fn add(&mut self, kind: &str, target: &str, external: bool) -> String {
        let mut number = 1;
        while self.by_id(&format!("rIdU{number}")).is_some() {
            number += 1;
        }
        let id = format!("rIdU{number}");
        self.rels.push(Rel { id: id.clone(), kind: kind_uri(kind), target: target.to_owned(), external });
        self.dirty = true;
        id
    }

    fn to_xml(&self) -> String {
        let mut out = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        for rel in &self.rels {
            let mut el = El::new("Relationship")
                .with("Id", rel.id.clone())
                .with("Type", rel.kind.clone())
                .with("Target", rel.target.clone());
            if rel.external {
                el.set_attr("TargetMode", "External");
            }
            el.write(&mut out);
        }
        out.push_str("</Relationships>");
        out
    }
}

/// The whole package.
#[derive(Debug, Clone, Default)]
pub struct Package {
    pub parts: Vec<Part>,
    /// Content types by extension, lower case.
    pub defaults: Vec<(String, String)>,
    /// Content types by part name, without the leading slash.
    pub overrides: Vec<(String, String)>,
    pub content_types_dirty: bool,
    /// Relationships by the part they belong to; `""` is the package's own.
    pub rels: BTreeMap<String, Rels>,
}

impl Package {
    /// Read the package out of the zip's entries.
    pub fn read(entries: Vec<crate::zip::Entry>) -> Result<Package, String> {
        let mut package = Package::default();
        for entry in entries {
            // A zip made on Windows by hand can name a folder with a backslash, and a few tools write a
            // leading slash; the package name is neither.
            let name = entry.name.replace('\\', "/").trim_start_matches('/').to_owned();
            package.parts.push(Part { name, data: Arc::new(entry.data) });
        }
        let types = package
            .part(CONTENT_TYPES)
            .ok_or("the file has no [Content_Types].xml, so it is not an Office document")?;
        let types = xml::parse_bytes(types)?;
        for el in types.elements() {
            match el.local() {
                "Default" => package.defaults.push((
                    el.attr("Extension").unwrap_or_default().to_lowercase(),
                    el.attr("ContentType").unwrap_or_default().to_owned(),
                )),
                "Override" => package.overrides.push((
                    el.attr("PartName").unwrap_or_default().trim_start_matches('/').to_owned(),
                    el.attr("ContentType").unwrap_or_default().to_owned(),
                )),
                _ => {}
            }
        }
        let rel_parts: Vec<String> =
            package.parts.iter().map(|part| part.name.clone()).filter(|name| name.ends_with(".rels")).collect();
        for name in rel_parts {
            let Some(source) = source_of_rels(&name) else { continue };
            let el = xml::parse_bytes(package.part(&name).unwrap_or_default())?;
            let rels = el
                .elements()
                .filter(|rel| rel.local() == "Relationship")
                .map(|rel| Rel {
                    id: rel.attr("Id").unwrap_or_default().to_owned(),
                    kind: rel.attr("Type").unwrap_or_default().to_owned(),
                    target: rel.attr("Target").unwrap_or_default().to_owned(),
                    external: rel.attr("TargetMode") == Some("External"),
                })
                .collect();
            package.rels.insert(source, Rels { rels, dirty: false });
        }
        Ok(package)
    }

    /// The bytes of the part called `name`.
    pub fn part(&self, name: &str) -> Option<&[u8]> {
        self.parts.iter().find(|part| part.name == name).map(|part| part.data.as_slice())
    }

    /// Put `data` in the part called `name`, adding the part at the end when it is new.
    pub fn set_part(&mut self, name: &str, data: Vec<u8>) {
        match self.parts.iter_mut().find(|part| part.name == name) {
            Some(part) => part.data = Arc::new(data),
            None => self.parts.push(Part { name: name.to_owned(), data: Arc::new(data) }),
        }
    }

    /// The content type of a part, from its override or its extension's default.
    pub fn content_type(&self, name: &str) -> Option<&str> {
        if let Some((_, kind)) = self.overrides.iter().find(|(part, _)| part == name) {
            return Some(kind);
        }
        let extension = name.rsplit_once('.')?.1.to_lowercase();
        self.defaults.iter().find(|(ext, _)| *ext == extension).map(|(_, kind)| kind.as_str())
    }

    /// Say a part's content type, by override.
    pub fn set_override(&mut self, name: &str, kind: &str) {
        if self.content_type(name) == Some(kind) {
            return;
        }
        self.overrides.retain(|(part, _)| part != name);
        self.overrides.push((name.to_owned(), kind.to_owned()));
        self.content_types_dirty = true;
    }

    /// Make sure an extension has a default content type, for a new picture.
    pub fn ensure_default(&mut self, extension: &str, kind: &str) {
        let extension = extension.to_lowercase();
        if self.defaults.iter().any(|(ext, _)| *ext == extension) {
            return;
        }
        self.defaults.push((extension, kind.to_owned()));
        self.content_types_dirty = true;
    }

    /// The relationships of `part`, made when it has none.
    pub fn rels_mut(&mut self, part: &str) -> &mut Rels {
        self.rels.entry(part.to_owned()).or_default()
    }

    /// The part a relationship of `source` points at, as a package name.
    pub fn resolve(source: &str, target: &str) -> String {
        if let Some(absolute) = target.strip_prefix('/') {
            return absolute.to_owned();
        }
        let folder = source.rsplit_once('/').map_or("", |(folder, _)| folder);
        let mut parts: Vec<&str> = folder.split('/').filter(|piece| !piece.is_empty()).collect();
        for piece in target.split('/') {
            match piece {
                ".." => {
                    parts.pop();
                }
                "." | "" => {}
                other => parts.push(other),
            }
        }
        parts.join("/")
    }

    /// A target for `part` written relative to `source`'s folder, the way Word writes one.
    pub fn relative(source: &str, part: &str) -> String {
        let folder = source.rsplit_once('/').map_or("", |(folder, _)| folder);
        match part.strip_prefix(&format!("{folder}/")) {
            Some(inside) if !folder.is_empty() => inside.to_owned(),
            _ => format!("/{part}"),
        }
    }

    /// A part name not yet used, from a stem and an extension: `word/media/image7.png`.
    pub fn unused_name(&self, stem: &str, extension: &str) -> String {
        let mut number = 1;
        loop {
            let name = format!("{stem}{number}.{extension}");
            if self.part(&name).is_none() {
                return name;
            }
            number += 1;
        }
    }

    /// Write the content types and every changed set of relationships back into the parts.
    pub fn write_housekeeping(&mut self) {
        let dirty: Vec<(String, String)> = self
            .rels
            .iter()
            .filter(|(_, rels)| rels.dirty)
            .map(|(source, rels)| (rels_name(source), rels.to_xml()))
            .collect();
        for (name, text) in dirty {
            self.set_part(&name, text.into_bytes());
            let kind = "application/vnd.openxmlformats-package.relationships+xml";
            self.ensure_default("rels", kind);
        }
        for rels in self.rels.values_mut() {
            rels.dirty = false;
        }
        if self.content_types_dirty {
            let mut out = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
            );
            for (extension, kind) in &self.defaults {
                El::new("Default").with("Extension", extension.clone()).with("ContentType", kind.clone()).write(&mut out);
            }
            for (name, kind) in &self.overrides {
                El::new("Override").with("PartName", format!("/{name}")).with("ContentType", kind.clone()).write(&mut out);
            }
            out.push_str("</Types>");
            self.set_part(CONTENT_TYPES, out.into_bytes());
            self.content_types_dirty = false;
        }
    }
}

/// The part a `.rels` part belongs to: `word/_rels/document.xml.rels` is `word/document.xml`'s, and
/// `_rels/.rels` is the package's (`""`).
pub fn source_of_rels(name: &str) -> Option<String> {
    let (folder, file) = name.rsplit_once('/').unwrap_or(("", name));
    let source = file.strip_suffix(".rels")?;
    let folder = folder.strip_suffix("_rels")?.trim_end_matches('/');
    Some(match (folder.is_empty(), source.is_empty()) {
        (_, true) => String::new(),
        (true, false) => source.to_owned(),
        (false, false) => format!("{folder}/{source}"),
    })
}

/// The `.rels` part of a part.
pub fn rels_name(source: &str) -> String {
    match source.rsplit_once('/') {
        _ if source.is_empty() => "_rels/.rels".to_owned(),
        Some((folder, file)) => format!("{folder}/_rels/{file}.rels"),
        None => format!("_rels/{source}.rels"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rels_part_names_the_part_it_belongs_to_and_back() {
        for (rels, source) in [
            ("_rels/.rels", ""),
            ("word/_rels/document.xml.rels", "word/document.xml"),
            ("word/_rels/header1.xml.rels", "word/header1.xml"),
        ] {
            assert_eq!(source_of_rels(rels).as_deref(), Some(source));
            assert_eq!(rels_name(source), rels);
        }
    }

    #[test]
    fn targets_resolve_against_the_source_folder() {
        assert_eq!(Package::resolve("word/document.xml", "media/image1.png"), "word/media/image1.png");
        assert_eq!(Package::resolve("word/document.xml", "../customXml/item1.xml"), "customXml/item1.xml");
        assert_eq!(Package::resolve("", "word/document.xml"), "word/document.xml");
        assert_eq!(Package::resolve("word/document.xml", "/word/styles.xml"), "word/styles.xml");
        assert_eq!(Package::relative("word/document.xml", "word/media/image1.png"), "media/image1.png");
    }

    #[test]
    fn a_new_relationship_id_cannot_be_one_word_makes() {
        let mut rels = Rels::default();
        rels.rels.push(Rel { id: "rId1".into(), kind: kind_uri("styles"), target: "styles.xml".into(), external: false });
        assert_eq!(rels.add(kind::IMAGE, "media/image1.png", false), "rIdU1");
        assert_eq!(rels.add(kind::IMAGE, "media/image2.png", false), "rIdU2");
        assert!(rels.dirty);
    }
}
