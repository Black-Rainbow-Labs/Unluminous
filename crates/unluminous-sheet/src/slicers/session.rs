//! Making, selecting and deleting slicers and timelines. Each change is one undo step.
//!
//! A selection is made in the pivot table: a slicer's items are the items of the field it filters, and
//! choosing items shows those and hides the rest in every table the slicer's cache filters.

use super::sync::{home_of, items_of};
use super::{Slicer, SlicerCache, Timeline, TimelineCache};
use crate::coord::CellRef;
use crate::edit::Session;
use crate::model::Workbook;
use crate::objects::geometry::{point_of, Rect};
use crate::objects::{Anchor, AnchorKind, DrawingObject, ObjectKind, Placement};
use crate::pivot::cache::CacheValue;
use crate::pivot::table::{Filter, FilterKind};
use crate::xlsx::xmltree;

const SLICER_SIZE: (i64, i64) = (1_828_800, 2_286_000);
const TIMELINE_SIZE: (i64, i64) = (3_810_000, 1_270_000);

/// The text an item of a pivot field shows.
pub fn item_text(book: &Workbook, cache: usize, field: usize, x: usize) -> String {
    match book.pivots.caches[cache].fields[field].items().get(x) {
        Some(CacheValue::Blank) | None => "(blank)".to_owned(),
        Some(CacheValue::Text(t)) => t.to_string(),
        Some(v) => v.to_value().to_text(),
    }
}

/// The graphic frame of a slicer or timeline, in an `mc:AlternateContent` as Excel writes it.
fn frame_xml(timeline: bool, id: u32, name: &str, size: (i64, i64)) -> String {
    let (requires, ns_decl, uri, tag, note) = if timeline {
        (
            "tsle",
            "xmlns:tsle=\"http://schemas.microsoft.com/office/drawing/2012/timeslicer\"",
            "http://schemas.microsoft.com/office/drawing/2012/timeslicer",
            "tsle:timeslicer",
            "Timeline: Works in Excel 2013 or higher. Do not move or resize.",
        )
    } else {
        (
            "a14",
            "xmlns:a14=\"http://schemas.microsoft.com/office/drawing/2010/main\"",
            "http://schemas.microsoft.com/office/drawing/2010/slicer",
            "sle:slicer",
            "This shape represents a slicer. Slicers are supported in Excel 2010 or later.",
        )
    };
    let tag_ns = if timeline {
        ""
    } else {
        " xmlns:sle=\"http://schemas.microsoft.com/office/drawing/2010/slicer\""
    };
    let name = crate::xlsx::escape(name);
    format!(
        "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><mc:Choice {ns_decl} Requires=\"{requires}\"><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{uri}\"><{tag}{tag_ns} name=\"{name}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice><mc:Fallback><xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"0\" name=\"\"/><xdr:cNvSpPr><a:spLocks noTextEdit=\"1\"/></xdr:cNvSpPr></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:prstClr val=\"white\"/></a:solidFill><a:ln w=\"1\"><a:solidFill><a:prstClr val=\"green\"/></a:solidFill></a:ln></xdr:spPr><xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\"/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\" sz=\"1100\"/><a:t>{note}</a:t></a:r></a:p></xdr:txBody></xdr:sp></mc:Fallback></mc:AlternateContent>",
        size.0, size.1
    )
}

impl Session {
    /// The pivot table a name names, or the reason there is none.
    fn pivot_named(&self, name: &str) -> Result<usize, String> {
        self.book.pivots.find(name).ok_or_else(|| format!("There is no PivotTable named {name}."))
    }

    /// A drawing object for a slicer or a timeline with its top left corner at a cell of a sheet.
    fn frame_object(
        &mut self,
        sheet: usize,
        timeline: bool,
        name: &str,
        at: CellRef,
    ) -> Result<(), String> {
        let size = if timeline { TIMELINE_SIZE } else { SLICER_SIZE };
        let id =
            self.book.sheets[sheet].drawing.objects.iter().map(|o| o.id).max().map_or(2, |m| m + 1);
        let tree = xmltree::parse(&frame_xml(timeline, id, name, size))
            .ok_or("the slicer's frame could not be made")?;
        let (x, y) = point_of(&self.book.sheets[sheet], (at, 0, 0));
        let mut anchor =
            Anchor { kind: AnchorKind::TwoCell, placement: Placement::Move, ..Anchor::default() };
        anchor.set_rect(&self.book.sheets[sheet], Rect::new(x, y, size.0, size.1));
        let nothing = Rect::new(0, 0, 0, 0);
        self.book.sheets[sheet].drawing.objects.push(DrawingObject {
            id,
            name: name.to_owned(),
            descr: String::new(),
            hidden: false,
            kind: ObjectKind::Other,
            anchor,
            frame: nothing,
            rotation: 0.0,
            frame_size: (0, 0),
            flip_h: false,
            flip_v: false,
            lock_aspect: false,
            locked: true,
            prints: true,
            picture: None,
            shape: None,
            children: Vec::new(),
            fallback: None,
            child_space: nothing,
            tree,
            wrapped: false,
        });
        self.book.sheets[sheet].drawing.changed = true;
        Ok(())
    }

