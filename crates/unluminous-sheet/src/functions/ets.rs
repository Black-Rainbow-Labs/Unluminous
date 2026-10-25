//! Exponential smoothing with an additive error, trend and season (ETS AAA), the model behind the
//! FORECAST.ETS functions, and the preparation of a timeline for it.
//!
//! What was measured against Excel 16 and what was not. The model is Excel's: each step the forecast
//! error `e` corrects the level, the trend and the season by their own gains, `level += trend + alpha e`,
//! `trend += beta e`, `season += gamma e`. With the trend started at the least squares slope and the
//! level at the first value less one slope, the errors, the root mean square error and the forecast
//! Excel gives are reproduced to the last digit when it is given the same alpha and beta. Which alpha,
//! beta and gamma Excel picks is not known: it gives values such as 0.002, 0.126 and 0.25 that are not
//! the minimum of any error measure tried, so the parameters here are the ones that minimise the sum of
//! squared errors, searched on a grid of 0.001. A forecast that depends on them differs from Excel's
//! by the difference those parameters make. A series that fits exactly has no parameters to choose,
//! and there the answers agree to the last digit.

use crate::dates;
use crate::value::ErrorKind;

/// The smallest and largest value a smoothing gain may take.
const LOWEST: f64 = 0.001;
const HIGHEST: f64 = 0.999;

/// How the points of a timeline are spaced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spacing {
    /// A fixed distance between neighbours, in the units of the timeline.
    Steps { first: f64, step: f64 },
    /// Calendar months, which are not a fixed number of days: the first month and the months between.
    Months { first: i64, step: i64 },
}

/// A series made ready for smoothing: values at consecutive steps, and how the steps map to the timeline.
#[derive(Debug, Clone)]
pub struct Series {
    pub y: Vec<f64>,
    pub spacing: Spacing,
    /// The smallest distance between two points of the timeline as given, which is what the step size
    /// statistic reports.
    pub step_size: f64,
}

impl Series {
    /// Where a point of the timeline falls, in steps from the first. Months count their days as a
    /// fraction of the month, so the 12th of February is 11 of 28 of the way through the month.
    pub fn position(&self, t: f64, date1904: bool) -> Option<f64> {
        match self.spacing {
            Spacing::Steps { first, step } => Some((t - first) / step),
            Spacing::Months { first, step } => {
                let (year, month, day) = dates::date(t, date1904)?;
                let index = year * 12 + i64::from(month) - 1;
                let fraction = f64::from(day - 1) / f64::from(dates::days_in_month(year, month));
                Some(((index - first) as f64 + fraction) / step as f64)
            }
        }
    }
}

/// How values at one moment of the timeline are made into one.
#[derive(Debug, Clone, Copy)]
pub enum Aggregation {
    Average,
    Count,
    CountAll,
    Max,
    Median,
    Min,
    Sum,
}

impl Aggregation {
    /// The aggregation a number from 1 to 7 names.
    pub fn from_code(code: f64) -> Result<Self, ErrorKind> {
        match code.trunc() as i64 {
            _ if code.fract() != 0.0 => Err(ErrorKind::Num),
            1 => Ok(Self::Average),
            2 => Ok(Self::Count),
            3 => Ok(Self::CountAll),
            4 => Ok(Self::Max),
            5 => Ok(Self::Median),
            6 => Ok(Self::Min),
            7 => Ok(Self::Sum),
            _ => Err(ErrorKind::Num),
        }
    }

    fn apply(self, values: &[f64]) -> f64 {
        let n = values.len() as f64;
        match self {
            Self::Average => values.iter().sum::<f64>() / n,
            Self::Count | Self::CountAll => n,
            Self::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            Self::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
            Self::Sum => values.iter().sum(),
            Self::Median => super::stats::median(values).unwrap_or(0.0),
        }
    }
}

/// Points sorted by time with those at the same moment made into one.
fn merged(points: &[(f64, f64)], how: Aggregation) -> Vec<(f64, f64)> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut group: Vec<f64> = Vec::new();
    for (i, (t, y)) in sorted.iter().enumerate() {
        group.push(*y);
        let last = sorted.get(i + 1).is_none_or(|next| next.0 != *t);
        if last {
            out.push((*t, how.apply(&group)));
            group.clear();
        }
    }
    out
}

