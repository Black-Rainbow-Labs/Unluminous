#!/usr/bin/env node
// How smooth the window is to use while the machine is busy (`task-2288`).
//
//   node tools/perf-smooth/bench.mjs --bin <folder with unluminous.exe> --out <variant dir> --rep <n>
//        --load <idle|cap19|all24|over36> [--cases a,b,...] [--presentmon <PresentMon.exe>]
//
// The report was that a window that is quick on an idle machine stutters while a build or the window
// suite holds most of the processors: dragging the window, opening a modal or a menu, resizing a pane.
// Every earlier bench here measured cost on an idle machine. This one measures what a person sees, under
// the load they were under:
//
// - **How long the window keeps Windows waiting.** `probe.ps1` sends the window WM_NULL every sixteen
//   milliseconds from a thread at high priority and times each answer. Every message a mouse makes the
//   window answer goes through the same queue: the hit test, each step of a drag or a resize.
// - **How long an interaction takes to reach the screen.** The window writes `event control <command>
//   <ms>` when a command arrives and `at <ms>` on every frame, on the same clock, so the answer is the
//   frame that completed it minus the arrival. The command line's own start up is not in it.
// - **How evenly frames come while something moves.** `input drag` and `input wheel` move one step a
//   frame, so the gaps between frames during a gesture are what the eye sees. The display here runs at
//   164 Hz, so the floor is 6.1 ms.
// - **Why a frame was slow.** Each frame line carries `cpu`, the window thread's own processor time, so
//   `frame - cpu` is the time it spent waiting for a processor rather than working.
//
// The load is `load.mjs`: cap19 is the window suite's own processor share (the fastest 19 processors,
// which on this machine is every performance core), all24 is every processor, over36 is a build that has
// started more compilers than there are processors. Each case appends one row to <out>/results.jsonl in
// the eval-hillclimb shape, and the trace goes to <out>/traces/. Lower is better for every number.

import { spawn, execFileSync } from 'node:child_process';
import { appendFileSync, copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createInterface } from 'node:readline';

const args = process.argv.slice(2);
/** Read a `--name value` argument, or the fallback. */
function option(name, fallback) {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
}
const bin = path.resolve(option('bin', 'target/release'));
const out = path.resolve(option('out', '_agent_output/perf-smooth/run'));
const rep = Number(option('rep', '0'));
const load = option('load', 'idle');
const only = option('cases', '') ? option('cases', '').split(',') : null;
const presentmon = option('presentmon', '');
// `--env NAME=value`, any number of times: set in the window's environment, so one build can be measured
// with each of its switches.
const windowEnv = args.flatMap((word, at) => (word === '--env' ? [args[at + 1]] : [])).map((pair) => pair.split('='));
const variant = path.basename(out);

const here = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));
const repo = path.resolve(here, '..', '..');
const home = path.join(os.tmpdir(), 'unluminous-smooth-bench', `${variant}-${load}-r${rep}`);
const project = path.join(home, 'project');
const appdata = path.join(home, 'appdata');
mkdirSync(path.join(out, 'traces'), { recursive: true });
const trace = path.join(out, 'traces', `${load}_rep${rep}.trace.txt`);
const results = path.join(out, 'results.jsonl');

os.setPriority(0, os.constants.priority.PRIORITY_HIGH);
const sleep = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
const now = () => performance.timeOrigin + performance.now();

