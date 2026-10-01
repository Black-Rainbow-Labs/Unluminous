// Publishes the study to the ai-service Labs page (TDD section 7.7, R12): reads the run folders the
// loop's record names and writes `page.json` beside the `findings.json` written by hand.
//
//   node tools/search-eval/publish.mjs
//
// `<eval root>/results.json` is the record: which run is the rg baseline, which tool level and agent
// level runs are current, the freshness runs, and the progression. Every number on the page is read out
// of a run folder here; the page itself holds none.

import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, GOALS, HERE, evalRoot } from './lib/config.mjs';
import { familySummary, speed } from './lib/scorecard.mjs';

const PUBLISHED = process.env.SEARCH_EVAL_PUBLISH || 'C:/jason/dev/ai-service/_supervised-learning/unluminous-code-index-study/published';
const METRIC = { F1: 'definition at rank 1', F2: 'exact set equality', F3: 'recall of uses', F4: 'file in top 3', F5: 'file recall at 5', F6: 'nDCG at 10', F8: 'correct abstention' };

/**
 * Reads a run folder's rows and manifest.
 * @param runId - the run's folder name
 */
function readRun(runId) {
  const dir = path.join(evalRoot(), 'runs', runId);
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, 'manifest.json'), 'utf8'));
  const file = fs.existsSync(path.join(dir, 'per-query.jsonl')) ? 'per-query.jsonl' : 'per-query.sealed.jsonl';
  const rows = fs.readFileSync(path.join(dir, file), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  return { manifest, rows };
}

/**
 * The per family table of a run, both arms beside each other.
 * @param run - a run with an rg arm and one index arm
 */
function familiesOf(run) {
  const index = run.manifest.arms.find((a) => a !== 'rg');
  const groups = new Map();
  for (const r of run.rows) {
    const key = `${r.repo}|${r.family}`;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(r);
  }
  return [...groups.entries()].sort().map(([key, rows]) => {
    const [corpus, family] = key.split('|');
    const s = familySummary(rows, run.manifest.arms);
    const pick = (a) => a && { metric: +s[a].primary.toFixed(4), medianMs: +(s[a].medianMs ?? 0).toFixed(2), tokens: Math.round(s[a].tokens), failures: s[a].failures };
    const rg = pick('rg'), ix = pick(index);
    const verdict = !ix ? null : ix.metric > rg.metric + 1e-9 ? 'better' : ix.metric < rg.metric - 1e-9 ? 'worse' : 'equal';
    return { corpus, family, n: s.n, metric: METRIC[family], rg, index: ix, verdict };
  });
}

/**
 * The G1 speed table of a run, one row a corpus.
 * @param run - a run with an rg arm and one index arm
 */
function speedOf(run) {
  const index = run.manifest.arms.find((a) => a !== 'rg');
  const out = [];
  for (const corpus of CORPORA.map((c) => c.name)) {
    const s = speed(run.rows.filter((r) => r.repo === corpus), index);
    if (s) out.push({ corpus, n: s.n, rgMedianMs: +s.rgMedianMs.toFixed(2), indexMedianMs: +s.indexMedianMs.toFixed(2), ratio: +s.point.toFixed(2), lo: +s.lo.toFixed(2), hi: +s.hi.toFixed(2), pass: s.pass });
  }
  return out;
}

/**
 * The G1 goal row from the speed table: the medium and large corpora decide it (TDD section 2.1).
 * @param speedRows - the speed table
 * @param split - the split it was measured on
 */
function speedGoal(speedRows, split) {
  const deciding = speedRows.filter((r) => CORPORA.find((c) => c.name === r.corpus)?.tier !== 'small');
  const worst = deciding.length ? deciding.reduce((a, b) => (a.lo < b.lo ? a : b)) : null;
  return {
    id: 'G1', name: 'Speed', split,
    definition: 'Geometric mean of paired per query speed ratios on F1 to F4, the index over MCP against the rg inside claude.exe with the Grep tool\'s flags, both pinned to the performance cores, warm cache.',
    bar: `at least ${GOALS.speed.toFixed(1)}x with the lower end of the 95% interval at least ${GOALS.speed.toFixed(1)}x, on the medium and the large corpus`,
    point: worst?.ratio ?? null, lo: worst?.lo ?? null, hi: worst?.hi ?? null,
    pass: deciding.length ? deciding.every((r) => r.pass) : null,
    absolutes: worst ? { corpus: worst.corpus, rgMedianMs: worst.rgMedianMs, indexMedianMs: worst.indexMedianMs } : {},
  };
}

const record = JSON.parse(fs.readFileSync(path.join(evalRoot(), 'results.json'), 'utf8'));
const manifest = JSON.parse(fs.readFileSync(path.join(HERE, 'queries', 'manifest.json'), 'utf8'));
const baseline = record.baselineRun ? familiesOf({ ...readRun(record.baselineRun), manifest: { ...readRun(record.baselineRun).manifest, arms: ['rg'] } }).map((f) => ({ corpus: f.corpus, family: f.family, n: f.n, metric: f.rg.metric, medianMs: f.rg.medianMs, tokens: f.rg.tokens })) : [];
const tool = record.toolRun ? readRun(record.toolRun) : null;
const speedRows = tool ? speedOf(tool) : [];
const page = {
  sample: false,
  study: {
    title: 'Unluminous code index against ripgrep',
    subtitle: 'A local code index on Inillucent, measured against the ripgrep Claude Code runs on frozen query sets, until three goals pass on a held out set.',
    updatedAt: new Date().toISOString(), unluminousSha: tool?.manifest.unluminousSha ?? null, ticket: 'task-2139',
    corpora: CORPORA.map((c) => ({ name: c.name, tier: c.tier, sha: (c.sha || c.ref).slice(0, 12) })),
  },
  goals: [speedGoal(speedRows, tool?.manifest.split ?? 'dev'), ...(record.goals || [])],
  speed: speedRows,
  families: tool ? familiesOf(tool) : [],
  baseline,
  agent: record.agent || [],
  freshness: record.freshness || [],
  progression: record.progression || [],
  querySets: Object.entries(manifest.sets).map(([family, s]) => ({ family, name: s.file.replace(/^F\d-|\.jsonl$/g, ''), count: s.count, heldout: s.heldout, metric: METRIC[family], sha256: s.sha256.slice(0, 12) })),
  weights: manifest.weights,
};
fs.mkdirSync(PUBLISHED, { recursive: true });
fs.writeFileSync(path.join(PUBLISHED, 'page.json'), JSON.stringify(page, null, 2));
const findings = path.join(evalRoot(), 'findings.json');
if (fs.existsSync(findings)) fs.copyFileSync(findings, path.join(PUBLISHED, 'findings.json'));
console.log(`published to ${PUBLISHED}`);
