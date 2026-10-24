//! Probability distributions, and the special functions under them: the log gamma function, the
//! regularised incomplete gamma and beta functions, the error function and the inverse normal.
//!
//! The error function is the regularised incomplete gamma P(1/2, x²), which converges to the last digit
//! by its series and continued fraction, rather than one of the short rational approximations that stop
//! at seven digits. The inverse normal is Acklam's algorithm refined by one Halley step.

use super::{num, number, opt_bool, s, Function};
use crate::eval::Ctx;
use crate::value::{ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    s(
        "NORM.DIST",
        4,
        4,
        "x, mean, standard_dev, cumulative",
        "The normal distribution.",
        norm_dist,
    ),
    s("NORMDIST", 4, 4, "x, mean, standard_dev, cumulative", "The normal distribution.", norm_dist),
    s("NORM.S.DIST", 2, 2, "z, cumulative", "The standard normal distribution.", |ctx, a| {
        let z = num(ctx, &a[0])?;
        number(if opt_bool(a, 1, true)? { normal_cdf(z) } else { normal_pdf(z) })
    }),
    s("NORMSDIST", 1, 1, "z", "The cumulative standard normal distribution.", |ctx, a| {
        number(normal_cdf(num(ctx, &a[0])?))
    }),
    s(
        "NORM.INV",
        3,
        3,
        "probability, mean, standard_dev",
        "The inverse of the normal distribution.",
        norm_inv,
    ),
    s(
        "NORMINV",
        3,
        3,
        "probability, mean, standard_dev",
        "The inverse of the normal distribution.",
        norm_inv,
    ),
    s(
        "NORM.S.INV",
        1,
        1,
        "probability",
        "The inverse of the standard normal distribution.",
        |ctx, a| {
            let p = num(ctx, &a[0])?;
            if p <= 0.0 || p >= 1.0 {
                return Err(ErrorKind::Num);
            }
            number(normal_inverse(p))
        },
    ),
    s(
        "NORMSINV",
        1,
        1,
        "probability",
        "The inverse of the standard normal distribution.",
        |ctx, a| {
            let p = num(ctx, &a[0])?;
            if p <= 0.0 || p >= 1.0 {
                return Err(ErrorKind::Num);
            }
            number(normal_inverse(p))
        },
    ),
    s("PHI", 1, 1, "x", "The standard normal density.", |ctx, a| {
        number(normal_pdf(num(ctx, &a[0])?))
    }),
    s(
        "GAUSS",
        1,
        1,
        "z",
        "The probability a standard normal value is between the mean and z.",
        |ctx, a| number(normal_cdf(num(ctx, &a[0])?) - 0.5),
    ),
    s("ERF", 1, 2, "lower_limit, [upper_limit]", "The error function.", |ctx, a| {
        let lo = num(ctx, &a[0])?;
        match a.get(1) {
            Some(v) => number(erf(num(ctx, v)?) - erf(lo)),
            None => number(erf(lo)),
        }
    }),
    s("ERF.PRECISE", 1, 1, "x", "The error function.", |ctx, a| number(erf(num(ctx, &a[0])?))),
    s("ERFC", 1, 1, "x", "The complementary error function.", |ctx, a| {
        number(erfc(num(ctx, &a[0])?))
    }),
    s("ERFC.PRECISE", 1, 1, "x", "The complementary error function.", |ctx, a| {
        number(erfc(num(ctx, &a[0])?))
    }),
    s("GAMMALN", 1, 1, "x", "The natural logarithm of the gamma function.", gammaln_fn),
    s("GAMMALN.PRECISE", 1, 1, "x", "The natural logarithm of the gamma function.", gammaln_fn),
    s("GAMMA", 1, 1, "number", "The gamma function.", |ctx, a| {
        let x = num(ctx, &a[0])?;
        if x == 0.0 || x < 0.0 && x.fract() == 0.0 {
            return Err(ErrorKind::Num);
        }
        number(gamma(x))
    }),
    s(
        "BINOM.DIST",
        4,
        4,
        "number_s, trials, probability_s, cumulative",
        "The binomial distribution.",
        binom_dist,
    ),
    s(
        "BINOMDIST",
        4,
        4,
        "number_s, trials, probability_s, cumulative",
        "The binomial distribution.",
        binom_dist,
    ),
    s("POISSON.DIST", 3, 3, "x, mean, cumulative", "The Poisson distribution.", poisson),
    s("POISSON", 3, 3, "x, mean, cumulative", "The Poisson distribution.", poisson),
    s("EXPON.DIST", 3, 3, "x, lambda, cumulative", "The exponential distribution.", expon),
    s("EXPONDIST", 3, 3, "x, lambda, cumulative", "The exponential distribution.", expon),
    s(
        "LOGNORM.DIST",
        4,
        4,
        "x, mean, standard_dev, cumulative",
        "The lognormal distribution.",
        |ctx, a| {
            let (x, m, sd) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
            if x <= 0.0 || sd <= 0.0 {
                return Err(ErrorKind::Num);
            }
            let z = (x.ln() - m) / sd;
            number(if opt_bool(a, 3, true)? { normal_cdf(z) } else { normal_pdf(z) / (x * sd) })
        },
    ),
    s(
        "LOGNORM.INV",
        3,
        3,
        "probability, mean, standard_dev",
        "The inverse of the lognormal distribution.",
        |ctx, a| {
            let (p, m, sd) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
            if p <= 0.0 || p >= 1.0 || sd <= 0.0 {
                return Err(ErrorKind::Num);
            }
            number((m + sd * normal_inverse(p)).exp())
        },
    ),
    s("WEIBULL.DIST", 4, 4, "x, alpha, beta, cumulative", "The Weibull distribution.", weibull),
    s("WEIBULL", 4, 4, "x, alpha, beta, cumulative", "The Weibull distribution.", weibull),
    s(
        "CONFIDENCE.NORM",
        3,
        3,
        "alpha, standard_dev, size",
        "The confidence interval for a mean, normal distribution.",
        confidence,
    ),
    s(
        "CONFIDENCE",
        3,
        3,
        "alpha, standard_dev, size",
        "The confidence interval for a mean, normal distribution.",
        confidence,
    ),
    s("T.DIST", 3, 3, "x, deg_freedom, cumulative", "Student's t distribution.", |ctx, a| {
        let (x, df) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
        if df < 1.0 {
            return Err(ErrorKind::Num);
        }
        number(if opt_bool(a, 2, true)? { t_cdf(x, df) } else { t_pdf(x, df) })
    }),
    s(
        "T.DIST.RT",
        2,
        2,
        "x, deg_freedom",
        "The right tail of Student's t distribution.",
        |ctx, a| {
            let (x, df) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
            if df < 1.0 {
                return Err(ErrorKind::Num);
            }
            number(1.0 - t_cdf(x, df))
        },
    ),
    s("T.DIST.2T", 2, 2, "x, deg_freedom", "Both tails of Student's t distribution.", |ctx, a| {
        let (x, df) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
        if df < 1.0 || x < 0.0 {
            return Err(ErrorKind::Num);
        }
        number(2.0 * (1.0 - t_cdf(x, df)))
    }),
    s(
        "TDIST",
        3,
        3,
        "x, deg_freedom, tails",
        "Student's t distribution, one or two tails.",
        |ctx, a| {
            let (x, df, tails) =
                (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc(), num(ctx, &a[2])?.trunc());
            if df < 1.0 || x < 0.0 || !(tails == 1.0 || tails == 2.0) {
                return Err(ErrorKind::Num);
            }
            number(tails * (1.0 - t_cdf(x, df)))
        },
    ),
    s(
        "T.INV",
        2,
        2,
        "probability, deg_freedom",
        "The inverse of Student's t distribution.",
        t_inv,
    ),
    s(
        "T.INV.2T",
        2,
        2,
        "probability, deg_freedom",
        "The two tailed inverse of Student's t distribution.",
        t_inv_2t,
    ),
    s(
        "TINV",
        2,
        2,
        "probability, deg_freedom",
        "The two tailed inverse of Student's t distribution.",
        t_inv_2t,
    ),
    s(
        "CHISQ.DIST",
        3,
        3,
        "x, deg_freedom, cumulative",
        "The chi-squared distribution.",
        |ctx, a| {
            let (x, df) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
            if x < 0.0 || df < 1.0 {
                return Err(ErrorKind::Num);
            }
            number(if opt_bool(a, 2, true)? {
                gamma_p(df / 2.0, x / 2.0)
            } else if x == 0.0 {
                // The density at 0 is infinite with one degree of freedom, which Excel gives as #NUM!.
                match df {
                    1.0 => return Err(ErrorKind::Num),
                    2.0 => 0.5,
                    _ => 0.0,
                }
            } else {
                ((df / 2.0 - 1.0) * x.ln() - x / 2.0 - (df / 2.0) * 2f64.ln() - ln_gamma(df / 2.0))
                    .exp()
            })
        },
    ),
    s(
        "CHISQ.DIST.RT",
        2,
        2,
        "x, deg_freedom",
        "The right tail of the chi-squared distribution.",
        chisq_rt,
    ),
    s(
        "CHIDIST",
        2,
        2,
        "x, deg_freedom",
        "The right tail of the chi-squared distribution.",
        chisq_rt,
    ),
    s(
        "CHISQ.INV",
        2,
        2,
        "probability, deg_freedom",
        "The inverse of the chi-squared distribution.",
        |ctx, a| {
            let (p, df) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
            if !(0.0..1.0).contains(&p) || df < 1.0 {
                return Err(ErrorKind::Num);
            }
            number(invert(|x| gamma_p(df / 2.0, x / 2.0), p, 0.0, df.max(1.0) * 100.0 + 100.0))
        },
    ),
    s(
        "CHISQ.INV.RT",
        2,
        2,
        "probability, deg_freedom",
        "The inverse of the right tail of the chi-squared distribution.",
        chisq_inv_rt,
    ),
    s(
        "CHIINV",
        2,
        2,
        "probability, deg_freedom",
        "The inverse of the right tail of the chi-squared distribution.",
        chisq_inv_rt,
    ),
    s("GAMMA.DIST", 4, 4, "x, alpha, beta, cumulative", "The gamma distribution.", gamma_dist),
    s("GAMMADIST", 4, 4, "x, alpha, beta, cumulative", "The gamma distribution.", gamma_dist),
    s(
        "BETA.DIST",
        4,
        6,
        "x, alpha, beta, cumulative, [A], [B]",
        "The beta distribution.",
        |ctx, a| {
            let (x, alpha, beta) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
            let cumulative = opt_bool(a, 3, true)?;
            let lo = super::opt_num(ctx, a, 4, 0.0)?;
            let hi = super::opt_num(ctx, a, 5, 1.0)?;
            if alpha <= 0.0 || beta <= 0.0 || x < lo || x > hi || lo == hi {
                return Err(ErrorKind::Num);
            }
            let t = (x - lo) / (hi - lo);
            number(if cumulative {
                beta_i(t, alpha, beta)
            } else {
                (((alpha - 1.0) * t.ln() + (beta - 1.0) * (1.0 - t).ln()) - ln_beta(alpha, beta))
                    .exp()
                    / (hi - lo)
            })
        },
    ),
    s(
        "F.DIST",
        4,
        4,
        "x, deg_freedom1, deg_freedom2, cumulative",
        "The F distribution.",
        |ctx, a| {
            let (x, d1, d2) =
                (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc(), num(ctx, &a[2])?.trunc());
            if x < 0.0 || d1 < 1.0 || d2 < 1.0 {
                return Err(ErrorKind::Num);
            }
            number(if opt_bool(a, 3, true)? {
                beta_i(d1 * x / (d1 * x + d2), d1 / 2.0, d2 / 2.0)
            } else if x == 0.0 {
                // The density at 0 is 0, 1 or infinite as the first degrees of freedom are above, at or
                // under 2; infinite is #NUM!.
                match d1 {
                    1.0 => return Err(ErrorKind::Num),
                    2.0 => 1.0,
                    _ => 0.0,
                }
            } else {
                ((d1 * x).powf(d1) * d2.powf(d2) / (d1 * x + d2).powf(d1 + d2)).sqrt()
                    / (x * ln_beta(d1 / 2.0, d2 / 2.0).exp())
            })
        },
    ),
    s(
        "F.DIST.RT",
        3,
        3,
        "x, deg_freedom1, deg_freedom2",
        "The right tail of the F distribution.",
        f_rt,
    ),
    s(
        "FDIST",
        3,
        3,
        "x, deg_freedom1, deg_freedom2",
        "The right tail of the F distribution.",
        f_rt,
    ),
];