/// Whether every distance between points is a whole number of `step`.
fn fits_steps(times: &[f64], step: f64) -> bool {
    times.iter().all(|t| {
        let k = (t - times[0]) / step;
        (k - k.round()).abs() < 1e-6
    })
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a.abs()
    } else {
        gcd(b, a % b)
    }
}

/// Calendar months of dates that all fall on the same day of the month, or all on its last day: the
/// first month and the greatest step in months every distance is a whole number of.
fn month_spacing(times: &[f64], date1904: bool) -> Option<(i64, i64)> {
    let dates: Vec<(i64, u32, u32)> =
        times.iter().map(|t| dates::date(*t, date1904)).collect::<Option<_>>()?;
    let same_day = dates.iter().all(|d| d.2 == dates[0].2);
    let month_ends = dates.iter().all(|&(y, m, d)| d == dates::days_in_month(y, m));
    if !(same_day || month_ends) {
        return None;
    }
    let months: Vec<i64> = dates.iter().map(|&(y, m, _)| y * 12 + i64::from(m) - 1).collect();
    let step = months.windows(2).fold(0, |g, w| gcd(g, w[1] - w[0]));
    (step >= 1).then_some((months[0], step))
}

/// How the sorted, distinct times are spaced and where each falls as a whole number of steps.
fn spacing_of(times: &[f64], date1904: bool) -> Result<(Spacing, Vec<usize>), ErrorKind> {
    let smallest = times.windows(2).map(|w| w[1] - w[0]).fold(f64::INFINITY, f64::min);
    if smallest.is_finite() && smallest > 0.0 && fits_steps(times, smallest) {
        let positions =
            times.iter().map(|t| ((t - times[0]) / smallest).round() as usize).collect();
        return Ok((Spacing::Steps { first: times[0], step: smallest }, positions));
    }
    let (first, step) = month_spacing(times, date1904).ok_or(ErrorKind::Num)?;
    let positions = times
        .iter()
        .map(|t| {
            let (y, m, _) = dates::date(*t, date1904).unwrap_or((0, 1, 1));
            ((y * 12 + i64::from(m) - 1 - first) / step) as usize
        })
        .collect();
    Ok((Spacing::Months { first, step }, positions))
}

/// The values at every step from the first to the last, with a missing step filled by interpolation or,
/// when `interpolate` is off, by zero.
fn filled(points: &[(f64, f64)], positions: &[usize], interpolate: bool) -> Vec<f64> {
    let length = positions.last().map_or(0, |last| last + 1);
    let mut y = vec![0.0; length];
    for (i, &p) in positions.iter().enumerate() {
        y[p] = points[i].1;
        if interpolate && i > 0 && p > positions[i - 1] + 1 {
            let (from, to) = (positions[i - 1], p);
            let (a, b) = (points[i - 1].1, points[i].1);
            for (k, slot) in y.iter_mut().enumerate().take(to).skip(from + 1) {
                *slot = a + (b - a) * (k - from) as f64 / (to - from) as f64;
            }
        }
    }
    y
}

