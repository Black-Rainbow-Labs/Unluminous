// The agent study as an eval: a real agent drives a real Unluminous window through every case in
// `cases.mjs`, and each case is graded from what the window and the project hold afterwards
// (`task-2314`). The layout it writes is the one the hill climb report builder reads.
//
//   node tools/agent-study/eval.mjs --variant baseline --window-bin <folder> --cli <unluminous-cli>
//   node tools/agent-study/eval.mjs --variant v1 --cli <built cli> --split test --reps 2
//   node tools/agent-study/eval.mjs --variant v1 --cli <built cli> --cases s05-split,n02-close-others
//
// `--cli` is the program the agent's MCP server runs, and it is the thing most variants change: a
// tool description, a refusal, a new tool. `--window-bin` is the folder holding the `unluminous`
// window and the `unluminous-cli` the checks read through. It is fixed for a whole flow so two
// variants are graded by the same reader, unless a variant changes the window itself.
// `--skill <SKILL.md>` puts a skill where opencode finds a project's skills, for a variant that
// measures one.
//
// The agent is opencode on the local model, because the eval is run every round of a hill climb and
// a hosted model would spend the account's usage on it. One case is one fresh window, one fresh copy
// of the sample project and one fresh settings folder (`window.mjs`), so no case can inherit a
// theme, a font size or a ticket from the one before it.
//
// Each case appends one row to `<flow>/<variant>/results.jsonl` the moment it finishes, and a run
// that is started again skips the (case, rep) pairs already written. A case that could not be run
// at all goes to `errors.jsonl`, so it is run again rather than scored as the model's failure.
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { PROJECT, REPO, ROOT, cli, freshWindow, makeTemplate, stop, useWindowBuild, windowEnv } from './window.mjs';
import { isUnluminous, loadCases, verbOf } from './cases.mjs';

const args = parseArgs(process.argv.slice(2));
const VARIANT = args.variant ?? 'baseline';
if (!/^(baseline|v\d+|pilot\w*)$/.test(VARIANT)) die('--variant is baseline, v<N> or pilot');
const FLOW = path.resolve(args.flow ?? path.join(REPO, '_agent_output', 'task-2314', 'flow'));
if (args['window-bin']) useWindowBuild(args['window-bin']);
const AGENT_CLI = path.resolve(args.cli ?? die('--cli is required: the unluminous-cli the agent\'s MCP server runs'));
const MCP_ARGS = String(args['mcp-args'] ?? '').split(' ').filter(Boolean);
const REPS = Number(args.reps ?? 1);
const CASE_LIMIT_MS = Number(args['case-limit'] ?? 600) * 1000;
const MODEL_URL = args['base-url'] ?? 'http://localhost:8080/v1';
const MODEL = 'study/local';
const SPLIT = args.split ?? 'all';
const ONLY = args.cases ? String(args.cases).split(',') : null;
const SKILL = args.skill ? path.resolve(args.skill) : null;
// Cases to run again although they have rows, because their check was fixed. The new row is written
// after the old one, and `score.mjs` reads the last row of each (case, rep).
const REDO = args.redo ? String(args.redo).split(',') : [];
const OPENCODE = process.env.EVAL_OPENCODE ?? findOpencode();

/** The agent's tools that neither do the job nor go round Unluminous. */
const NEUTRAL = new Set(['todowrite', 'todoread', 'skill', 'question', 'invalid']);

/** FNV-1a of the id: a third of the cases, chosen by name, are held out as the test split. */
export function isTest(id) {
  let h = 0x811c9dc5;
  for (const ch of Buffer.from(id)) { h ^= ch; h = Math.imul(h, 0x01000193) >>> 0; }
  return h % 3 === 0;
}

const cases = loadCases(import.meta.dirname)
  .filter(c => c.check)
  .filter(c => !ONLY || ONLY.includes(c.id))
  .filter(c => SPLIT === 'all' || (SPLIT === 'test') === isTest(c.id));
if (!cases.length) die('no cases matched');

const OUT = path.join(FLOW, VARIANT);
fs.mkdirSync(path.join(OUT, 'traces'), { recursive: true });
const resultsFile = path.join(OUT, 'results.jsonl');
const done = new Set();
if (fs.existsSync(resultsFile)) {
  for (const line of fs.readFileSync(resultsFile, 'utf8').split('\n').filter(Boolean)) {
    const r = JSON.parse(line);
    if (!REDO.includes(r.prompt_id)) done.add(`${r.prompt_id}#${r.rep}`);
  }
}

