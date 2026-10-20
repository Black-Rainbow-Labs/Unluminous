// Starts and stops the one Unluminous window the accuracy eval drives, and puts everything back
// before each case.
//
// Each case gets a fresh window, a fresh copy of the sample project and a fresh settings folder. The
// study in `tools/agent-study` reset one long running window with a list of `unluminous-cli` calls,
// which left whatever the list forgot: a theme a case chose, a font size, a canvas, a ticket. A new
// process on a copied settings folder forgets everything by construction.
//
// Nothing here touches the person's own windows or settings. The window runs with `APPDATA` pointed
// at the eval's own folder, and every `unluminous-cli` call, including the `mcp serve` the agent
// starts, is given `UNLUMINOUS_INSTANCES` and `UNLUMINOUS_INSTANCE` so it can only find this window.
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

export const REPO = path.resolve(import.meta.dirname, '..', '..');
export const ROOT = process.env.EVAL_ROOT ?? path.join(os.tmpdir(), 'unluminous-agent-eval');
export const PROJECT = path.join(ROOT, 'scratch-project');
export const APPDATA = path.join(ROOT, 'appdata');
export const TEMPLATE = path.join(ROOT, 'appdata-template');
export const INSTANCES = path.join(APPDATA, 'Unluminous', 'instances');
const EXE = process.platform === 'win32' ? '.exe' : '';
/**
 * The window and the reader the checks ask through. Fixed for a whole flow, so two variants are
 * graded by the same program; a variant that changes the window itself names another folder.
 */
let BIN = process.env.UNLUMINOUS_BIN ?? path.join(REPO, 'target', 'release');
export let APP = path.join(BIN, `unluminous${EXE}`);
export let CLI = path.join(BIN, `unluminous-cli${EXE}`);

/** Points the window and the reader at the build in `folder`. */
export function useWindowBuild(folder) {
  BIN = path.resolve(folder);
  APP = path.join(BIN, `unluminous${EXE}`);
  CLI = path.join(BIN, `unluminous-cli${EXE}`);
}

/** The environment every program that talks to the eval's window runs with. */
export function windowEnv(pid) {
  const env = { ...process.env, UNLUMINOUS_INSTANCES: INSTANCES };
  if (pid) env.UNLUMINOUS_INSTANCE = String(pid);
  return env;
}

/** Runs one `unluminous-cli` command against the eval's window and answers with its output. */
export function cli(pid, args, { json = true, timeout = 30000 } = {}) {
  const r = spawnSync(CLI, json ? [...args, '--json'] : args, {
    env: windowEnv(pid), encoding: 'utf8', timeout,
  });
  const text = `${r.stdout ?? ''}${r.stderr ?? ''}`.trim();
  if (!json) return { ok: r.status === 0, text };
  try {
    return { ok: r.status === 0, text, value: JSON.parse(r.stdout) };
  } catch {
    return { ok: r.status === 0, text, value: null };
  }
}

/** Writes the sample project from nothing, so a case cannot see what the case before it did. */
export function freshProject() {
  fs.rmSync(PROJECT, { recursive: true, force: true });
  const r = spawnSync('node', [path.join(REPO, 'tools', 'agent-study', 'make-sample-project.mjs')], {
    env: { ...process.env, STUDY_PROJECT: PROJECT }, encoding: 'utf8',
  });
  if (r.status !== 0) throw new Error(`could not write the sample project: ${r.stderr}`);
}

/** Copies the settings folder every case starts from over the one the last case used. */
function freshSettings() {
  fs.rmSync(APPDATA, { recursive: true, force: true });
  if (fs.existsSync(TEMPLATE)) fs.cpSync(TEMPLATE, APPDATA, { recursive: true });
  else fs.mkdirSync(APPDATA, { recursive: true });
  // The setup dialog opens by itself in a window whose setup was never finished, and a modal takes
  // every key press, so every case would start by fighting it. Written as declined and finished.
  const folder = path.join(APPDATA, 'Unluminous');
  fs.mkdirSync(folder, { recursive: true });
  fs.writeFileSync(path.join(folder, 'setup.conf'), 'setup.prompt = off\nsetup.finished = true\n');
}

/** Waits until a window answers `status`, or gives up. */
async function answering(pid, ms) {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    if (cli(pid, ['status'], { timeout: 5000 }).ok) return true;
    await new Promise(r => setTimeout(r, 500));
  }
  return false;
}

/** Starts a window on the project, in the background, and answers with its process id. */
export async function start() {
  const child = spawn(APP, [PROJECT, '--background'], {
    env: { ...process.env, APPDATA, UNLUMINOUS_SESSION: 'off', UNLUMINOUS_TELEMETRY: 'off' },
    detached: true, stdio: 'ignore',
  });
  child.unref();
  if (!(await answering(child.pid, 60000))) {
    stop(child.pid);
    throw new Error('the eval window did not answer within a minute');
  }
  return child.pid;
}

/** Closes the window, by asking first and by its process id if it does not go. */
export function stop(pid) {
  if (!pid) return;
  cli(pid, ['quit'], { timeout: 5000 });
  const until = Date.now() + 5000;
  while (Date.now() < until) {
    try { process.kill(pid, 0); } catch { return; }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250);
  }
  try { process.kill(pid); } catch {}
}

/** A fresh project, a fresh settings folder and a fresh window: everything a case starts from. */
export async function freshWindow() {
  freshProject();
  freshSettings();
  return start();
}

/**
 * Makes the settings folder every case is copied from, once: a window started on an empty folder
 * writes the bundled backgrounds and its first settings, which takes long enough to be worth doing
 * once rather than every case.
 */
export async function makeTemplate() {
  if (fs.existsSync(TEMPLATE)) return;
  freshProject();
  fs.rmSync(APPDATA, { recursive: true, force: true });
  freshSettings();
  const pid = await start();
  stop(pid);
  fs.rmSync(path.join(APPDATA, 'Unluminous', 'instances'), { recursive: true, force: true });
  fs.cpSync(APPDATA, TEMPLATE, { recursive: true });
}