    /// A name no slicer or timeline has, made by adding a number to the one wanted.
    fn unique_frame_name(&self, wanted: &str) -> String {
        let taken = |n: &str| {
            self.book.slicers.slicers.iter().any(|s| s.name.eq_ignore_ascii_case(n))
                || self.book.slicers.timelines.iter().any(|s| s.name.eq_ignore_ascii_case(n))
        };
        if !taken(wanted) {
            return wanted.to_owned();
        }
        (1..).map(|n| format!("{wanted} {n}")).find(|n| !taken(n)).unwrap_or_default()
    }

    /// Give the pivot cache the number slicers know it by, and write its definition again.
    fn number_the_pivot_cache(&mut self, table: usize) {
        let cache = self.book.pivots.tables[table].cache;
        let c = &mut self.book.pivots.caches[cache];
        // Excel binds a slicer only to a cache that says which versions made and refreshed it.
        let versions =
            [("createdVersion", "8"), ("refreshedVersion", "8"), ("minRefreshableVersion", "3")];
        let missing = versions.iter().any(|(k, _)| !c.attrs.iter().any(|(n, _)| n == k));
        if c.slicer_id.is_none() || missing {
            c.slicer_id.get_or_insert(1_000_000_000 + u64::from(c.id) * 7919);
            for (k, v) in versions {
                if !c.attrs.iter().any(|(n, _)| n == k) {
                    c.attrs.push((k.to_owned(), v.to_owned()));
                }
            }
            c.dirty = true;
            crate::pivot::xml::sync_cache(&mut self.book, cache);
        }
    }

    // ------------------------------------------------------------------------------------- slicers

    /// Insert a slicer for a field of a pivot table, with its top left corner at a cell. Returns its name.
    pub fn slicer_add(
        &mut self,
        sheet: usize,
        pivot: &str,
        field: &str,
        name: Option<&str>,
        at: CellRef,
    ) -> Result<String, String> {
        self.check_sheet(sheet)?;
        let t = self.pivot_named(pivot)?;
        let cache_index = self.book.pivots.tables[t].cache;
        let f = self.book.pivots.caches[cache_index]
            .fields
            .iter()
            .position(|x| x.name.eq_ignore_ascii_case(field))
            .ok_or_else(|| format!("There is no field named {field}."))?;
        self.book.pivots.caches[cache_index].ensure_shared(f);
        let source = self.book.pivots.caches[cache_index].fields[f].name.clone();
        let table_name = self.book.pivots.tables[t].name.clone();
        self.remember_whole("Insert Slicer");
        self.number_the_pivot_cache(t);
        let cache = self.cache_for_field(&source, &table_name);
        let slicer_name = self.unique_frame_name(name.unwrap_or(&source));
        self.frame_object(sheet, false, &slicer_name, at)?;
        self.book.slicers.slicers.push(Slicer {
            name: slicer_name.clone(),
            cache,
            caption: Some(slicer_name.clone()),
            attrs: vec![("rowHeight".to_owned(), "241300".to_owned())],
            uid: None,
            sheet,
            extra: String::new(),
        });
        self.book.slicers.changed = true;
        Ok(slicer_name)
    }

    /// The slicer cache for a field of a pivot table: the one there is, or a new one named `Slicer_<field>`.
    fn cache_for_field(&mut self, source: &str, table_name: &str) -> String {
        if let Some(c) = self.book.slicers.caches.iter().find(|c| {
            c.source.eq_ignore_ascii_case(source) && c.pivots.iter().any(|p| p == table_name)
        }) {
            return c.name.clone();
        }
        let base = format!("Slicer_{}", source.replace(' ', "_"));
        let taken =
            |n: &str| self.book.slicers.caches.iter().any(|c| c.name.eq_ignore_ascii_case(n));
        let name = if taken(&base) {
            (1..).map(|n| format!("{base}{n}")).find(|n| !taken(n)).unwrap_or_default()
        } else {
            base
        };
        self.book.slicers.caches.push(SlicerCache {
            name: name.clone(),
            source: source.to_owned(),
            pivots: vec![table_name.to_owned()],
            ..SlicerCache::default()
        });
        name
    }

