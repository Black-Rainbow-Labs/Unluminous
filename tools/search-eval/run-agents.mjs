// The agent level run of TDD section 7.5: real Claude Code sessions doing F5 and F6 tasks, one arm with
// the Grep, Glob and Read tools and one with the code index's MCP tool and Read, same model, same
// instructions apart from the tool names, graded against gold.
//
//   node tools/search-eval/run-agents.mjs --split dev|heldout --model haiku|sonnet|opus [--draws 1]
//        [--arms rg,index] [--parallel 3] [--label text] [--freeze]
//
// `--freeze` writes queries/agent-manifest.json, the hash of rubrics.json, once, before the first agent
// run; every later run refuses to start when the rubric no longer matches it. Usage is read only through
// ai-service's claudeUsageCache.cjs and no session starts above 80% of the weekly limit (lib/agent.mjs).

import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, HERE, REPO, SEED, evalRoot, snapshotDir } from './lib/config.mjs';
import { cloneAtCommit } from './lib/corpora.mjs';
import { lastJson, runClaude } from './lib/agent.mjs';
import { CLI } from './lib/index-arm.mjs';
import { bootstrap, geomean, mean, median, seeded } from './lib/stats.mjs';

const QUERIES = path.join(HERE, 'queries');
const RUBRICS = path.join(HERE, 'rubrics.json');
const AGENT_MANIFEST = path.join(QUERIES, 'agent-manifest.json');

/**
 * Reads the command line.
 * @param argv - the arguments after the script
 */
function options(argv) {
  const get = (name, fallback) => { const i = argv.indexOf(`--${name}`); return i >= 0 ? argv[i + 1] : fallback; };
  return { split: get('split', 'dev'), model: get('model', 'haiku'), draws: Number(get('draws', 1)), arms: get('arms', 'rg,index').split(','),
    parallel: Number(get('parallel', 3)), label: get('label', ''), freeze: argv.includes('--freeze'), only: get('only', ''), limit: Number(get('limit', 0)) };
}

/**
 * Hashes the rubric into the agent manifest when asked to, and otherwise refuses a rubric that changed.
 * @param freeze - whether to write the manifest
 */
function verifiedRubric(freeze) {
  const sha = crypto.createHash('sha256').update(fs.readFileSync(RUBRICS)).digest('hex');
  if (freeze && !fs.existsSync(AGENT_MANIFEST)) fs.writeFileSync(AGENT_MANIFEST, JSON.stringify({ frozenAt: new Date().toISOString(), rubricSha256: sha }, null, 2) + '\n');
  const manifest = JSON.parse(fs.readFileSync(AGENT_MANIFEST, 'utf8'));
  if (manifest.rubricSha256 !== sha) { console.error('RUBRIC CHANGED: rubrics.json no longer matches agent-manifest.json. Nothing was run.'); process.exit(3); }
  return JSON.parse(fs.readFileSync(RUBRICS, 'utf8'));
}

/**
 * The tasks of a split: on dev, the fixed seeded subset; on held out, all of them.
 * @param split - dev or heldout
 * @param rubric - the rubric, which says the subset's size
 */
function tasksOf(split, rubric) {
  const read = (file) => fs.readFileSync(path.join(QUERIES, file), 'utf8').trim().split('\n').map((l) => JSON.parse(l)).filter((q) => q.split === split);
  const pick = (rows, n, salt) => {
    if (split !== 'dev') return rows;
    const random = seeded(SEED ^ salt);
    const a = [...rows];
    for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(random() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; }
    return a.slice(0, n);
  };
  return [...pick(read('F5-localisation.jsonl'), rubric.subset.devF5, 5), ...pick(read('F6-concept.jsonl'), rubric.subset.devF6, 6)];
}

/**
 * The folder a task runs in: the ticket's parent commit for F5, the corpus snapshot for F6.
 * @param task - an F5 or F6 query
 */
function folderOf(task) {
  const corpus = CORPORA.find((c) => c.name === task.repo);
  if (task.family !== 'F5') return snapshotDir(corpus);
  const dir = path.join(evalRoot(), 'checkouts', `${task.repo}@${task.parent.slice(0, 12)}`);
  if (!fs.existsSync(path.join(dir, '.snapshot-ready'))) {
    cloneAtCommit(corpus.source, task.parent, dir);
    fs.writeFileSync(path.join(dir, '.snapshot-ready'), task.parent);
    fs.appendFileSync(path.join(dir, '.git', 'info', 'exclude'), '\n.snapshot-ready\n');
  }
  return dir;
}

