//! Everything about a chart other than where it sits and what its series point at.
//!
//! A chart part is read into a [`ChartSpec`]: the chart groups the plot area holds (a column group, a line
//! group on the secondary axis, a pie), the axes, the legend, the data table, and for each series its
//! labels, trendlines, error bars and point formats. The things the model does not name, a series' fill and
//! outline, text properties, layouts, extension lists, are kept as the XML they were read as, so writing
//! the chart again puts them back where the schema wants them.
//!
//! The newer Office 2016 chart types (treemap, sunburst, histogram, Pareto, box and whisker, waterfall,
//! funnel, map) live in separate `chartEx` parts and are held in [`ExSpec`].

/// How a chart's hidden and empty cells are treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blanks {
    /// Empty cells leave a gap.
    #[default]
    Gap,
    /// Empty cells are plotted as zero.
    Zero,
    /// The points either side of an empty cell are joined.
    Span,
}

impl Blanks {
    pub fn xml(self) -> &'static str {
        match self {
            Blanks::Gap => "gap",
            Blanks::Zero => "zero",
            Blanks::Span => "span",
        }
    }

    pub fn from_xml(text: &str) -> Self {
        match text {
            "zero" => Blanks::Zero,
            "span" => Blanks::Span,
            _ => Blanks::Gap,
        }
    }
}

/// What kind of plot a chart group draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupKind {
    #[default]
    Bar,
    Line,
    Area,
    Pie,
    Doughnut,
    /// A pie of pie or a bar of pie.
    OfPie,
    Scatter,
    Bubble,
    Radar,
    Stock,
    Surface,
}

impl GroupKind {
    /// The chart part's element for this kind, without its 3D suffix.
    pub fn element(self, three_d: bool) -> &'static str {
        match (self, three_d) {
            (GroupKind::Bar, false) => "barChart",
            (GroupKind::Bar, true) => "bar3DChart",
            (GroupKind::Line, false) => "lineChart",
            (GroupKind::Line, true) => "line3DChart",
            (GroupKind::Area, false) => "areaChart",
            (GroupKind::Area, true) => "area3DChart",
            (GroupKind::Pie, false) => "pieChart",
            (GroupKind::Pie, true) => "pie3DChart",
            (GroupKind::Doughnut, _) => "doughnutChart",
            (GroupKind::OfPie, _) => "ofPieChart",
            (GroupKind::Scatter, _) => "scatterChart",
            (GroupKind::Bubble, _) => "bubbleChart",
            (GroupKind::Radar, _) => "radarChart",
            (GroupKind::Stock, _) => "stockChart",
            (GroupKind::Surface, false) => "surfaceChart",
            (GroupKind::Surface, true) => "surface3DChart",
        }
    }

    /// The kind and 3D flag an element name stands for.
    pub fn from_element(name: &str) -> Option<(Self, bool)> {
        Some(match name {
            "barChart" => (GroupKind::Bar, false),
            "bar3DChart" => (GroupKind::Bar, true),
            "lineChart" => (GroupKind::Line, false),
            "line3DChart" => (GroupKind::Line, true),
            "areaChart" => (GroupKind::Area, false),
            "area3DChart" => (GroupKind::Area, true),
            "pieChart" => (GroupKind::Pie, false),
            "pie3DChart" => (GroupKind::Pie, true),
            "doughnutChart" => (GroupKind::Doughnut, false),
            "ofPieChart" => (GroupKind::OfPie, false),
            "scatterChart" => (GroupKind::Scatter, false),
            "bubbleChart" => (GroupKind::Bubble, false),
            "radarChart" => (GroupKind::Radar, false),
            "stockChart" => (GroupKind::Stock, false),
            "surfaceChart" => (GroupKind::Surface, false),
            "surface3DChart" => (GroupKind::Surface, true),
            _ => return None,
        })
    }

    /// Whether the kind is drawn against a category and a value axis or two value axes.
    pub fn has_axes(self) -> bool {
        !matches!(self, GroupKind::Pie | GroupKind::Doughnut | GroupKind::OfPie)
    }
}

/// How the series of a group are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Grouping {
    /// Line and area charts: each series on its own.
    #[default]
    Standard,
    /// Bar charts: side by side.
    Clustered,
    Stacked,
    PercentStacked,
}

impl Grouping {
    pub fn xml(self) -> &'static str {
        match self {
            Grouping::Standard => "standard",
            Grouping::Clustered => "clustered",
            Grouping::Stacked => "stacked",
            Grouping::PercentStacked => "percentStacked",
        }
    }

    pub fn from_xml(text: &str) -> Self {
        match text {
            "clustered" => Grouping::Clustered,
            "stacked" => Grouping::Stacked,
            "percentStacked" => Grouping::PercentStacked,
            _ => Grouping::Standard,
        }
    }
}

/// Whether the second chart of a pie of pie is a pie or a bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OfPieKind {
    #[default]
    Pie,
    Bar,
}

/// How a pie of pie decides which slices go in the second chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitBy {
    #[default]
    Auto,
    Position,
    Value,
    Percent,
    Custom,
}