    /// The items of a slicer in the order it lists them, each with whether it is selected.
    pub fn slicer_items(&self, name: &str) -> Result<Vec<(String, bool)>, String> {
        let slicer = self
            .book
            .slicers
            .slicer(name)
            .ok_or_else(|| format!("There is no slicer named {name}."))?;
        let cache = self.book.slicers.cache(&slicer.cache).ok_or("The slicer has no cache.")?;
        let table = cache
            .pivots
            .iter()
            .find_map(|p| self.book.pivots.find(p))
            .ok_or("The slicer's PivotTable is gone.")?;
        let pc = self.book.pivots.tables[table].cache;
        let f = self.book.pivots.caches[pc]
            .fields
            .iter()
            .position(|x| x.name.eq_ignore_ascii_case(&cache.source))
            .ok_or("The slicer's field is gone.")?;
        Ok(items_of(&self.book, cache, table)
            .into_iter()
            .map(|(x, selected)| (item_text(&self.book, pc, f, x), selected))
            .collect())
    }

    /// Select the items of a slicer: those named are shown in every PivotTable it filters and the others are
    /// hidden. `None`, or no names, clears the filter.
    pub fn slicer_select(&mut self, name: &str, items: Option<&[String]>) -> Result<(), String> {
        let slicer = self
            .book
            .slicers
            .slicer(name)
            .cloned()
            .ok_or_else(|| format!("There is no slicer named {name}."))?;
        let cache =
            self.book.slicers.cache(&slicer.cache).cloned().ok_or("The slicer has no cache.")?;
        let known = self.slicer_items(name)?;
        let wanted: Option<Vec<String>> = match items {
            Some(list) if !list.is_empty() => {
                for item in list {
                    if !known.iter().any(|(k, _)| k.eq_ignore_ascii_case(item)) {
                        return Err(format!("The slicer has no item {item}."));
                    }
                }
                Some(list.iter().map(|s| s.to_lowercase()).collect())
            }
            _ => None,
        };
        self.remember_whole("Slicer");
        self.show_only_in_pivots(&cache.pivots, &cache.source, |text| {
            wanted.as_ref().is_none_or(|w| w.contains(&text.to_lowercase()))
        })?;
        self.book.slicers.changed = true;
        Ok(())
    }

    /// In every table named, show the items of a field for which `keep` is true and hide the others.
    fn show_only_in_pivots(
        &mut self,
        pivots: &[String],
        source: &str,
        keep: impl Fn(&str) -> bool,
    ) -> Result<(), String> {
        for p in pivots {
            let Some(t) = self.book.pivots.find(p) else { continue };
            let pc = self.book.pivots.tables[t].cache;
            let Some(f) = self.book.pivots.caches[pc]
                .fields
                .iter()
                .position(|x| x.name.eq_ignore_ascii_case(source))
            else {
                continue;
            };
            self.fill_items(t, f);
            let shown: Vec<bool> = self.book.pivots.tables[t].fields[f]
                .items
                .iter()
                .map(|e| keep(&item_text(&self.book, pc, f, e.x)))
                .collect();
            let table = &mut self.book.pivots.tables[t];
            for (entry, show) in table.fields[f].items.iter_mut().zip(shown) {
                entry.hidden = !show;
            }
            table.fields[f].filters.clear();
            for page in &mut table.pages {
                if page.field == f {
                    page.item = None;
                }
            }
            self.relayout(t)?;
        }
        Ok(())
    }

