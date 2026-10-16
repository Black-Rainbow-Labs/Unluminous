#!/usr/bin/env node
// The speed bench for the glass theme (task-2281): one build, one rep, four workloads.
//
//   node tools/glass-bench/bench.mjs <bin folder> <variant dir> <rep>
//
// It starts the build in the background, without taking the keyboard, on a small project under the temporary
// folder with a settings folder of its own and a frame trace, in the glass theme over glass-spheres.jpg with
// the Agent-Chat gallery and the Agent-Tasks board showing (BENCH_THEME names another theme), and then:
//
// - drag: moves the window 60 times through `window position`, which reaches the same `Moved` handling a
//   drag with the pointer does, and reads the frames drawn meanwhile. A frame's cost on the main thread is
//   `frame` (UnluminousApp::ui) plus `egui-cpu` (egui's own layout and tessellation of it).
// - modal: opens Settings three times, each from closed with a pause long enough that the modal photographs
//   the window again, and reads the marks modal.rs writes: from `modal-photo-asked` to `modal-photo-shown`,
//   the frames between them and the costliest of those frames.
//
// It appends one row per workload to <variant dir>/results.jsonl in the eval-hillclimb shape and writes
// the trace to <variant dir>/traces/<workload>_rep<k>.txt. Lower is better for every number.

import { execFileSync, spawnSync } from 'node:child_process';
import { appendFileSync, copyFileSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const [bin, variantDir, repText] = process.argv.slice(2);
if (!bin || !variantDir) {
  console.error('usage: node bench.mjs <bin folder> <variant dir> <rep>');
  process.exit(2);
}
const rep = Number(repText || 0);
const here = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));
const repo = path.resolve(here, '..', '..');
const home = path.join(os.tmpdir(), 'unluminous-glass-bench');
const project = path.join(home, 'project');
const appdata = path.join(home, 'appdata');
// The same two files every time, so every build opens the same thing.
mkdirSync(path.join(project, 'src'), { recursive: true });
copyFileSync(path.join(repo, 'crates/unluminous-app/src/theme/glass.rs'), path.join(project, 'src/glass.rs'));
copyFileSync(path.join(repo, 'README.md'), path.join(project, 'README.md'));
mkdirSync(`${variantDir}/traces`, { recursive: true });
const trace = path.resolve(`${variantDir}/traces/run_rep${rep}.txt`);
rmSync(trace, { force: true });
mkdirSync(appdata, { recursive: true });

const sleep = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);

// Start the window the way drive-a-window.ps1 does: in the background, so nobody's keyboard moves.
const started = execFileSync(
  'pwsh',
  [
    '-NoProfile',
    '-Command',
    `$env:APPDATA='${appdata}'; $env:UNLUMINOUS_SESSION='off'; $env:UNLUMINOUS_FRAME_TRACE='${trace}'; ` +
      `(Start-Process -FilePath '${bin}/unluminous.exe' -ArgumentList @('--background','${project}') -PassThru).Id`,
  ],
  { encoding: 'utf8' },
).trim();
const pid = Number(started);

/** One unluminous-cli command against the bench window, answering its standard output. */
function cli(...args) {
  const result = spawnSync(`${bin}/unluminous-cli.exe`, ['--instance', String(pid), ...args], {
    encoding: 'utf8',
    env: { ...process.env, APPDATA: appdata },
  });
  return result.stdout;
}

sleep(4000);
cli('window', 'size', '--width', '1600', '--height', '1000');
cli('window', 'position', '--x', '80', '--y', '60');
cli('theme', 'set', process.env.BENCH_THEME || 'Unluminous Glass');
cli('background', 'use', 'glass-spheres.jpg');
cli('tab', 'open', 'src/glass.rs');
cli('plugins', 'pane', 'agent-chat/chat', '--show');
cli('plugins', 'run', 'agent-chat', 'gallery');
cli('plugins', 'run', 'agent-chat', 'motion', 'off');
cli('terminal', 'hide');
cli('plugins', 'pane', 'agent-tasks/board', '--show');
sleep(4000);

/** The trace's lines so far. */
const lines = () => (existsSync(trace) ? readFileSync(trace, 'utf8').split(/\r?\n/) : []);

/** A frame line read into its total and its phases. */
function frameOf(line) {
  const words = line.trim().split(/\s+/);
  const frame = { ui: Number(words[1]), phases: {} };
  const bar = words.indexOf('|');
  for (let at = bar + 1; at + 1 < words.length; at += 2) {
    if (/^[a-z]/.test(words[at])) frame.phases[words[at]] = Number(words[at + 1]);
  }
  frame.cpu = frame.ui + (frame.phases['egui-cpu'] || 0);
  return frame;
}