/// One chart group of the plot area: a set of series drawn the same way against the same axes.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub kind: GroupKind,
    pub three_d: bool,
    /// Bars run horizontally.
    pub horizontal: bool,
    pub grouping: Grouping,
    pub vary_colors: Option<bool>,
    pub gap_width: Option<u32>,
    pub overlap: Option<i32>,
    pub gap_depth: Option<u32>,
    /// A 3D bar's shape: `box`, `cylinder`, `cone`, `coneToMax`, `pyramid`, `pyramidToMax`.
    pub shape: Option<String>,
    pub series_lines: bool,
    pub drop_lines: bool,
    pub hi_low_lines: bool,
    /// Up and down bars of a line or stock group, and their gap width.
    pub up_down_bars: Option<u32>,
    /// A line group: whether its markers are shown.
    pub marker: Option<bool>,
    pub smooth: Option<bool>,
    /// A scatter group's style: `lineMarker`, `line`, `marker`, `smooth`, `smoothMarker`, `none`.
    pub scatter_style: Option<String>,
    /// A radar group's style: `standard`, `marker` or `filled`.
    pub radar_style: Option<String>,
    pub wireframe: bool,
    pub bubble_scale: Option<u32>,
    pub show_negative_bubbles: Option<bool>,
    /// `area` or `w`.
    pub size_represents: Option<String>,
    pub bubble_3d: bool,
    pub of_pie: OfPieKind,
    pub split_by: SplitBy,
    pub split_at: Option<f64>,
    pub second_size: Option<u32>,
    /// Custom split points, kept as the element.
    pub custom_split: Option<String>,
    pub first_slice_angle: Option<u32>,
    pub hole_size: Option<u32>,
    pub labels: Option<DataLabels>,
    /// Band formats of a surface group, kept.
    pub band_formats: Option<String>,
    /// The ids of the axes the group is drawn against.
    pub axes: Vec<u32>,
    pub ext: Option<String>,
}

impl Default for Group {
    fn default() -> Self {
        Self {
            kind: GroupKind::Bar,
            three_d: false,
            horizontal: false,
            grouping: Grouping::Clustered,
            vary_colors: None,
            gap_width: None,
            overlap: None,
            gap_depth: None,
            shape: None,
            series_lines: false,
            drop_lines: false,
            hi_low_lines: false,
            up_down_bars: None,
            marker: None,
            smooth: None,
            scatter_style: None,
            radar_style: None,
            wireframe: false,
            bubble_scale: None,
            show_negative_bubbles: None,
            size_represents: None,
            bubble_3d: false,
            of_pie: OfPieKind::Pie,
            split_by: SplitBy::Auto,
            split_at: None,
            second_size: None,
            custom_split: None,
            first_slice_angle: None,
            hole_size: None,
            labels: None,
            band_formats: None,
            axes: Vec::new(),
            ext: None,
        }
    }
}

/// Where a data label sits relative to its point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelPosition {
    Center,
    InsideEnd,
    InsideBase,
    OutsideEnd,
    Left,
    Right,
    Above,
    Below,
    BestFit,
}

impl LabelPosition {
    pub fn xml(self) -> &'static str {
        match self {
            LabelPosition::Center => "ctr",
            LabelPosition::InsideEnd => "inEnd",
            LabelPosition::InsideBase => "inBase",
            LabelPosition::OutsideEnd => "outEnd",
            LabelPosition::Left => "l",
            LabelPosition::Right => "r",
            LabelPosition::Above => "t",
            LabelPosition::Below => "b",
            LabelPosition::BestFit => "bestFit",
        }
    }

    pub fn from_xml(text: &str) -> Option<Self> {
        Some(match text {
            "ctr" => LabelPosition::Center,
            "inEnd" => LabelPosition::InsideEnd,
            "inBase" => LabelPosition::InsideBase,
            "outEnd" => LabelPosition::OutsideEnd,
            "l" => LabelPosition::Left,
            "r" => LabelPosition::Right,
            "t" => LabelPosition::Above,
            "b" => LabelPosition::Below,
            "bestFit" => LabelPosition::BestFit,
            _ => return None,
        })
    }
}

/// What a data label shows and how it looks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LabelFields {
    /// A number format and whether it follows the source cells.
    pub num_fmt: Option<(String, bool)>,
    pub sp_pr: Option<String>,
    pub tx_pr: Option<String>,
    pub position: Option<LabelPosition>,
    pub show_legend_key: bool,
    pub show_value: bool,
    pub show_category: bool,
    pub show_series: bool,
    pub show_percent: bool,
    pub show_bubble_size: bool,
    pub separator: Option<String>,
}

impl LabelFields {
    /// Whether any part of a label would be drawn.
    pub fn shows_anything(&self) -> bool {
        self.show_value
            || self.show_category
            || self.show_series
            || self.show_percent
            || self.show_bubble_size
            || self.show_legend_key
    }
}

/// A label set on one point, over the series' own.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PointLabel {
    pub index: u32,
    pub delete: bool,
    pub layout: Option<String>,
    /// Text written in place of the computed label, kept as its `c:tx` element.
    pub text: Option<String>,
    pub fields: LabelFields,
    pub ext: Option<String>,
}

/// The data labels of a series or a group.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataLabels {
    pub points: Vec<PointLabel>,
    pub delete: bool,
    pub fields: LabelFields,
    pub show_leader_lines: Option<bool>,
    pub leader_lines: Option<String>,
    pub ext: Option<String>,
}

/// A marker on a line, scatter or radar point.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Marker {
    /// `circle`, `dash`, `diamond`, `dot`, `none`, `picture`, `plus`, `square`, `star`, `triangle`, `x`, `auto`.
    pub symbol: Option<String>,
    pub size: Option<u32>,
    pub sp_pr: Option<String>,
}

/// The format of one point of a series.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PointFormat {
    pub index: u32,
    pub invert_if_negative: Option<bool>,
    pub marker: Option<Marker>,
    pub bubble_3d: Option<bool>,
    pub explosion: Option<u32>,
    pub sp_pr: Option<String>,
    pub ext: Option<String>,
}

/// A fitted line through a series.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrendKind {
    Linear,
    Exponential,
    Logarithmic,
    /// Polynomial of an order from 2 to 6.
    Polynomial(u32),
    Power,
    /// Moving average over a period of 2 or more.
    MovingAverage(u32),
}

