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

/// The most major intervals Excel puts on an axis.
const MOST_INTERVALS: f64 = 10.0;

/// The least length in points Excel leaves between two gridlines (measured against Excel at three heights).
const LEAST_SPACING: f64 = 12.75;

/// The range an axis must cover for data between `low` and `high`: zero is included unless the data sit
/// far from it, and the data are padded by a twentieth of their range.
fn extent(low: f64, high: f64) -> (f64, f64) {
    if low == high {
        return match low {
            v if v == 0.0 => (0.0, 1.0),
            v if v > 0.0 => (0.0, v * 1.05),
            v => (v * 1.05, 0.0),
        };
    }
    let range = high - low;
    if low >= 0.0 {
        // Data far above zero are shown from half a range below their minimum.
        if range / high > 1.0 / 6.0 {
            return (0.0, high + 0.05 * high);
        }
        return (low - range / 2.0, high + 0.05 * range);
    }
    if high <= 0.0 {
        if range / -low > 1.0 / 6.0 {
            return (low + 0.05 * low, 0.0);
        }
        return (low - 0.05 * range, high + range / 2.0);
    }
    (low - 0.05 * range, high + 0.05 * range)
}

/// How many intervals of `step` cover `low` to `high`, with the ends moved out to a multiple of `step`.
fn covered(low: f64, high: f64, step: f64) -> (f64, f64, f64) {
    let min = (low / step + 1e-9).floor() * step;
    let max = (high / step - 1e-9).ceil() * step;
    (min, max, ((max - min) / step).round())
}

/// The smallest step of 1, 2 or 5 times a power of ten that leaves few enough intervals, spaced widely enough.
fn nice_step(low: f64, high: f64, length: f64) -> f64 {
    for power in -30..=30 {
        for multiple in [1.0, 2.0, 5.0] {
            let step = multiple * 10f64.powi(power);
            let (_, _, count) = covered(low, high, step);
            if count >= 1.0 && count <= MOST_INTERVALS && length / count >= LEAST_SPACING {
                return step;
            }
        }
    }
    (high - low).max(f64::MIN_POSITIVE)
}

/// Choose the scale of a value axis from the extremes of its data and the length of the axis in points.
pub fn value_scale(data_min: f64, data_max: f64, fixed: Fixed, length: f64) -> AxisScale {
    if let Some(base) = fixed.log_base {
        return log_scale(data_min, data_max, fixed, base);
    }
    let (low, high) = extent(data_min, data_max);
    let major = fixed.major.unwrap_or_else(|| nice_step(low, high, length));
    let (snapped_min, snapped_max, _) = covered(low, high, major);
    let min = fixed.min.unwrap_or(snapped_min);
    let max = fixed.max.unwrap_or(snapped_max);
    let minor = fixed.minor.unwrap_or(major / 5.0);
    AxisScale { min, max, major, minor, log: false }
}

fn log_scale(data_min: f64, data_max: f64, fixed: Fixed, base: f64) -> AxisScale {
    let low = data_min.max(f64::MIN_POSITIVE);
    let high = data_max.max(low);
    // Excel starts a logarithmic axis at 1 when the data lie above it and run over more than three decades
    // (measured on one data set; the others it was asked about start at the decade below the minimum).
    let from_one = low >= 1.0 && high / low >= 1000.0;
    let min =
        fixed.min.unwrap_or_else(|| if from_one { 1.0 } else { base.powf(low.log(base).floor()) });
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