/** A fresh project and settings folder, so nothing remembered decides what is drawn. */
function prepare() {
  rmSync(home, { recursive: true, force: true });
  mkdirSync(path.join(project, 'src'), { recursive: true });
  copyFileSync(path.join(repo, 'crates/unluminous-app/src/app/mod.rs'), path.join(project, 'src/app.rs'));
  copyFileSync(path.join(repo, 'crates/unluminous-app/src/theme/glass.rs'), path.join(project, 'src/glass.rs'));
  copyFileSync(path.join(repo, 'README.md'), path.join(project, 'README.md'));
  copyFileSync(path.join(repo, 'CHANGELOG.md'), path.join(project, 'CHANGELOG.md'));
  execFileSync('git', ['-C', project, 'init', '-q']);
  execFileSync('git', ['-C', project, 'add', '-A']);
  execFileSync('git', ['-C', project, '-c', 'user.name=bench', '-c', 'user.email=bench@example.com', 'commit', '-qm', 'project']);
  mkdirSync(path.join(appdata, 'Unluminous'), { recursive: true });
  // The settings of the person who reported it, as far as they decide what is drawn: the desert picture
  // behind a translucent window, a 20 point interface and a 24 point editor, and the plugins' depth on.
  // The language servers and the embeddings stay off, because what they cost is their own and varies.
  writeFileSync(path.join(appdata, 'Unluminous', 'settings.conf'), [
    'appearance.background.image = landscape-dolomites-cliffs.jpg',
    'appearance.background.opacity = 0.754',
    'appearance.font.family = Consolas',
    'appearance.font.size = 24',
    'appearance.ui.font.size = 20.0',
    'editor.servers = off',
    'plugins.chrome = true',
    'search.embeddings = false',
    'telemetry.enabled = false',
    'update.check = never',
    '',
  ].join('\n'));
}

/** Start the load. Answers the process, or null when idle; `--tied` ends it when this bench lets go of it. */
function startLoad() {
  if (load === 'idle') return null;
  const threads = { cap19: 19, all24: 24, over36: 36 }[load];
  if (!threads) throw new Error(`unknown load ${load}`);
  const script = path.join(here, 'load.mjs');
  const loadArgs = [script, '--threads', String(threads), '--seconds', '1800', '--tied'];
  const command = load === 'cap19'
    ? ['pwsh', ['-NoProfile', '-File', path.join(repo, 'tools/capped.ps1'), 'node', ...loadArgs]]
    : ['node', loadArgs];
  return spawn(command[0], command[1], { stdio: ['pipe', 'pipe', 'inherit'] });
}

let pid = 0;
/** One `unluminous-cli` command against the bench window. */
function cli(...words) {
  return execFileSync(path.join(bin, 'unluminous-cli.exe'), ['--instance', String(pid), '--timeout', '30000', ...words], {
    encoding: 'utf8',
    env: { ...process.env, APPDATA: appdata },
  });
}

/** Every whole line of the trace so far. The file ends in a newline, so the empty piece after it is not a line,
 *  and counting it made a case's first line look as though it had been written before the case began. */
function traceLines() {
  if (!existsSync(trace)) return [];
  const lines = readFileSync(trace, 'utf8').split(/\r?\n/);
  if (lines[lines.length - 1] === '') lines.pop();
  return lines;
}

/** A frame line read into numbers. */
function frameOf(line) {
  const [head, tail = ''] = line.split('|');
  const words = head.trim().split(/\s+/);
  const frame = { total: Number(words[1]), phases: {} };
  for (let at = 2; at + 1 < words.length; at += 2) frame[words[at]] = words[at] === 'core' ? words[at + 1] : Number(words[at + 1]);
  const rest = tail.trim().split(/\s+/).filter(Boolean);
  for (let at = 0; at + 1 < rest.length; at += 2) frame.phases[rest[at]] = (frame.phases[rest[at]] ?? 0) + Number(rest[at + 1]);
  frame.main = frame.total;
  return frame;
}

/** One frame of the 164 Hz display this was measured on, in milliseconds. */
const DISPLAY_INTERVAL = 1000 / 164;
/** A gap this long is the window resting between gestures: `app::HEARTBEAT` is 500 ms. */
const RESTING = 400;
const sorted = (values) => [...values].sort((a, b) => a - b);
const quantile = (values, share) => {
  const list = sorted(values);
  return list.length ? list[Math.min(list.length - 1, Math.floor(list.length * share))] : null;
};
const round = (value) => (value === null || Number.isNaN(value) ? null : Math.round(value * 1000) / 1000);

/** The frames written after one line of the trace, and the events and marks among them, with their times. */
function since(from) {
  const lines = traceLines().slice(from);
  const frames = [];
  const events = [];
  for (const line of lines) {
    if (line.startsWith('frame ')) {
      const frame = frameOf(line);
      // `egui-cpu` is eframe's own count of a whole frame on the window's thread, `UnluminousApp::ui`
      // included. It is read after the next frame's `ui` and noted for the line after that, so the number on
      // this line belongs to the frame two lines up.
      const owner = frames[frames.length - 2];
      if (owner && frame.phases['egui-cpu'] !== undefined) owner.main = frame.phases['egui-cpu'];
      frame.main = frame.total;
      frames.push(frame);
    }
    else if (line.startsWith('event ')) {
      const words = line.split(/\s+/);
      events.push({ name: words[1], detail: words[2], at: Number(words[3]), frame: frames.length });
    } else if (line.startsWith('mark ')) events.push({ name: 'mark', detail: line.split(/\s+/)[1], frame: frames.length });
  }
  return { frames, events };
}