impl TrendKind {
    pub fn xml(self) -> &'static str {
        match self {
            TrendKind::Linear => "linear",
            TrendKind::Exponential => "exp",
            TrendKind::Logarithmic => "log",
            TrendKind::Polynomial(_) => "poly",
            TrendKind::Power => "power",
            TrendKind::MovingAverage(_) => "movingAvg",
        }
    }
}

/// A trendline and what it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Trendline {
    pub name: Option<String>,
    pub kind: TrendKind,
    /// Periods the line is extended forward and backward.
    pub forward: f64,
    pub backward: f64,
    /// A value the line is forced to pass through on the vertical axis.
    pub intercept: Option<f64>,
    pub show_r_squared: bool,
    pub show_equation: bool,
    pub sp_pr: Option<String>,
    /// The label's layout, text, number format and formats, kept.
    pub label: Option<String>,
}

impl Default for Trendline {
    fn default() -> Self {
        Self {
            name: None,
            kind: TrendKind::Linear,
            forward: 0.0,
            backward: 0.0,
            intercept: None,
            show_r_squared: false,
            show_equation: false,
            sp_pr: None,
            label: None,
        }
    }
}

/// Which way an error bar runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorDirection {
    X,
    Y,
}

/// Which sides of the point an error bar covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorSides {
    #[default]
    Both,
    Plus,
    Minus,
}

/// How the size of an error bar is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorAmount {
    #[default]
    Fixed,
    Percentage,
    StandardDeviation,
    StandardError,
    Custom,
}

/// Where custom error amounts come from.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorSource {
    /// A range formula.
    Range(String),
    /// Numbers written into the chart.
    Numbers(Vec<f64>),
}

/// Error bars on a series.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorBars {
    pub direction: Option<ErrorDirection>,
    pub sides: ErrorSides,
    pub amount: ErrorAmount,
    pub no_cap: bool,
    pub value: Option<f64>,
    pub plus: Option<ErrorSource>,
    pub minus: Option<ErrorSource>,
    pub sp_pr: Option<String>,
}

impl Default for ErrorBars {
    fn default() -> Self {
        Self {
            direction: None,
            sides: ErrorSides::Both,
            amount: ErrorAmount::Fixed,
            no_cap: false,
            value: Some(1.0),
            plus: None,
            minus: None,
            sp_pr: None,
        }
    }
}

/// Everything about one series other than its three formulas.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeriesFormat {
    pub sp_pr: Option<String>,
    pub marker: Option<Marker>,
    pub points: Vec<PointFormat>,
    pub labels: Option<DataLabels>,
    pub trendlines: Vec<Trendline>,
    pub error_bars: Vec<ErrorBars>,
    pub smooth: Option<bool>,
    pub explosion: Option<u32>,
    pub invert_if_negative: Option<bool>,
    pub bubble_3d: Option<bool>,
    pub shape: Option<String>,
    pub picture_options: Option<String>,
    /// The series is filtered out of the chart (`c15:filteredSeries`).
    pub filtered: bool,
    /// Category positions filtered out of the series.
    pub filtered_categories: Vec<u32>,
    pub ext: Option<String>,
    /// The `c:idx` and `c:order` the series had.
    pub idx: Option<u32>,
    pub order: Option<u32>,
}

/// Which kind of axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AxisKind {
    #[default]
    Category,
    Value,
    Date,
    Series,
}

/// Where an axis sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AxisPosition {
    #[default]
    Bottom,
    Left,
    Right,
    Top,
}

impl AxisPosition {
    pub fn xml(self) -> &'static str {
        match self {
            AxisPosition::Bottom => "b",
            AxisPosition::Left => "l",
            AxisPosition::Right => "r",
            AxisPosition::Top => "t",
        }
    }

    pub fn from_xml(text: &str) -> Self {
        match text {
            "l" => AxisPosition::Left,
            "r" => AxisPosition::Right,
            "t" => AxisPosition::Top,
            _ => AxisPosition::Bottom,
        }
    }
}

/// Tick marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TickMark {
    None,
    Inside,
    #[default]
    Outside,
    Cross,
}

impl TickMark {
    pub fn xml(self) -> &'static str {
        match self {
            TickMark::None => "none",
            TickMark::Inside => "in",
            TickMark::Outside => "out",
            TickMark::Cross => "cross",
        }
    }

    pub fn from_xml(text: &str) -> Self {
        match text {
            "none" => TickMark::None,
            "in" => TickMark::Inside,
            "cross" => TickMark::Cross,
            _ => TickMark::Outside,
        }
    }
}

/// Where an axis' tick labels sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TickLabels {
    #[default]
    NextTo,
    Low,
    High,
    Hidden,
}

impl TickLabels {
    pub fn xml(self) -> &'static str {
        match self {
            TickLabels::NextTo => "nextTo",
            TickLabels::Low => "low",
            TickLabels::High => "high",
            TickLabels::Hidden => "none",
        }
    }

    pub fn from_xml(text: &str) -> Self {
        match text {
            "low" => TickLabels::Low,
            "high" => TickLabels::High,
            "none" => TickLabels::Hidden,
            _ => TickLabels::NextTo,
        }
    }
}

/// Where the other axis crosses this one.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Crosses {
    #[default]
    Automatic,
    Minimum,
    Maximum,
    At(f64),
}

/// A date axis' base unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Days,
    Months,
    Years,
}

