#!/usr/bin/env node
// Time a whole session with a real release window, from opening a project to closing it, and write
// one JSON row a case.
//
//   node tools/perf-session.mjs --bin <folder with unluminous.exe> --label <name> --out <folder>
//        --corpus <template folder> [--round N] [--observe <seconds>]
//
// `tools/perf-bench.mjs` (task-2218) measures what an interaction costs inside a window on a small
// corpus with no language server and no code index worth the name. This one measures what `task-2279`
// reported: a window on a real project, with every program the window starts counted beside it. The
// counters are summed over the window's whole process tree, because rust-analyzer, tsserver and the
// shells are the window's children and they are what Task Manager shows under its name.
//
// The cases, in the order they run:
//
// - `cold-open`: a fresh settings folder and a fresh index cache. The window is started, a Rust file is
//   opened, and the tree is watched for `--observe` seconds. Ready time, processor seconds, peak and
//   final memory.
// - `cold-idle`: the next thirty seconds, which is what "at rest" means.
// - `toggle-explorer`, `toggle-terminal`: each panel shown and put away ten times.
// - `drag-explorer`, `drag-terminal`: the divider dragged out and back.
// - `resize-window`: the window's size set twelve times.
// - `tab-switch`, `type`, `scroll`: the three things a person does most.
// - `close`: from `quit` until the window and every child have gone.
// - `warm-open`, `warm-idle`, `warm-close`: the same project opened again with what it remembered.

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
const out = resolve(option('out', '_agent_output/task-2279-perf/bench'));
const template = resolve(option('corpus', '_agent_output/task-2279-perf/template'));
const round = Number(option('round', '0'));
const observe = Number(option('observe', '90')) * 1000;
const settingsFile = option('settings', '');

const exe = join(bin, 'unluminous.exe');
const cli = join(bin, 'unluminous-cli.exe');
const run = join(out, 'runs', `${label}-r${round}`);
rmSync(run, { recursive: true, force: true });
mkdirSync(run, { recursive: true });
const appdata = join(run, 'appdata');
const cache = join(run, 'index-cache');
const corpus = join(run, 'corpus');
const trace = join(run, 'trace.txt');
mkdirSync(join(appdata, 'Unluminous'), { recursive: true });
mkdirSync(cache, { recursive: true });
if (settingsFile) cpSync(settingsFile, join(appdata, 'Unluminous', 'settings.conf'));
cpSync(template, corpus, { recursive: true });
execFileSync('git', ['-c', 'core.longpaths=true', '-C', corpus, 'init', '-q']);
execFileSync('git', ['-c', 'core.longpaths=true', '-C', corpus, 'add', '-A']);
execFileSync('git', ['-c', 'core.longpaths=true', '-C', corpus, '-c', 'user.name=bench', '-c', 'user.email=bench@example.com', 'commit', '-qm', 'corpus']);

const env = {
  ...process.env,
  APPDATA: appdata,
  ATRIUS_INDEX_CACHE: cache,
  UNLUMINOUS_FRAME_TRACE: trace,
  // A window started with --background sends nothing, but say so for the record.
  UNLUMINOUS_TELEMETRY: 'off',
};
const results = join(out, 'results.jsonl');

// One PowerShell kept open for the run: a process id in, the whole tree's counters out. One CIM query
// answers every process at once, so the tree is summed from a single moment.
const sampler = spawn('pwsh', ['-NoProfile', '-NonInteractive', '-Command', `
  while ($true) {
    $line = [Console]::In.ReadLine()
    if ($null -eq $line) { break }
    $root = [int]$line
    $all = Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name,WorkingSetSize,PrivatePageCount,KernelModeTime,UserModeTime
    $kids = @{}
    foreach ($p in $all) { if (-not $kids.ContainsKey([int]$p.ParentProcessId)) { $kids[[int]$p.ParentProcessId] = New-Object System.Collections.ArrayList }; [void]$kids[[int]$p.ParentProcessId].Add($p) }
    $byId = @{}; foreach ($p in $all) { $byId[[int]$p.ProcessId] = $p }
    if (-not $byId.ContainsKey($root)) { [Console]::Out.WriteLine('{"gone":true}'); [Console]::Out.Flush(); continue }
    $queue = New-Object System.Collections.Queue; $queue.Enqueue($byId[$root]); $tree = @()
    while ($queue.Count -gt 0) { $p = $queue.Dequeue(); $tree += $p; if ($kids.ContainsKey([int]$p.ProcessId)) { foreach ($k in $kids[[int]$p.ProcessId]) { if ([int]$k.ProcessId -ne $root) { $queue.Enqueue($k) } } } }
    $groups = @{}
    foreach ($p in $tree) {
      $n = $p.Name
      if (-not $groups.ContainsKey($n)) { $groups[$n] = @{ cpu = 0.0; ws = 0.0; priv = 0.0; count = 0 } }
      $groups[$n].cpu += ($p.KernelModeTime + $p.UserModeTime) / 10000.0
      $groups[$n].ws += $p.WorkingSetSize / 1MB
      $groups[$n].priv += $p.PrivatePageCount / 1MB
      $groups[$n].count += 1
    }
    $w = $byId[$root]
    $o = @{ cpu = ($tree | Measure-Object -Sum -Property { ($_.KernelModeTime + $_.UserModeTime) / 10000.0 }).Sum; ws = ($tree | Measure-Object -Sum -Property { $_.WorkingSetSize / 1MB }).Sum; priv = ($tree | Measure-Object -Sum -Property { $_.PrivatePageCount / 1MB }).Sum; wcpu = ($w.KernelModeTime + $w.UserModeTime) / 10000.0; wws = $w.WorkingSetSize / 1MB; wpriv = $w.PrivatePageCount / 1MB; procs = $tree.Count; groups = $groups }
    [Console]::Out.WriteLine(($o | ConvertTo-Json -Compress -Depth 4))
    [Console]::Out.Flush()
  }`], { stdio: ['pipe', 'pipe', 'inherit'] });
