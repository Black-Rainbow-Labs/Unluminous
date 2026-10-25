//! The scale Excel chooses for an axis when nobody has set one.
//!
//! `tests/charts.rs` checks this against the minimum, maximum and major unit Excel reports through COM for
//! the data sets in `tests/fixtures/charts/scale-cases.jsonl`.

/// What the person has fixed on an axis; `None` is automatic.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Fixed {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub major: Option<f64>,
    pub minor: Option<f64>,
    pub log_base: Option<f64>,
}

/// A scale as drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisScale {
    pub min: f64,
    pub max: f64,
    pub major: f64,
    pub minor: f64,
    pub log: bool,
}

/// The step Excel takes between gridlines for a range, given how many points of axis there are.
fn nice_step(range: f64, length: f64) -> f64 {
    let wanted = (length / 28.0).clamp(2.0, 12.0);
    let raw = range / wanted;
    let power = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * power)
        .find(|s| *s >= raw * 0.999)
        .unwrap_or(power * 10.0)
}

/// Choose the scale of a value axis from the extremes of its data and the length of the axis in points.
pub fn value_scale(data_min: f64, data_max: f64, fixed: Fixed, length: f64) -> AxisScale {
    if let Some(base) = fixed.log_base {
        return log_scale(data_min, data_max, fixed, base);
    }
    let (mut low, mut high) = (data_min, data_max);
    if low == high {
        if low == 0.0 {
            high = 1.0;
        } else if low > 0.0 {
            low = 0.0;
            high *= 1.2;
        } else {
            high = 0.0;
            low *= 1.2;
        }
    } else if low >= 0.0 && (high - low) / high > 1.0 / 6.0 {
        low = 0.0;
    } else if high <= 0.0 && (low - high) / low > 1.0 / 6.0 {
        high = 0.0;
    }
    let pad = 0.05 * (high - low);
    let (low_pad, high_pad) = (low - if low == 0.0 { 0.0 } else { pad }, high + if high == 0.0 { 0.0 } else { pad });
    let major = fixed.major.unwrap_or_else(|| nice_step(high_pad - low_pad, length));
    let min = fixed.min.unwrap_or_else(|| (low_pad / major).floor() * major);
    let max = fixed.max.unwrap_or_else(|| (high_pad / major).ceil() * major);
    let minor = fixed.minor.unwrap_or(major / 5.0);
    AxisScale { min, max, major, minor, log: false }
}

fn log_scale(data_min: f64, data_max: f64, fixed: Fixed, base: f64) -> AxisScale {
    let low = data_min.max(f64::MIN_POSITIVE);
    let high = data_max.max(low);
    let min = fixed.min.unwrap_or_else(|| base.powf(low.log(base).floor()));
    let max = fixed.max.unwrap_or_else(|| base.powf(high.log(base).ceil()));
    AxisScale { min, max, major: base, minor: base, log: true }
}

/// The values of the gridlines and tick labels of a scale, from the minimum up.
pub fn ticks(scale: &AxisScale) -> Vec<f64> {
    let mut out = Vec::new();
    if scale.log {
        let mut v = scale.min;
        while v <= scale.max * 1.0000001 && out.len() < 400 {
            out.push(v);
            v *= scale.major;
        }
        return out;
    }
    let count = ((scale.max - scale.min) / scale.major).round() as usize;
    for i in 0..=count.min(1000) {
        out.push(scale.min + scale.major * i as f64);
    }
    out
}