/**
 * Starts the index host for a folder and waits until it is ready, so an agent's first search is not
 * answered while the index builds.
 * @param dir - the folder
 */
function warmIndex(dir) {
  const env = { ...process.env, UNLUMINOUS_INDEX_CACHE: path.join(evalRoot(), 'index-cache') };
  for (let i = 0; i < 600; i++) {
    const r = spawnSync(CLI, ['search', 'status', '--root', dir, '--json'], { encoding: 'utf8', env, windowsHide: true });
    try { if (JSON.parse(r.stdout).result?.passagesReady) return true; } catch { /* not up yet */ }
    spawnSync(process.execPath, ['-e', 'setTimeout(()=>{},1000)']);
  }
  return false;
}

/**
 * The session settings of an arm: which tools it has and which MCP server.
 * @param arm - rg or index
 * @param dir - the task's folder
 */
function armSettings(arm, dir) {
  if (arm === 'rg') return { tools: 'Grep,Glob,Read', mcpConfig: { mcpServers: {} } };
  return {
    tools: 'Read',
    allowedTools: 'mcp__unluminous__unluminous_search,Read',
    mcpConfig: { mcpServers: { unluminous: { command: CLI, args: ['mcp', 'serve', '--areas', 'search'], env: { UNLUMINOUS_INDEX_CACHE: path.join(evalRoot(), 'index-cache'), CLAUDE_PROJECT_DIR: dir } } } },
  };
}

/**
 * Fills a task's instructions from the rubric.
 * @param task - the task
 * @param arm - rg or index
 * @param rubric - the rubric
 */
function promptFor(task, arm, rubric) {
  const template = rubric.instructions[task.family];
  return template.replace('{title}', task.title || '').replace('{description}', task.description || '').replace('{question}', task.question || '').replace('{tools}', rubric.tools[arm]);
}

/**
 * Counts the tool calls of a session by tool name.
 * @param events - the session's stream events
 */
function toolCalls(events) {
  const out = {};
  for (const e of events) {
    if (e.type !== 'assistant') continue;
    for (const c of e.message?.content || []) if (c.type === 'tool_use') out[c.name] = (out[c.name] || 0) + 1;
  }
  return out;
}

/**
 * Normalises a path an agent wrote to one relative to the repository.
 * @param p - the path
 * @param dir - the task's folder
 */
