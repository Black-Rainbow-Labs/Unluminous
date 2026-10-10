#!/usr/bin/env node
// The Unluminous engine of the completion evaluation (`task-2231` §8.1).
//
// It starts a background window (`unluminous --background`, which never takes the keyboard) on a run
// copy of each corpus, with an `APPDATA` of its own so no remembered tab, setting or session of the
// person's decides anything, and speaks the control channel directly (`unluminous-cli/docs/protocol.md`)
// rather than starting `unluminous-cli` three times a query.
//
// For every query it puts the edited text in the tab with `editor.set-text` (the identifier cut to its
// prefix, and for `needs-import` the import statement deleted), asks `editor.complete` at the caret, and
// puts the text back with `editor.undo`. Nothing is ever saved. The time recorded is the
// `editor.complete` round trip alone.
//
//   node tools/completion-eval/engine-unluminous.mjs --run <name> --binary <folder holding unluminous.exe>
//        [--split tune|held|all] [--corpus <name>] [--limit <n per corpus>] [--controls]
//        [--servers off|automatic] [--wait <ms>] [--warm <seconds>]

import crypto from 'node:crypto';
import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { CORPORA_ROOT, EVAL_ROOT } from './prepare-corpora.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

/** The command line, as a map of `--name value` and a set of switches. */
function readArguments(argv) {
  const options = {};
  for (let i = 0; i < argv.length; i++) {
    if (!argv[i].startsWith('--')) continue;
    const name = argv[i].slice(2);
    const next = argv[i + 1];
    if (next === undefined || next.startsWith('--')) options[name] = true;
    else options[name] = argv[++i];
  }
  return options;
}

/**
 * Copies a pristine corpus for one run, linking `node_modules` rather than copying it.
 * @param name - the corpus
 * @param dest - where the run's copy goes
 */
export function copyCorpus(name, dest) {
  const source = path.join(CORPORA_ROOT, name);
  if (!fs.existsSync(dest)) {
    fs.mkdirSync(dest, { recursive: true });
    // robocopy answers 0 to 7 for success.
    try {
      // `/R:0 /W:0`: robocopy's default is a million retries thirty seconds apart, and ripgrep's corpus holds
      // one file Windows will not copy, which held a whole run for an hour.
      execFileSync('robocopy', [source, dest, '/MIR', '/XD', 'node_modules', '/R:0', '/W:0', '/NFL', '/NDL', '/NJH', '/NJS', '/NP'], { stdio: 'ignore' });
    } catch (error) {
      if (error.status >= 16) throw error;
    }
    const modules = path.join(source, 'node_modules');
    if (fs.existsSync(modules)) fs.symlinkSync(fs.realpathSync(modules), path.join(dest, 'node_modules'), 'junction');
  }
  return dest;
}

/**
 * The text a query asks about, and where its caret is, in bytes.
 * @param original - the file as a Buffer, with `\n` line breaks
 * @param position - the position
 * @param prefix - the prefix typed
 */
export function queryText(original, position, prefix) {
  const cuts = [...position.remove.map((r) => ({ ...r, text: '' })), { start: position.start, end: position.end, text: prefix }];
  cuts.sort((a, b) => b.start - a.start);
  let bytes = original;
  for (const cut of cuts) {
    bytes = Buffer.concat([bytes.subarray(0, cut.start), Buffer.from(cut.text, 'utf8'), bytes.subarray(cut.end)]);
  }
  const removedBefore = position.remove.filter((r) => r.end <= position.start).reduce((sum, r) => sum + (r.end - r.start), 0);
  return { text: bytes.toString('utf8'), caret: position.start - removedBefore + Buffer.byteLength(prefix, 'utf8') };
}

/** A window this engine started, and how to talk to it. */
export class Window {
  constructor(pid, port, token) {
    Object.assign(this, { pid, port, token });
  }

  /** Sends one request and resolves with the reply. */
  ask(command, args = {}, deadline = 30000) {
    return new Promise((resolve, reject) => {
      const socket = net.connect(this.port, '127.0.0.1');
      let data = '';
      socket.setEncoding('utf8');
      socket.setTimeout(deadline + 5000, () => {
        socket.destroy();
        reject(new Error(`${command} timed out`));
      });
      socket.on('connect', () => socket.write(`${JSON.stringify({ token: this.token, command, arguments: args, deadline_ms: deadline })}\n`));
      socket.on('data', (chunk) => {
        data += chunk;
        const end = data.indexOf('\n');
        if (end >= 0) {
          socket.destroy();
          try {
            resolve(JSON.parse(data.slice(0, end)));
          } catch (error) {
            reject(error);
          }
        }
      });
      socket.on('error', reject);
    });
  }
}

