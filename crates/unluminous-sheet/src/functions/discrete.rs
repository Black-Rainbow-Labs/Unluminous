//! Discrete distributions: the binomial range and inverse, the hypergeometric and the negative
//! binomial.
//!
//! A probability here is a product of binomial coefficients and powers whose parts overflow a double
//! long before the answer does: C(2000, 1000) is about 2e600 while the probability is about 0.02. The
//! products are therefore kept as a mantissa and a power of two (`Scaled`) and only the finished value
//! is turned back into a double. Taking logarithms of the gamma function instead loses about three
//! digits at these sizes, because a logarithm near 1400 can only be rounded to 2e-13.

use super::{boolean, int, num, number, opt, s, Function};
use crate::eval::Ctx;
use crate::value::{ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    s(
        "BINOM.DIST.RANGE",
        3,
        4,
        "trials, probability_s, number_s, [number_s2]",
        "The probability of a number of successes within a range.",
        binom_dist_range,
    ),
    s(
        "BINOM.INV",
        3,
        3,
        "trials, probability_s, alpha",
        "The smallest number of successes whose cumulative probability reaches alpha.",
        binom_inv,
    ),
    s(
        "CRITBINOM",
        3,
        3,
        "trials, probability_s, alpha",
        "The smallest number of successes whose cumulative probability reaches alpha.",
        binom_inv,
    ),
    s(
        "HYPGEOM.DIST",
        5,
        5,
        "sample_s, number_sample, population_s, number_pop, cumulative",
        "The hypergeometric distribution.",
        hypgeom_dist,
    ),
    s(
        "HYPGEOMDIST",
        4,
        4,
        "sample_s, number_sample, population_s, number_pop",
        "The hypergeometric probability of exactly sample_s successes.",
        |ctx, a| hypergeometric_value(ctx, a, false),
    ),
    s(
        "NEGBINOM.DIST",
        4,
        4,
        "number_f, number_s, probability_s, cumulative",
        "The negative binomial distribution.",
        negbinom_dist,
    ),
    s(
        "NEGBINOMDIST",
        3,
        3,
        "number_f, number_s, probability_s",
        "The negative binomial probability of exactly number_f failures.",
        |ctx, a| negative_binomial_value(ctx, a, false),
    ),
];

/// The most steps a loop over the successes may take, which keeps a huge trial count from stalling.
const LONGEST_RUN: i64 = 50_000_000;

/// A positive number held as a mantissa in [0.5, 1) and a power of two.
#[derive(Debug, Clone, Copy)]
struct Scaled {
    mantissa: f64,
    exponent: i64,
}

impl Scaled {
    /// A double as a scaled number.
    fn new(x: f64) -> Self {
        Self { mantissa: x, exponent: 0 }.normalised()
    }

    /// The same number with the mantissa brought back into [0.5, 1).
    fn normalised(self) -> Self {
        if self.mantissa == 0.0 || !self.mantissa.is_finite() {
            return self;
        }
        let bits = self.mantissa.to_bits();
        let field = ((bits >> 52) & 0x7ff) as i64;
        if field == 0 {
            // A subnormal mantissa is scaled up by 2^64 first.
            let up = Self { mantissa: self.mantissa * 2f64.powi(64), exponent: self.exponent - 64 };
            return up.normalised();
        }
        let kept = (bits & !(0x7ffu64 << 52)) | (1022u64 << 52);
        Self { mantissa: f64::from_bits(kept), exponent: self.exponent + field - 1022 }
    }

    fn times(self, x: f64) -> Self {
        Self { mantissa: self.mantissa * x, exponent: self.exponent }.normalised()
    }

    fn over(self, x: f64) -> Self {
        Self { mantissa: self.mantissa / x, exponent: self.exponent }.normalised()
    }

    fn mul(self, other: Self) -> Self {
        Self { mantissa: self.mantissa * other.mantissa, exponent: self.exponent + other.exponent }
            .normalised()
    }

    fn div(self, other: Self) -> Self {
        Self { mantissa: self.mantissa / other.mantissa, exponent: self.exponent - other.exponent }
            .normalised()
    }

    /// `base` raised to a whole power by repeated squaring.
    fn power(base: f64, mut n: i64) -> Self {
        let (mut result, mut square) = (Self::new(1.0), Self::new(base));
        while n > 0 {
            if n & 1 == 1 {
                result = result.mul(square);
            }
            square = square.mul(square);
            n >>= 1;
        }
        result
    }

    /// The number of ways to choose `k` of `n`.
    fn choose(n: i64, k: i64) -> Self {
        let k = k.min(n - k);
        let mut c = Self::new(1.0);
        for i in 1..=k {
            c = c.times((n - k + i) as f64).over(i as f64);
        }
        c
    }

    /// The nearest double, which is 0 or infinity when the number is out of range.
    fn value(self) -> f64 {
        if self.mantissa == 0.0 || self.exponent < -1200 {
            return 0.0;
        }
        if self.exponent > 1100 {
            return f64::INFINITY;
        }
        let half = (self.exponent / 2) as i32;
        self.mantissa * 2f64.powi(half) * 2f64.powi(self.exponent as i32 - half)
    }
}

/// The probability of exactly `k` successes in `n` trials, for 0 < p < 1.
fn binomial_pmf(n: i64, k: i64, p: f64) -> Scaled {
    Scaled::choose(n, k).mul(Scaled::power(p, k)).mul(Scaled::power(1.0 - p, n - k))
}