await makeTemplate();
const config = writeConfig();
const todo = [];
for (let rep = 0; rep < REPS; rep++) for (const c of cases) if (!done.has(`${c.id}#${rep}`)) todo.push({ c, rep });
console.log(`${VARIANT}: ${todo.length} to run (${cases.length} cases x ${REPS} reps, ${done.size} done), agent cli ${AGENT_CLI}`);
const began = Date.now();
let finished = 0;
for (const { c, rep } of todo) {
  let pid = null;
  try {
    pid = await freshWindow();
    await runCase(c, rep, pid);
  } catch (e) {
    fs.appendFileSync(path.join(OUT, 'errors.jsonl'), JSON.stringify({ prompt_id: c.id, rep, failure: 'harness', message: String(e?.stack ?? e).slice(0, 2000) }) + '\n');
    console.log(`${c.id}#${rep}: harness failure ${String(e?.message ?? e).slice(0, 300)}`);
  } finally {
    stop(pid);
  }
  finished += 1;
  const each = (Date.now() - began) / finished / 1000;
  fs.writeFileSync(path.join(OUT, 'progress.txt'), `${finished}/${todo.length} done, ~${Math.round(each * (todo.length - finished) / 60)} min left\n`);
}
if (!fs.existsSync(path.join(OUT, 'summary.json'))) fs.writeFileSync(path.join(OUT, 'summary.json'), JSON.stringify({ description: VARIANT }, null, 2));
console.log(`finished in ${Math.round((Date.now() - began) / 60000)} min`);

/** One case, one rep, in a window nothing else has touched. */
async function runCase(c, rep, pid) {
  const originals = Object.fromEntries(['src/main.rs', 'src/shapes.rs', 'README.md'].map(f => [f, read(f)]));
  installSkill();
  for (const cmd of Object.values(c.before ?? {})) cli(pid, cmd);
  const t0 = Date.now();
  const deadline = t0 + CASE_LIMIT_MS;
  const turns = [];
  let session = null, raw = '';
  for (const prompt of c.prompts) {
    if (Date.now() > deadline) { turns.push({ prompt, timedOut: true, steps: [], tokens: null }); break; }
    const t = Date.now();
    const r = await turn(prompt, session, pid, deadline);
    raw += r.out;
    const d = distil(r.out);
    session = session ?? d.sessionID;
    turns.push({ prompt, seconds: (Date.now() - t) / 1000, timedOut: r.timedOut, stderr: r.err.slice(-1500), ...d });
    if (!d.sessionID && !d.steps.length) throw new Error(`the agent produced nothing: ${r.err.slice(-400)}`);
    if (r.timedOut) break;
  }
  const seconds = (Date.now() - t0) / 1000;
  const calls = turns.flatMap(t => t.steps.filter(s => s.kind === 'tool'));
  const text = turns.map(t => t.steps.filter(s => s.kind === 'text').map(s => s.text).join('\n')).join('\n');
  const ctx = {
    calls, text, project: PROJECT,
    cli: async (...a) => cli(pid, a).value?.result ?? null,
    file: rel => read(rel),
    original: rel => originals[rel] ?? null,
  };
  let verdict;
  try { verdict = await c.check(ctx); } catch (e) { verdict = { pass: false, why: `the check failed: ${e.message}` }; }
  const refused = calls.filter(s => s.error).length;
  const own = calls.filter(s => !isUnluminous(s) && !NEUTRAL.has(s.tool)).length;
  const discovery = calls.filter(isDiscovery).length;
  const usage = turns.reduce((a, t) => ({
    input_tokens: a.input_tokens + (t.tokens?.input ?? 0),
    output_tokens: a.output_tokens + (t.tokens?.output ?? 0),
    cache_read_input_tokens: a.cache_read_input_tokens + (t.tokens?.cache ?? 0),
    cache_creation_input_tokens: 0,
  }), { input_tokens: 0, output_tokens: 0, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 });
  const split = isTest(c.id) ? 'test' : 'train';
  const row = {
    prompt_id: c.id, rep, prompt: c.prompts.join('\n\n'), tags: [c.area.split('/')[0], split], split,
    status: turns.some(t => t.timedOut) ? 'timeout' : 'ok', stop_reason: 'end_turn', model: MODEL,
    grade: {
      success: verdict.pass ? 1 : 0,
      clean: verdict.pass && refused === 0 && own === 0 ? 1 : 0,
      wasted: refused + discovery, refused, discovery, own, calls: calls.length,
    },
    explanation: { success: verdict.why },
    latency_s: Math.round(seconds), tool_calls: calls.length, usage,
    meta: { expect: c.expect, refusals: calls.filter(s => s.error).map(s => `${verbOf(s)}: ${String(s.error).slice(0, 200)}`) },
  };
  fs.writeFileSync(path.join(OUT, 'traces', `${c.id}_rep${rep}.json`), JSON.stringify(trace(turns), null, 1));
  fs.mkdirSync(path.join(OUT, 'raw'), { recursive: true });
  fs.writeFileSync(path.join(OUT, 'raw', `${c.id}_rep${rep}.jsonl`), raw);
  fs.appendFileSync(resultsFile, JSON.stringify(row) + '\n');
  console.log(`${c.id}#${rep} ${verdict.pass ? 'PASS' : 'FAIL'} ${Math.round(seconds)}s calls ${calls.length} refused ${refused} discovery ${discovery} own ${own} | ${verdict.why}`);
}