const sampled = createInterface({ input: sampler.stdout });
const waiting = [];
sampled.on('line', (line) => waiting.shift()?.(JSON.parse(line)));
/** The tree's processor time and memory now. */
function sample(pid) {
  return new Promise((done) => {
    waiting.push(done);
    sampler.stdin.write(`${pid}\n`);
  });
}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
let pid = 0;
let child = null;

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

/** The value below which nine in ten fall. */
function p90(values) {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.9))];
}

/** Write one row. */
function record(row) {
  const full = { variant: label, rep: round, model: 'n/a', tags: [row.prompt_id.replace(/-.*$/, ''), row.prompt_id], ...row };
  appendFileSync(results, `${JSON.stringify(full)}\n`);
  const shown = Object.entries(row).filter(([k]) => k !== 'groups' && k !== 'phase_medians').map(([k, v]) => `${k}=${typeof v === 'number' ? v.toFixed(1) : v}`).join(' ');
  console.log(`${label} r${round} ${shown}`);
}

/** The group counters as `{name: {cpu, ws, priv}}` with rounded numbers. */
function groupsOf(s) {
  return Object.fromEntries(Object.entries(s.groups ?? {}).map(([k, v]) => [k, { cpu: Math.round(v.cpu), ws: Math.round(v.ws), priv: Math.round(v.priv), count: v.count }]));
}

/** Start the window and wait for its control channel. Answers the milliseconds it took. */
async function launch() {
  const started = performance.now();
  child = spawn(exe, [corpus, '--background'], { env, detached: false, stdio: 'ignore' });
  pid = child.pid;
  for (;;) {
    try {
      execFileSync(cli, ['status', '--instance', String(pid), '--timeout', '500'], { env, stdio: 'ignore' });
      return performance.now() - started;
    } catch {
      await sleep(20);
      if (performance.now() - started > 60000) throw new Error('the window never answered');
    }
  }
}

/** Watch the tree for a while, sampling every two seconds. Answers the first sample, the last and the peaks. */
async function watch(ms) {
  const first = await sample(pid);
  let peakWs = first.ws;
  let peakPriv = first.priv;
  let peakWindowWs = first.wws;
  const began = performance.now();
  let last = first;
  while (performance.now() - began < ms) {
    await sleep(2000);
    last = await sample(pid);
    peakWs = Math.max(peakWs, last.ws);
    peakPriv = Math.max(peakPriv, last.priv);
    peakWindowWs = Math.max(peakWindowWs, last.wws);
  }
  return { first, last, peakWs, peakPriv, peakWindowWs, seconds: (performance.now() - began) / 1000 };
}

/** A session phase: open, then watch, as one row. */
async function openCase(name) {
  const ready = await launch();
  drive('window', 'size', '--width', '1600', '--height', '1000');
  drive('tab', 'open', 'crates/unluminous-app/src/app/mod.rs', '--permanent');
  const w = await watch(observe);
  record({
    prompt_id: name,
    ready_ms: ready,
    cpu_ms: w.last.cpu,
    window_cpu_ms: w.last.wcpu,
    ws_mb: w.last.ws,
    priv_mb: w.last.priv,
    window_ws_mb: w.last.wws,
    window_priv_mb: w.last.wpriv,
    peak_ws_mb: w.peakWs,
    peak_priv_mb: w.peakPriv,
    peak_window_ws_mb: w.peakWindowWs,
    procs: w.last.procs,
    groups: groupsOf(w.last),
  });
}

/** An idle phase: thirty seconds with nothing done, as processor milliseconds a second. */
async function idleCase(name, ms = 30000) {
  const w = await watch(ms);
  record({
    prompt_id: name,
    cpu_ms_per_s: (w.last.cpu - w.first.cpu) / w.seconds,
    window_cpu_ms_per_s: (w.last.wcpu - w.first.wcpu) / w.seconds,
    ws_mb: w.last.ws,
    priv_mb: w.last.priv,
    window_ws_mb: w.last.wws,
    window_priv_mb: w.last.wpriv,
    groups: groupsOf(w.last),
  });
}

