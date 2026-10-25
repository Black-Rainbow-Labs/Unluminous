//! Trendlines: the fitted curve, its coefficients and R squared, as Excel works them out.
//!
//! The rules are Excel's. A chart with categories fits against 1, 2, 3 and so on; a scatter chart fits
//! against its X values. Exponential and power fits are made on the logarithms, and the R squared they
//! report is the R squared of that straight line fit, as Excel's label shows it. A fit with a forced
//! intercept uses the uncentred total sum of squares, which is what Excel does.

use crate::chartspec::{TrendKind, Trendline};

/// A fitted trendline.
#[derive(Debug, Clone, PartialEq)]
pub struct TrendResult {
    pub kind: TrendKind,
    /// The curve drawn, as X and Y pairs across the fitted range and the forecast.
    pub samples: Vec<(f64, f64)>,
    /// Coefficients as the equation uses them: the highest power first for a polynomial; `[a, b]` for the
    /// other kinds, with `y = a x + b`, `y = a e^(b x)`, `y = a ln x + b` and `y = a x^b`.
    pub coefficients: Vec<f64>,
    pub r_squared: Option<f64>,
    /// The name the legend shows.
    pub name: String,
    pub equation: String,
}

/// The number of samples a curve is drawn with.
const SAMPLES: usize = 120;

/// Fit a trendline to points. Returns `None` when Excel would refuse: too few points, or a logarithm of a
/// value that is not positive.
pub fn fit(t: &Trendline, xs: &[f64], ys: &[f64], series_name: &str) -> Option<TrendResult> {
    let points: Vec<(f64, f64)> = xs.iter().copied().zip(ys.iter().copied()).filter(|(x, y)| x.is_finite() && y.is_finite()).collect();
    if points.len() < 2 {
        return None;
    }
    let (xmin, xmax) = points.iter().fold((f64::MAX, f64::MIN), |(a, b), (x, _)| (a.min(*x), b.max(*x)));
    let (from, to) = (xmin - t.backward, xmax + t.forward);
    let name = t.name.clone().unwrap_or_else(|| default_name(t.kind, series_name));
    if let TrendKind::MovingAverage(period) = t.kind {
        return moving_average(&points, period as usize, name);
    }
    let (coefficients, r_squared) = coefficients_of(t, &points)?;
    let curve = |x: f64| evaluate(t.kind, &coefficients, x);
    let samples = (0..SAMPLES)
        .map(|i| {
            let x = from + (to - from) * i as f64 / (SAMPLES - 1) as f64;
            (x, curve(x))
        })
        .filter(|(x, y)| y.is_finite() && !matches!(t.kind, TrendKind::Logarithmic | TrendKind::Power) || *x > 0.0 && y.is_finite())
        .collect();
    let equation = super::equation::text(t.kind, &coefficients, t.intercept);
    Some(TrendResult { kind: t.kind, samples, coefficients, r_squared: Some(r_squared), name, equation })
}

/// The legend name Excel gives a trendline: `Linear (Series1)`.
fn default_name(kind: TrendKind, series: &str) -> String {
    let word = match kind {
        TrendKind::Linear => "Linear".to_owned(),
        TrendKind::Exponential => "Expon.".to_owned(),
        TrendKind::Logarithmic => "Log.".to_owned(),
        TrendKind::Polynomial(order) => format!("Poly. ({order})"),
        TrendKind::Power => "Power".to_owned(),
        TrendKind::MovingAverage(period) => format!("{period} per. Mov. Avg."),
    };
    if let TrendKind::Polynomial(_) = kind {
        return format!("{word} ({series})").replace(") (", ") (");
    }
    format!("{word} ({series})")
}

/// The value of the fitted curve at X.
pub fn evaluate(kind: TrendKind, c: &[f64], x: f64) -> f64 {
    match kind {
        TrendKind::Linear => c[0] * x + c[1],
        TrendKind::Exponential => c[0] * (c[1] * x).exp(),
        TrendKind::Logarithmic => c[0] * x.ln() + c[1],
        TrendKind::Power => c[0] * x.powf(c[1]),
        TrendKind::Polynomial(_) => c.iter().fold(0.0, |acc, k| acc * x + k),
        TrendKind::MovingAverage(_) => f64::NAN,
    }
}