/// The probability the binomial puts on each of `from..=to` successes, 0 < p < 1, added in order.
fn binomial_sum(n: i64, from: i64, to: i64, p: f64) -> f64 {
    let ratio = p / (1.0 - p);
    let mut term = binomial_pmf(n, from, p);
    let mut sum = 0.0;
    for k in from..=to {
        sum += term.value();
        term = term.times((n - k) as f64 / (k + 1) as f64 * ratio);
    }
    sum
}

fn binom_dist_range(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (n, p, low) = (int(ctx, &a[0])?, num(ctx, &a[1])?, int(ctx, &a[2])?);
    let high = match opt(a, 3) {
        Some(v) => int(ctx, v)?,
        None => low,
    };
    if n < 0 || !(0.0..=1.0).contains(&p) || low < 0 || low > n || high < low || high > n {
        return Err(ErrorKind::Num);
    }
    if high - low > LONGEST_RUN {
        return Err(ErrorKind::Num);
    }
    if p == 0.0 {
        return number(f64::from(u8::from(low == 0)));
    }
    if p == 1.0 {
        return number(f64::from(u8::from(high == n)));
    }
    number(binomial_sum(n, low, high, p))
}

fn binom_inv(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (n, p, alpha) = (int(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
    if n < 0 || p <= 0.0 || p >= 1.0 || alpha <= 0.0 || alpha >= 1.0 || n > LONGEST_RUN {
        return Err(ErrorKind::Num);
    }
    let ratio = p / (1.0 - p);
    let mut term = Scaled::power(1.0 - p, n);
    let mut cumulative = 0.0;
    for k in 0..=n {
        cumulative += term.value();
        if cumulative >= alpha {
            return number(k as f64);
        }
        term = term.times((n - k) as f64 / (k + 1) as f64 * ratio);
    }
    number(n as f64)
}

/// The four whole numbers every hypergeometric function starts from: successes in the sample, the
/// sample size, successes in the population and the population size.
fn hypergeometric_arguments(ctx: &mut Ctx, a: &[Value]) -> Result<[i64; 4], ErrorKind> {
    let x = [int(ctx, &a[0])?, int(ctx, &a[1])?, int(ctx, &a[2])?, int(ctx, &a[3])?];
    let [sample_hits, sample, hits, population] = x;
    let bad = sample_hits < 0
        || sample < 0
        || sample > population
        || hits < 0
        || hits > population
        || population < 1;
    if bad {
        Err(ErrorKind::Num)
    } else {
        Ok(x)
    }
}

/// The probability of exactly `x` successes in a sample, as a scaled number.
fn hypergeometric_pmf(x: i64, sample: i64, hits: i64, population: i64) -> Scaled {
    Scaled::choose(hits, x)
        .mul(Scaled::choose(population - hits, sample - x))
        .div(Scaled::choose(population, sample))
}

/// The hypergeometric probability of `x` successes, or of at most `x` when `cumulative`.
fn hypergeometric_value(ctx: &mut Ctx, a: &[Value], cumulative: bool) -> Result<Value, ErrorKind> {
    let [x, sample, hits, population] = hypergeometric_arguments(ctx, a)?;
    let first = (sample + hits - population).max(0);
    let last = sample.min(hits);
    if x < first {
        return number(0.0);
    }
    if x > last {
        return number(f64::from(u8::from(cumulative)));
    }
    if !cumulative {
        return number(hypergeometric_pmf(x, sample, hits, population).value());
    }
    let mut term = hypergeometric_pmf(first, sample, hits, population);
    let mut sum = 0.0;
    for i in first..=x {
        sum += term.value();
        let up = (sample - i) as f64 * (hits - i) as f64;
        let down = (i + 1) as f64 * (population - hits - sample + i + 1) as f64;
        term = term.times(up).over(down);
    }
    number(sum)
}

fn hypgeom_dist(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let cumulative = boolean(&a[4])?;
    hypergeometric_value(ctx, &a[..4], cumulative)
}

/// The negative binomial probability of `f` failures before the `s`-th success, or of at most `f`.
fn negative_binomial_value(
    ctx: &mut Ctx,
    a: &[Value],
    cumulative: bool,
) -> Result<Value, ErrorKind> {
    let (f, successes, p) = (int(ctx, &a[0])?, int(ctx, &a[1])?, num(ctx, &a[2])?);
    if f < 0 || successes < 1 || p <= 0.0 || p >= 1.0 || f > LONGEST_RUN {
        return Err(ErrorKind::Num);
    }
    let q = 1.0 - p;
    let mut term = Scaled::power(p, successes);
    let mut sum = 0.0;
    for i in 0..f {
        sum += term.value();
        term = term.times((i + successes) as f64 / (i + 1) as f64 * q);
    }
    number(if cumulative { sum + term.value() } else { term.value() })
}

fn negbinom_dist(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let cumulative = boolean(&a[3])?;
    negative_binomial_value(ctx, &a[..3], cumulative)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_products_survive_values_a_double_cannot_hold() {
        // C(2000, 1000) is about 2e600, and the probability built from it is an ordinary number.
        let p = binomial_pmf(2000, 1000, 0.5).value();
        // The central binomial probability is 1 / sqrt(pi n) times (1 - 1 / 8n), here n = 1000.
        assert!((p - 0.017_839_011).abs() < 1e-8, "{p}");
        assert_eq!(Scaled::power(0.5, 1100).value(), 0.0);
        assert!((Scaled::choose(10, 3).value() - 120.0).abs() < 1e-12);
    }
}
