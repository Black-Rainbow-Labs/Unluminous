#!/usr/bin/env node
// Turn `tools/perf-session.mjs` rows into a hill climb record and a table.
//
//   node tools/perf-session-score.mjs --results <results.jsonl> --flow <folder> --base <label> <candidate> ...
//
// Each case has one number that decides it (the `primary` below), lower being better for all of them.
// A candidate's score for a case is the baseline's median divided by the candidate's median, so 2.0 is
// twice as good and 1.0 is unchanged. The split is fixed by the case name: FNV-1a of the name, 0 mod 3
// is test. The flow folder gets `baseline/` and one `vN/` a candidate, each with `results.jsonl` (one
// row a case a round) and `summary.json`, plus `_state.json` and `metrics.md`, which is the layout the
// hill climb report builder reads.

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';

const args = process.argv.slice(2);
/** Read a `--name value` argument, or the fallback. */
function option(name, fallback) {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
}
const results = resolve(option('results', '_agent_output/task-2279-perf/bench/results.jsonl'));
const flow = resolve(option('flow', '_agent_output/task-2279-perf/flow'));
const base = option('base', 'base');
const at = args.indexOf('--base');
const candidates = args.slice(at + 2).filter((word) => !word.startsWith('--'));

/** The number that decides each case, and what it is called. */
const PRIMARY = {
  'cold-open': ['peak_ws_mb', 'peak working set of the tree, MB'],
  'cold-idle': ['cpu_ms_per_s', 'tree processor ms a second at rest'],
  'warm-open': ['cpu_ms', 'tree processor ms in the first 90 s'],
  'warm-idle': ['ws_mb', 'tree working set at rest, MB'],
  'cold-close': ['close_ms', 'ms from quit until the window has gone'],
  'warm-close': ['close_ms', 'ms from quit until the window has gone'],
};
/** The primary for an interaction is its median frame. */
const primaryOf = (name) => PRIMARY[name] ?? ['frame_ms_median', 'median frame, ms'];

/** FNV-1a, 32 bit. */
function fnv(text) {
  let hash = 0x811c9dc5;
  for (const ch of Buffer.from(text, 'utf8')) {
    hash ^= ch;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash;
}
const isTest = (name) => fnv(name) % 3 === 0;

/** The middle value. */
function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted.length ? sorted[Math.floor(sorted.length / 2)] : NaN;
}

const rows = readFileSync(results, 'utf8').trim().split('\n').map((line) => JSON.parse(line));
const cases = [...new Set(rows.map((r) => r.prompt_id))];
const medianOf = (label, name) => median(rows.filter((r) => r.variant === label && r.prompt_id === name).map((r) => r[primaryOf(name)[0]]).filter((v) => Number.isFinite(v)));

mkdirSync(flow, { recursive: true });
const variants = [base, ...candidates];
variants.forEach((label, index) => {
  const dir = join(flow, index === 0 ? 'baseline' : `v${index}`);
  mkdirSync(dir, { recursive: true });
  const lines = [];
  for (const row of rows.filter((r) => r.variant === label)) {
    const [field] = primaryOf(row.prompt_id);
    const baseline = medianOf(base, row.prompt_id);
    const value = row[field];
    const ratio = Number.isFinite(value) && value > 0 ? baseline / value : null;
    lines.push(JSON.stringify({
      prompt_id: row.prompt_id,
      rep: row.rep,
      tags: [isTest(row.prompt_id) ? 'test' : 'train', row.prompt_id],
      model: 'n/a',
      prompt: `${row.prompt_id}: ${primaryOf(row.prompt_id)[1]}`,
      grade: { speedup: ratio, value },
      meta: row,
    }));
  }
  writeFileSync(join(dir, 'results.jsonl'), `${lines.join('\n')}\n`);
  writeFileSync(join(dir, 'summary.json'), JSON.stringify({ description: index === 0 ? `${base} (0.88.0)` : label, target: 'code' }, null, 2));
});
writeFileSync(join(flow, '_state.json'), JSON.stringify({
  current_round: candidates.length,
  reps: 3,
  goal: { target: 'speedup', direction: 'higher', hold: [] },
  approve_each_round: false,
  train_ids: cases.filter((c) => !isTest(c)),
  test_ids: cases.filter(isTest),
  metrics: [{ id: 'speedup', kind: 'float', label: 'Baseline median / value (higher is better)' }, { id: 'value', kind: 'float', better: 'lower' }],
}, null, 2));
writeFileSync(join(flow, 'metrics.md'), `# Metrics\n\nEach case is scored by one number, lower being better. \`speedup\` is the baseline's median over its rounds divided by this round's value, so 2.0 is twice as good.\n\n${cases.map((c) => `- \`${c}\` (${isTest(c) ? 'test' : 'train'}): ${primaryOf(c)[1]}`).join('\n')}\n`);

// The table: per case medians and the ratio, then the geometric mean of the ratios per split.
const header = ['case', 'split', 'measure', ...variants.flatMap((v, i) => (i === 0 ? [v] : [v, 'x']))];
const table = [header, header.map(() => '---')];
const logs = Object.fromEntries(candidates.map((c) => [c, { train: [], test: [] }]));
for (const name of cases) {
  const b = medianOf(base, name);
  const row = [name, isTest(name) ? 'test' : 'train', primaryOf(name)[1], b.toFixed(1)];
  for (const c of candidates) {
    const v = medianOf(c, name);
    const ratio = b / v;
    row.push(v.toFixed(1), `${ratio.toFixed(2)}x`);
    if (Number.isFinite(ratio) && ratio > 0) logs[c][isTest(name) ? 'test' : 'train'].push(Math.log(ratio));
  }
  table.push(row);
}
const geo = (list) => (list.length ? Math.exp(list.reduce((a, b) => a + b, 0) / list.length) : NaN);
console.log(table.map((r) => `| ${r.join(' | ')} |`).join('\n'));
for (const c of candidates) console.log(`\n${c}: geometric mean of the ratios, train ${geo(logs[c].train).toFixed(2)}x, test ${geo(logs[c].test).toFixed(2)}x`);
