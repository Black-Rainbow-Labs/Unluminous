// The tool level run of the families written in plain English (TDD section 7.2): F5 localisation, F6
// concept and F8 unanswerable, through the index's passage search over MCP.
//
//   node tools/search-eval/run-meaning.mjs --split dev [--families F5,F6,F8] [--corpora a,b] [--replay <agent run>]
//
// ripgrep has no single call that answers a question in English, so its arm for these families is the
// replay of the Grep calls a baseline agent really made for the same question (`--replay` names that
// agent run). Without one, only the index arm is scored.

import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, HERE, REPO, evalRoot, snapshotDir } from './lib/config.mjs';
import { cloneAtCommit } from './lib/corpora.mjs';
import { openServer } from './lib/index-arm.mjs';
import { grepArgs, readJsonMatches, runRgSync } from './lib/rg.mjs';
import { scoreConcept, scoreLocalisation, scoreUnanswerable } from './lib/score.mjs';
import { mean, median } from './lib/stats.mjs';

const QUERIES = process.env.SEARCH_EVAL_QUERIES || path.join(HERE, 'queries');
const FILES = { F5: 'F5-localisation.jsonl', F6: 'F6-concept.jsonl', F8: 'F8-unanswerable.jsonl' };

/**
 * Reads the command line.
 * @param argv - the arguments after the script
 */
function options(argv) {
  const get = (name, fallback) => { const i = argv.indexOf(`--${name}`); return i >= 0 ? argv[i + 1] : fallback; };
  return { split: get('split', 'dev'), families: get('families', 'F5,F6,F8').split(','), corpora: get('corpora', 'unluminous,ai-service,inillucent').split(','), replay: get('replay', ''), label: get('label', ''), budget: Number(get('budget', 1500)) };
}

/**
 * Refuses a run whose frozen sets changed, as run.mjs does.
 */
function verifySets() {
  const manifest = JSON.parse(fs.readFileSync(path.join(QUERIES, 'manifest.json'), 'utf8'));
  for (const set of Object.values(manifest.sets)) {
    const sha = crypto.createHash('sha256').update(fs.readFileSync(path.join(QUERIES, set.file))).digest('hex');
    if (sha !== set.sha256) { console.error(`FROZEN SET CHANGED: ${set.file}. Nothing was run.`); process.exit(3); }
  }
}

/**
 * The folder a query is answered in: an F5 ticket's parent commit, otherwise the corpus snapshot.
 * @param query - the query
 */
function folderOf(query) {
  const corpus = CORPORA.find((c) => c.name === query.repo);
  if (query.family !== 'F5') return snapshotDir(corpus);
  const dir = path.join(evalRoot(), 'checkouts', `${query.repo}@${query.parent.slice(0, 12)}`);
  if (!fs.existsSync(path.join(dir, '.snapshot-ready'))) {
    cloneAtCommit(corpus.source, query.parent, dir);
    fs.writeFileSync(path.join(dir, '.snapshot-ready'), query.parent);
    fs.appendFileSync(path.join(dir, '.git', 'info', 'exclude'), '\n.snapshot-ready\n');
  }
  return dir;
}

/**
 * The question an F5, F6 or F8 query asks, as the index is asked it.
 * @param query - the query
 */
function questionOf(query) {
  if (query.family === 'F5') return `${query.title}\n${query.description}`.slice(0, 1200);
  return query.question;
}

/**
 * The index arm's answer: passage search over MCP, files and hits in its order.
 * @param server - the MCP server for the folder
 * @param query - the query
 * @param budget - the token budget
 */
async function indexAnswer(server, query, budget) {
  const { message, ms } = await server.call('find', { query: questionOf(query), mode: 'semantic', budget, structured: true });
  const value = message.result?.structuredContent || {};
  const hits = (value.hits || []).map(([p, start, header, end]) => ({ path: p, line: start, end, header }));
  return { ms, hits, files: value.files || [...new Set(hits.map((h) => h.path))], empty: hits.length === 0, text: message.result?.content?.map((c) => c.text).join('\n') || '' };
}

/**
 * The rg arm's answer, replayed: the Grep calls the baseline agent made for this query, run again
 * against the folder, their files in the order the calls found them.
 * @param replay - the agent run's folder
 * @param query - the query
 * @param dir - the folder
 */