impl TimeUnit {
    pub fn xml(self) -> &'static str {
        match self {
            TimeUnit::Days => "days",
            TimeUnit::Months => "months",
            TimeUnit::Years => "years",
        }
    }

    pub fn from_xml(text: &str) -> Option<Self> {
        Some(match text {
            "days" => TimeUnit::Days,
            "months" => TimeUnit::Months,
            "years" => TimeUnit::Years,
            _ => return None,
        })
    }
}

/// Gridlines of an axis.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gridlines {
    pub sp_pr: Option<String>,
}

/// The formatting of a title: everything but its words.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TitleFormat {
    /// The title's `c:tx` element as read, kept so rich text survives an edit that leaves the words alone.
    pub rich: Option<String>,
    pub layout: Option<String>,
    pub overlay: bool,
    pub sp_pr: Option<String>,
    pub tx_pr: Option<String>,
}

/// An axis title: its words (or `=formula`) and format.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AxisTitle {
    pub text: String,
    pub format: TitleFormat,
}

/// An axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    pub id: u32,
    pub kind: AxisKind,
    pub position: AxisPosition,
    pub deleted: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub log_base: Option<f64>,
    /// Values run from the far end: categories in reverse order.
    pub reverse: bool,
    pub major_gridlines: Option<Gridlines>,
    pub minor_gridlines: Option<Gridlines>,
    pub title: Option<AxisTitle>,
    pub num_fmt: Option<(String, bool)>,
    pub major_tick: TickMark,
    pub minor_tick: TickMark,
    pub tick_labels: TickLabels,
    pub sp_pr: Option<String>,
    pub tx_pr: Option<String>,
    /// The id of the axis this one crosses.
    pub cross_axis: u32,
    pub crosses: Crosses,
    /// Whether the value axis crosses between categories (`between`) or at them (`midCat`).
    pub between: Option<bool>,
    pub major_unit: Option<f64>,
    pub minor_unit: Option<f64>,
    pub display_units: Option<String>,
    // A category axis.
    pub auto: Option<bool>,
    pub label_align: Option<String>,
    pub label_offset: Option<u32>,
    pub label_skip: Option<u32>,
    pub mark_skip: Option<u32>,
    pub no_multi_level: Option<bool>,
    // A date axis.
    pub base_unit: Option<TimeUnit>,
    pub major_time_unit: Option<TimeUnit>,
    pub minor_time_unit: Option<TimeUnit>,
    pub ext: Option<String>,
}

impl Default for Axis {
    fn default() -> Self {
        Self {
            id: 0,
            kind: AxisKind::Category,
            position: AxisPosition::Bottom,
            deleted: false,
            min: None,
            max: None,
            log_base: None,
            reverse: false,
            major_gridlines: None,
            minor_gridlines: None,
            title: None,
            num_fmt: None,
            major_tick: TickMark::Outside,
            minor_tick: TickMark::None,
            tick_labels: TickLabels::NextTo,
            sp_pr: None,
            tx_pr: None,
            cross_axis: 0,
            crosses: Crosses::Automatic,
            between: None,
            major_unit: None,
            minor_unit: None,
            display_units: None,
            auto: None,
            label_align: None,
            label_offset: None,
            label_skip: None,
            mark_skip: None,
            no_multi_level: None,
            base_unit: None,
            major_time_unit: None,
            minor_time_unit: None,
            ext: None,
        }
    }
}

/// Where the legend sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LegendPosition {
    Bottom,
    TopRight,
    Left,
    #[default]
    Right,
    Top,
}

impl LegendPosition {
    pub fn xml(self) -> &'static str {
        match self {
            LegendPosition::Bottom => "b",
            LegendPosition::TopRight => "tr",
            LegendPosition::Left => "l",
            LegendPosition::Right => "r",
            LegendPosition::Top => "t",
        }
    }

    pub fn from_xml(text: &str) -> Self {
        match text {
            "b" => LegendPosition::Bottom,
            "tr" => LegendPosition::TopRight,
            "l" => LegendPosition::Left,
            "t" => LegendPosition::Top,
            _ => LegendPosition::Right,
        }
    }
}

/// The legend.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Legend {
    pub position: LegendPosition,
    pub overlay: bool,
    /// Entries removed from the legend, by index.
    pub deleted_entries: Vec<u32>,
    pub layout: Option<String>,
    pub sp_pr: Option<String>,
    pub tx_pr: Option<String>,
}

/// The data table under the chart.
#[derive(Debug, Clone, PartialEq)]
pub struct DataTable {
    pub horizontal_borders: bool,
    pub vertical_borders: bool,
    pub outline: bool,
    pub legend_keys: bool,
    pub sp_pr: Option<String>,
    pub tx_pr: Option<String>,
}

impl Default for DataTable {
    fn default() -> Self {
        Self {
            horizontal_borders: true,
            vertical_borders: true,
            outline: true,
            legend_keys: true,
            sp_pr: None,
            tx_pr: None,
        }
    }
}

/// Parts of a chart the model does not name, as the XML they were read as.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChartRaw {
    pub date1904: Option<String>,
    pub lang: Option<String>,
    pub clr_map_ovr: Option<String>,
    pub pivot_source: Option<String>,
    pub protection: Option<String>,
    pub pivot_formats: Option<String>,
    pub view_3d: Option<String>,
    pub floor: Option<String>,
    pub side_wall: Option<String>,
    pub back_wall: Option<String>,
    pub plot_layout: Option<String>,
    pub plot_sp_pr: Option<String>,
    pub chart_ext: Option<String>,
    pub sp_pr: Option<String>,
    pub tx_pr: Option<String>,
    pub external_data: Option<String>,
    pub print_settings: Option<String>,
    pub user_shapes: Option<String>,
    pub ext: Option<String>,
    /// The namespaces declared on the root element, written back with it.
    pub namespaces: Vec<(String, String)>,
    /// `mc:Ignorable` on the root.
    pub ignorable: Option<String>,
}