    /// Delete a slicer. The filter goes with the last slicer that held it.
    pub fn slicer_delete(&mut self, name: &str) -> Result<(), String> {
        let slicer = self
            .book
            .slicers
            .slicer(name)
            .cloned()
            .ok_or_else(|| format!("There is no slicer named {name}."))?;
        self.remember_whole("Delete Slicer");
        self.drop_frame("slicer", &slicer.name);
        self.book.slicers.slicers.retain(|s| !s.name.eq_ignore_ascii_case(name));
        let used =
            self.book.slicers.slicers.iter().any(|s| s.cache.eq_ignore_ascii_case(&slicer.cache));
        if !used {
            if let Some(cache) = self.book.slicers.cache(&slicer.cache).cloned() {
                self.show_only_in_pivots(&cache.pivots, &cache.source, |_| true)?;
            }
            self.book.slicers.caches.retain(|c| !c.name.eq_ignore_ascii_case(&slicer.cache));
        }
        self.book.slicers.changed = true;
        Ok(())
    }

    /// Take the drawing object that carries a slicer or a timeline off its sheet.
    fn drop_frame(&mut self, local: &str, name: &str) {
        if let Some(sheet) = home_of(&self.book, local, name) {
            let drawing = &mut self.book.sheets[sheet].drawing;
            drawing
                .objects
                .retain(|o| !o.tree.find(local).is_some_and(|e| e.attr("name") == Some(name)));
            drawing.changed = true;
        }
    }

    // ----------------------------------------------------------------------------------- timelines

    /// Insert a timeline for a date field of a pivot table. Returns its name.
    pub fn timeline_add(
        &mut self,
        sheet: usize,
        pivot: &str,
        field: &str,
        name: Option<&str>,
        at: CellRef,
    ) -> Result<String, String> {
        self.check_sheet(sheet)?;
        let t = self.pivot_named(pivot)?;
        let pc = self.book.pivots.tables[t].cache;
        let f = self.book.pivots.caches[pc]
            .fields
            .iter()
            .position(|x| x.name.eq_ignore_ascii_case(field))
            .ok_or_else(|| format!("There is no field named {field}."))?;
        if !self.book.pivots.caches[pc].is_date_field(f) {
            return Err(format!("{field} holds no dates, so a timeline cannot use it."));
        }
        self.book.pivots.caches[pc].ensure_shared(f);
        let (start, end) = self.date_bounds(pc, f);
        let source = self.book.pivots.caches[pc].fields[f].name.clone();
        let table_name = self.book.pivots.tables[t].name.clone();
        self.remember_whole("Insert Timeline");
        self.number_the_pivot_cache(t);
        let cache_name = self.unique_timeline_cache(&source);
        let timeline_name = self.unique_frame_name(name.unwrap_or(&source));
        self.frame_object(sheet, true, &timeline_name, at)?;
        self.book.slicers.timeline_caches.push(TimelineCache {
            name: cache_name.clone(),
            source,
            pivots: vec![table_name],
            bounds: Some((start.clone(), end)),
            ..TimelineCache::default()
        });
        self.book.slicers.timelines.push(Timeline {
            name: timeline_name.clone(),
            cache: cache_name,
            caption: Some(timeline_name.clone()),
            attrs: vec![
                ("level".to_owned(), "2".to_owned()),
                ("selectionLevel".to_owned(), "2".to_owned()),
                ("scrollPosition".to_owned(), start),
            ],
            uid: None,
            sheet,
            extra: String::new(),
        });
        self.book.slicers.changed = true;
        Ok(timeline_name)
    }

    fn unique_timeline_cache(&self, source: &str) -> String {
        let base = format!("NativeTimeline_{}", source.replace(' ', "_"));
        let taken = |n: &str| {
            self.book.slicers.timeline_caches.iter().any(|c| c.name.eq_ignore_ascii_case(n))
        };
        if !taken(&base) {
            return base;
        }
        (1..).map(|n| format!("{base}{n}")).find(|n| !taken(n)).unwrap_or_default()
    }

    /// The first day of the year of the earliest date and of the year after the latest, as Excel's bounds.
    fn date_bounds(&self, pc: usize, f: usize) -> (String, String) {
        let dates: Vec<f64> = self.book.pivots.caches[pc].fields[f]
            .items()
            .iter()
            .filter_map(CacheValue::number)
            .collect();
        let first = dates.iter().copied().fold(f64::INFINITY, f64::min);
        let last = dates.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let year =
            |serial: f64| crate::dates::date(serial, self.book.date1904).map_or(1900, |d| d.0);
        let iso = |y: i64| format!("{y:04}-01-01T00:00:00");
        (iso(year(first)), iso(year(last) + 1))
    }