/// Make a series from (time, value) points: sort them, merge those at one time, find the step and fill
/// the steps that have no point.
pub fn prepare(
    points: &[(f64, f64)],
    how: Aggregation,
    interpolate: bool,
    date1904: bool,
) -> Result<Series, ErrorKind> {
    let points = merged(points, how);
    match points.len() {
        0 => return Err(ErrorKind::Num),
        1 => return Err(ErrorKind::Div0),
        _ => {}
    }
    let times: Vec<f64> = points.iter().map(|p| p.0).collect();
    let (spacing, positions) = spacing_of(&times, date1904)?;
    // Months have no one length in days: Excel reports the first (31 for monthly dates from 1 January).
    let step_size = match spacing {
        Spacing::Steps { step, .. } => step,
        Spacing::Months { .. } => times[1] - times[0],
    };
    Ok(Series { y: filled(&points, &positions, interpolate), spacing, step_size })
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// The least squares slope of values against their position.
fn slope(y: &[f64]) -> f64 {
    let (mx, my) = ((y.len() as f64 - 1.0) / 2.0, mean(y));
    let sxy: f64 = y.iter().enumerate().map(|(i, v)| (i as f64 - mx) * (v - my)).sum();
    let sxx: f64 = (0..y.len()).map(|i| (i as f64 - mx).powi(2)).sum();
    if sxx == 0.0 {
        0.0
    } else {
        sxy / sxx
    }
}

/// The first autocorrelation peak in the straight line residuals: the shortest period whose correlation
/// with itself reaches nine tenths of the best, and at least 0.83 once the correlation is scaled up for
/// the shorter overlap (n / (n - p)). A period is only looked for when the series holds three of it,
/// which is as few as Excel recognises (checked at periods 2 to 12, where this agrees with it on all 128
/// series tried). Excel also finds periods in about a fifth of series of plain noise, which nothing
/// here reproduces.
pub fn detect_period(y: &[f64]) -> usize {
    let longest = y.len() / 3;
    if longest < 2 {
        return 0;
    }
    let (mx, my) = ((y.len() as f64 - 1.0) / 2.0, mean(y));
    let b = slope(y);
    let r: Vec<f64> = y.iter().enumerate().map(|(i, v)| v - my - b * (i as f64 - mx)).collect();
    let energy: f64 = r.iter().map(|v| v * v).sum();
    let n = y.len() as f64;
    if energy == 0.0 {
        return 0;
    }
    let correlations: Vec<(usize, f64)> = (2..=longest)
        .map(|p| {
            (p, (p..r.len()).map(|i| r[i] * r[i - p]).sum::<f64>() / energy * n / (n - p as f64))
        })
        .collect();
    let best = correlations.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max);
    if best < 0.83 {
        return 0;
    }
    correlations.iter().find(|c| c.1 >= 0.9 * best).map_or(0, |c| c.0)
}

/// The smoothing gains and the period of a fitted model.
#[derive(Debug, Clone, Copy)]
pub struct Gains {
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
}

/// A model after it has been run over a series.
#[derive(Debug, Clone)]
pub struct Fit {
    pub gains: Gains,
    pub period: usize,
    /// The one step forecast of every point of the series.
    pub fitted: Vec<f64>,
    pub level: f64,
    pub trend: f64,
    /// The seasonal effect for each position in the period, counted from the first point.
    pub season: Vec<f64>,
    pub length: usize,
}

/// The starting level, trend and seasonal effects.
fn start(y: &[f64], period: usize) -> (f64, f64, Vec<f64>) {
    if period < 2 {
        let b = slope(y);
        return (y[0] - b, b, Vec::new());
    }
    let groups: Vec<Vec<(f64, f64)>> = (0..period)
        .map(|g| (g..y.len()).step_by(period).map(|i| (i as f64, y[i])).collect())
        .collect();
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for g in &groups {
        let (mi, my) = (
            mean(&g.iter().map(|p| p.0).collect::<Vec<_>>()),
            mean(&g.iter().map(|p| p.1).collect::<Vec<_>>()),
        );
        sxy += g.iter().map(|p| (p.0 - mi) * (p.1 - my)).sum::<f64>();
        sxx += g.iter().map(|p| (p.0 - mi).powi(2)).sum::<f64>();
    }
    let b = if sxx == 0.0 { 0.0 } else { sxy / sxx };
    let intercepts: Vec<f64> =
        groups.iter().map(|g| mean(&g.iter().map(|p| p.1 - b * p.0).collect::<Vec<_>>())).collect();
    let a = mean(&intercepts);
    (a - b, b, intercepts.iter().map(|c| c - a).collect())
}

/// Run the model over the series with these gains.
fn run(y: &[f64], period: usize, gains: Gains) -> Fit {
    let (mut level, mut trend, mut season) = start(y, period);
    let mut fitted = Vec::with_capacity(y.len());
    for (i, v) in y.iter().enumerate() {
        let seasonal = if period < 2 { 0.0 } else { season[i % period] };
        let forecast = level + trend + seasonal;
        let error = v - forecast;
        fitted.push(forecast);
        level = level + trend + gains.alpha * error;
        trend += gains.beta * error;
        if period >= 2 {
            season[i % period] += gains.gamma * error;
        }
    }
    Fit { gains, period, fitted, level, trend, season, length: y.len() }
}

fn squared_error(y: &[f64], fit: &Fit) -> f64 {
    y.iter().zip(&fit.fitted).map(|(a, b)| (a - b) * (a - b)).sum()
}

/// A gain on the grid of 0.001 inside its limits.
fn on_grid(g: f64) -> f64 {
    ((g * 1000.0).round() / 1000.0).clamp(LOWEST, HIGHEST)
}