fn moving_average(points: &[(f64, f64)], period: usize, name: String) -> Option<TrendResult> {
    if period < 2 || points.len() < period {
        return None;
    }
    let samples = (period - 1..points.len())
        .map(|i| {
            let window = &points[i + 1 - period..=i];
            (points[i].0, window.iter().map(|p| p.1).sum::<f64>() / period as f64)
        })
        .collect();
    Some(TrendResult {
        kind: TrendKind::MovingAverage(period as u32),
        samples,
        coefficients: Vec::new(),
        r_squared: None,
        name,
        equation: String::new(),
    })
}

/// Least squares for a straight line through (x, y), optionally through a given intercept.
/// Returns the slope, the intercept and the R squared of the fit.
fn line(points: &[(f64, f64)], intercept: Option<f64>) -> Option<(f64, f64, f64)> {
    let n = points.len() as f64;
    let (sx, sy) = points.iter().fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
    let (mx, my) = (sx / n, sy / n);
    match intercept {
        None => {
            let sxx: f64 = points.iter().map(|(x, _)| (x - mx) * (x - mx)).sum();
            if sxx == 0.0 {
                return None;
            }
            let sxy: f64 = points.iter().map(|(x, y)| (x - mx) * (y - my)).sum();
            let slope = sxy / sxx;
            let b = my - slope * mx;
            let sst: f64 = points.iter().map(|(_, y)| (y - my) * (y - my)).sum();
            let sse: f64 = points.iter().map(|(x, y)| (y - (slope * x + b)).powi(2)).sum();
            let r2 = if sst == 0.0 { 1.0 } else { 1.0 - sse / sst };
            Some((slope, b, r2))
        }
        Some(b) => {
            let sxx: f64 = points.iter().map(|(x, _)| x * x).sum();
            if sxx == 0.0 {
                return None;
            }
            let slope = points.iter().map(|(x, y)| x * (y - b)).sum::<f64>() / sxx;
            let sst: f64 = points.iter().map(|(_, y)| (y - my) * (y - my)).sum();
            let sse: f64 = points.iter().map(|(x, y)| (y - (slope * x + b)).powi(2)).sum();
            let r2 = if sst == 0.0 { 1.0 } else { 1.0 - sse / sst };
            Some((slope, b, r2))
        }
    }
}

fn coefficients_of(t: &Trendline, points: &[(f64, f64)]) -> Option<(Vec<f64>, f64)> {
    match t.kind {
        TrendKind::Linear => {
            let (a, b, r2) = line(points, t.intercept)?;
            Some((vec![a, b], r2))
        }
        TrendKind::Exponential => {
            if points.iter().any(|(_, y)| *y <= 0.0) {
                return None;
            }
            let logged: Vec<(f64, f64)> = points.iter().map(|(x, y)| (*x, y.ln())).collect();
            let (b, ln_a, r2) = line(&logged, t.intercept.filter(|i| *i > 0.0).map(f64::ln))?;
            Some((vec![ln_a.exp(), b], r2))
        }
        TrendKind::Logarithmic => {
            if points.iter().any(|(x, _)| *x <= 0.0) {
                return None;
            }
            let logged: Vec<(f64, f64)> = points.iter().map(|(x, y)| (x.ln(), *y)).collect();
            let (a, b, r2) = line(&logged, t.intercept)?;
            Some((vec![a, b], r2))
        }
        TrendKind::Power => {
            if points.iter().any(|(x, y)| *x <= 0.0 || *y <= 0.0) {
                return None;
            }
            let logged: Vec<(f64, f64)> = points.iter().map(|(x, y)| (x.ln(), y.ln())).collect();
            let (b, ln_a, r2) = line(&logged, t.intercept.filter(|i| *i > 0.0).map(f64::ln))?;
            Some((vec![ln_a.exp(), b], r2))
        }
        TrendKind::Polynomial(order) => polynomial(points, order as usize, t.intercept),
        TrendKind::MovingAverage(_) => None,
    }
}