function normalPath(p, dir) {
  let s = String(p || '').replace(/\\/g, '/').replace(/:\d+(-\d+)?$/, '');
  const root = dir.replace(/\\/g, '/');
  if (s.toLowerCase().startsWith(root.toLowerCase())) s = s.slice(root.length);
  return s.replace(/^\.?\//, '').toLowerCase();
}

/**
 * Grades an F5 answer: half of the gold files among the first five named.
 * @param task - the task
 * @param answer - the agent's JSON block
 * @param dir - the task's folder
 */
function gradeF5(task, answer, dir) {
  const named = (answer?.files || []).slice(0, 5).map((f) => normalPath(f, dir));
  const gold = task.gold.files.map((f) => f.toLowerCase());
  const hit = gold.filter((g) => named.includes(g)).length;
  return { success: hit >= gold.length / 2, recall: hit / gold.length };
}

/**
 * Grades an F6 answer: a cited location in the gold span (within 5 lines), and the grader's verdict.
 * @param task - the task
 * @param answer - the agent's JSON block
 * @param dir - the task's folder
 * @param rubric - the rubric
 */
async function gradeF6(task, answer, dir, rubric) {
  const g = task.gold;
  const located = (answer?.locations || []).slice(0, 3).some((loc) => {
    const m = String(loc).match(/^(.*?):(\d+)/);
    if (!m) return false;
    const line = Number(m[2]);
    return normalPath(m[1], dir) === g.file.toLowerCase() && line >= g.start - 5 && line <= g.end + 5;
  });
  const code = fs.readFileSync(path.join(dir, g.file), 'utf8').split('\n').slice(g.start - 1, g.end).join('\n');
  const prompt = rubric.grader.prompt.replace('{question}', task.question).replace('{file}', g.file).replace('{start}', g.start).replace('{end}', g.end).replace('{code}', code).replace('{answer}', answer?.answer || '(no answer)');
  const graded = await runClaude({ model: rubric.grader.model, cwd: evalRoot(), prompt, systemPrompt: 'You grade answers. Reply with one JSON object and nothing else.' });
  const verdict = lastJson(graded.text);
  return { success: located && verdict?.correct === true, located, graderCorrect: verdict?.correct === true, graderWhy: verdict?.why || '', graderTokens: graded.usage.all };
}

/**
 * Runs one session of one task in one arm and grades it.
 * @param task - the task
 * @param arm - rg or index
 * @param draw - the draw number
 * @param opts - the run options
 * @param rubric - the rubric
 */
async function runTask(task, arm, draw, opts, rubric) {
  const dir = folderOf(task);
  if (arm === 'index') warmIndex(dir);
  const session = await runClaude({ model: opts.model, cwd: dir, prompt: promptFor(task, arm, rubric), timeoutMs: 1_200_000, ...armSettings(arm, dir) });
  const answer = lastJson(session.text);
  const grade = task.family === 'F5' ? gradeF5(task, answer, dir) : await gradeF6(task, answer, dir, rubric);
  return { id: task.id, family: task.family, repo: task.repo, arm, draw, model: opts.model, ok: session.status === 0, ms: session.ms, usage: session.usage, tools: toolCalls(session.events), answer, ...grade, transcript: session.events };
}

/**
 * Runs a list of jobs a few at a time.
 * @param jobs - functions returning promises
 * @param width - how many at once
 */
async function pool(jobs, width) {
  const results = [];
  let next = 0;
  await Promise.all(Array.from({ length: width }, async () => {
    while (next < jobs.length) { const i = next++; results[i] = await jobs[i](); }
  }));
  return results;
}

/**
 * Summarises a run: success and tokens for each arm, and the paired token ratio over the tasks both arms
 * succeeded at (TDD section 2.1, G2).
 * @param rows - the graded sessions
 */
export function summarise(rows) {
  const arms = [...new Set(rows.map((r) => r.arm))];
  const out = { arms: {} };
  for (const arm of arms) {
    const mine = rows.filter((r) => r.arm === arm);
    out.arms[arm] = { sessions: mine.length, success: mean(mine.map((r) => (r.success ? 1 : 0))), tokensMedian: median(mine.map((r) => r.usage.all)), turnsMedian: median(mine.map((r) => r.usage.turns)), firstInputMedian: median(mine.map((r) => r.usage.firstInput || 0)) };
  }
  const key = (r) => `${r.id}|${r.draw}`;
  const rg = new Map(rows.filter((r) => r.arm === 'rg').map((r) => [key(r), r]));
  const pairs = rows.filter((r) => r.arm === 'index' && rg.has(key(r)) && r.success && rg.get(key(r)).success).map((r) => r.usage.all / rg.get(key(r)).usage.all);
  if (pairs.length) out.tokenRatio = { n: pairs.length, ...bootstrap(pairs, geomean) };
  out.failedEither = rows.filter((r) => r.arm === 'index' && rg.has(key(r)) && !(r.success && rg.get(key(r)).success)).length;
  return out;
}

const opts = options(process.argv.slice(2));
const rubric = verifiedRubric(opts.freeze);
let tasks = tasksOf(opts.split, rubric);
if (opts.only) tasks = tasks.filter((t) => t.id.includes(opts.only));
if (opts.limit) tasks = tasks.slice(0, opts.limit);
const sha = spawnSync('git', ['-C', REPO, 'rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).stdout.trim();
const runId = `agents-${Math.floor(Date.now() / 1000)}-${sha}`;
const dir = path.join(evalRoot(), 'runs', runId);
fs.mkdirSync(path.join(dir, 'transcripts'), { recursive: true });
const jobs = [];
for (const task of tasks) for (let draw = 1; draw <= opts.draws; draw++) for (const arm of opts.arms) jobs.push(async () => {
  const row = await runTask(task, arm, draw, opts, rubric);
  fs.writeFileSync(path.join(dir, 'transcripts', `${task.id.replace(/[:/]/g, '_')}-${arm}-${draw}.jsonl`), row.transcript.map((e) => JSON.stringify(e)).join('\n'));
  delete row.transcript;
  fs.appendFileSync(path.join(dir, opts.split === 'heldout' ? 'rows.sealed.jsonl' : 'rows.jsonl'), JSON.stringify(row) + '\n');
  console.log(`${task.id} ${arm} draw ${draw}: ${row.success ? 'success' : 'miss'}, ${row.usage.all} tokens, ${row.usage.turns} turns`);
  return row;
});
const rows = await pool(jobs, opts.parallel);
const summary = { runId, label: opts.label, split: opts.split, model: opts.model, draws: opts.draws, tasks: tasks.length, at: new Date().toISOString(), unluminousSha: sha, ...summarise(rows) };
fs.writeFileSync(path.join(dir, 'summary.json'), JSON.stringify(summary, null, 2));
console.log(JSON.stringify(summary, null, 2));
