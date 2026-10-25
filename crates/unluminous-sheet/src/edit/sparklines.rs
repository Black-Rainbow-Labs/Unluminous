//! Sparklines: inserting a group over a data range and a location range, changing every option of a group,
//! grouping and ungrouping, and clearing. Each method is one undo step and refuses what Excel refuses.

use super::Session;
use crate::coord::{CellRef, Range};
use crate::objects::sparkline::*;
use crate::styles::Color;

/// Excel's colours for a group made with Insert > Sparklines.
fn default_colors() -> SparklineColors {
    let red = Some(Color::Rgb(0xFFD0_0000));
    SparklineColors {
        series: Some(Color::Rgb(0xFF37_6092)),
        negative: red,
        axis: Some(Color::Rgb(0xFF00_0000)),
        markers: red,
        first: red,
        last: red,
        high: red,
        low: red,
    }
}

/// The text of a data reference on a sheet: `Sheet1!A1:D1`, quoted when the name needs it.
fn reference(sheet: &str, range: Range) -> String {
    let plain = sheet.chars().all(|c| c.is_alphanumeric() || c == '_') && !sheet.starts_with(|c: char| c.is_ascii_digit());
    let name = if plain { sheet.to_owned() } else { format!("'{}'", sheet.replace('\'', "''")) };
    format!("{name}!{}", range.a1())
}

/// The data range for each location cell: a column of locations takes a row of data each, a row of
/// locations takes a column each, and one cell takes the whole range.
fn pair_up(data: Range, location: Range) -> Result<Vec<(Range, CellRef)>, String> {
    let bad = || "The location reference is not valid because the dimensions do not match the data.".to_owned();
    let cells: Vec<CellRef> = location.cells().collect();
    if cells.len() == 1 {
        return Ok(vec![(data, cells[0])]);
    }
    if location.cols() == 1 && location.rows() == data.rows() {
        return Ok(cells
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let row = data.first.row + i as u32;
                (Range::new(CellRef::new(row, data.first.col), CellRef::new(row, data.last.col)), c)
            })
            .collect());
    }
    if location.rows() == 1 && location.cols() == data.cols() {
        return Ok(cells
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let col = data.first.col + i as u32;
                (Range::new(CellRef::new(data.first.row, col), CellRef::new(data.last.row, col)), c)
            })
            .collect());
    }
    Err(bad())
}

