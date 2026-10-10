#!/usr/bin/env node
// Scores completion evaluation runs (`task-2231` §8.1): Recall@1, Recall@5, Recall@10, MRR, miss rate,
// list length and latency, per engine, language, class and prefix length, read from each run's
// results.jsonl and nothing else.
//
// With two or more runs it also writes the per position table of every query where the first run
// (the reference, normally IntelliJ) ranked the expected name higher than another run did, because that
// table is where the fixes come from.
//
//   node tools/completion-eval/score.mjs --run <reference run> --run <other run> [--split held|tune|all]
//        [--out <file.md>]

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { EVAL_ROOT } from './prepare-corpora.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const CLASSES = ['member', 'path', 'local', 'global', 'type', 'import', 'needs-import', 'keyword'];

/**
 * The name a label stands for: the part a person types, without a signature, generic arguments, a
 * macro's bang, call parentheses or a trailing detail an engine added.
 * @param label - the label as an engine wrote it
 */
export function normalise(label) {
  let name = String(label).trim();
  for (const stop of ['(', '<', ' ', '!', ':', '?']) {
    const at = name.indexOf(stop);
    if (at > 0) name = name.slice(0, at);
  }
  return name;
}

/** The 1 based rank of the expected name in a list of labels, or 0 when it is absent. */
export function rankOf(labels, expected) {
  const at = labels.findIndex((label) => normalise(label) === expected);
  return at < 0 ? 0 : at + 1;
}

/** One run's results, keyed `id#prefix`, and its run.json. */
function readRun(name) {
  const folder = path.join(EVAL_ROOT, 'runs', name);
  const results = new Map();
  for (const line of fs.readFileSync(path.join(folder, 'results.jsonl'), 'utf8').split('\n').filter(Boolean)) {
    const row = JSON.parse(line);
    results.set(`${row.id}#${row.prefix}`, row);
  }
  const info = fs.existsSync(path.join(folder, 'run.json')) ? JSON.parse(fs.readFileSync(path.join(folder, 'run.json'), 'utf8')) : {};
  return { name, results, info };
}

/** The value at a fraction of a sorted list. */
function quantile(sorted, fraction) {
  if (!sorted.length) return null;
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * fraction))];
}

/** Metrics over a list of ranked queries. */
function metrics(rows) {
  const n = rows.length;
  if (!n) return null;
  const ranks = rows.map((r) => r.rank);
  const ms = rows.map((r) => r.ms).filter((m) => typeof m === 'number').sort((a, b) => a - b);
  const within = (k) => ranks.filter((r) => r > 0 && r <= k).length / n;
  return {
    n,
    r1: within(1),
    r5: within(5),
    r10: within(10),
    mrr: ranks.reduce((sum, r) => sum + (r > 0 ? 1 / r : 0), 0) / n,
    miss: ranks.filter((r) => r === 0).length / n,
    length: rows.reduce((sum, r) => sum + r.length, 0) / n,
    p50: quantile(ms, 0.5),
    p95: quantile(ms, 0.95),
    errors: rows.filter((r) => r.error).length,
  };
}

/**
 * The scored queries of a run: one row a (position, prefix) the run answered, restricted to the given
 * positions and to the queries every compared run answered.
 */
function ranked(run, positions, keys) {
  const out = [];
  for (const key of keys) {
    const row = run.results.get(key);
    const position = positions.get(key.split('#')[0]);
    out.push({
      key,
      position,
      prefix: Number(key.split('#')[1]),
      rank: rankOf(row.labels || [], position.expected),
      length: (row.labels || []).length,
      ms: row.ms,
      error: row.error,
    });
  }
  return out;
}

/** A percentage with one decimal. */
const pct = (v) => (v === null || v === undefined ? '' : (100 * v).toFixed(1));
const num = (v, d = 3) => (v === null || v === undefined ? '' : v.toFixed(d));