fn norm_dist(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (x, m, sd) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
    if sd <= 0.0 {
        return Err(ErrorKind::Num);
    }
    let z = (x - m) / sd;
    number(if opt_bool(a, 3, true)? { normal_cdf(z) } else { normal_pdf(z) / sd })
}

fn norm_inv(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (p, m, sd) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
    if p <= 0.0 || p >= 1.0 || sd <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number(m + sd * normal_inverse(p))
}

fn gammaln_fn(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let x = num(ctx, &a[0])?;
    if x <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number(ln_gamma(x))
}

fn binom_dist(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (k, n, p) = (num(ctx, &a[0])?.trunc(), num(ctx, &a[1])?.trunc(), num(ctx, &a[2])?);
    if k < 0.0 || k > n || !(0.0..=1.0).contains(&p) {
        return Err(ErrorKind::Num);
    }
    let pmf = |i: f64| -> f64 {
        if p == 0.0 {
            return if i == 0.0 { 1.0 } else { 0.0 };
        }
        if p == 1.0 {
            return if i == n { 1.0 } else { 0.0 };
        }
        (ln_gamma(n + 1.0) - ln_gamma(i + 1.0) - ln_gamma(n - i + 1.0)
            + i * p.ln()
            + (n - i) * (1.0 - p).ln())
        .exp()
    };
    number(if opt_bool(a, 3, true)? {
        (0..=k as i64).map(|i| pmf(i as f64)).sum::<f64>().min(1.0)
    } else {
        pmf(k)
    })
}