function rgReplay(replay, query, dir) {
  const file = path.join(evalRoot(), 'runs', replay, 'transcripts', `${query.id.replace(/[:/]/g, '_')}-rg-1.jsonl`);
  if (!fs.existsSync(file)) return null;
  const calls = fs.readFileSync(file, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l)).flatMap((e) => (e.type === 'assistant' ? e.message?.content || [] : [])).filter((c) => c.type === 'tool_use' && c.name === 'Grep');
  const hits = [];
  for (const call of calls) {
    const scope = call.input.path ? path.relative(dir, path.resolve(dir, call.input.path)).replace(/\\/g, '/') : '';
    const q = { pattern: call.input.pattern, ignoreCase: !!call.input['-i'], path: scope, globs: call.input.glob ? [call.input.glob] : [], types: call.input.type ? [call.input.type] : [] };
    const r = runRgSync(grepArgs(q), dir);
    hits.push(...readJsonMatches(r.stdout));
  }
  return { hits, files: [...new Set(hits.map((h) => h.path))], empty: hits.length === 0, calls: calls.length };
}

/**
 * Scores an answer with the family's primary metric.
 * @param query - the query
 * @param answer - the answer
 */
function scoreOf(query, answer) {
  if (query.family === 'F5') return scoreLocalisation(answer, query.gold);
  if (query.family === 'F6') return scoreConcept({ hits: answer.hits.map((h) => ({ ...h, line: h.end && h.line < query.gold.start && h.end >= query.gold.start ? query.gold.start : h.line })) }, query.gold);
  return scoreUnanswerable(answer);
}

const opts = options(process.argv.slice(2));
verifySets();
const sha = spawnSync('git', ['-C', REPO, 'rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).stdout.trim();
const runId = `meaning-${Math.floor(Date.now() / 1000)}-${sha}`;
const out = path.join(evalRoot(), 'runs', runId);
fs.mkdirSync(out, { recursive: true });
const servers = new Map();
const rows = [];
for (const family of opts.families) {
  const set = fs.readFileSync(path.join(QUERIES, FILES[family]), 'utf8').trim().split('\n').map((l) => JSON.parse(l)).filter((q) => q.split === opts.split && opts.corpora.includes(q.repo));
  for (const query of set) {
    const dir = folderOf(query);
    if (!servers.has(dir)) servers.set(dir, await openServer(dir, true));
    const ix = await indexAnswer(servers.get(dir), query, opts.budget);
    const row = { id: query.id, family, repo: query.repo, arms: { 'index-mcp': { ms: ix.ms, score: scoreOf(query, ix), files: ix.files.slice(0, 5), empty: ix.empty, tokens: Math.ceil(ix.text.length / 3.6) } } };
    if (opts.replay) {
      const rg = rgReplay(opts.replay, query, dir);
      if (rg) row.arms.rg = { score: scoreOf(query, rg), files: rg.files.slice(0, 5), empty: rg.empty, calls: rg.calls };
    }
    rows.push(row);
    if (servers.size > 6) { const [first] = servers.keys(); servers.get(first).close(); servers.delete(first); }
  }
  process.stdout.write(`${family}: ${rows.filter((r) => r.family === family).length} queries\n`);
}
for (const s of servers.values()) s.close();
fs.writeFileSync(path.join(out, opts.split === 'heldout' ? 'per-query.sealed.jsonl' : 'per-query.jsonl'), rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
const table = [];
for (const family of opts.families) for (const corpus of opts.corpora) {
  const mine = rows.filter((r) => r.family === family && r.repo === corpus);
  if (!mine.length) continue;
  const arm = (a) => { const xs = mine.filter((r) => r.arms[a]); return xs.length ? { n: xs.length, metric: +mean(xs.map((r) => r.arms[a].score.primary)).toFixed(4), medianMs: a === 'index-mcp' ? +median(xs.map((r) => r.arms[a].ms)).toFixed(2) : null } : null; };
  table.push({ family, corpus, index: arm('index-mcp'), rg: arm('rg') });
}
const summary = { runId, label: opts.label, split: opts.split, replay: opts.replay || null, at: new Date().toISOString(), unluminousSha: sha, table };
fs.writeFileSync(path.join(out, 'summary.json'), JSON.stringify(summary, null, 2));
for (const t of table) console.log(`${t.family} ${t.corpus}: index ${t.index?.metric} (${t.index?.n}, ${t.index?.medianMs} ms)${t.rg ? `, rg replay ${t.rg.metric} (${t.rg.n})` : ''}`);
console.log(`run folder: ${out}`);