/// Pattern search over the gains from one starting point, with steps from 0.2 down to 0.001.
fn descend(y: &[f64], period: usize, from: Gains) -> (f64, Gains) {
    let mut best = (squared_error(y, &run(y, period, from)), from);
    let free = if period >= 2 { 3 } else { 2 };
    for step in [0.2, 0.1, 0.05, 0.02, 0.01, 0.005, 0.002, 0.001] {
        loop {
            let mut moved = false;
            for which in 0..free {
                for sign in [-1.0, 1.0] {
                    let mut g = best.1;
                    match which {
                        0 => g.alpha = on_grid(g.alpha + sign * step),
                        1 => g.beta = on_grid(g.beta + sign * step),
                        _ => g.gamma = on_grid(g.gamma + sign * step),
                    }
                    let error = squared_error(y, &run(y, period, g));
                    if error < best.0 * (1.0 - 1e-12) {
                        best = (error, g);
                        moved = true;
                    }
                }
            }
            if !moved {
                break;
            }
        }
    }
    best
}

/// The model with the gains that make the squared errors smallest. A series the model fits exactly
/// has no best gains, and then Excel's own resting values are kept.
pub fn fit(y: &[f64], period: usize) -> Fit {
    let period = if period >= 2 && y.len() >= 2 * period { period } else { 0 };
    let gamma_resting = if period >= 2 { LOWEST } else { f64::EPSILON };
    let resting = Gains { alpha: 0.9, beta: LOWEST, gamma: gamma_resting };
    let mut best = (squared_error(y, &run(y, period, resting)), resting);
    for alpha in [0.002, 0.1, 0.25, 0.5, 0.75, 0.999] {
        for beta in [0.001, 0.05, 0.3] {
            let from = Gains { alpha, beta, gamma: gamma_resting };
            let found = descend(y, period, from);
            if found.0 < best.0 * (1.0 - 1e-12) {
                best = found;
            }
        }
    }
    run(y, period, best.1)
}

impl Fit {
    /// The forecast `h` steps after the last point, for a whole `h` of at least 1.
    fn ahead(&self, h: usize) -> f64 {
        let seasonal =
            if self.period < 2 { 0.0 } else { self.season[(self.length - 1 + h) % self.period] };
        self.level + h as f64 * self.trend + seasonal
    }

    /// The value at a position counted in steps from the first point: the fitted value inside the
    /// series and the forecast after it, with a position between steps read along the line between them.
    pub fn value_at(&self, position: f64) -> f64 {
        let last = (self.length - 1) as f64;
        let at = |p: f64| -> f64 {
            if p <= last {
                self.fitted[p as usize]
            } else {
                self.ahead((p - last) as usize)
            }
        };
        let (low, high) = (position.floor(), position.ceil());
        if low == high {
            return at(low);
        }
        let share = position - low;
        at(low) * (1.0 - share) + at(high) * share
    }

    /// The width of the confidence interval `h` steps ahead, for a root mean square error `sigma` and
    /// the normal quantile `z`. The variance grows by the square of the weight each past error has on
    /// the forecast: `alpha + j beta`, and `gamma` more at each whole period.
    pub fn interval(&self, h: usize, sigma: f64, z: f64) -> f64 {
        let g = self.gains;
        let growth: f64 = (1..h)
            .map(|j| {
                let season = if self.period >= 2 && j % self.period == 0 { g.gamma } else { 0.0 };
                (g.alpha + j as f64 * g.beta + season).powi(2)
            })
            .sum();
        z * sigma * (1.0 + growth).sqrt()
    }

    /// The errors of the fit against the series, all of them.
    fn errors(&self, y: &[f64]) -> Vec<f64> {
        y.iter().zip(&self.fitted).map(|(a, b)| a - b).collect()
    }

    /// The errors Excel's statistics are made from: from eleven points on, only those of the last 40%
    /// of the points (rounded down), the rest being the training part; with fewer, all of them.
    /// Measured by reproducing its RMSE, MAE, SMAPE and MASE on series of 11 to 24 points.
    fn test_errors(&self, y: &[f64]) -> Vec<f64> {
        let e = self.errors(y);
        e[test_start(y.len())..].to_vec()
    }

