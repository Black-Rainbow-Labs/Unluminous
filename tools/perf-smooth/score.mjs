#!/usr/bin/env node
// Compare smoothness bench variants against a baseline, case by case (`task-2288`).
//
//   node tools/perf-smooth/score.mjs --flow <hill climb folder> --base baseline [--json] <variant> ...
//
// A case is one gesture under one load, such as `open-modal@over36`. For each case it takes the median of
// the case's primary number over the reps of each variant, and the ratio to the baseline's median, so a
// ratio under one is better. The headline is the geometric mean of the ratios over the test cases, and
// over the train cases beside it; a case whose baseline median is under a floor (a millisecond for most
// numbers) is left out of the means, because a ratio of two numbers that small is noise.

import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';

const args = process.argv.slice(2);
/** Read a `--name value` argument, or the fallback. */
function option(name, fallback) {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
}
const flow = path.resolve(option('flow', '_agent_output/perf-smooth/hillclimb'));
const base = option('base', 'baseline');
const json = args.includes('--json');
const variants = args.filter((word, at) => !word.startsWith('--') && !['flow', 'base'].includes(args[at - 1]?.slice(2)));
const FLOOR = 1.0;

/** Every row of a variant's results. */
function rows(variant) {
  const file = path.join(flow, variant, 'results.jsonl');
  if (!existsSync(file)) return [];
  return readFileSync(file, 'utf8').split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line));
}

/** The middle value. */
function median(values) {
  const list = values.filter((value) => value !== null && Number.isFinite(value)).sort((a, b) => a - b);
  return list.length ? list[Math.floor(list.length / 2)] : null;
}

/** Each case's median primary number and its split. */
function cases(variant) {
  const found = new Map();
  for (const row of rows(variant)) {
    const entry = found.get(row.prompt_id) ?? { split: row.tags[0], values: [] };
    entry.values.push(row.grade.primary);
    found.set(row.prompt_id, entry);
  }
  return new Map([...found].map(([id, entry]) => [id, { split: entry.split, median: median(entry.values), reps: entry.values.length }]));
}

const baseline = cases(base);
const geomean = (values) => (values.length ? Math.exp(values.reduce((sum, value) => sum + Math.log(value), 0) / values.length) : null);
const report = { base, variants: {} };
for (const variant of variants) {
  const mine = cases(variant);
  const table = [];
  for (const [id, before] of baseline) {
    const after = mine.get(id);
    const ratio = after && before.median > FLOOR && after.median !== null ? Math.max(after.median, 0.01) / before.median : null;
    table.push({ id, split: before.split, base: before.median, value: after?.median ?? null, ratio, reps: after?.reps ?? 0 });
  }
  const test = geomean(table.filter((row) => row.split === 'test' && row.ratio !== null).map((row) => row.ratio));
  const train = geomean(table.filter((row) => row.split === 'train' && row.ratio !== null).map((row) => row.ratio));
  report.variants[variant] = { test, train, table };
}

if (json) {
  console.log(JSON.stringify(report, null, 2));
} else {
  for (const [variant, result] of Object.entries(report.variants)) {
    console.log(`\n${variant} against ${base}: test ${result.test?.toFixed(3)}, train ${result.train?.toFixed(3)} (geometric mean of ratios, under 1 is better)`);
    console.log('| case | split | base | this | ratio |');
    console.log('|---|---|---|---|---|');
    for (const row of result.table.sort((a, b) => a.id.localeCompare(b.id))) {
      const show = (value) => (value === null ? '-' : value.toFixed(1));
      console.log(`| ${row.id} | ${row.split} | ${show(row.base)} | ${show(row.value)} | ${row.ratio === null ? '-' : row.ratio.toFixed(2)} |`);
    }
  }
}