/** Runs from the command line. */
function main() {
  const argv = process.argv.slice(2);
  const runs = [];
  let split = 'all';
  let out = null;
  let positionsPath = path.join(HERE, 'positions.json');
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--run') runs.push(argv[++i]);
    else if (argv[i] === '--split') split = argv[++i];
    else if (argv[i] === '--out') out = argv[++i];
    else if (argv[i] === '--positions') positionsPath = argv[++i];
  }
  if (!runs.length) {
    console.error('usage: score.mjs --run <name> [--run <name> ...] [--split held|tune|all] [--out <file.md>]');
    process.exit(2);
  }
  const frozen = JSON.parse(fs.readFileSync(positionsPath, 'utf8'));
  const positions = new Map(frozen.positions.map((p) => [p.id, p]));
  const loaded = runs.map(readRun);
  // The queries every run answered, inside the split asked for.
  let keys = [...loaded[0].results.keys()];
  for (const run of loaded.slice(1)) keys = keys.filter((k) => run.results.has(k));
  keys = keys.filter((k) => {
    const position = positions.get(k.split('#')[0]);
    return position && (split === 'all' || position.split === split);
  });
  const scored = loaded.map((run) => ({ run, rows: ranked(run, positions, keys) }));
  const lines = [];
  lines.push(`# Completion evaluation: ${runs.join(' vs ')}`, '');
  lines.push(`Split: ${split}. ${keys.length} queries answered by every run. Positions sha256 ${loaded[0].info.positionsSha256 || 'unknown'}.`, '');
  const summary = { runs, split, queries: keys.length, by: {} };
  const languages = [...new Set(keys.map((k) => positions.get(k.split('#')[0]).language))];
  for (const language of languages) {
    lines.push(`## ${language}`, '');
    lines.push('| Class | Run | n | R@1 | R@5 | R@10 | MRR | Miss | Len | p50 ms | p95 ms |');
    lines.push('|---|---|---|---|---|---|---|---|---|---|---|');
    for (const cls of [...CLASSES, 'all']) {
      for (const { run, rows } of scored) {
        const picked = rows.filter((r) => r.position.language === language && (cls === 'all' || r.position.class === cls));
        const m = metrics(picked);
        if (!m) continue;
        summary.by[`${language}|${cls}|${run.name}`] = m;
        lines.push(`| ${cls} | ${run.name} | ${m.n} | ${pct(m.r1)} | ${pct(m.r5)} | ${pct(m.r10)} | ${num(m.mrr)} | ${pct(m.miss)} | ${num(m.length, 1)} | ${num(m.p50, 1)} | ${num(m.p95, 1)} |`);
      }
    }
    lines.push('', `### ${language} by prefix length`, '');
    lines.push('| Prefix | Run | n | R@1 | R@5 | MRR | Miss |');
    lines.push('|---|---|---|---|---|---|---|');
    for (const prefix of [0, 1, 2, 3]) {
      for (const { run, rows } of scored) {
        const m = metrics(rows.filter((r) => r.position.language === language && r.prefix === prefix));
        if (!m) continue;
        summary.by[`${language}|prefix${prefix}|${run.name}`] = m;
        lines.push(`| ${prefix} | ${run.name} | ${m.n} | ${pct(m.r1)} | ${pct(m.r5)} | ${num(m.mrr)} | ${pct(m.miss)} |`);
      }
    }
    lines.push('');
  }
  if (scored.length > 1) {
    const reference = scored[0];
    for (const other of scored.slice(1)) {
      const worse = [];
      for (let i = 0; i < keys.length; i++) {
        const a = reference.rows[i];
        const b = other.rows[i];
        const better = a.rank > 0 && (b.rank === 0 || a.rank < b.rank);
        if (better) worse.push({ key: a.key, ...a.position, prefix: a.prefix, reference: a.rank, other: b.rank });
      }
      const table = path.join(EVAL_ROOT, 'runs', other.run.name, `worse-than-${reference.run.name}-${split}.jsonl`);
      fs.writeFileSync(table, worse.map((w) => JSON.stringify(w)).join('\n'));
      lines.push(`## Where ${reference.run.name} ranked the expected name higher than ${other.run.name}`, '');
      lines.push(`${worse.length} of ${keys.length} queries. Every row is in \`${table}\`. The first 40:`, '');
      lines.push(`| Position | Class | Prefix | Expected | ${reference.run.name} | ${other.run.name} |`, '|---|---|---|---|---|---|');
      for (const w of worse.slice(0, 40)) {
        lines.push(`| ${w.id} \`${w.path}\` | ${w.class} | ${w.prefix} | \`${w.expected}\` | ${w.reference} | ${w.other || 'absent'} |`);
      }
      lines.push('');
    }
  }
  const text = lines.join('\n');
  if (out) fs.writeFileSync(out, text);
  fs.mkdirSync(path.join(HERE, 'runs'), { recursive: true });
  fs.writeFileSync(path.join(HERE, 'runs', `${runs.join('__vs__')}-${split}.json`), JSON.stringify(summary, null, 1));
  console.log(text);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