/// What a chart is, as Excel's Insert Chart dialog names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartType {
    ColumnClustered,
    ColumnStacked,
    ColumnStacked100,
    Column3DClustered,
    Column3DStacked,
    Column3DStacked100,
    Column3D,
    BarClustered,
    BarStacked,
    BarStacked100,
    Bar3DClustered,
    Bar3DStacked,
    Bar3DStacked100,
    Line,
    LineStacked,
    LineStacked100,
    LineMarkers,
    LineMarkersStacked,
    LineMarkersStacked100,
    Line3D,
    Pie,
    PieExploded,
    Pie3D,
    Pie3DExploded,
    PieOfPie,
    BarOfPie,
    Doughnut,
    DoughnutExploded,
    Area,
    AreaStacked,
    AreaStacked100,
    Area3D,
    Area3DStacked,
    Area3DStacked100,
    ScatterMarkers,
    ScatterSmooth,
    ScatterSmoothNoMarkers,
    ScatterLines,
    ScatterLinesNoMarkers,
    Bubble,
    Bubble3D,
    StockHlc,
    StockOhlc,
    StockVhlc,
    StockVohlc,
    Surface,
    SurfaceWireframe,
    SurfaceTopView,
    SurfaceTopViewWireframe,
    Radar,
    RadarMarkers,
    RadarFilled,
    Treemap,
    Sunburst,
    Histogram,
    Pareto,
    BoxWhisker,
    Waterfall,
    Funnel,
    Map,
    /// More than one kind of group in one chart.
    Combo,
}