    /// The root mean square error over every point, which is what the confidence interval is made from.
    pub fn rmse_all(&self, y: &[f64]) -> f64 {
        let e = self.errors(y);
        (e.iter().map(|v| v * v).sum::<f64>() / e.len() as f64).sqrt()
    }

    pub fn rmse(&self, y: &[f64]) -> f64 {
        let e = self.test_errors(y);
        (e.iter().map(|v| v * v).sum::<f64>() / e.len() as f64).sqrt()
    }

    pub fn mae(&self, y: &[f64]) -> f64 {
        let e = self.test_errors(y);
        e.iter().map(|v| v.abs()).sum::<f64>() / e.len() as f64
    }

    /// The mean absolute error as a share of what carrying the last value forward would have missed by
    /// over the training part (the last season, for a seasonal model).
    pub fn mase(&self, y: &[f64]) -> f64 {
        let lag = self.period.max(1);
        let training = &y[..if y.len() > 10 { test_start(y.len()) } else { y.len() }];
        let steps = training.len().saturating_sub(lag).max(1);
        let naive: f64 =
            (lag..training.len()).map(|i| (training[i] - training[i - lag]).abs()).sum::<f64>()
                / steps as f64;
        if naive == 0.0 {
            0.0
        } else {
            self.mae(y) / naive
        }
    }

    /// The mean of the errors as a share of the average of the value and its forecast.
    pub fn smape(&self, y: &[f64]) -> f64 {
        let from = test_start(y.len());
        let total: f64 = y[from..]
            .iter()
            .zip(&self.fitted[from..])
            .map(|(a, f)| {
                let scale = (a.abs() + f.abs()) / 2.0;
                if scale == 0.0 {
                    0.0
                } else {
                    (a - f).abs() / scale
                }
            })
            .sum();
        total / (y.len() - from) as f64
    }
}

/// Where the part of a series that Excel's statistics are taken over starts.
fn test_start(length: usize) -> usize {
    if length > 10 {
        length - length * 2 / 5
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_line_is_forecast_exactly() {
        let y: Vec<f64> = (1..=6).map(f64::from).collect();
        let fit = fit(&y, 0);
        assert!((fit.value_at(10.0) - 11.0).abs() < 1e-12);
        assert_eq!((fit.gains.alpha, fit.gains.beta), (0.9, 0.001));
    }

    #[test]
    fn excels_model_gives_its_forecast_for_its_own_gains() {
        // Measured: Excel reports alpha 0.002, beta 0.001 for this series and forecasts 15.651857052867411.
        let y = [3.0, 5.0, 4.0, 7.0, 8.0, 8.0, 11.0, 12.0, 12.0, 15.0];
        let gains = Gains { alpha: 0.002, beta: 0.001, gamma: 0.0 };
        let run = run(&y, 0, gains);
        assert!((run.value_at(10.0) - 15.651_857_052_867_411).abs() < 1e-12);
        assert!((run.rmse(&y) - 0.808_562_666_463_050_2).abs() < 1e-12);
    }

    #[test]
    fn the_statistics_follow_excels_for_its_gains_on_a_longer_series() {
        // Measured: for this series of 24 points Excel reports alpha 0.9, beta 0.001 and the statistics below.
        let y = [
            112.0, 118.0, 132.0, 129.0, 121.0, 135.0, 148.0, 148.0, 136.0, 119.0, 104.0, 118.0,
            115.0, 126.0, 141.0, 135.0, 125.0, 149.0, 170.0, 170.0, 158.0, 133.0, 114.0, 140.0,
        ];
        let run = run(&y, 0, Gains { alpha: 0.9, beta: 0.001, gamma: 0.0 });
        assert!((run.mae(&y) - 16.440_205_364_564_743).abs() < 1e-9);
        assert!((run.rmse(&y) - 18.502_643_289_824_206).abs() < 1e-9);
        assert!((run.smape(&y) - 0.117_333_597_033_023_97).abs() < 1e-12);
        assert!((run.mase(&y) - 1.587_330_173_130_389).abs() < 1e-9);
    }

    #[test]
    fn a_period_is_found_when_three_of_it_are_there() {
        let pattern = [10.0, 20.0, 15.0, 30.0, 12.0];
        let y: Vec<f64> = (0..15).map(|i| pattern[i % 5]).collect();
        assert_eq!(detect_period(&y), 5);
        assert_eq!(detect_period(&y[..10]), 0);
    }
}