/** An interaction: the window's own processor time and the frames it drew. */
async function interact(name, count, body) {
  await sleep(1500);
  const before = await sample(pid);
  const framesBefore = framesSoFar().length;
  const started = performance.now();
  await body();
  const wall = performance.now() - started;
  await sleep(300);
  const after = await sample(pid);
  const frames = framesSoFar().slice(framesBefore).map(parseFrame);
  const phases = {};
  for (const frame of frames) for (const [key, value] of Object.entries(frame.phases)) (phases[key] ??= []).push(value);
  record({
    prompt_id: name,
    window_cpu_ms: after.wcpu - before.wcpu,
    per_event_cpu_ms: (after.wcpu - before.wcpu) / count,
    wall_ms: wall,
    per_event_wall_ms: wall / count,
    frames: frames.length,
    frame_ms_median: median(frames.map((f) => f.total)),
    frame_ms_p90: p90(frames.map((f) => f.total)),
    frame_ms_max: frames.reduce((m, f) => Math.max(m, f.total), 0),
    outside_ms_median: median(frames.map((f) => f.outside)),
    phase_medians: Object.fromEntries(Object.entries(phases).map(([key, values]) => [key, median(values)])),
  });
}

/** Ask the window to quit and time until it and every child have gone. */
async function closeCase(name) {
  const started = performance.now();
  try { drive('quit'); } catch { /* the window may close before it answers */ }
  for (;;) {
    const s = await sample(pid);
    if (s.gone) break;
    if (performance.now() - started > 30000) { try { process.kill(pid); } catch { /* gone */ } break; }
    await sleep(50);
  }
  const windowGone = performance.now() - started;
  record({ prompt_id: name, close_ms: windowGone });
}

/** Where a panel is, from `panel list`. */
function panelRect(name) {
  const list = JSON.parse(drive('panel', 'list', '--json')).result;
  const panels = list.panels ?? list;
  const found = panels.find((p) => p.panel === name || p.name === name);
  return found?.rect ?? found?.area ?? null;
}

try {
  await openCase('cold-open');
  await idleCase('cold-idle');

  await interact('toggle-explorer', 20, async () => {
    for (let at = 0; at < 20; at++) drive('action', 'run', 'toggle-explorer');
  });
  await interact('toggle-terminal', 20, async () => {
    for (let at = 0; at < 20; at++) drive('action', 'run', 'toggle-terminal');
  });
  drive('action', 'run', 'toggle-terminal');
  await sleep(1500);
  const explorer = panelRect('explorer');
  if (explorer) {
    const x = explorer.x + explorer.width - 1;
    const y = explorer.y + explorer.height / 2;
    await interact('drag-explorer', 60, async () => {
      drive('input', 'drag', String(x), String(y), '--to-x', String(x + 200), '--to-y', String(y), '--steps', '30');
      drive('input', 'drag', String(x + 200), String(y), '--to-x', String(x), '--to-y', String(y), '--steps', '30');
    });
  }
  const terminal = panelRect('terminal');
  if (terminal) {
    const x = terminal.x + terminal.width / 2;
    const y = terminal.y + 1;
    await interact('drag-terminal', 60, async () => {
      drive('input', 'drag', String(x), String(y), '--to-x', String(x), '--to-y', String(y - 200), '--steps', '30');
      drive('input', 'drag', String(x), String(y - 200), '--to-x', String(x), '--to-y', String(y), '--steps', '30');
    });
  }
  await interact('resize-window', 12, async () => {
    for (let at = 0; at < 12; at++) drive('window', 'size', '--width', String(1200 + (at % 2) * 400), '--height', String(800 + (at % 3) * 100));
  });
  drive('window', 'size', '--width', '1600', '--height', '1000');
  drive('tab', 'open', 'crates/unluminous-core/src/layout.rs', '--permanent');
  await interact('tab-switch', 20, async () => {
    for (let at = 0; at < 10; at++) {
      drive('tab', 'open', 'crates/unluminous-app/src/app/mod.rs');
      drive('tab', 'open', 'crates/unluminous-core/src/layout.rs');
    }
  });
  drive('input', 'click', '900', '500');
  await interact('type', 46, async () => drive('input', 'text', 'let quick = brown_fox(jumps, over, lazy_dog);'));
  await interact('scroll', 30, async () => {
    drive('input', 'move', '900', '500');
    for (let at = 0; at < 15; at++) drive('input', 'wheel', '-5');
    for (let at = 0; at < 15; at++) drive('input', 'wheel', '5');
  });
  drive('tab', 'save');
  await closeCase('cold-close');

  await openCase('warm-open');
  await idleCase('warm-idle');
  await closeCase('warm-close');
} finally {
  try { process.kill(pid); } catch { /* already gone */ }
  sampler.stdin.end();
  writeFileSync(join(run, 'done.txt'), new Date().toISOString());
}