fn poisson(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (k, m) = (num(ctx, &a[0])?.trunc(), num(ctx, &a[1])?);
    if k < 0.0 || m < 0.0 {
        return Err(ErrorKind::Num);
    }
    let pmf = |i: f64| (i * m.ln() - m - ln_gamma(i + 1.0)).exp();
    number(if opt_bool(a, 2, true)? {
        if m == 0.0 {
            1.0
        } else {
            1.0 - gamma_p(k + 1.0, m)
        }
    } else if m == 0.0 {
        if k == 0.0 {
            1.0
        } else {
            0.0
        }
    } else {
        pmf(k)
    })
}

fn expon(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (x, l) = (num(ctx, &a[0])?, num(ctx, &a[1])?);
    if x < 0.0 || l <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number(if opt_bool(a, 2, true)? { 1.0 - (-l * x).exp() } else { l * (-l * x).exp() })
}

fn weibull(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (x, k, l) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
    if x < 0.0 || k <= 0.0 || l <= 0.0 {
        return Err(ErrorKind::Num);
    }
    let t = (x / l).powf(k);
    number(if opt_bool(a, 3, true)? {
        1.0 - (-t).exp()
    } else if x == 0.0 {
        // Excel answers 0 for the density at 0 whatever the shape.
        0.0
    } else {
        k / l * (x / l).powf(k - 1.0) * (-t).exp()
    })
}

