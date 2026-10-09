#!/usr/bin/env node
// Time what the real window costs to use, case by case, and write one JSON row a case.
//
//   node tools/perf-bench.mjs --bin <folder with unluminous.exe> --label <name> --out <folder>
//        [--corpus <template folder>] [--round N] [--cases a,b,...]
//
// Every other cost tool in this repository measures one component with no window behind it. This one
// drives a release build the way a person does, through `unluminous-cli input`, and asks the
// operating system how much processor time the process spent and how much memory it holds. A case is
// one interaction repeated enough times to be measurable: typing a line, scrolling a page, zooming the
// editor, zooming and panning a canvas, switching tabs. `task-2218` is what it was written for.
//
// Three things are deliberate, and each is a rule for a comparison between two builds.
//
// - **Every launch starts from nothing.** The project is a fresh copy of the template with its own git
//   repository, and `APPDATA` is a fresh folder, so no remembered tab, theme or background decides what
//   is drawn and nothing touches the person's own settings.
// - **Processor time is the process's own**, read from `Get-Process` by one long lived PowerShell, so the
//   cost of the `unluminous-cli` processes sending the commands is not in it. The command line round
//   trip is in `wall_ms` and nowhere else.
// - **A case is measured settled.** Before each case the window is left until a frame trace line has
//   not been written for a moment, so one case's tail is not charged to the next.