const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted.length ? sorted[Math.floor(sorted.length / 2)] : NaN;
};
const percentile = (values, share) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * share))] : NaN;
};

// idle: five seconds with nothing happening. A window that keeps drawing while idle competes with a drag.
const idleFrom = lines().length;
sleep(5000);
const idleFrames = lines()
  .slice(idleFrom)
  .filter((line) => line.startsWith('frame '))
  .map(frameOf);
const idle = {
  frames_per_s: idleFrames.length / 5,
  cpu_ms_per_s: idleFrames.reduce((sum, frame) => sum + frame.cpu, 0) / 5,
};

// move: the window moved from the command line, which is not a move loop.
const before = lines().length;
const dragStart = Date.now();
for (let step = 0; step < 60; step++) {
  const x = 80 + (step < 30 ? step : 60 - step) * 8;
  cli('window', 'position', '--x', String(x), '--y', String(60 + (step % 2)));
}
const dragWall = Date.now() - dragStart;
sleep(1000);
const dragFrames = lines()
  .slice(before)
  .filter((line) => line.startsWith('frame '))
  .map(frameOf);
const dragCpu = dragFrames.map((frame) => frame.cpu);
const move = {
  frames: dragFrames.length,
  median_ms: median(dragCpu),
  p95_ms: percentile(dragCpu, 0.95),
  max_ms: Math.max(...dragCpu),
  over_16ms: dragCpu.filter((value) => value > 16.7).length,
  wall_ms_per_move: dragWall / 60,
};

// drag: Windows' move loop, as drag.ps1 delivers it. How long each step kept the drag waiting, and how many
// frames the window drew while it moved.
sleep(1000);
const dragFrom = lines().length;
const dragOut = execFileSync(
  'pwsh',
  ['-NoProfile', '-File', path.join(here, 'drag.ps1'), String(pid), '60', '8'],
  { encoding: 'utf8' },
).trim();
sleep(1000);
const drag = {
  ...JSON.parse(dragOut.split(/\r?\n/).pop()),
  frames: lines().slice(dragFrom).filter((line) => line.startsWith('frame ')).length,
};

// modal
for (let open = 0; open < 3; open++) {
  cli('modal', 'open', 'settings');
  sleep(2500);
  cli('modal', 'cancel');
  sleep(2000);
}
cli('quit');
sleep(1500);
// Read once the window has gone, so every mark has been flushed.
const all = lines();
const at = (line) => Number(line.trim().split(/\s+/)[2]);
// A modal's opening, from the frame it is first drawn to the end of the frame it is drawn complete, plus
// egui's own layout and tessellation of that frame, which the next frame's line reports as `egui-cpu`.
// Builds before the opening marks have only the photograph's, which mark the same two frames.
const newer = all.some((line) => line.startsWith('mark modal-opened'));
const opened = newer ? 'mark modal-opened' : 'mark modal-photo-asked';
const complete = newer ? 'mark modal-complete' : 'mark modal-photo-shown';
const nextFrame = (from) => all.findIndex((line, later) => later > from && line.startsWith('frame '));
const opens = [];
for (let index = 0; index < all.length; index++) {
  if (!all[index].startsWith(opened)) continue;
  const done = all.findIndex((line, later) => later > index && line.startsWith(complete));
  if (done < 0) continue;
  const doneFrame = nextFrame(done);
  const after = nextFrame(doneFrame);
  if (doneFrame < 0 || after < 0) continue;
  const between = all.slice(index, doneFrame + 1).filter((line) => line.startsWith('frame ')).map(frameOf);
  opens.push({
    latency_ms:
      at(all[done]) - at(all[index]) + frameOf(all[doneFrame]).ui + (frameOf(all[after]).phases['egui-cpu'] || 0),
    frames: between.length,
    max_ms: Math.max(...between.map((frame) => frame.cpu)),
  });
}
const modal = {
  latency_ms: median(opens.map((open) => open.latency_ms)),
  frames: median(opens.map((open) => open.frames)),
  max_ms: median(opens.map((open) => open.max_ms)),

  opens,
};

const row = (id, perf, grade) =>
  JSON.stringify({ prompt_id: id, rep, model: 'n/a', grade, perf, tags: [id] }) + '\n';
appendFileSync(`${variantDir}/results.jsonl`, row('idle', idle, { cpu_ms_per_s: idle.cpu_ms_per_s }));
appendFileSync(`${variantDir}/results.jsonl`, row('move', move, { median_ms: move.median_ms }));
appendFileSync(`${variantDir}/results.jsonl`, row('drag', drag, { p95_ms: drag.p95_ms }));
appendFileSync(`${variantDir}/results.jsonl`, row('modal', modal, { latency_ms: modal.latency_ms }));
console.log(JSON.stringify({ rep, idle, move, drag, modal: { ...modal, opens: undefined } }));
