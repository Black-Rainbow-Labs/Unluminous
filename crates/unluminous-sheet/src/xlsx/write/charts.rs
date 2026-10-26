//! The chart half of the writer: drawings with changed charts, chart parts written from the model, chart
//! sheets made here, and the parts of charts that were removed.

use std::collections::BTreeSet;

use super::*;
use crate::model::Chart;
use crate::xlsx::chart_drawing::{
    chart_sheet_xml, rebuild, CT_CHART_EX, CT_CHART_SHEET, REL_CHART_EX,
};

/// Every part that goes when a chart is removed: the chart, its relationships and what they name.
pub(super) fn dropped_parts(book: &Workbook) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for sheet in &book.sheets {
        for part in &sheet.chart_removed {
            let rels = rels_path(part);
            if let Some(bytes) = book.kept.get(&rels) {
                for r in parse_rels(&String::from_utf8_lossy(bytes)) {
                    if !r.external {
                        out.insert(resolve(part, &r.target));
                    }
                }
            }
            out.insert(rels);
            out.insert(part.clone());
        }
    }
    out
}

impl<'a> Writer<'a> {
    /// The part a chart is written to, with its content type.
    fn chart_bytes(&self, chart: &Chart) -> Vec<u8> {
        if chart.spec.ex.is_some() {
            let base = self.ex_name_base();
            let refs = self.ex_refs();
            let name_of = move |r: &str| match refs.iter().position(|x| x == r) {
                Some(i) => format!("_xlchart.v1.{}", base + i),
                None => r.to_owned(),
            };
            return crate::xlsx::chartex::write(chart, &name_of).into_bytes();
        }
        let book = self.book;
        let resolve = move |f: &str| crate::chartdata::resolve(book, 0, f);
        crate::xlsx::chart_write::chart_xml(chart, &crate::xlsx::chart_write::Env { resolve: &resolve }).into_bytes()
    }

    /// The next free number for an `_xlchart.v1.N` name, after the ones the file already has.
    pub(super) fn ex_name_base(&self) -> usize {
        let used = self.book.names.iter().filter_map(|n| n.name.strip_prefix("_xlchart.v1.")?.parse::<usize>().ok());
        used.max().map_or(0, |m| m + 1)
    }

    /// The distinct ranges the extended charts written from the model refer to, in the order they are named.
    pub(super) fn ex_refs(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for chart in self.book.sheets.iter().flat_map(|s| s.charts.iter()) {
            if chart.spec.ex.is_none() || !(chart.new || chart.spec.dirty) {
                continue;
            }
            for s in &chart.series {
                let name = s.name.iter().filter(|n| !n.starts_with('"'));
                for r in s.categories.iter().chain(std::iter::once(&s.values)).chain(name) {
                    if !out.contains(r) {
                        out.push(r.clone());
                    }
                }
            }
        }
        out
    }

    /// Charts read from the file and changed since are written from the model over the part they came from.
    pub(super) fn write_changed_charts(&mut self) {
        for sheet in &self.book.sheets {
            for chart in sheet.charts.iter().filter(|c| !c.new && c.spec.dirty && !c.part.is_empty()) {
                let bytes = self.chart_bytes(chart);
                self.entries.retain(|e| e.name != chart.part);
                self.add(&chart.part.clone(), None, bytes);
            }
        }
    }

    /// The `drawing` element of a sheet, after the charts that were added, moved or removed.
    ///
    /// `raw_drawing` is the element as it was read; it is returned unchanged when no chart changed.
    pub(super) fn chart_drawing(
        &mut self,
        sheet: &Sheet,
        part: &str,
        rels: &mut Vec<Relationship>,
        r: &str,
        raw_drawing: String,
    ) -> String {
        let changed = sheet.charts.iter().any(|c| c.new || c.spec.moved) || !sheet.chart_removed.is_empty();
        if !changed {
            return raw_drawing;
        }
        let existing = sheet.raw.rels.iter().find(|x| x.kind == REL_DRAWING).cloned();
        let (drawing_part, old_rels, old_xml) = self.drawing_source(&existing, part);
        self.taken.insert(drawing_part.clone());
        let mut new_rels = old_rels.clone();
        let mut anchors = String::new();
        let mut shape = 1000u32;
        for chart in sheet.charts.iter().filter(|c| c.new) {
            let id = self.add_new_chart(chart, &drawing_part, &mut new_rels);
            shape += 1;
            anchors.push_str(&crate::xlsx::chart::anchor_xml(chart, &id, shape));
        }
        let plain: Vec<Chart> = sheet.charts.clone();
        let built = rebuild(old_xml.as_deref(), &new_rels, &drawing_part, &plain, &sheet.chart_removed, &anchors);
        let content_type = if existing.is_some() { None } else { Some(CT_DRAWING) };
        self.add(&drawing_part, content_type, built.xml.into_bytes());
        self.add(&rels_path(&drawing_part), None, write_rels(&built.rels).into_bytes());
        match existing {
            Some(_) => raw_drawing,
            None => relate_new_drawing(rels, part, &drawing_part, r),
        }
    }