impl ChartType {
    /// Every type Excel's Insert Chart dialog offers, in its order.
    pub const ALL: &'static [ChartType] = &[
        ChartType::ColumnClustered,
        ChartType::ColumnStacked,
        ChartType::ColumnStacked100,
        ChartType::Column3DClustered,
        ChartType::Column3DStacked,
        ChartType::Column3DStacked100,
        ChartType::Column3D,
        ChartType::BarClustered,
        ChartType::BarStacked,
        ChartType::BarStacked100,
        ChartType::Bar3DClustered,
        ChartType::Bar3DStacked,
        ChartType::Bar3DStacked100,
        ChartType::Line,
        ChartType::LineStacked,
        ChartType::LineStacked100,
        ChartType::LineMarkers,
        ChartType::LineMarkersStacked,
        ChartType::LineMarkersStacked100,
        ChartType::Line3D,
        ChartType::Pie,
        ChartType::PieExploded,
        ChartType::Pie3D,
        ChartType::Pie3DExploded,
        ChartType::PieOfPie,
        ChartType::BarOfPie,
        ChartType::Doughnut,
        ChartType::DoughnutExploded,
        ChartType::Area,
        ChartType::AreaStacked,
        ChartType::AreaStacked100,
        ChartType::Area3D,
        ChartType::Area3DStacked,
        ChartType::Area3DStacked100,
        ChartType::ScatterMarkers,
        ChartType::ScatterSmooth,
        ChartType::ScatterSmoothNoMarkers,
        ChartType::ScatterLines,
        ChartType::ScatterLinesNoMarkers,
        ChartType::Bubble,
        ChartType::Bubble3D,
        ChartType::StockHlc,
        ChartType::StockOhlc,
        ChartType::StockVhlc,
        ChartType::StockVohlc,
        ChartType::Surface,
        ChartType::SurfaceWireframe,
        ChartType::SurfaceTopView,
        ChartType::SurfaceTopViewWireframe,
        ChartType::Radar,
        ChartType::RadarMarkers,
        ChartType::RadarFilled,
        ChartType::Treemap,
        ChartType::Sunburst,
        ChartType::Histogram,
        ChartType::Pareto,
        ChartType::BoxWhisker,
        ChartType::Waterfall,
        ChartType::Funnel,
        ChartType::Map,
    ];

    /// The name a command word uses: lower case words joined by dashes.
    pub fn name(self) -> &'static str {
        match self {
            ChartType::ColumnClustered => "column",
            ChartType::ColumnStacked => "column-stacked",
            ChartType::ColumnStacked100 => "column-stacked-100",
            ChartType::Column3DClustered => "column-3d-clustered",
            ChartType::Column3DStacked => "column-3d-stacked",
            ChartType::Column3DStacked100 => "column-3d-stacked-100",
            ChartType::Column3D => "column-3d",
            ChartType::BarClustered => "bar",
            ChartType::BarStacked => "bar-stacked",
            ChartType::BarStacked100 => "bar-stacked-100",
            ChartType::Bar3DClustered => "bar-3d-clustered",
            ChartType::Bar3DStacked => "bar-3d-stacked",
            ChartType::Bar3DStacked100 => "bar-3d-stacked-100",
            ChartType::Line => "line",
            ChartType::LineStacked => "line-stacked",
            ChartType::LineStacked100 => "line-stacked-100",
            ChartType::LineMarkers => "line-markers",
            ChartType::LineMarkersStacked => "line-markers-stacked",
            ChartType::LineMarkersStacked100 => "line-markers-stacked-100",
            ChartType::Line3D => "line-3d",
            ChartType::Pie => "pie",
            ChartType::PieExploded => "pie-exploded",
            ChartType::Pie3D => "pie-3d",
            ChartType::Pie3DExploded => "pie-3d-exploded",
            ChartType::PieOfPie => "pie-of-pie",
            ChartType::BarOfPie => "bar-of-pie",
            ChartType::Doughnut => "doughnut",
            ChartType::DoughnutExploded => "doughnut-exploded",
            ChartType::Area => "area",
            ChartType::AreaStacked => "area-stacked",
            ChartType::AreaStacked100 => "area-stacked-100",
            ChartType::Area3D => "area-3d",
            ChartType::Area3DStacked => "area-3d-stacked",
            ChartType::Area3DStacked100 => "area-3d-stacked-100",
            ChartType::ScatterMarkers => "scatter",
            ChartType::ScatterSmooth => "scatter-smooth",
            ChartType::ScatterSmoothNoMarkers => "scatter-smooth-no-markers",
            ChartType::ScatterLines => "scatter-lines",
            ChartType::ScatterLinesNoMarkers => "scatter-lines-no-markers",
            ChartType::Bubble => "bubble",
            ChartType::Bubble3D => "bubble-3d",
            ChartType::StockHlc => "stock-hlc",
            ChartType::StockOhlc => "stock-ohlc",
            ChartType::StockVhlc => "stock-vhlc",
            ChartType::StockVohlc => "stock-vohlc",
            ChartType::Surface => "surface",
            ChartType::SurfaceWireframe => "surface-wireframe",
            ChartType::SurfaceTopView => "contour",
            ChartType::SurfaceTopViewWireframe => "contour-wireframe",
            ChartType::Radar => "radar",
            ChartType::RadarMarkers => "radar-markers",
            ChartType::RadarFilled => "radar-filled",
            ChartType::Treemap => "treemap",
            ChartType::Sunburst => "sunburst",
            ChartType::Histogram => "histogram",
            ChartType::Pareto => "pareto",
            ChartType::BoxWhisker => "box-whisker",
            ChartType::Waterfall => "waterfall",
            ChartType::Funnel => "funnel",
            ChartType::Map => "map",
            ChartType::Combo => "combo",
        }
    }

    /// The type a command word names.
    pub fn parse(word: &str) -> Option<Self> {
        let word = word.trim().to_ascii_lowercase().replace([' ', '_'], "-");
        Self::ALL.iter().copied().chain([ChartType::Combo]).find(|t| t.name() == word)
    }

    /// Excel's `XlChartType` number for the type.
    pub fn excel_number(self) -> i32 {
        match self {
            ChartType::ColumnClustered => 51,
            ChartType::ColumnStacked => 52,
            ChartType::ColumnStacked100 => 53,
            ChartType::Column3DClustered => 54,
            ChartType::Column3DStacked => 55,
            ChartType::Column3DStacked100 => 56,
            ChartType::Column3D => -4100,
            ChartType::BarClustered => 57,
            ChartType::BarStacked => 58,
            ChartType::BarStacked100 => 59,
            ChartType::Bar3DClustered => 60,
            ChartType::Bar3DStacked => 61,
            ChartType::Bar3DStacked100 => 62,
            ChartType::Line => 4,
            ChartType::LineStacked => 63,
            ChartType::LineStacked100 => 64,
            ChartType::LineMarkers => 65,
            ChartType::LineMarkersStacked => 66,
            ChartType::LineMarkersStacked100 => 67,
            ChartType::Line3D => -4101,
            ChartType::Pie => 5,
            ChartType::PieExploded => 69,
            ChartType::Pie3D => -4102,
            ChartType::Pie3DExploded => 70,
            ChartType::PieOfPie => 68,
            ChartType::BarOfPie => 71,
            ChartType::Doughnut => -4120,
            ChartType::DoughnutExploded => 80,
            ChartType::Area => 1,
            ChartType::AreaStacked => 76,
            ChartType::AreaStacked100 => 77,
            ChartType::Area3D => -4098,
            ChartType::Area3DStacked => 78,
            ChartType::Area3DStacked100 => 79,
            ChartType::ScatterMarkers => -4169,
            ChartType::ScatterSmooth => 72,
            ChartType::ScatterSmoothNoMarkers => 73,
            ChartType::ScatterLines => 74,
            ChartType::ScatterLinesNoMarkers => 75,
            ChartType::Bubble => 15,
            ChartType::Bubble3D => 87,
            ChartType::StockHlc => 88,
            ChartType::StockOhlc => 89,
            ChartType::StockVhlc => 90,
            ChartType::StockVohlc => 91,
            ChartType::Surface => 83,
            ChartType::SurfaceWireframe => 84,
            ChartType::SurfaceTopView => 85,
            ChartType::SurfaceTopViewWireframe => 86,
            ChartType::Radar => -4151,
            ChartType::RadarMarkers => 81,
            ChartType::RadarFilled => 82,
            ChartType::Treemap => 117,
            ChartType::Sunburst => 120,
            ChartType::Histogram => 118,
            ChartType::Pareto => 122,
            ChartType::BoxWhisker => 121,
            ChartType::Waterfall => 119,
            ChartType::Funnel => 123,
            ChartType::Map => 140,
            ChartType::Combo => -4111,
        }
    }

    /// Whether the type is one of the Office 2016 types held in a `chartEx` part.
    pub fn is_extended(self) -> bool {
        matches!(
            self,
            ChartType::Treemap
                | ChartType::Sunburst
                | ChartType::Histogram
                | ChartType::Pareto
                | ChartType::BoxWhisker
                | ChartType::Waterfall
                | ChartType::Funnel
                | ChartType::Map
        )
    }
}

/// How a `chartEx` chart lays its series out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExLayout {
    #[default]
    Treemap,
    Sunburst,
    /// A histogram or the columns of a Pareto chart.
    ClusteredColumn,
    ParetoLine,
    BoxWhisker,
    Waterfall,
    Funnel,
    RegionMap,
}