/**
 * Starts a background window on a folder and waits for its control channel.
 * @param binary - the folder holding unluminous.exe
 * @param folder - the project
 * @param appdata - the APPDATA it is given
 */
export async function startWindow(binary, folder, appdata) {
  const env = { ...process.env, APPDATA: appdata };
  const child = spawn(path.join(binary, 'unluminous.exe'), [folder, '--background'], { env, stdio: 'ignore' });
  const instance = path.join(appdata, 'Unluminous', 'instances', `${child.pid}.conf`);
  const began = Date.now();
  for (;;) {
    if (fs.existsSync(instance)) {
      const values = Object.fromEntries(
        fs.readFileSync(instance, 'utf8').split('\n').filter((l) => l.includes('=')).map((l) => l.split('=').map((s) => s.trim())),
      );
      const window = new Window(child.pid, Number(values.port), values.token);
      try {
        const reply = await window.ask('status', {}, 2000);
        if (reply.ok) return { window, child, ready: Date.now() - began };
      } catch {
        // not answering yet
      }
    }
    if (Date.now() - began > 120000) throw new Error(`the window on ${folder} never answered`);
    await sleep(100);
  }
}

/** Stops a window: asked to quit, then killed by its own process id. */
export async function stopWindow(window, child) {
  try {
    await window.ask('quit', {}, 3000);
  } catch {
    // the kill below is the answer
  }
  await sleep(1500);
  try {
    process.kill(child.pid);
  } catch {
    // already gone
  }
}

/**
 * A label cut to the name a person would type: `vec![…]` is `vec`, `draw(…)` is `draw`, and a label
 * with a detail after a space keeps only the name. The README's contract says each engine does this
 * for its own labels.
 * @param label - the label as offered
 */
export function typedName(label) {
  const match = /^[\p{L}\p{N}_$]+/u.exec(label);
  return match ? match[0] : label;
}

/** The labels of a reply to `editor.complete`. */
function labelsOf(reply) {
  if (!reply.ok) return [];
  return (reply.result?.rows || []).map((row) => typedName(row.name));
}

/**
 * Waits for the language server that answers for the showing file to be ready, so a run measures a
 * warm server and not one still reading the project. Gives up after `READY_WITHIN` and says what state
 * it was left in, which the run records.
 * @param window - the window
 */
async function waitForTheServer(window) {
  const began = performance.now();
  let last = 'none';
  // rust-analyzer can say it is ready between answering `initialize` and starting to index, so ready
  // only counts once it has held for STEADY_FOR without the server going back to indexing.
  let readySince = null;
  while (performance.now() - began < READY_WITHIN) {
    const reply = await window.ask('status', { section: 'servers' });
    const running = reply.result?.servers?.running || [];
    const mine = running.find((s) => s.answersTheShowingFile) || running[0];
    last = mine ? mine.state : 'none';
    const done = mine && ['failed', 'absent'].includes(mine.state);
    if (mine?.state === 'ready') readySince ??= performance.now();
    else readySince = null;
    if (done || (readySince !== null && performance.now() - readySince >= STEADY_FOR)) {
      console.log(`  server ${mine.server}: ${mine.describe} after ${Math.round(performance.now() - began)} ms`);
      return { state: mine.state, describe: mine.describe, ms: Math.round(performance.now() - began) };
    }
    await sleep(1000);
  }
  console.log(`  server never became ready; last state ${last}`);
  return { state: last, ms: READY_WITHIN };
}

/** How long a language server may take to read a project before a run gives up waiting for it. */
const READY_WITHIN = 10 * 60 * 1000;
/** How long a server has to stay ready, without indexing again, before a run starts asking it. */
const STEADY_FOR = 15 * 1000;

