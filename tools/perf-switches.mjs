#!/usr/bin/env node
// Check that `editor.servers` and `search.embeddings` take effect while a window runs, in both
// directions, and measure how long the memory takes to come back once each is switched off.
//
//   node tools/perf-switches.mjs --bin <folder with unluminous.exe> --out <folder> --corpus <template>
//
// `task-2279`: rust-analyzer kept running after `editor.servers` was switched off, and the embedding
// model could only be stopped from the environment. This starts a window with both on, waits for
// rust-analyzer and the model to be in memory, switches each off in turn through `settings set`, and
// samples the process tree every 250 ms until the memory has come back. One JSON row per step.

import { spawn, execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, rmSync, appendFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const args = process.argv.slice(2);
/** Read a `--name value` argument, or the fallback. */
function option(name, fallback) {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
}
const bin = resolve(option('bin', 'target/release'));
const out = resolve(option('out', '_agent_output/task-2279-perf/switches'));
const template = resolve(option('corpus', '_agent_output/task-2279-perf/template'));
const exe = join(bin, 'unluminous.exe');
const cli = join(bin, 'unluminous-cli.exe');
const run = join(out, 'run');
rmSync(run, { recursive: true, force: true });
const appdata = join(run, 'appdata');
const corpus = join(run, 'corpus');
mkdirSync(join(appdata, 'Unluminous'), { recursive: true });
writeFileSync(join(appdata, 'Unluminous', 'settings.conf'), 'editor.servers = automatic\nsearch.embeddings = true\n');
cpSync(template, corpus, { recursive: true });
execFileSync('git', ['-c', 'core.longpaths=true', '-C', corpus, 'init', '-q']);
const env = { ...process.env, APPDATA: appdata, ATRIUS_INDEX_CACHE: join(run, 'cache') };
const results = join(out, 'results.jsonl');
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

/** The window's own working set and the names of its children. */
function tree(pid) {
  const text = execFileSync('pwsh', ['-NoProfile', '-Command', `
    $all = Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name,WorkingSetSize
    $w = $all | Where-Object ProcessId -eq ${pid}
    $kids = $all | Where-Object ParentProcessId -eq ${pid} | ForEach-Object Name
    @{ ws = $w.WorkingSetSize / 1MB; kids = @($kids) } | ConvertTo-Json -Compress`], { encoding: 'utf8' });
  return JSON.parse(text);
}

const child = spawn(exe, [corpus, '--background'], { env, stdio: 'ignore' });
const pid = child.pid;
/** Drive the window. */
const drive = (...words) => execFileSync(cli, [...words, '--instance', String(pid), '--timeout', '30000'], { env, encoding: 'utf8' });
try {
  for (let at = 0; ; at++) {
    try { drive('status', '--section', 'window'); break; } catch { await sleep(100); if (at > 300) throw new Error('no window'); }
  }
  drive('tab', 'open', 'crates/unluminous-app/src/app/mod.rs', '--permanent');
  // Wait for rust-analyzer and the model.
  let state = tree(pid);
  for (let at = 0; at < 120 && !(state.kids.includes('rust-analyzer.exe') && state.ws > 1000); at++) {
    await sleep(1000);
    state = tree(pid);
  }
  appendFileSync(results, `${JSON.stringify({ step: 'both-on', ws_mb: state.ws, kids: state.kids })}\n`);
  console.log('both on', state);

  drive('settings', 'set', 'editor.servers', 'off');
  const offAt = performance.now();
  for (;;) {
    state = tree(pid);
    if (!state.kids.includes('rust-analyzer.exe') || performance.now() - offAt > 30000) break;
    await sleep(250);
  }
  const serversGone = performance.now() - offAt;
  appendFileSync(results, `${JSON.stringify({ step: 'servers-off', gone_ms: serversGone, still_running: state.kids.includes('rust-analyzer.exe'), kids: state.kids })}\n`);
  console.log('rust-analyzer gone after', serversGone.toFixed(0), 'ms', state.kids);

  const before = state.ws;
  drive('settings', 'set', 'search.embeddings', 'false');
  const embedOffAt = performance.now();
  for (;;) {
    state = tree(pid);
    if (state.ws < before - 500 || performance.now() - embedOffAt > 60000) break;
    await sleep(250);
  }
  const modelGone = performance.now() - embedOffAt;
  appendFileSync(results, `${JSON.stringify({ step: 'embeddings-off', gone_ms: modelGone, ws_before_mb: before, ws_after_mb: state.ws })}\n`);
  console.log('model gone after', modelGone.toFixed(0), 'ms', before.toFixed(0), '->', state.ws.toFixed(0), 'MB');

  drive('settings', 'set', 'search.embeddings', 'true');
  const onAt = performance.now();
  for (;;) {
    state = tree(pid);
    if (state.ws > before - 200 || performance.now() - onAt > 60000) break;
    await sleep(250);
  }
  appendFileSync(results, `${JSON.stringify({ step: 'embeddings-on', loaded_ms: performance.now() - onAt, ws_mb: state.ws })}\n`);
  console.log('model back after', (performance.now() - onAt).toFixed(0), 'ms', state.ws.toFixed(0), 'MB');
} finally {
  try { drive('quit'); } catch { /* closing */ }
  await sleep(2000);
  try { process.kill(pid); } catch { /* gone */ }
}