fn confidence(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (alpha, sd, n) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?.trunc());
    if alpha <= 0.0 || alpha >= 1.0 || sd <= 0.0 || n < 1.0 {
        return Err(ErrorKind::Num);
    }
    number(normal_inverse(1.0 - alpha / 2.0) * sd / n.sqrt())
}

fn t_inv_2t(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (p, df) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
    if p <= 0.0 || p > 1.0 || df < 1.0 {
        return Err(ErrorKind::Num);
    }
    if p == 1.0 {
        // Half of the probability is on each side of 0, so the answer is exactly 0.
        return number(0.0);
    }
    number(t_inverse(1.0 - p / 2.0, df))
}

/// The one tailed inverse, which is exactly 0 at one half.
fn t_inv(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (p, df) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
    if p <= 0.0 || p >= 1.0 || df < 1.0 {
        return Err(ErrorKind::Num);
    }
    number(if p == 0.5 { 0.0 } else { t_inverse(p, df) })
}

fn chisq_rt(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (x, df) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
    if x < 0.0 || df < 1.0 {
        return Err(ErrorKind::Num);
    }
    number(1.0 - gamma_p(df / 2.0, x / 2.0))
}

fn chisq_inv_rt(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (p, df) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
    if p <= 0.0 || p > 1.0 || df < 1.0 {
        return Err(ErrorKind::Num);
    }
    number(invert(
        |x| 1.0 - (1.0 - gamma_p(df / 2.0, x / 2.0)),
        1.0 - p,
        0.0,
        df.max(1.0) * 100.0 + 100.0,
    ))
}