impl Session {
    /// Insert a group of sparklines on a sheet: `data` is on the sheet `data_sheet`, `location` on `sheet`.
    /// Returns the index of the new group.
    pub fn insert_sparklines(
        &mut self,
        sheet: usize,
        kind: SparklineKind,
        data_sheet: usize,
        data: Range,
        location: Range,
    ) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        self.check_sheet(data_sheet)?;
        let pairs = pair_up(data, location)?;
        let taken = |at: CellRef| self.book.sheets[sheet].sparklines.iter().any(|g| g.sparklines.iter().any(|s| s.location == at));
        if pairs.iter().any(|(_, c)| taken(*c)) {
            return Err("A sparkline already exists in the location range. Clear it first.".to_owned());
        }
        let data_name = self.book.sheets[data_sheet].name.clone();
        let group = SparklineGroup {
            kind,
            negative: kind == SparklineKind::WinLoss,
            colors: default_colors(),
            sparklines: pairs
                .into_iter()
                .map(|(d, c)| Sparkline { data: reference(&data_name, d), location: c })
                .collect(),
            ..SparklineGroup::default()
        };
        self.remember_whole("Insert sparklines");
        self.book.sheets[sheet].sparklines.push(group);
        Ok(self.book.sheets[sheet].sparklines.len() - 1)
    }

    /// The group a cell holds a sparkline of, and the sparkline's place in it.
    pub fn sparkline_at(&self, sheet: usize, at: CellRef) -> Option<(usize, usize)> {
        let groups = &self.book.sheets.get(sheet)?.sparklines;
        groups.iter().enumerate().find_map(|(g, grp)| grp.sparklines.iter().position(|s| s.location == at).map(|s| (g, s)))
    }

    /// Change a group's options with a function; the group is checked to be there first.
    pub fn update_sparkline_group(
        &mut self,
        sheet: usize,
        group: usize,
        change: impl FnOnce(&mut SparklineGroup),
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if group >= self.book.sheets[sheet].sparklines.len() {
            return Err("There is no such sparkline group.".to_owned());
        }
        self.remember_whole("Sparkline options");
        change(&mut self.book.sheets[sheet].sparklines[group]);
        Ok(())
    }

    /// Change a group to line, column or win/loss.
    pub fn set_sparkline_kind(&mut self, sheet: usize, group: usize, kind: SparklineKind) -> Result<(), String> {
        self.update_sparkline_group(sheet, group, |g| g.kind = kind)
    }

    /// Show or hide the points a group highlights: markers, high, low, first, last and negative.
    pub fn set_sparkline_points(&mut self, sheet: usize, group: usize, points: SparklinePoints) -> Result<(), String> {
        self.update_sparkline_group(sheet, group, |g| {
            g.markers = points.markers;
            g.high = points.high;
            g.low = points.low;
            g.first = points.first;
            g.last = points.last;
            g.negative = points.negative;
        })
    }

    /// Set the colours of a group. A colour left `None` is not changed.
    pub fn set_sparkline_colors(&mut self, sheet: usize, group: usize, colors: SparklineColors) -> Result<(), String> {
        self.update_sparkline_group(sheet, group, |g| {
            let c = &mut g.colors;
            c.series = colors.series.or(c.series);
            c.negative = colors.negative.or(c.negative);
            c.axis = colors.axis.or(c.axis);
            c.markers = colors.markers.or(c.markers);
            c.first = colors.first.or(c.first);
            c.last = colors.last.or(c.last);
            c.high = colors.high.or(c.high);
            c.low = colors.low.or(c.low);
        })
    }

    /// Set the line weight of a group in points.
    pub fn set_sparkline_weight(&mut self, sheet: usize, group: usize, weight: f64) -> Result<(), String> {
        if !(0.0..=10.0).contains(&weight) {
            return Err("The line weight must be between 0 and 10 points.".to_owned());
        }
        self.update_sparkline_group(sheet, group, |g| g.weight = Some(weight))
    }

    /// Set the vertical axis of a group.
    pub fn set_sparkline_axis(&mut self, sheet: usize, group: usize, min: AxisBound, max: AxisBound) -> Result<(), String> {
        if let (AxisBound::Custom(lo), AxisBound::Custom(hi)) = (min, max) {
            if lo >= hi {
                return Err("The minimum must be less than the maximum.".to_owned());
            }
        }
        self.update_sparkline_group(sheet, group, |g| {
            g.min = min;
            g.max = max;
            g.min_value = if let AxisBound::Custom(v) = min { Some(v) } else { None };
            g.max_value = if let AxisBound::Custom(v) = max { Some(v) } else { None };
        })
    }

    /// Show the horizontal axis, and plot right to left.
    pub fn set_sparkline_horizontal_axis(&mut self, sheet: usize, group: usize, show: bool, right_to_left: bool) -> Result<(), String> {
        self.update_sparkline_group(sheet, group, |g| {
            g.show_axis = show;
            g.right_to_left = right_to_left;
        })
    }

    /// Use a range of dates as the horizontal axis, or none.
    pub fn set_sparkline_date_axis(&mut self, sheet: usize, group: usize, dates: Option<&str>) -> Result<(), String> {
        let dates = dates.map(str::to_owned);
        self.update_sparkline_group(sheet, group, |g| g.date_axis = dates)
    }

    /// Choose how empty cells show and whether hidden cells are plotted.
    pub fn set_sparkline_hidden_and_empty(&mut self, sheet: usize, group: usize, empty: EmptyCells, hidden: bool) -> Result<(), String> {
        self.update_sparkline_group(sheet, group, |g| {
            g.empty = empty;
            g.hidden_cells = hidden;
        })
    }

    /// Point a sparkline at other data.
    pub fn set_sparkline_data(&mut self, sheet: usize, at: CellRef, data_sheet: usize, data: Range) -> Result<(), String> {
        self.check_sheet(data_sheet)?;
        let (g, s) = self.sparkline_at(sheet, at).ok_or("There is no sparkline in that cell.")?;
        let text = reference(&self.book.sheets[data_sheet].name, data);
        self.remember_whole("Edit sparkline data");
        self.book.sheets[sheet].sparklines[g].sparklines[s].data = text;
        Ok(())
    }

    /// Group the sparklines in these cells, so they share their looks. They take the looks of the first.
    pub fn group_sparklines(&mut self, sheet: usize, cells: &[CellRef]) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        let mut found = Vec::new();
        for &c in cells {
            found.push(self.sparkline_at(sheet, c).ok_or("Select cells that hold sparklines to group them.")?);
        }
        if found.len() < 2 {
            return Err("Select two or more sparklines to group them.".to_owned());
        }
        self.remember_whole("Group sparklines");
        let groups = &mut self.book.sheets[sheet].sparklines;
        let (first, _) = found[0];
        let mut moved = Vec::new();
        for (g, s) in &found {
            if *g != first {
                moved.push((*g, *s));
            }
        }
        let mut taken: Vec<Sparkline> = Vec::new();
        moved.sort_by(|a, b| b.cmp(a));
        for (g, s) in moved {
            taken.push(groups[g].sparklines.remove(s));
        }
        groups[first].sparklines.extend(taken.into_iter().rev());
        groups.retain(|g| !g.sparklines.is_empty());
        Ok(first.min(groups.len().saturating_sub(1)))
    }

    /// Ungroup a sparkline from its group: it keeps the looks and becomes a group of one.
    pub fn ungroup_sparkline(&mut self, sheet: usize, at: CellRef) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        let (g, s) = self.sparkline_at(sheet, at).ok_or("There is no sparkline in that cell.")?;
        if self.book.sheets[sheet].sparklines[g].sparklines.len() < 2 {
            return Err("That sparkline is not grouped with others.".to_owned());
        }
        self.remember_whole("Ungroup sparklines");
        let groups = &mut self.book.sheets[sheet].sparklines;
        let one = groups[g].sparklines.remove(s);
        let mut alone = groups[g].clone();
        alone.sparklines = vec![one];
        alone.extra.retain(|(n, _)| !n.ends_with("uid"));
        groups.push(alone);
        Ok(groups.len() - 1)
    }

    /// Clear the sparklines in a range of cells.
    pub fn clear_sparklines(&mut self, sheet: usize, range: Range) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        let count: usize = self.book.sheets[sheet]
            .sparklines
            .iter()
            .map(|g| g.sparklines.iter().filter(|s| range.contains(s.location)).count())
            .sum();
        if count == 0 {
            return Ok(0);
        }
        self.remember_whole("Clear sparklines");
        let groups = &mut self.book.sheets[sheet].sparklines;
        for g in groups.iter_mut() {
            g.sparklines.retain(|s| !range.contains(s.location));
        }
        groups.retain(|g| !g.sparklines.is_empty());
        Ok(count)
    }

    /// Clear the whole group that has a sparkline in a cell.
    pub fn clear_sparkline_group(&mut self, sheet: usize, at: CellRef) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let (g, _) = self.sparkline_at(sheet, at).ok_or("There is no sparkline in that cell.")?;
        self.remember_whole("Clear sparkline group");
        self.book.sheets[sheet].sparklines.remove(g);
        Ok(())
    }
}

/// The points a group of sparklines highlights.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SparklinePoints {
    pub markers: bool,
    pub high: bool,
    pub low: bool,
    pub first: bool,
    pub last: bool,
    pub negative: bool,
}