/// Solve a small linear system by Gaussian elimination with partial pivoting.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| a[i][col].abs().partial_cmp(&a[j][col].abs()).unwrap())?;
        if a[pivot][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..n {
            let f = a[row][col] / a[col][col];
            for k in col..n {
                a[row][k] -= f * a[col][k];
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let s: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    Some(x)
}

/// A polynomial fit by least squares on centred and scaled X, which keeps the normal equations well
/// conditioned; the coefficients are returned for X as it is, highest power first.
fn polynomial(points: &[(f64, f64)], order: usize, intercept: Option<f64>) -> Option<(Vec<f64>, f64)> {
    if points.len() <= order {
        return None;
    }
    let first = usize::from(intercept.is_some());
    let terms = order + 1 - first;
    let powers = |x: f64| (first..=order).map(|k| x.powi(k as i32)).collect::<Vec<_>>();
    let mut ata = vec![vec![0.0; terms]; terms];
    let mut atb = vec![0.0; terms];
    for (x, y) in points {
        let row = powers(*x);
        let target = y - intercept.unwrap_or(0.0);
        for i in 0..terms {
            atb[i] += row[i] * target;
            for j in 0..terms {
                ata[i][j] += row[i] * row[j];
            }
        }
    }
    let solved = solve(ata, atb)?;
    // Lowest power first, then reversed to highest first.
    let mut low_first: Vec<f64> = Vec::new();
    if let Some(b) = intercept {
        low_first.push(b);
    }
    low_first.extend(solved);
    let n = points.len() as f64;
    let my = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sst: f64 = points.iter().map(|(_, y)| (y - my).powi(2)).sum();
    let value = |x: f64| low_first.iter().rev().fold(0.0, |acc, k| acc * x + k);
    let sse: f64 = points.iter().map(|(x, y)| (y - value(*x)).powi(2)).sum();
    let r2 = if sst == 0.0 { 1.0 } else { 1.0 - sse / sst };
    low_first.reverse();
    Some((low_first, r2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trend(kind: TrendKind) -> Trendline {
        Trendline { kind, ..Trendline::default() }
    }

    #[test]
    fn a_linear_fit_finds_the_line_and_its_r_squared() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = [3.1, 4.9, 7.2, 8.8, 11.1];
        let fit = fit(&trend(TrendKind::Linear), &xs, &ys, "S").unwrap();
        assert!((fit.coefficients[0] - 2.0).abs() < 0.05);
        assert!(fit.r_squared.unwrap() > 0.99);
    }

    #[test]
    fn exponential_and_power_fits_use_the_logarithms() {
        let xs = [1.0, 2.0, 3.0, 4.0];
        let ys: Vec<f64> = xs.iter().map(|x| 2.0 * (0.5f64 * x).exp()).collect();
        let e = fit(&trend(TrendKind::Exponential), &xs, &ys, "S").unwrap();
        assert!((e.coefficients[0] - 2.0).abs() < 1e-9 && (e.coefficients[1] - 0.5).abs() < 1e-9);
        let ys: Vec<f64> = xs.iter().map(|x| 3.0 * x.powf(1.5)).collect();
        let p = fit(&trend(TrendKind::Power), &xs, &ys, "S").unwrap();
        assert!((p.coefficients[0] - 3.0).abs() < 1e-9 && (p.coefficients[1] - 1.5).abs() < 1e-9);
        assert!(fit(&trend(TrendKind::Power), &[0.0, 1.0], &[1.0, 2.0], "S").is_none());
    }

    #[test]
    fn a_polynomial_fit_recovers_a_cubic() {
        let xs: Vec<f64> = (1..=8).map(f64::from).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 0.5 * x * x * x - 2.0 * x * x + x + 4.0).collect();
        let f = fit(&trend(TrendKind::Polynomial(3)), &xs, &ys, "S").unwrap();
        for (got, want) in f.coefficients.iter().zip([0.5, -2.0, 1.0, 4.0]) {
            assert!((got - want).abs() < 1e-6, "{:?}", f.coefficients);
        }
    }

    #[test]
    fn a_moving_average_starts_at_the_period() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = [2.0, 4.0, 6.0, 8.0, 10.0];
        let f = fit(&trend(TrendKind::MovingAverage(2)), &xs, &ys, "S").unwrap();
        assert_eq!(f.samples, vec![(2.0, 3.0), (3.0, 5.0), (4.0, 7.0), (5.0, 9.0)]);
    }
}