async function main() {
  const options = readArguments(process.argv.slice(2));
  if (!options.run || !options.binary) {
    console.error('usage: engine-unluminous.mjs --run <name> --binary <folder> [--split tune|held|all] [--corpus <name>] [--limit <n>] [--controls] [--servers off|automatic] [--wait <ms>]');
    process.exit(2);
  }
  const positionsPath = options.positions || path.join(HERE, 'positions.json');
  const positionsBytes = fs.readFileSync(positionsPath);
  const frozen = JSON.parse(positionsBytes.toString('utf8'));
  const split = options.split || 'tune';
  const runFolder = path.join(EVAL_ROOT, 'runs', options.run);
  fs.mkdirSync(runFolder, { recursive: true });
  const resultsPath = path.join(runFolder, 'results.jsonl');
  const done = new Set();
  if (fs.existsSync(resultsPath)) {
    for (const line of fs.readFileSync(resultsPath, 'utf8').split('\n').filter(Boolean)) {
      const row = JSON.parse(line);
      done.add(`${row.id}#${row.prefix}`);
    }
  }
  const appdata = path.join(runFolder, 'appdata');
  fs.mkdirSync(appdata, { recursive: true });
  const previous = fs.existsSync(path.join(runFolder, 'run.json')) ? JSON.parse(fs.readFileSync(path.join(runFolder, 'run.json'), 'utf8')) : null;
  const run = {
    engine: 'unluminous',
    binary: options.binary,
    servers: options.servers || 'default',
    wait: options.wait ? Number(options.wait) : null,
    split,
    positions: positionsPath,
    positionsSha256: crypto.createHash('sha256').update(positionsBytes).digest('hex'),
    started: previous?.started || new Date().toISOString(),
    // A resumed run keeps what the earlier part of it recorded about each corpus.
    corpora: previous?.corpora || {},
  };
  const corpora = Object.entries(frozen.corpora).filter(([name, spec]) => {
    if (options.corpus && options.corpus !== name) return false;
    return options.controls ? true : !spec.control;
  });
  for (const [name] of corpora) {
    let chosen = frozen.positions.filter((p) => p.corpus === name && (split === 'all' || p.split === split));
    if (options.limit) chosen = chosen.slice(0, Number(options.limit));
    if (!chosen.length) continue;
    const copy = copyCorpus(name, path.join(runFolder, 'corpora', name));
    const { window, child, ready } = await startWindow(options.binary, copy, appdata);
    console.log(`${name}: window ${window.pid} answered after ${ready} ms`);
    try {
      if (options.servers) {
        const set = await window.ask('settings.set', { key: 'editor.servers', value: options.servers });
        if (!set.ok) console.log(`settings.set editor.servers: ${set.error?.message}`);
      }
      // The window builds its index of the project on a thread; give it the time a person would have
      // spent looking at the window before typing.
      await sleep(Number(options.warm || 20) * 1000);
      run.corpora[name] = { ...(run.corpora[name] || {}), ready, queries: run.corpora[name]?.queries || 0 };
      delete run.corpora[name].server;
      let openPath = null;
      let original = null;
      for (const position of chosen) {
        for (const p of [0, 1, 2, 3]) {
          const prefix = [...position.expected].slice(0, p).join('');
          if (p > [...position.expected].length) continue;
          if (done.has(`${position.id}#${p}`)) continue;
          if (openPath !== position.path) {
            if (openPath) await window.ask('tab.close', { discard: true });
            const opened = await window.ask('tab.open', { path: position.path, permanent: true });
            if (!opened.ok) throw new Error(`tab.open ${position.path}: ${opened.error?.message}`);
            openPath = position.path;
            if (options.servers === 'automatic' && !run.corpora[name].server) run.corpora[name].server = await waitForTheServer(window);
            original = Buffer.from(fs.readFileSync(path.join(copy, position.path)).toString('utf8').replaceAll('\r\n', '\n'), 'utf8');
          }
          const { text, caret } = queryText(original, position, prefix);
          const row = { id: position.id, prefix: p };
          try {
            // `editor set-text` reads backslash escapes, so `\\n` in a string in the source would arrive
            // as a line break and move every offset after it. Each backslash is doubled to arrive as one.
            const set = await window.ask('editor.set-text', { text: text.replaceAll('\\', '\\\\') });
            if (!set.ok) throw new Error(`set-text: ${set.error?.message}`);
            const args = { offset: caret, limit: 50 };
            // `--first`: the list the popup draws at once, before any server has answered, and when
            // it came. G4 is the time to a list holding the right answer, and that list may be this one.
            if (options.first) {
                const began0 = performance.now();
                const first = await window.ask('editor.complete', { ...args, wait: 0 });
                row.ms0 = performance.now() - began0;
                row.labels0 = labelsOf(first);
            }
            if (options.wait) args.wait = Number(options.wait);
            const began = performance.now();
            const reply = await window.ask('editor.complete', args);
            row.ms = performance.now() - began;
            row.labels = labelsOf(reply);
            if (!reply.ok && reply.error?.code !== 'not-applicable') row.error = reply.error?.message;
            const undo = await window.ask('editor.undo');
            if (!undo.ok) throw new Error(`undo: ${undo.error?.message}`);
          } catch (error) {
            row.labels = row.labels || [];
            row.error = String(error.message || error);
          }
          fs.appendFileSync(resultsPath, `${JSON.stringify(row)}\n`);
          run.corpora[name].queries += 1;
        }
      }
      if (openPath) await window.ask('tab.close', { discard: true });
    } finally {
      await stopWindow(window, child);
    }
    run.ended = new Date().toISOString();
    fs.writeFileSync(path.join(runFolder, 'run.json'), JSON.stringify(run, null, 2));
  }
  run.ended = new Date().toISOString();
  fs.writeFileSync(path.join(runFolder, 'run.json'), JSON.stringify(run, null, 2));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