impl ExLayout {
    pub fn id(self) -> &'static str {
        match self {
            ExLayout::Treemap => "treemap",
            ExLayout::Sunburst => "sunburst",
            ExLayout::ClusteredColumn => "clusteredColumn",
            ExLayout::ParetoLine => "paretoLine",
            ExLayout::BoxWhisker => "boxWhisker",
            ExLayout::Waterfall => "waterfall",
            ExLayout::Funnel => "funnel",
            ExLayout::RegionMap => "regionMap",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "treemap" => ExLayout::Treemap,
            "sunburst" => ExLayout::Sunburst,
            "clusteredColumn" => ExLayout::ClusteredColumn,
            "paretoLine" => ExLayout::ParetoLine,
            "boxWhisker" => ExLayout::BoxWhisker,
            "waterfall" => ExLayout::Waterfall,
            "funnel" => ExLayout::Funnel,
            "regionMap" => ExLayout::RegionMap,
            _ => return None,
        })
    }
}

/// How a histogram chooses its bins.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BinBy {
    /// Excel's automatic bin width (Scott's rule).
    #[default]
    Automatic,
    Width(f64),
    Count(u32),
    /// Every distinct category is its own bin.
    Category,
}

/// A histogram's bin options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Binning {
    pub by: BinBy,
    /// Values above this go in one overflow bin.
    pub overflow: Option<f64>,
    /// Values below this go in one underflow bin.
    pub underflow: Option<f64>,
    /// Whether the right end of each interval is the closed one.
    pub right_closed: bool,
}

impl Default for Binning {
    fn default() -> Self {
        Self { by: BinBy::Automatic, overflow: None, underflow: None, right_closed: true }
    }
}

/// The statistics options of a box and whisker chart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxOptions {
    /// `exclusive` (the median is left out of the quartile halves) or `inclusive`.
    pub exclusive_median: bool,
    pub mean_line: bool,
    pub mean_marker: bool,
    pub inner_points: bool,
    pub outlier_points: bool,
}

impl Default for BoxOptions {
    fn default() -> Self {
        Self {
            exclusive_median: true,
            mean_line: false,
            mean_marker: true,
            inner_points: false,
            outlier_points: true,
        }
    }
}

/// What a `chartEx` series shows besides its shape.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExSeries {
    pub layout: ExLayout,
    pub unique_id: String,
    pub format_index: u32,
    pub hidden: bool,
    /// Index into [`crate::model::Chart::series`] of the series' data.
    pub data: usize,
    pub binning: Option<Binning>,
    pub subtotals: Vec<u32>,
    pub box_options: Option<BoxOptions>,
    /// A treemap or sunburst parent label layout: `banner`, `overlapping` or `none`.
    pub parent_labels: Option<String>,
    /// A Pareto series' aggregation is by category.
    pub aggregation: bool,
    pub geography: Option<String>,
    /// What the data labels show: series name, category name and value.
    pub labels: Option<(bool, bool, bool)>,
    pub label_position: Option<String>,
    pub label_num_fmt: Option<String>,
    pub sp_pr: Option<String>,
    pub tx_pr: Option<String>,
    pub points: Vec<(u32, String)>,
    pub ext: Option<String>,
}

/// An axis of a `chartEx` chart.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExAxis {
    pub id: u32,
    pub value_axis: bool,
    pub gap_width: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub major_unit: Option<f64>,
    pub minor_unit: Option<f64>,
    pub hidden: bool,
    pub title: Option<String>,
    pub major_gridlines: bool,
    pub minor_gridlines: bool,
    pub tick_labels: bool,
    pub num_fmt: Option<String>,
    pub raw: Option<String>,
}

/// What a `chartEx` part says beyond its series' data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExSpec {
    pub series: Vec<ExSeries>,
    pub axes: Vec<ExAxis>,
    /// Title position and alignment as written.
    pub title_pos: Option<String>,
    pub title_align: Option<String>,
    pub legend_pos: Option<String>,
    pub legend_align: Option<String>,
    pub legend_overlay: bool,
    pub legend: bool,
    pub plot_area_extra: Option<String>,
    pub chart_extra: Option<String>,
    pub space_extra: Option<String>,
    /// The formulas of the data were `_xlchart` names, resolved on reading.
    pub used_names: bool,
}

/// The layout of the chart's elements as Excel's quick layouts set them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChartSpec {
    pub groups: Vec<Group>,
    pub axes: Vec<Axis>,
    pub title: TitleFormat,
    /// The automatic title is turned off (`autoTitleDeleted`).
    pub title_deleted: bool,
    /// A title element with no words of its own: the title Excel makes from the series name.
    pub title_auto: bool,
    pub legend: Option<Legend>,
    pub data_table: Option<DataTable>,
    pub plot_visible_only: Option<bool>,
    pub blanks: Option<Blanks>,
    pub labels_over_max: Option<bool>,
    pub style: Option<u32>,
    pub rounded_corners: Option<bool>,
    pub raw: ChartRaw,
    /// The object's name in the drawing, and its description.
    pub name: Option<String>,
    pub alt_text: Option<String>,
    /// The chart has been changed since it was read, so its part is written from the model.
    pub dirty: bool,
    /// The chart's anchor changed since it was read.
    pub moved: bool,
    /// The anchor element as read: `twoCellAnchor`, `oneCellAnchor` or `absoluteAnchor`, and its `editAs`.
    pub anchor_edit_as: Option<String>,
    /// A `chartEx` chart's details.
    pub ex: Option<ExSpec>,
    /// A chart bound to a pivot table: its `c:pivotSource` text, kept.
    pub pivot: bool,
}

impl ChartSpec {
    /// Whether the plot area clips to visible cells: Excel's default is yes.
    pub fn visible_only(&self) -> bool {
        self.plot_visible_only.unwrap_or(true)
    }

