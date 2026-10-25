//! What kind of chart a [`Chart`] is, worked out from its groups and series.

use crate::chartspec::{ChartType, GroupKind};
use crate::model::{Chart, ChartKind};

impl Chart {
    /// The chart's type as Excel's Insert Chart dialog names it, with the stock variants told apart by
    /// their series.
    pub fn chart_type(&self) -> ChartType {
        let by_groups = self.spec.chart_type();
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