/** A call that only asked what commands there are, rather than doing any of the job. */
function isDiscovery(call) {
  if (!isUnluminous(call)) return false;
  const verb = verbOf(call);
  const tool = String(call.tool ?? '');
  if (/(^|_)unluminous$/.test(tool) && /^(commands|version|instances|help)$/.test(verb)) return true;
  if (/unluminous_mcp$/.test(tool) && /tools|status/.test(verb)) return true;
  if (/unluminous_plugins$/.test(tool) && /^(list|show)$/.test(verb)) return true;
  if (/unluminous_action$/.test(tool) && /^list$/.test(verb)) return true;
  return /--help/.test(JSON.stringify(call.input ?? {}));
}

/** The opencode configuration a case runs with: the local model and the variant's MCP server. */
function writeConfig() {
  const conf = {
    $schema: 'https://opencode.ai/config.json',
    provider: { study: { npm: '@ai-sdk/openai-compatible', options: { baseURL: MODEL_URL }, models: { local: { limit: { context: 131072, output: 16000 } } } } },
    mcp: { unluminous: { type: 'local', enabled: true, command: [AGENT_CLI, 'mcp', 'serve', ...MCP_ARGS] } },
  };
  // OPENCODE_CONFIG is merged over the person's own configuration rather than replacing it, so each
  // of their other MCP servers is switched off here: what is measured is opencode's own tools and
  // Unluminous's, and nothing that competes with them by accident.
  for (const name of globalMcpServers()) if (name !== 'unluminous') conf.mcp[name] = { type: 'local', enabled: false, command: ['node'] };
  const file = path.join(ROOT, `opencode-${VARIANT}.json`);
  fs.mkdirSync(ROOT, { recursive: true });
  fs.writeFileSync(file, JSON.stringify(conf, null, 2));
  return file;
}

/** The MCP servers in the person's own opencode configuration, read through opencode itself. */
function globalMcpServers() {
  const r = spawnSync(OPENCODE, ['debug', 'config'], { encoding: 'utf8', env: { ...process.env, OPENCODE_CONFIG: '' } });
  try { return Object.keys(JSON.parse(r.stdout).mcp ?? {}); } catch { return ['aiservice-web', 'inillucent']; }
}

/** Puts the skill under test where opencode finds a project's skills. */
function installSkill() {
  if (!SKILL) return;
  const name = path.basename(path.dirname(SKILL));
  const into = path.join(PROJECT, '.opencode', 'skills', name);
  fs.mkdirSync(into, { recursive: true });
  fs.copyFileSync(SKILL, path.join(into, 'SKILL.md'));
}

