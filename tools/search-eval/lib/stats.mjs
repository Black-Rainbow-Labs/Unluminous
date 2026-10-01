// The statistics of TDD section 7.6: geometric means of paired ratios, a paired bootstrap with a fixed
// seed, and the four verdicts `inillucent-bench` uses.

import { RESAMPLES, SEED } from './config.mjs';

/**
 * A small seeded generator (mulberry32), so every bootstrap draws the same resamples.
 * @param seed - the 32 bit seed
 */
export function seeded(seed = SEED) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * The geometric mean of positive numbers.
 * @param values - the numbers
 */
export function geomean(values) {
  if (!values.length) return NaN;
  return Math.exp(values.reduce((s, v) => s + Math.log(v), 0) / values.length);
}

/**
 * The mean of numbers.
 * @param values - the numbers
 */
export function mean(values) {
  return values.length ? values.reduce((s, v) => s + v, 0) / values.length : NaN;
}

/**
 * The median of numbers.
 * @param values - the numbers
 */
export function median(values) {
  if (!values.length) return NaN;
  const s = [...values].sort((x, y) => x - y);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

/**
 * A percentile bootstrap interval of a statistic over paired values, resampling the pairs.
 * @param values - one value per pair (for example one ratio per query)
 * @param statistic - the function of a sample to bound, such as geomean
 * @param level - the interval's coverage, 0.95 by default
 */
export function bootstrap(values, statistic, level = 0.95) {
  const random = seeded();
  const draws = new Float64Array(RESAMPLES);
  const sample = new Array(values.length);
  for (let r = 0; r < RESAMPLES; r++) {
    for (let i = 0; i < values.length; i++) sample[i] = values[Math.floor(random() * values.length)];
    draws[r] = statistic(sample);
  }
  draws.sort();
  const lo = draws[Math.floor(((1 - level) / 2) * RESAMPLES)];
  const hi = draws[Math.min(RESAMPLES - 1, Math.floor((1 - (1 - level) / 2) * RESAMPLES))];
  return { point: statistic(values), lo, hi };
}

/**
 * The verdict on a ratio's interval against 1.0 and against a bar: better when the whole interval is
 * above 1, worse when the whole interval is below, equivalent when it sits within 2% of 1, inconclusive
 * otherwise.
 * @param interval - `{ lo, hi }` of a ratio where above 1 means the index arm is better
 */
export function verdict(interval) {
  if (interval.lo > 1) return 'better';
  if (interval.hi < 1) return 'worse';
  if (interval.lo >= 0.98 && interval.hi <= 1.02) return 'equivalent';
  return 'inconclusive';
}

/**
 * Writes a ratio for a reader: above 1 as "N% faster", below 1 as "N% slower", as the house style asks.
 * @param ratio - the index arm's speed divided into the rg arm's time
 */
export function describeRatio(ratio) {
  if (ratio >= 1) return `${((ratio - 1) * 100).toFixed(0)}% faster`;
  return `${((1 / ratio - 1) * 100).toFixed(0)}% slower`;
}