    /// Select the dates of a timeline, as first and last serial inclusive, or every date with `None`.
    pub fn timeline_select(&mut self, name: &str, span: Option<(f64, f64)>) -> Result<(), String> {
        let timeline = self
            .book
            .slicers
            .timeline(name)
            .cloned()
            .ok_or_else(|| format!("There is no timeline named {name}."))?;
        let index = self
            .book
            .slicers
            .timeline_caches
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(&timeline.cache))
            .ok_or("The timeline has no cache.")?;
        let cache = self.book.slicers.timeline_caches[index].clone();
        self.remember_whole("Timeline");
        let date1904 = self.book.date1904;
        self.show_dates(&cache, span)?;
        let iso = |serial: f64| crate::pivot::xml_write::iso(serial.floor(), date1904);
        self.book.slicers.timeline_caches[index].selection = span.map(|(a, b)| (iso(a), iso(b)));
        self.book.slicers.changed = true;
        Ok(())
    }

    /// Show the items of the field whose dates are in the span and hide the others.
    fn show_dates(
        &mut self,
        cache: &TimelineCache,
        span: Option<(f64, f64)>,
    ) -> Result<(), String> {
        for p in &cache.pivots {
            let Some(t) = self.book.pivots.find(p) else { continue };
            let pc = self.book.pivots.tables[t].cache;
            let Some(f) = self.book.pivots.caches[pc]
                .fields
                .iter()
                .position(|x| x.name.eq_ignore_ascii_case(&cache.source))
            else {
                continue;
            };
            self.fill_items(t, f);
            // A timeline filters as Excel's does: a date filter on the field, which stays in force while the
            // field is in no area.
            let field = &mut self.book.pivots.tables[t].fields[f];
            field.filters.retain(|x| x.kind != FilterKind::DateBetween);
            for entry in &mut field.items {
                entry.hidden = false;
            }
            if let Some((a, b)) = span {
                field.filters.push(Filter {
                    kind: FilterKind::DateBetween,
                    data: None,
                    value1: (a.floor() as i64).to_string(),
                    value2: Some((b.floor() as i64).to_string()),
                    top: true,
                    id: 1,
                });
            }
            self.relayout(t)?;
        }
        Ok(())
    }

    /// Delete a timeline and show every date again.
    pub fn timeline_delete(&mut self, name: &str) -> Result<(), String> {
        let timeline = self
            .book
            .slicers
            .timeline(name)
            .cloned()
            .ok_or_else(|| format!("There is no timeline named {name}."))?;
        self.remember_whole("Delete Timeline");
        self.drop_frame("timeslicer", &timeline.name);
        self.book.slicers.timelines.retain(|t| !t.name.eq_ignore_ascii_case(name));
        let cache = self
            .book
            .slicers
            .timeline_caches
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&timeline.cache))
            .cloned();
        if let Some(cache) = cache {
            self.show_dates(&cache, None)?;
        }
        let used = self
            .book
            .slicers
            .timelines
            .iter()
            .any(|t| t.cache.eq_ignore_ascii_case(&timeline.cache));
        if !used {
            self.book
                .slicers
                .timeline_caches
                .retain(|c| !c.name.eq_ignore_ascii_case(&timeline.cache));
        }
        self.book.slicers.changed = true;
        Ok(())
    }

    /// A pivot table was deleted: the slicers and timelines that filtered only it go with it.
    pub(crate) fn forget_slicers_of(&mut self, pivot: &str) {
        for c in &mut self.book.slicers.caches {
            c.pivots.retain(|p| !p.eq_ignore_ascii_case(pivot));
        }
        for c in &mut self.book.slicers.timeline_caches {
            c.pivots.retain(|p| !p.eq_ignore_ascii_case(pivot));
        }
        let s = &self.book.slicers;
        let gone: Vec<String> =
            s.caches.iter().filter(|c| c.pivots.is_empty()).map(|c| c.name.clone()).collect();
        let slicers: Vec<String> =
            s.slicers.iter().filter(|x| gone.contains(&x.cache)).map(|x| x.name.clone()).collect();
        let gone_t: Vec<String> = s
            .timeline_caches
            .iter()
            .filter(|c| c.pivots.is_empty())
            .map(|c| c.name.clone())
            .collect();
        let timelines: Vec<String> = s
            .timelines
            .iter()
            .filter(|x| gone_t.contains(&x.cache))
            .map(|x| x.name.clone())
            .collect();
        for name in slicers {
            self.drop_frame("slicer", &name);
        }
        for name in timelines {
            self.drop_frame("timeslicer", &name);
        }
        self.book.slicers.changed = true;
    }
}
