// The tool level run of TDD section 7.4: every frozen query of the chosen families, on every corpus,
// through every arm, with the rules of section 7.6 enforced.
//
//   node tools/search-eval/run.mjs --arms rg --split dev --families F1,F2,F3,F4 [--corpora a,b]
//        [--reps 5] [--warmup 1] [--label text] [--record-quiet] [--no-quiet-check]
//
// Exit 4 is "NOT GRADED": the rg arm's reference queries ran more than 3% slower than the quiet
// reference, so the box was busier than when the reference was taken and no ratio from this run is
// believed.

import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, HERE, REPO, evalRoot, snapshotDir } from './lib/config.mjs';
import { digestOf, rgArm } from './lib/arms.mjs';
import { busierThanReference, machine, pinToPerformanceCores, writeQuietReference } from './lib/machine.mjs';
import { approxTokens, scoreExact, scoreFiles, scoreIdentifier, scoreReferences } from './lib/score.mjs';
import { scorecardMarkdown } from './lib/scorecard.mjs';
import { median } from './lib/stats.mjs';

// `SEARCH_EVAL_QUERIES` points the run at another copy of the sets, which is how the harness's own
// tests show that a changed set is refused (test-harness.mjs). A measured run never sets it.
const QUERIES = process.env.SEARCH_EVAL_QUERIES || path.join(HERE, 'queries');
const FILES = { F1: 'F1-identifier.jsonl', F2: 'F2-exact.jsonl', F3: 'F3-references.jsonl', F4: 'F4-files.jsonl' };

/**
 * Reads the command line into options.
 * @param argv - the arguments after the script
 */
function options(argv) {
  const get = (name, fallback) => { const i = argv.indexOf(`--${name}`); return i >= 0 ? argv[i + 1] : fallback; };
  return {
    arms: get('arms', 'rg').split(','), split: get('split', 'dev'), families: get('families', 'F1,F2,F3,F4').split(','),
    corpora: get('corpora', CORPORA.map((c) => c.name).join(',')).split(','), reps: Number(get('reps', 5)), warmup: Number(get('warmup', 1)),
    label: get('label', ''), recordQuiet: argv.includes('--record-quiet'), quietCheck: !argv.includes('--no-quiet-check'),
    max: Number(get('max', 0)),
  };
}

/**
 * Checks every frozen set against the manifest's hash and refuses to run when one has changed
 * (TDD section 8.5).
 */
function verifiedManifest() {
  const manifest = JSON.parse(fs.readFileSync(path.join(QUERIES, 'manifest.json'), 'utf8'));
  for (const set of Object.values(manifest.sets)) {
    const sha = crypto.createHash('sha256').update(fs.readFileSync(path.join(QUERIES, set.file))).digest('hex');
    if (sha !== set.sha256) { console.error(`FROZEN SET CHANGED: ${set.file} no longer matches the manifest. Nothing was run.`); process.exit(3); }
  }
  return manifest;
}

/**
 * Loads the arms named on the command line.
 * @param names - arm names
 */
async function loadArms(names) {
  const arms = [];
  for (const name of names) {
    if (name === 'rg') arms.push(rgArm);
    // An arm that fails every call, for the harness's own test that a crashed arm is a failure row.
    else if (name === 'crash') arms.push({ name, async time() { throw new Error('the arm crashed'); }, async order() { throw new Error('the arm crashed'); } });
    else arms.push((await import('./lib/index-arm.mjs')).indexArm(name));
  }
  return arms;
}

/**
 * Scores one arm's answer to one query with the family's primary metric.
 * @param query - the frozen query
 * @param answer - the arm's answer
 */
function score(query, answer) {
  if (query.family === 'F1') return scoreIdentifier(answer, query.gold);
  if (query.family === 'F2') return scoreExact(digestOf(answer.hits), query.gold);
  if (query.family === 'F3') return scoreReferences(answer, query.gold);
  return scoreFiles(answer, query.gold);
}

/**
 * Runs one query through one arm: an untimed ordered answer for the metric, warm ups, then the timed
 * repetitions. For F2 a timing counts only when the timed answer's digest is the gold digest; a
 * mismatch is a failure row (TDD section 7.6).
 * @param arm - the arm
 * @param query - the frozen query
 * @param dir - the corpus snapshot
 * @param opts - the run options
 */
async function runOne(arm, query, dir, opts) {
  try {
    const ordered = await arm.order(query, dir);
    for (let i = 0; i < opts.warmup; i++) await arm.time(query, dir);
    const times = [];
    let failure = null;
    let status = null;
    for (let i = 0; i < opts.reps; i++) {
      const t = await arm.time(query, dir);
      status = t.status ?? null;
      if (query.family === 'F2' && digestOf(t.answer.hits) !== query.gold.digest) failure = `result digest ${digestOf(t.answer.hits)} is not the gold ${query.gold.digest}`;
      times.push(t.ms);
    }
    return { ms: median(times), times, score: score(query, ordered), tokens: approxTokens(ordered.text), hits: ordered.hits.length, files: ordered.files.length, failure, status };
  } catch (error) {
    return { failure: String(error.message || error), score: { primary: 0 } };
  }
}

