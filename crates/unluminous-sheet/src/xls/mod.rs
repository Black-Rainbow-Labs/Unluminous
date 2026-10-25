//! The `.xls` file: a BIFF8 workbook stream inside a compound file, read into a [`Workbook`].
//!
//! Reading only. Excel steers a person who opens an old format to save as `.xlsx`, and this version does
//! the same: [`crate::save`] refuses `.xls` with that advice.
//!
//! The workbook stream is a list of records. The first part (the globals) holds the sheet list, the
//! shared strings, the formats and the names; each sheet has its own run of records after that. Formulas
//! are lists of tokens, which [`ptg`] writes out as text. What the model has no place for (charts,
//! pictures, the macro project) is kept as bytes under `legacy/` in [`crate::model::Kept`].

pub mod crypto;
pub mod features;
pub mod globals;
pub mod ptg;
pub mod read;
pub mod records;
pub mod sheet;
pub mod styles;

pub use read::read;

/// Give each workbook a formula refers to an external link part, so the `[1]` in a formula written to
/// an `.xlsx` file names something. The link holds the other workbook's address and sheet names and no
/// cached values.
pub(crate) fn keep_external_links(book: &mut crate::model::Workbook, supbooks: &[ptg::SupBook]) {
    use crate::model::Relationship;
    use crate::xlsx::{escape, REL_NS};
    use std::sync::Arc;
    const KIND: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink";
    const FILE: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath";
    const TYPE: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml";
    let mut references = String::new();
    let mut number = 0;
    for supbook in supbooks {
        let ptg::SupBook::External { url, sheets } = supbook else { continue };
        number += 1;
        let names: String =
            sheets.iter().map(|s| format!("<sheetName val=\"{}\"/>", escape(s))).collect();
        let part = format!("xl/externalLinks/externalLink{number}.xml");
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<externalLink xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"{REL_NS}\"><externalBook r:id=\"rId1\"><sheetNames>{names}</sheetNames></externalBook></externalLink>"
        );
        let rels = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{FILE}\" Target=\"{}\" TargetMode=\"External\"/></Relationships>",
            escape(url)
        );
        let rels_part = format!("xl/externalLinks/_rels/externalLink{number}.xml.rels");
        book.kept.parts.push((part.clone(), Arc::new(xml.into_bytes())));
        book.kept.parts.push((rels_part, Arc::new(rels.into_bytes())));
        book.kept.overrides.push((format!("/{part}"), TYPE.to_owned()));
        let id = format!("rIdExt{number}");
        book.kept.workbook_rels.push(Relationship {
            id: id.clone(),
            kind: KIND.to_owned(),
            target: format!("externalLinks/externalLink{number}.xml"),
            external: false,
        });
        references.push_str(&format!("<externalReference r:id=\"{id}\"/>"));
    }
    if !references.is_empty() {
        let xml = format!("<externalReferences>{references}</externalReferences>");
        book.raw.push(("externalReferences".to_owned(), xml));
    }
}

/// Record types, by the names the file format specification gives them.
pub mod id {
    pub const BOF: u16 = 0x0809;
    pub const EOF: u16 = 0x000A;
    pub const CODEPAGE: u16 = 0x0042;
    pub const DATEMODE: u16 = 0x0022;
    pub const FILEPASS: u16 = 0x002F;
    pub const BOUNDSHEET: u16 = 0x0085;
    pub const SST: u16 = 0x00FC;
    pub const EXTSST: u16 = 0x00FF;
    pub const FONT: u16 = 0x0031;
    pub const FORMAT: u16 = 0x041E;
    pub const XF: u16 = 0x00E0;
    pub const PALETTE: u16 = 0x0092;
    pub const NAME: u16 = 0x0018;
    pub const SUPBOOK: u16 = 0x01AE;
    pub const EXTERNNAME: u16 = 0x0023;
    pub const EXTERNSHEET: u16 = 0x0017;
    pub const WINDOW1: u16 = 0x003D;
    pub const CALCMODE: u16 = 0x000D;
    pub const CALCCOUNT: u16 = 0x000C;
    pub const ITERATION: u16 = 0x0011;
    pub const DELTA: u16 = 0x0010;
    pub const REFMODE: u16 = 0x000F;
    pub const PRECISION: u16 = 0x000E;
    pub const WINDOWPROTECT: u16 = 0x0019;
    pub const PROTECT: u16 = 0x0012;
    pub const OBJPROTECT: u16 = 0x0063;
    pub const SCENPROTECT: u16 = 0x00DD;
    pub const PASSWORD: u16 = 0x0013;

    pub const DIMENSIONS: u16 = 0x0200;
    pub const ROW: u16 = 0x0208;
    pub const BLANK: u16 = 0x0201;
    pub const MULBLANK: u16 = 0x00BE;
    pub const NUMBER: u16 = 0x0203;
    pub const LABEL: u16 = 0x0204;
    pub const BOOLERR: u16 = 0x0205;
    pub const FORMULA: u16 = 0x0006;
    pub const STRING: u16 = 0x0207;
    pub const RK: u16 = 0x027E;
    pub const MULRK: u16 = 0x00BD;
    pub const RSTRING: u16 = 0x00D6;
    pub const LABELSST: u16 = 0x00FD;
    pub const SHRFMLA: u16 = 0x04BC;
    pub const ARRAY: u16 = 0x0221;
    pub const TABLE: u16 = 0x0236;
    pub const COLINFO: u16 = 0x007D;
    pub const DEFCOLWIDTH: u16 = 0x0055;
    pub const DEFAULTROWHEIGHT: u16 = 0x0225;
    pub const MERGEDCELLS: u16 = 0x00E5;
    pub const WINDOW2: u16 = 0x023E;
    pub const PANE: u16 = 0x0041;
    pub const SCL: u16 = 0x00A0;
    pub const SELECTION: u16 = 0x001D;
    pub const NOTE: u16 = 0x001C;
    pub const OBJ: u16 = 0x005D;
    pub const TXO: u16 = 0x01B6;
    pub const MSODRAWING: u16 = 0x00EC;
    pub const HLINK: u16 = 0x01B8;
    pub const LINKTIP: u16 = 0x0800;
    pub const CONDFMT: u16 = 0x01B0;
    pub const CF: u16 = 0x01B1;
    pub const CONDFMT12: u16 = 0x0879;
    pub const CF12: u16 = 0x087A;
    pub const DVAL: u16 = 0x01B2;
    pub const DV: u16 = 0x01BE;
    pub const AUTOFILTERINFO: u16 = 0x009D;
    pub const AUTOFILTER: u16 = 0x009E;
    pub const FILTERMODE: u16 = 0x009B;
    pub const SHEETPROTECTION: u16 = 0x0867;
    pub const SHEETEXT: u16 = 0x0862;
    pub const GUTS: u16 = 0x0080;
    pub const WSBOOL: u16 = 0x0081;
}