    /// The part, relationships and XML of the drawing a sheet already has, or a fresh part with none.
    fn drawing_source(&mut self, existing: &Option<Relationship>, part: &str) -> (String, Vec<Relationship>, Option<String>) {
        let book = self.book;
        match existing {
            Some(rel) => {
                let path = resolve(part, &rel.target);
                let text = |p: &str| book.kept.get(p).map(|b| String::from_utf8_lossy(b).into_owned());
                let xml = text(&path);
                let old_rels = parse_rels(&text(&rels_path(&path)).unwrap_or_default());
                (path, old_rels, xml)
            }
            None => (self.fresh("xl/drawings/drawing{}.xml"), Vec::new(), None),
        }
    }

    /// Write a chart made here as a new part and relate the drawing to it; returns the relationship id.
    fn add_new_chart(&mut self, chart: &Chart, drawing_part: &str, rels: &mut Vec<Relationship>) -> String {
        let extended = chart.spec.ex.is_some();
        let pattern = if extended { "xl/charts/chartEx{}.xml" } else { "xl/charts/chart{}.xml" };
        let chart_part = self.fresh(pattern);
        let (kind, content_type) =
            if extended { (REL_CHART_EX, CT_CHART_EX) } else { (REL_CHART, CT_CHART) };
        let bytes = self.chart_bytes(chart);
        self.add(&chart_part, Some(content_type), bytes);
        let id = next_id(rels);
        rels.push(Relationship {
            id: id.clone(),
            kind: kind.to_owned(),
            target: relative(drawing_part, &chart_part),
            external: false,
        });
        id
    }

    /// A chart sheet made here: its part, its drawing and its chart. Returns the part's path.
    pub(super) fn new_chart_sheet(&mut self, sheet: &Sheet) -> String {
        let part = self.fresh("xl/chartsheets/sheet{}.xml");
        self.taken.insert(part.clone());
        let drawing_part = self.fresh("xl/drawings/drawing{}.xml");
        self.taken.insert(drawing_part.clone());
        let mut drawing_rels = Vec::new();
        let mut anchors = String::new();
        for chart in sheet.charts.iter().take(1) {
            let id = self.add_new_chart(chart, &drawing_part, &mut drawing_rels);
            let mut whole = chart.clone();
            whole.spec.anchor_edit_as = Some("twoCell".to_owned());
            anchors.push_str(&whole_sheet_anchor(&whole, &id));
        }
        self.add(&drawing_part, Some(CT_DRAWING), crate::xlsx::chart::drawing_xml(&anchors).into_bytes());
        self.add(&rels_path(&drawing_part), None, write_rels(&drawing_rels).into_bytes());
        let sheet_rels = vec![Relationship {
            id: "rId1".to_owned(),
            kind: REL_DRAWING.to_owned(),
            target: relative(&part, &drawing_part),
            external: false,
        }];
        self.add(&part, Some(CT_CHART_SHEET), chart_sheet_xml("rId1").into_bytes());
        self.add(&rels_path(&part), None, write_rels(&sheet_rels).into_bytes());
        part
    }
}

/// Relate a sheet to a drawing part that was just made, and return the `drawing` element naming it.
fn relate_new_drawing(rels: &mut Vec<Relationship>, part: &str, drawing_part: &str, r: &str) -> String {
    let id = next_id(rels);
    rels.push(Relationship {
        id: id.clone(),
        kind: REL_DRAWING.to_owned(),
        target: relative(part, drawing_part),
        external: false,
    });
    format!("<drawing {r}:id=\"{id}\"/>")
}

/// The anchor of the chart on a chart sheet, which Excel fills the window with.
fn whole_sheet_anchor(chart: &Chart, rel_id: &str) -> String {
    let xml = crate::xlsx::chart::anchor_xml(chart, rel_id, 2);
    xml.replace("<xdr:twoCellAnchor editAs=\"twoCell\">", "<xdr:absoluteAnchor><xdr:pos x=\"0\" y=\"0\"/><xdr:ext cx=\"9144000\" cy=\"6096000\"/>")
        .replace("</xdr:twoCellAnchor>", "</xdr:absoluteAnchor>")
        .replacen(&format!("<xdr:from><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:from>", chart.from.0.col, chart.from.1, chart.from.0.row, chart.from.2), "", 1)
        .replacen(&format!("<xdr:to><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:to>", chart.to.0.col, chart.to.1, chart.to.0.row, chart.to.2), "", 1)
}
