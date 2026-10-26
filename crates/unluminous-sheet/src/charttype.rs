//! What kind of chart a [`Chart`] is, worked out from its groups and series.

use crate::chartspec::{ChartType, GroupKind};
use crate::model::{Chart, ChartKind, ChartSeries};

impl ChartSeries {
    /// A series of a name, categories and values, in the first group, with no formatting.
    pub fn new(name: Option<String>, categories: Option<String>, values: String) -> Self {
        Self { name, categories, values, ..Self::default() }
    }
}

impl Chart {
    /// The chart's type as Excel's Insert Chart dialog names it, with the stock variants told apart by
    /// their series.
    pub fn chart_type(&self) -> ChartType {
        let by_groups = self.refine_by_series(self.spec.chart_type());
        let Some(stock) = self.spec.groups.iter().position(|g| g.kind == GroupKind::Stock) else {
            return by_groups;
        };
        let count = self.series.iter().filter(|s| s.group == stock).count();
        let has_volume = self.spec.groups.iter().any(|g| g.kind == GroupKind::Bar);
        match (has_volume, count) {
            (false, 4) => ChartType::StockOhlc,
            (false, _) => ChartType::StockHlc,
            (true, 4) => ChartType::StockVohlc,
            (true, _) => ChartType::StockVhlc,
        }
    }

    /// Excel tells a line chart from one with markers, and the kinds of scatter chart apart, by how the
    /// series are drawn, not by anything on the group.
    fn refine_by_series(&self, by_groups: ChartType) -> ChartType {
        use ChartType as T;
        let Some(group) = self.spec.groups.first() else { return by_groups };
        let members: Vec<&crate::model::ChartSeries> = self.series.iter().filter(|s| s.group == 0).collect();
        let Some(first) = members.first() else { return by_groups };
        let no_markers = members.iter().all(|s| s.fmt.marker.as_ref().and_then(|m| m.symbol.as_deref()) == Some("none"));
        match (group.kind, by_groups) {
            (GroupKind::Line, T::LineMarkers) if no_markers => T::Line,
            (GroupKind::Line, T::LineMarkersStacked) if no_markers => T::LineStacked,
            (GroupKind::Line, T::LineMarkersStacked100) if no_markers => T::LineStacked100,
            (GroupKind::Scatter, _) if self.spec.groups.len() == 1 => {
                let lines_off = first.fmt.sp_pr.as_deref().is_some_and(|x| x.contains("<a:ln") && x.contains("<a:noFill/></a:ln>"));
                let smooth = first.fmt.smooth == Some(true);
                match (lines_off, smooth, no_markers) {
                    (true, _, _) => T::ScatterMarkers,
                    (false, true, false) => T::ScatterSmooth,
                    (false, true, true) => T::ScatterSmoothNoMarkers,
                    (false, false, false) => T::ScatterLines,
                    (false, false, true) => T::ScatterLinesNoMarkers,
                }
            }
            _ => by_groups,
        }
    }

    /// Set the coarse [`ChartKind`] from the model, after the groups changed.
    pub fn sync_kind(&mut self) {
        self.kind = ChartKind::of_type(self.chart_type());
    }
}

impl ChartKind {
    /// The coarse family a precise chart type belongs to.
    pub fn of_type(t: ChartType) -> Self {
        use ChartKind as K;
        use ChartType as T;
        match t {
            T::ColumnClustered
            | T::ColumnStacked
            | T::ColumnStacked100
            | T::Column3DClustered
            | T::Column3DStacked
            | T::Column3DStacked100
            | T::Column3D => K::Column,
            T::BarClustered
            | T::BarStacked
            | T::BarStacked100
            | T::Bar3DClustered
            | T::Bar3DStacked
            | T::Bar3DStacked100 => K::Bar,
            T::Line
            | T::LineStacked
            | T::LineStacked100
            | T::LineMarkers
            | T::LineMarkersStacked
            | T::LineMarkersStacked100
            | T::Line3D
            | T::StockHlc
            | T::StockOhlc
            | T::StockVhlc
            | T::StockVohlc => K::Line,
            T::Area
            | T::AreaStacked
            | T::AreaStacked100
            | T::Area3D
            | T::Area3DStacked
            | T::Area3DStacked100 => K::Area,
            T::Pie | T::PieExploded | T::Pie3D | T::Pie3DExploded | T::PieOfPie | T::BarOfPie => {
                K::Pie
            }
            T::Doughnut | T::DoughnutExploded => K::Doughnut,
            T::ScatterMarkers
            | T::ScatterSmooth
            | T::ScatterSmoothNoMarkers
            | T::ScatterLines
            | T::ScatterLinesNoMarkers
            | T::Bubble
            | T::Bubble3D => K::Scatter,
            T::Surface | T::SurfaceWireframe | T::SurfaceTopView | T::SurfaceTopViewWireframe => {
                K::Other("surface")
            }
            T::Radar | T::RadarMarkers | T::RadarFilled => K::Other("radar"),
            T::Treemap => K::Other("treemap"),
            T::Sunburst => K::Other("sunburst"),
            T::Histogram => K::Other("histogram"),
            T::Pareto => K::Other("pareto"),
            T::BoxWhisker => K::Other("box and whisker"),
            T::Waterfall => K::Other("waterfall"),
            T::Funnel => K::Other("funnel"),
            T::Map => K::Other("map"),
            T::Combo => K::Other("combo"),
        }
    }
}