// The helper that times the window's answers, at high priority.
const probe = spawn('pwsh', ['-NoProfile', '-NonInteractive', '-File', path.join(here, 'probe.ps1')], { stdio: ['pipe', 'pipe', 'inherit'] });
const probeLines = createInterface({ input: probe.stdout });
const probeWaiting = [];
probeLines.on('line', (line) => probeWaiting.shift()?.(JSON.parse(line)));
/** Send the probe one command and wait for its answer. */
function ask(line) {
  return new Promise((done) => {
    probeWaiting.push(done);
    probe.stdin.write(`${line}\n`);
  });
}

/** What a case measured, as one row. */
function row(name, split, grade, extra) {
  // `primary` is the case's headline number: the first one its grade names.
  const primary = Object.values(grade).find((value) => value !== null && value !== undefined) ?? null;
  const entry = { prompt_id: `${name}@${load}`, tags: [split, load, name], variant, rep, load, model: 'n/a', grade: { primary, ...grade }, ...extra };
  appendFileSync(results, `${JSON.stringify(entry)}\n`);
  const shown = Object.entries(grade).map(([key, value]) => `${key} ${value === null ? '-' : Number(value).toFixed(1)}`).join(', ');
  console.log(`${variant} ${load} r${rep} ${name}: ${shown}`);
}

/** The numbers every case reports about the frames it drew and the window's answers meanwhile. */
function describe(frames, pings) {
  const gaps = [];
  for (let at = 1; at < frames.length; at++) {
    if (!frames[at].at || !frames[at - 1].at) continue;
    const gap = frames[at].at - frames[at - 1].at;
    // A gap of most of `app::HEARTBEAT` is the window waiting for something to happen, not a frame late.
    if (gap < RESTING) gaps.push(gap);
  }
  const waits = frames.filter((frame) => frame.cpu !== undefined).map((frame) => Math.max(0, frame.total - frame.cpu));
  return {
    frames: frames.length,
    frame_p50: round(quantile(frames.map((frame) => frame.main), 0.5)),
    frame_p95: round(quantile(frames.map((frame) => frame.main), 0.95)),
    gap_p50: round(quantile(gaps, 0.5)),
    gap_p95: round(quantile(gaps, 0.95)),
    gap_max: round(gaps.length ? Math.max(...gaps) : null),
    hitches: gaps.filter((gap) => gap > 2 * DISPLAY_INTERVAL).length,
    wait_p95: round(quantile(waits, 0.95)),
    on_e_cores: frames.length ? round(frames.filter((frame) => frame.core === 'E').length / frames.length) : null,
    msg_p50: round(pings?.p50 ?? null),
    msg_p95: round(pings?.p95 ?? null),
    msg_max: round(pings?.max ?? null),
  };
}

/** Run one case: watch the window's answers, do it, and read the frames it drew. */
async function measure(name, split, body, grade) {
  if (only && !only.includes(name)) return;
  sleep(1200);
  const from = traceLines().length;
  await ask(`watch ${pid} 16`);
  const began = now();
  const extra = await body();
  const ended = now();
  sleep(250);
  const pings = await ask('stop');
  const { frames, events } = since(from);
  const summary = describe(frames, pings);
  row(name, split, grade({ frames, events, summary, extra, began, ended }), { summary, wall_ms: round(ended - began), extra: extra ?? null });
}

/** The `at` of the first frame that ended after the command `detail` arrived, minus the arrival. */
function answered(events, frames, detail, after = 1) {
  const arrived = events.find((event) => event.name === 'control' && event.detail.startsWith(detail));
  if (!arrived) return null;
  const answering = frames.slice(arrived.frame).filter((frame) => frame.at >= arrived.at);
  const frame = answering[after - 1];
  return frame ? frame.at - arrived.at : null;
}

