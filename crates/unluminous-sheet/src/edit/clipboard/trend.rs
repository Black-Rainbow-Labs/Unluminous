//! Least squares fits for the trend fills: a straight line and an exponential curve.

use crate::value::round_significant;

/// A number as a cell keeps it after a fill: to 15 significant digits, which is what Excel stores.
pub(super) fn round15(x: f64) -> f64 {
    round_significant(x, 15)
}

/// The least squares line through `y` at x = 0, 1, 2 and so on: the value at x = 0 and the slope.
pub(super) fn linear_fit(y: &[f64]) -> (f64, f64) {
    let n = y.len() as f64;
    if y.len() < 2 {
        return (y.first().copied().unwrap_or(0.0), 0.0);
    }
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = y.iter().sum::<f64>() / n;
    let sxx: f64 = (0..y.len()).map(|i| (i as f64 - mean_x).powi(2)).sum();
    let sxy: f64 = y.iter().enumerate().map(|(i, v)| (i as f64 - mean_x) * (v - mean_y)).sum();
    let slope = sxy / sxx;
    (mean_y - slope * mean_x, slope)
}

/// The least squares line through the logarithms of `y` at x = 0, 1, 2 and so on, as the log of the value
/// at x = 0 and the log of the growth factor; `None` when a value is not above zero.
pub(super) fn growth_fit(y: &[f64]) -> Option<(f64, f64)> {
    if y.iter().any(|v| *v <= 0.0) {
        return None;
    }
    let logs: Vec<f64> = y.iter().map(|v| v.ln()).collect();
    Some(linear_fit(&logs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_line_is_found_exactly() {
        let (a, b) = linear_fit(&[1.0, 3.0, 5.0]);
        assert!((a - 1.0).abs() < 1e-12 && (b - 2.0).abs() < 1e-12);
    }

    #[test]
    fn three_uneven_numbers_fit_a_line_between_them() {
        let (a, b) = linear_fit(&[1.0, 2.0, 4.0]);
        assert!((a + b * 3.0 - 5.333_333_333_333_333).abs() < 1e-12);
    }

    #[test]
    fn growth_needs_values_above_zero() {
        assert!(growth_fit(&[1.0, 0.0]).is_none());
        let (c, m) = growth_fit(&[1.0, 2.0, 4.0]).unwrap();
        assert!(((c + m * 3.0).exp() - 8.0).abs() < 1e-9);
    }
}