fn gamma_dist(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (x, alpha, beta) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
    if x < 0.0 || alpha <= 0.0 || beta <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number(if opt_bool(a, 3, true)? {
        gamma_p(alpha, x / beta)
    } else {
        ((alpha - 1.0) * x.ln() - x / beta - alpha * beta.ln() - ln_gamma(alpha)).exp()
    })
}

fn f_rt(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (x, d1, d2) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc(), num(ctx, &a[2])?.trunc());
    if x < 0.0 || d1 < 1.0 || d2 < 1.0 {
        return Err(ErrorKind::Num);
    }
    number(beta_i(d2 / (d2 + d1 * x), d2 / 2.0, d1 / 2.0))
}

pub fn normal_pdf(z: f64) -> f64 {
    (-0.5 * z * z).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

pub fn normal_cdf(z: f64) -> f64 {
    0.5 * erfc(-z / std::f64::consts::SQRT_2)
}

pub fn erf(x: f64) -> f64 {
    if x < 0.0 {
        return -erf(-x);
    }
    gamma_p(0.5, x * x)
}

pub fn erfc(x: f64) -> f64 {
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    gamma_q(0.5, x * x)
}

/// The natural log of the gamma function, by the Lanczos approximation.
pub fn ln_gamma(x: f64) -> f64 {
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        let s = (std::f64::consts::PI * x).sin();
        return (std::f64::consts::PI / s.abs()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = C[0];
    let t = x + G + 0.5;
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

pub fn gamma(x: f64) -> f64 {
    if x < 0.5 {
        return std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma(1.0 - x));
    }
    if x.fract() == 0.0 && x <= 171.0 {
        return (1..x as u64).fold(1.0, |acc, k| acc * k as f64);
    }
    ln_gamma(x).exp()
}

fn ln_beta(a: f64, b: f64) -> f64 {
    ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b)
}

/// The regularised lower incomplete gamma function P(a, x).
pub fn gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x < a + 1.0 {
        // The series.
        let mut sum = 1.0 / a;
        let mut term = sum;
        let mut n = a;
        for _ in 0..1000 {
            n += 1.0;
            term *= x / n;
            sum += term;
            if term.abs() < sum.abs() * 1e-17 {
                break;
            }
        }
        (sum.ln() - x + a * x.ln() - ln_gamma(a)).exp()
    } else {
        1.0 - gamma_q(a, x)
    }
}