/**
 * An F2 pattern ripgrep refuses (exit 2 with no lines, such as `
` without `--multiline`) and the
 * index also refuses is the same outcome: both answer with no lines. The frozen gold for such a query
 * is the empty set, so the index's refusal is scored as equal to ripgrep's rather than as a failure.
 * It applies only when the rg arm refused in this same run.
 * @param row - one query's row, changed in place
 */
function bothRefused(row) {
  if (row.family !== 'F2' || !row.arms.rg) return;
  const rg = row.arms.rg;
  if (rg.status !== 2 || rg.hits !== 0) return;
  for (const [name, arm] of Object.entries(row.arms)) {
    if (name === 'rg' || !arm.failure || !/refused|not allowed|parse/i.test(arm.failure)) continue;
    row.arms[name] = { ...arm, failure: null, score: { primary: 1 }, note: `both refused the pattern: ${arm.failure}` };
  }
}

/**
 * Times the rg arm on the reference queries (the first 20 dev F2 queries of ai-service) and returns
 * their median, which is compared with the quiet reference.
 * @param queries - every F2 query
 */
async function referenceMedian(queries) {
  const corpus = CORPORA.find((c) => c.name === 'ai-service');
  const reference = queries.filter((q) => q.repo === 'ai-service' && q.split === 'dev').slice(0, 20);
  const times = [];
  for (const q of reference) for (let i = 0; i < 3; i++) times.push((await rgArm.time(q, snapshotDir(corpus))).ms);
  return median(times);
}

const opts = options(process.argv.slice(2));
const manifest = verifiedManifest();
const affinity = pinToPerformanceCores();
const sets = Object.fromEntries(Object.entries(FILES).map(([f, file]) => [f, fs.readFileSync(path.join(QUERIES, file), 'utf8').trim().split('\n').map((l) => JSON.parse(l))]));
const quiet = await referenceMedian(sets.F2);
if (opts.recordQuiet) writeQuietReference(quiet, '20 dev F2 queries on ai-service, rg arm, 3 repetitions each');
const busy = busierThanReference(quiet);
if (opts.quietCheck && busy.busier) {
  console.error(`NOT GRADED: the rg arm's reference queries took ${quiet.toFixed(1)} ms against a quiet reference of ${busy.reference.toFixed(1)} ms (${((busy.ratio - 1) * 100).toFixed(1)}% slower).`);
  process.exit(4);
}
const arms = await loadArms(opts.arms);
const sha = execFileSync('git', ['-C', REPO, 'rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).trim();
const runId = `${Math.floor(Date.now() / 1000)}-${sha}`;
const dir = path.join(evalRoot(), 'runs', runId);
fs.mkdirSync(dir, { recursive: true });
const rows = [];
for (const corpusName of opts.corpora) {
  const corpus = CORPORA.find((c) => c.name === corpusName);
  for (const family of opts.families) {
    const chosen = sets[family].filter((q) => q.repo === corpusName && q.split === opts.split);
    // --max takes the first N of each family, for a smoke run; its scorecard says so in the manifest.
    for (const query of opts.max ? chosen.slice(0, opts.max) : chosen) {
      const row = { id: query.id, family, repo: corpusName, arms: {} };
      for (const arm of arms) row.arms[arm.name] = await runOne(arm, query, snapshotDir(corpus), opts);
      bothRefused(row);
      rows.push(row);
    }
    process.stdout.write(`${corpusName} ${family}: ${rows.filter((r) => r.repo === corpusName && r.family === family).length} queries\n`);
  }
}
for (const arm of arms) await arm.close?.();
const runManifest = { runId, label: opts.label, split: opts.split, at: new Date().toISOString(), unluminousSha: sha, arms: opts.arms, families: opts.families, corpora: opts.corpora, reps: opts.reps, warmup: opts.warmup,
  max: opts.max || null, querySets: manifest.sets, corporaShas: manifest.corpora, machine: machine(), affinity, quietReference: { thisRunMs: quiet, referenceMs: busy.reference, ratio: busy.ratio } };
fs.writeFileSync(path.join(dir, 'manifest.json'), JSON.stringify(runManifest, null, 2));
// Held out rows are kept sealed: written for the record, never printed (TDD section 8.1).
fs.writeFileSync(path.join(dir, opts.split === 'heldout' ? 'per-query.sealed.jsonl' : 'per-query.jsonl'), rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
const card = scorecardMarkdown(rows, opts.arms, runManifest, manifest.weights);
fs.writeFileSync(path.join(dir, 'scorecard.md'), card);
console.log(card);
console.log(`run folder: ${dir}`);