prepare();
const loader = startLoad();
// Let the load generator say it is ready before anything is measured.
if (loader) await new Promise((done) => {
  loader.stdout.on('data', (data) => { if (String(data).includes('ready')) done(); });
  setTimeout(done, 20000);
});
let presentmonChild = null;
try {
  const launched = execFileSync('pwsh', ['-NoProfile', '-Command',
    `$env:APPDATA='${appdata}'; $env:UNLUMINOUS_SESSION='off'; $env:UNLUMINOUS_FRAME_TRACE='${trace}'; ` +
    windowEnv.map(([name, value]) => `$env:${name}='${value}'; `).join('') +
    `(Start-Process -FilePath '${path.join(bin, 'unluminous.exe')}' -ArgumentList @('--background','${project}') -PassThru).Id`,
  ], { encoding: 'utf8' }).trim();
  pid = Number(launched);
  for (let attempt = 0; ; attempt++) {
    try {
      execFileSync(path.join(bin, 'unluminous-cli.exe'), ['--instance', String(pid), '--timeout', '1000', 'status', '--quiet'], { env: { ...process.env, APPDATA: appdata }, stdio: 'ignore' });
      break;
    } catch {
      if (attempt > 200) throw new Error('the window never answered');
      sleep(100);
    }
  }
  cli('window', 'size', '--width', '2200', '--height', '1500');
  cli('window', 'position', '--x', '120', '--y', '120');
  cli('tab', 'open', 'src/app.rs');
  cli('explorer', 'show');
  cli('plugins', 'pane', 'agent-chat/chat', '--show');
  cli('plugins', 'run', 'agent-chat', 'gallery');
  cli('plugins', 'pane', 'agent-tasks/board', '--show');
  sleep(5000);
  const panels = JSON.parse(cli('panel', 'list', '--json'));
  if (presentmon) {
    presentmonChild = spawn(presentmon, ['--process_id', String(pid), '--output_file', path.join(out, 'traces', `${load}_rep${rep}.presentmon.csv`),
      '--stop_existing_session', '--session_name', `smooth-${process.pid}`, '--no_console_stats', '--date_time', '--v1_metrics'], { stdio: 'ignore' });
    sleep(1500);
  }

  // At rest: the window draws its heartbeat and nothing else, and every message should be answered at once.
  await measure('rest', 'train', async () => { sleep(4000); }, ({ summary }) => ({ msg_p95: summary.msg_p95, msg_max: summary.msg_max }));

  // A drag of the window by its title bar: each step is a message the window has to answer before the
  // window moves, so the step's wait is the stutter.
  await measure('drag-window', 'test', async () => {
    const steps = [];
    for (let time = 0; time < 3; time++) steps.push(await ask(`drag ${pid} 60 8`));
    return { step_p50: Math.max(...steps.map((step) => step.p50)), step_p95: Math.max(...steps.map((step) => step.p95)), step_max: Math.max(...steps.map((step) => step.max)) };
  }, ({ extra }) => ({ step_p95: extra.step_p95, step_max: extra.step_max }));

  // Settings, opened and closed three times: from the command arriving to the frame the modal is
  // complete on, which is the moment it has finished opening.
  await measure('open-modal', 'test', async () => {
    for (let time = 0; time < 3; time++) {
      cli('modal', 'open', 'settings');
      sleep(900);
      cli('modal', 'cancel');
      // Past rux's twenty seconds of keeping a canvas nobody draws, so every opening is a cold one, which is
      // how a person meets Settings: once in a while, not three times in a row.
      if (time < 2) sleep(21000);
    }
  }, ({ events, frames, summary }) => {
    const opens = [];
    for (const event of events.filter((item) => item.name === 'control' && item.detail === 'modal.open')) {
      // A mark is written while its frame runs and the frame's line when it ends, so the frame a mark
      // belongs to is the next frame line after it.
      const complete = events.find((item) => item.name === 'mark' && item.detail === 'modal-complete' && item.frame >= event.frame);
      const frame = complete ? frames[complete.frame] : null;
      if (frame && frame.at >= event.at && frame.at - event.at < 1500) opens.push(frame.at - event.at);
    }
    return { open_ms: round(quantile(opens, 0.5)), open_max: round(opens.length ? Math.max(...opens) : null), opens: opens.map(round), gap_p95: summary.gap_p95 };
  });

  // A menu, opened by a click on its name in the title bar and closed with Escape.
  await measure('open-menu', 'train', async () => {
    for (let time = 0; time < 3; time++) {
      cli('input', 'click', '96', '18');
      sleep(600);
      cli('input', 'key', 'Escape');
      sleep(500);
    }
  }, ({ events, frames, summary }) => {
    const clicks = events.filter((item) => item.name === 'control' && item.detail.startsWith('input'));
    const opened = [];
    for (const click of clicks) {
      // A click is three frames: the pointer arrives, the button goes down, it comes up and the menu opens.
      const after = frames.slice(click.frame).filter((frame) => frame.at >= click.at);
      if (after[3]) opened.push(after[3].at - click.at);
    }
    return { open_ms: round(quantile(opened, 0.5)), gap_p95: summary.gap_p95 };
  });

  // The explorer and the chat pane, hidden and shown: from the command to the first frame without them.
  await measure('toggle-panes', 'test', async () => {
    for (let time = 0; time < 3; time++) {
      cli('explorer', 'toggle');
      sleep(400);
      cli('explorer', 'toggle');
      sleep(400);
      cli('plugins', 'pane', 'agent-chat/chat', '--hide');
      sleep(400);
      cli('plugins', 'pane', 'agent-chat/chat', '--show');
      sleep(400);
    }
  }, ({ events, frames, summary }) => {
    const answers = events.filter((item) => item.name === 'control').map((event) => {
      const frame = frames.slice(event.frame).find((item) => item.at >= event.at);
      return frame ? frame.at - event.at : null;
    }).filter((value) => value !== null);
    return { toggle_ms: round(quantile(answers, 0.5)), toggle_max: round(answers.length ? Math.max(...answers) : null), frame_p95: summary.frame_p95 };
  });

  // The divider between the explorer and the editor, dragged out and back in thirty steps a way.
  const explorer = (panels.panels ?? panels).find?.((panel) => panel.name === 'explorer' || panel.id === 'explorer');
  const edge = explorer?.rect ? explorer.rect.x + explorer.rect.width : 300;
  await measure('drag-divider', 'train', async () => {
    cli('input', 'drag', String(edge), '600', '--to-x', String(edge + 300), '--to-y', '600', '--steps', '30');
    cli('input', 'drag', String(edge + 300), '600', '--to-x', String(edge), '--to-y', '600', '--steps', '30');
  }, ({ summary }) => ({ gap_p95: summary.gap_p95, gap_p50: summary.gap_p50, wait_p95: summary.wait_p95 }));

  // The editor scrolled with the wheel: each notch glides, so the frames come one after another.
  await measure('scroll', 'test', async () => {
    cli('input', 'move', '1000', '700');
    for (let time = 0; time < 8; time++) { cli('input', 'wheel', '-4'); sleep(60); }
    for (let time = 0; time < 8; time++) { cli('input', 'wheel', '4'); sleep(60); }
  }, ({ summary }) => ({ gap_p95: summary.gap_p95, frame_p95: summary.frame_p95, wait_p95: summary.wait_p95 }));

  // Typing a line: one character a frame.
  await measure('type', 'train', async () => {
    cli('input', 'click', '1000', '700');
    cli('input', 'text', 'let quick = brown_fox(jumps, over, lazy_dog);');
  }, ({ summary }) => ({ gap_p95: summary.gap_p95, frame_p95: summary.frame_p95 }));

  // The pointer moved across the chat and the board, which redraws hover on every frame.
  await measure('hover', 'train', async () => {
    cli('input', 'drag', '1700', '300', '--to-x', '1700', '--to-y', '1300', '--steps', '40');
  }, ({ summary }) => ({ gap_p95: summary.gap_p95, msg_p95: summary.msg_p95 }));
} finally {
  try { cli('quit'); } catch { /* the kill below is the answer */ }
  sleep(1500);
  try { process.kill(pid); } catch { /* already gone */ }
  presentmonChild?.kill();
  loader?.stdin.end();
  loader?.kill();
  probe.stdin.write('quit\n');
  probe.stdin.end();
  setTimeout(() => probe.kill(), 2000).unref();
}