    /// How empty cells are plotted: Excel's default is a gap.
    pub fn blanks_as(&self) -> Blanks {
        self.blanks.unwrap_or_default()
    }

    /// The axis with an id.
    pub fn axis(&self, id: u32) -> Option<&Axis> {
        self.axes.iter().find(|a| a.id == id)
    }

    pub fn axis_mut(&mut self, id: u32) -> Option<&mut Axis> {
        self.axes.iter_mut().find(|a| a.id == id)
    }

    /// The category (or first value) axis and the value axis a group is drawn against.
    pub fn axes_of(&self, group: &Group) -> (Option<&Axis>, Option<&Axis>) {
        let first = group.axes.first().and_then(|id| self.axis(*id));
        let second = group.axes.get(1).and_then(|id| self.axis(*id));
        (first, second)
    }

    /// Whether a group is drawn against the secondary axes: the axes of the first group are the primary ones.
    pub fn is_secondary(&self, group: usize) -> bool {
        match (self.groups.first(), self.groups.get(group)) {
            (Some(first), Some(this)) => {
                group > 0 && !this.axes.is_empty() && this.axes.iter().all(|a| !first.axes.contains(a))
            }
            _ => false,
        }
    }

    /// The type of chart the groups make up.
    pub fn chart_type(&self) -> ChartType {
        if let Some(ex) = &self.ex {
            return ex.chart_type();
        }
        let mut types = self.groups.iter().map(Group::chart_type);
        let Some(first) = types.next() else { return ChartType::ColumnClustered };
        if types.all(|t| t == first) {
            first
        } else {
            ChartType::Combo
        }
    }
}

impl ExSpec {
    /// The type of chart the series' layouts make up.
    pub fn chart_type(&self) -> ChartType {
        let Some(first) = self.series.first() else { return ChartType::Histogram };
        match first.layout {
            ExLayout::Treemap => ChartType::Treemap,
            ExLayout::Sunburst => ChartType::Sunburst,
            ExLayout::BoxWhisker => ChartType::BoxWhisker,
            ExLayout::Waterfall => ChartType::Waterfall,
            ExLayout::Funnel => ChartType::Funnel,
            ExLayout::RegionMap => ChartType::Map,
            ExLayout::ParetoLine => ChartType::Pareto,
            ExLayout::ClusteredColumn => {
                if self.series.iter().any(|s| s.layout == ExLayout::ParetoLine) {
                    ChartType::Pareto
                } else {
                    ChartType::Histogram
                }
            }
        }
    }
}

impl Group {
    /// The type of chart this group alone makes.
    pub fn chart_type(&self) -> ChartType {
        use ChartType as T;
        let grouping = self.grouping;
        let pick = |clustered, stacked, percent| match grouping {
            Grouping::Stacked => stacked,
            Grouping::PercentStacked => percent,
            _ => clustered,
        };
        match (self.kind, self.three_d) {
            (GroupKind::Bar, false) if self.horizontal => {
                pick(T::BarClustered, T::BarStacked, T::BarStacked100)
            }
            (GroupKind::Bar, false) => pick(T::ColumnClustered, T::ColumnStacked, T::ColumnStacked100),
            (GroupKind::Bar, true) if self.horizontal => {
                pick(T::Bar3DClustered, T::Bar3DStacked, T::Bar3DStacked100)
            }
            (GroupKind::Bar, true) => {
                if grouping == Grouping::Standard {
                    T::Column3D
                } else {
                    pick(T::Column3DClustered, T::Column3DStacked, T::Column3DStacked100)
                }
            }
            (GroupKind::Line, true) => T::Line3D,
            (GroupKind::Line, false) => {
                let markers = self.marker != Some(false);
                match (markers, grouping) {
                    (false, Grouping::Stacked) => T::LineStacked,
                    (false, Grouping::PercentStacked) => T::LineStacked100,
                    (false, _) => T::Line,
                    (true, Grouping::Stacked) => T::LineMarkersStacked,
                    (true, Grouping::PercentStacked) => T::LineMarkersStacked100,
                    (true, _) => T::LineMarkers,
                }
            }
            (GroupKind::Stock, _) => T::StockHlc,
            (GroupKind::Area, false) => pick(T::Area, T::AreaStacked, T::AreaStacked100),
            (GroupKind::Area, true) => pick(T::Area3D, T::Area3DStacked, T::Area3DStacked100),
            (GroupKind::Pie, false) => T::Pie,
            (GroupKind::Pie, true) => T::Pie3D,
            (GroupKind::Doughnut, _) => T::Doughnut,
            (GroupKind::OfPie, _) => {
                if self.of_pie == OfPieKind::Bar {
                    T::BarOfPie
                } else {
                    T::PieOfPie
                }
            }
            (GroupKind::Scatter, _) => match self.scatter_style.as_deref() {
                Some("smoothMarker") => T::ScatterSmooth,
                Some("smooth") => T::ScatterSmoothNoMarkers,
                Some("lineMarker") => T::ScatterLines,
                Some("line") => T::ScatterLinesNoMarkers,
                _ => T::ScatterMarkers,
            },
            (GroupKind::Bubble, _) => {
                if self.bubble_3d {
                    T::Bubble3D
                } else {
                    T::Bubble
                }
            }
            (GroupKind::Radar, _) => match self.radar_style.as_deref() {
                Some("filled") => T::RadarFilled,
                Some("marker") => T::RadarMarkers,
                _ => T::Radar,
            },
            (GroupKind::Surface, three_d) => match (three_d, self.wireframe) {
                (true, false) => T::Surface,
                (true, true) => T::SurfaceWireframe,
                (false, false) => T::SurfaceTopView,
                (false, true) => T::SurfaceTopViewWireframe,
            },
        }
    }
}