import { spawn, execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, rmSync, existsSync, readFileSync, appendFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';

const args = process.argv.slice(2);
/** Read a `--name value` argument, or the fallback. */
function option(name, fallback) {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
}

const bin = resolve(option('bin', 'target/release'));
const label = option('label', 'build');
const out = resolve(option('out', '_agent_output/perf-bench'));
const template = resolve(option('corpus', '_agent_output/perf-bench/template'));
const round = Number(option('round', '0'));
const only = option('cases', '') ? option('cases', '').split(',') : null;

const exe = join(bin, 'unluminous.exe');
const cli = join(bin, 'unluminous-cli.exe');
const run = join(out, 'runs', `${label}-r${round}`);
rmSync(run, { recursive: true, force: true });
mkdirSync(run, { recursive: true });
const appdata = join(run, 'appdata');
const corpus = join(run, 'corpus');
const trace = join(run, 'trace.txt');
mkdirSync(appdata, { recursive: true });
cpSync(template, corpus, { recursive: true });
// A repository of its own, so the git status the window reads is the same small one every time.
execFileSync('git', ['-C', corpus, 'init', '-q']);
execFileSync('git', ['-C', corpus, 'add', '-A']);
execFileSync('git', ['-C', corpus, '-c', 'user.name=bench', '-c', 'user.email=bench@example.com', 'commit', '-qm', 'corpus']);

const env = { ...process.env, APPDATA: appdata, UNLUMINOUS_FRAME_TRACE: trace };
const results = join(out, 'results.jsonl');

// One PowerShell kept open for the whole run: a line with a process id in, a line of counters out.
const sampler = spawn('pwsh', ['-NoProfile', '-NonInteractive', '-Command', `
  while ($true) {
    $line = [Console]::In.ReadLine()
    if ($null -eq $line) { break }
    $p = Get-Process -Id ([int]$line) -ErrorAction SilentlyContinue
    if ($null -eq $p) { [Console]::Out.WriteLine('{}'); continue }
    $o = @{ cpu = $p.TotalProcessorTime.TotalMilliseconds; ws = $p.WorkingSet64 / 1MB; priv = $p.PrivateMemorySize64 / 1MB; handles = $p.HandleCount; threads = $p.Threads.Count }
    [Console]::Out.WriteLine(($o | ConvertTo-Json -Compress))
    [Console]::Out.Flush()
  }`], { stdio: ['pipe', 'pipe', 'inherit'] });
const sampled = createInterface({ input: sampler.stdout });
const waiting = [];
sampled.on('line', (line) => waiting.shift()?.(JSON.parse(line)));
/** The process's processor time and memory now. */
function sample(pid) {
  return new Promise((done) => {
    waiting.push(done);
    sampler.stdin.write(`${pid}\n`);
  });
}

let pid = 0;
/** Run one `unluminous-cli` command against the window and answer with its output. */
function drive(...words) {
  return execFileSync(cli, [...words, '--instance', String(pid), '--timeout', '60000'], { env, encoding: 'utf8' });
}

/** How many frames the trace holds so far. */
function framesSoFar() {
  if (!existsSync(trace)) return [];
  return readFileSync(trace, 'utf8').split(/\r?\n/).filter((line) => line.startsWith('frame '));
}

/** Read `frame <ms> outside <ms> | <phase> <ms> ...` into numbers. */
function parseFrame(line) {
  const [head, tail = ''] = line.split('|');
  const words = head.trim().split(/\s+/);
  const phases = {};
  const rest = tail.trim().split(/\s+/).filter(Boolean);
  for (let at = 0; at + 1 < rest.length; at += 2) phases[rest[at]] = (phases[rest[at]] ?? 0) + Number(rest[at + 1]);
  return { total: Number(words[1]), outside: words[2] === 'outside' ? Number(words[3]) : 0, phases };
}

/** The middle value. */
function median(values) {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

/** Wait until the window has drawn nothing but its heartbeat for a while. */
async function settle(ms = 1500) {
  await sleep(ms);
}
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

/** Time one case: counters before and after, frames drawn in between, and the wall clock. */
async function measure(name, split, body, count) {
  // A case left out still runs, unrecorded, because the cases after it start from where it left off.
  if (only && !only.includes(name)) { await body(); return; }
  await settle();
  const before = await sample(pid);
  const framesBefore = framesSoFar().length;
  const started = performance.now();
  await body();
  const wall = performance.now() - started;
  const after = await sample(pid);
  const frames = framesSoFar().slice(framesBefore).map(parseFrame);
  const phases = {};
  for (const frame of frames) for (const [key, value] of Object.entries(frame.phases)) (phases[key] ??= []).push(value);
  const row = {
    prompt_id: `${name}`,
    tags: [split, name],
    variant: label,
    rep: round,
    model: 'n/a',
    cpu_ms: after.cpu - before.cpu,
    per_event_cpu_ms: (after.cpu - before.cpu) / (count || 1),
    wall_ms: wall,
    frames: frames.length,
    frame_ms_median: median(frames.map((frame) => frame.total)),
    frame_ms_sum: frames.reduce((sum, frame) => sum + frame.total, 0),
    outside_ms_median: median(frames.map((frame) => frame.outside)),
    ws_mb: after.ws,
    priv_mb: after.priv,
    ws_delta_mb: after.ws - before.ws,
    handles: after.handles,
    threads: after.threads,
    phase_medians: Object.fromEntries(Object.entries(phases).map(([key, values]) => [key, median(values)])),
    phase_sums: Object.fromEntries(Object.entries(phases).map(([key, values]) => [key, values.reduce((a, b) => a + b, 0)])),
  };
  appendFileSync(results, `${JSON.stringify(row)}\n`);
  console.log(`${label} r${round} ${name}: cpu ${row.cpu_ms.toFixed(0)} ms, ${row.frames} frames, frame ${row.frame_ms_median.toFixed(2)} ms, ws ${row.ws_mb.toFixed(1)} MB`);
}

// The editing area's middle, the canvas's middle, in the window's own points at 1600 by 1000.
const EDITOR = [900, 500];
const EDITOR_SPLIT = [900, 230];
const CANVAS = [820, 720];

/** Type, scroll and zoom one file: the three things a person does most in an editor. */
async function editorCases(split, file) {
  await measure(`open-${split}`, split, async () => drive('tab', 'open', file, '--permanent'), 1);
  drive('input', 'click', String(EDITOR[0]), String(EDITOR[1]));
  await measure(`type-${split}`, split, async () => drive('input', 'text', 'let quick = brown_fox(jumps, over, lazy_dog);'), 46);
  await measure(`scroll-${split}`, split, async () => {
    drive('input', 'move', String(EDITOR[0]), String(EDITOR[1]));
    for (let at = 0; at < 15; at++) drive('input', 'wheel', '-5');
    for (let at = 0; at < 15; at++) drive('input', 'wheel', '5');
  }, 30);
  await measure(`zoom-${split}`, split, async () => {
    drive('input', 'move', String(EDITOR[0]), String(EDITOR[1]));
    for (let at = 0; at < 6; at++) drive('input', 'wheel', '1', '--ctrl');
    for (let at = 0; at < 6; at++) drive('input', 'wheel', '-1', '--ctrl');
  }, 12);
}

/** A canvas of nodes, zoomed with the wheel and panned with a drag. */
async function canvasCases(split, files) {
  drive('realm', 'show');
  if (split === 'test') drive('realm', 'new', 'second');
  drive('realm', 'add', 'editor', '--path', files[0], '--x', '0', '--y', '0', '--width', '700', '--height', '600');
  drive('realm', 'add', 'folder', '--x', '750', '--y', '0', '--width', '300', '--height', '500');
  drive('realm', 'add', 'note', `Plan-${split}`, '--x', '0', '--y', '650');
  drive('realm', 'add', 'editor', '--path', files[1], '--x', '1100', '--y', '0', '--width', '600', '--height', '700');
  drive('realm', 'camera', '--fit');
  await measure(`canvas-zoom-${split}`, split, async () => {
    drive('input', 'move', String(CANVAS[0]), String(CANVAS[1]));
    for (let at = 0; at < 8; at++) drive('input', 'wheel', '1', '--ctrl');
    for (let at = 0; at < 8; at++) drive('input', 'wheel', '-1', '--ctrl');
  }, 16);
  await measure(`canvas-pan-${split}`, split, async () => {
    drive('input', 'drag', '200', '920', '--to-x', '600', '--to-y', '880', '--steps', '30');
    drive('input', 'drag', '600', '880', '--to-x', '200', '--to-y', '920', '--steps', '30');
  }, 60);
  await measure(`canvas-idle-${split}`, split, async () => sleep(6000), 6);
  drive('realm', 'hide');
}

const launched = performance.now();
const child = spawn(exe, [corpus, '--background'], { env, detached: false, stdio: 'ignore' });
pid = child.pid;
try {
  // Startup: from spawning the process until the control channel answers.
  for (;;) {
    try {
      execFileSync(cli, ['status', '--instance', String(pid), '--timeout', '500'], { env, stdio: 'ignore' });
      break;
    } catch {
      await sleep(20);
      if (performance.now() - launched > 60000) throw new Error('the window never answered');
    }
  }
  const ready = performance.now() - launched;
  drive('window', 'size', '--width', '1600', '--height', '1000');
  drive('window', 'position', '--x', '40', '--y', '40');
  await sleep(4000);
  const startupSample = await sample(pid);
  appendFileSync(results, `${JSON.stringify({ prompt_id: 'startup', tags: ['startup', 'startup'], variant: label, rep: round, model: 'n/a', wall_ms: ready, cpu_ms: startupSample.cpu, ws_mb: startupSample.ws, priv_mb: startupSample.priv, handles: startupSample.handles, threads: startupSample.threads })}\n`);
  console.log(`${label} r${round} startup: ready ${ready.toFixed(0)} ms, cpu ${startupSample.cpu.toFixed(0)} ms, ws ${startupSample.ws.toFixed(1)} MB`);

  await measure('idle', 'train', async () => sleep(8000), 8);
  await editorCases('train', 'train.rs');
  await editorCases('test', 'test.rs');
  await measure('tab-switch', 'test', async () => {
    for (let at = 0; at < 10; at++) {
      drive('tab', 'open', 'train.rs');
      drive('tab', 'open', 'test.rs');
    }
  }, 20);
  await editorCases('train-md', 'train.md');
  await editorCases('test-md', 'test.md');
  await canvasCases('train', ['train.rs', 'test.md']);
  await canvasCases('test', ['test.rs', 'train.md']);
  await measure('idle-end', 'test', async () => sleep(8000), 8);
  const final = await sample(pid);
  appendFileSync(results, `${JSON.stringify({ prompt_id: 'memory-end', tags: ['memory', 'memory'], variant: label, rep: round, model: 'n/a', ws_mb: final.ws, priv_mb: final.priv, cpu_ms: final.cpu, handles: final.handles, threads: final.threads })}\n`);
} finally {
  try { drive('quit'); } catch { /* the kill below is the answer */ }
  await sleep(1500);
  try { process.kill(pid); } catch { /* already gone */ }
  sampler.stdin.end();
  writeFileSync(join(run, 'done.txt'), new Date().toISOString());
}