/// The regularised upper incomplete gamma function Q(a, x) = 1 - P(a, x).
pub fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        return 1.0 - gamma_p(a, x);
    }
    // Lentz's continued fraction.
    let tiny = 1e-300;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / tiny;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..1000 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < tiny {
            d = tiny;
        }
        c = b + an / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < 1e-17 {
            break;
        }
    }
    (-x + a * x.ln() - ln_gamma(a)).exp() * h
}

/// The regularised incomplete beta function I_x(a, b).
pub fn beta_i(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let front = (a * x.ln() + b * (1.0 - x).ln() - ln_beta(a, b)).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * beta_fraction(x, a, b) / a
    } else {
        1.0 - front * beta_fraction(1.0 - x, b, a) / b
    }
}

fn beta_fraction(x: f64, a: f64, b: f64) -> f64 {
    let tiny = 1e-300;
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..1000 {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < 1e-16 {
            break;
        }
    }
    h
}

fn t_pdf(x: f64, df: f64) -> f64 {
    (ln_gamma((df + 1.0) / 2.0) - ln_gamma(df / 2.0)).exp() / (df * std::f64::consts::PI).sqrt()
        * (1.0 + x * x / df).powf(-(df + 1.0) / 2.0)
}

fn t_cdf(x: f64, df: f64) -> f64 {
    let tail = 0.5 * beta_i(df / (df + x * x), df / 2.0, 0.5);
    if x >= 0.0 {
        1.0 - tail
    } else {
        tail
    }
}

fn t_inverse(p: f64, df: f64) -> f64 {
    invert(|x| t_cdf(x, df), p, -1e6, 1e6)
}

/// Solve `f(x) = target` for an increasing `f` by bisection, then polish.
fn invert(f: impl Fn(f64) -> f64, target: f64, mut lo: f64, mut hi: f64) -> f64 {
    for _ in 0..300 {
        let mid = 0.5 * (lo + hi);
        if f(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
        if (hi - lo).abs() <= 1e-15 * mid.abs().max(1e-300) {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// The inverse of the standard normal distribution: Acklam's rational approximation, then one step of
/// Halley's method against the exact distribution, which brings it to the last digit.
pub fn normal_inverse(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.383_577_518_672_69e2,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let low = 0.02425;
    let x = if p < low {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - low {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    let e = normal_cdf(x) - p;
    let u = e * (2.0 * std::f64::consts::PI).sqrt() * (x * x / 2.0).exp();
    x - u / (1.0 + x * u / 2.0)
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::check;

    #[test]
    fn distributions_answer_to_twelve_digits() {
        check(&[
            ("NORM.S.DIST(1,TRUE)", "0.841344746069"),
            ("NORM.DIST(42,40,1.5,TRUE)", "0.908788780274"),
            ("NORM.DIST(42,40,1.5,FALSE)", "0.109340049784"),
            ("NORM.S.INV(0.908789)", "1.33333467304"),
            ("NORM.INV(0.908789,40,1.5)", "42.0000020096"),
            ("ERF(1)", "0.84270079295"),
            ("ERFC(1)", "0.15729920705"),
            ("GAMMALN(4.5)", "2.45373657084"),
            ("GAMMA(5)", "24"),
            ("BINOM.DIST(6,10,0.5,FALSE)", "0.205078125"),
            ("BINOM.DIST(6,10,0.5,TRUE)", "0.828125"),
            ("POISSON.DIST(2,5,FALSE)", "0.0842243374886"),
            ("POISSON.DIST(2,5,TRUE)", "0.124652019483"),
            ("EXPON.DIST(0.2,10,TRUE)", "0.864664716763"),
            ("T.DIST(60,1,TRUE)", "0.994695326367"),
            ("T.DIST.2T(1.959999998,60)", "0.0546449299759"),
            ("T.INV(0.75,2)", "0.816496580928"),
            ("CHISQ.DIST(0.5,1,TRUE)", "0.520499877813"),
            ("CHISQ.DIST.RT(18.307,10)", "0.0500005890914"),
            ("CONFIDENCE.NORM(0.05,2.5,50)", "0.692951912175"),
        ]);
    }
}