/** One turn of the conversation through opencode, answering with its event stream. */
function turn(message, sessionId, pid, deadline) {
  return new Promise(resolve => {
    const a = ['run', '--dir', PROJECT, '-m', MODEL, '--format', 'json'];
    if (sessionId) a.push('-s', sessionId);
    a.push(message);
    // opencode's own program rather than npm's `opencode.cmd`, which only a shell can start, and a
    // shell would read the quotes and backticks in a prompt as its own. Its standard input is closed
    // because `opencode run` waits for it to end.
    const p = spawn(OPENCODE, a, {
      cwd: PROJECT, stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...windowEnv(pid), OPENCODE_CONFIG: config, OPENCODE_DISABLE_AUTOUPDATE: '1' },
    });
    let out = '', err = '', timedOut = false;
    const timer = setTimeout(() => { timedOut = true; killTree(p.pid); }, Math.max(1000, deadline - Date.now()));
    p.stdout.on('data', d => out += d);
    p.stderr.on('data', d => err += d);
    // On exit rather than close: the MCP server it started can hold the inherited pipe open.
    p.on('exit', () => { clearTimeout(timer); setTimeout(() => resolve({ out, err, timedOut }), 800); });
  });
}

/** The parts of opencode's event stream worth grading. */
function distil(raw) {
  const events = [];
  for (const line of raw.split('\n')) { const t = line.trim(); if (t.startsWith('{')) try { events.push(JSON.parse(t)); } catch {} }
  const steps = [];
  for (const e of events) {
    if (e.type === 'tool_use') {
      const s = e.part?.state ?? {};
      steps.push({ kind: 'tool', tool: e.part?.tool, status: s.status, input: s.input,
        output: typeof s.output === 'string' ? s.output : null,
        error: typeof s.error === 'string' ? s.error : (s.error ? JSON.stringify(s.error) : refusedOutput(s.output)) });
    } else if (e.type === 'text' && e.part?.text) steps.push({ kind: 'text', text: e.part.text });
    else if (e.type === 'error') steps.push({ kind: 'error', detail: JSON.stringify(e.error) });
  }
  const tokens = events.filter(e => e.type === 'step_finish').map(e => e.part?.tokens).filter(Boolean)
    .reduce((a, t) => ({ input: a.input + (t.input ?? 0), output: a.output + (t.output ?? 0) + (t.reasoning ?? 0), cache: a.cache + (t.cache?.read ?? 0) }), { input: 0, output: 0, cache: 0 });
  return { sessionID: events.find(e => e.sessionID)?.sessionID ?? null, steps, tokens };
}

/** An MCP reply marked as an error comes back as ordinary output text in opencode's stream. */
function refusedOutput(output) {
  if (typeof output !== 'string') return null;
  return /^(Error|There is no|`[^`]+` (needs|is not|takes)|.*is not one of|usage:)/i.test(output.trim()) || /"isError":\s*true/.test(output) ? output.slice(0, 600) : null;
}

/** The conversation in the report's trace shape. */
function trace(turns) {
  const out = [];
  for (const t of turns) {
    out.push({ role: 'user', content: t.prompt });
    for (const s of t.steps) {
      if (s.kind === 'tool') {
        out.push({ role: 'tool_call', name: s.tool, content: JSON.stringify(s.input ?? {}, null, 2) });
        out.push({ role: 'tool_result', content: s.error ? `REFUSED: ${s.error}` : String(s.output ?? '').slice(0, 4000) });
      } else if (s.kind === 'text') out.push({ role: 'assistant', content: s.text });
      else out.push({ role: 'assistant', content: `STREAM ERROR ${s.detail}` });
    }
  }
  return out;
}

/** Where npm put opencode's own program, found beside the `opencode.cmd` on the PATH. */
function findOpencode() {
  if (process.platform !== 'win32') return 'opencode';
  for (const dir of (process.env.PATH ?? '').split(path.delimiter)) {
    const exe = path.join(dir, 'node_modules', 'opencode-ai', 'bin', 'opencode.exe');
    if (fs.existsSync(path.join(dir, 'opencode.cmd')) && fs.existsSync(exe)) return exe;
  }
  die('opencode is not on the PATH; set EVAL_OPENCODE to its program');
}

/** Ends a process and everything it started, by its own id. */
function killTree(pid) {
  if (process.platform === 'win32') spawn('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' });
  else try { process.kill(-pid); } catch {}
}

function read(rel) { try { return fs.readFileSync(path.join(PROJECT, rel), 'utf8'); } catch { return null; } }
function die(m) { console.error(m); process.exit(2); }
function parseArgs(a) {
  const o = {};
  for (let i = 0; i < a.length; i++) if (a[i].startsWith('--')) { const k = a[i].slice(2); o[k] = a[i + 1] && !a[i + 1].startsWith('--') ? a[++i] : 'true'; }
  return o;
}
