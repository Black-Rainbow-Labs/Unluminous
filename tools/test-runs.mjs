// What `tools/test.mjs` and `tools/window-suite.mjs --run` share about running cargo's tests:
// a temporary folder for each run, reading libtest's output, running a failed test again, and
// the record of every run and every test that failed and then passed.
//
// `task-2364`. task-2359 spent hours getting a finished change through the gate, the window suite
// and the release, and most of it went to failures that had nothing to do with the change:
//
//   - 19 of the 28 Jupyter kernel tests timed out because Python lists the folder it starts in, and
//     the tests started it in `%TEMP%`, which held about 166,000 folders other suites had left
//     there. Nothing a test run writes was ever removed.
//   - A test that failed once made a whole cargo command fail, so every crate in that command was
//     run again on the next attempt, and a window test that failed once under load made its binary's
//     receipt go and the whole 40 minute suite run again.
//
// So each run gets a temporary folder of its own, which is removed when the run ends (see
// `startRunTemp`), and a test that fails is run once more on its own (see `retryFailures`). A test
// that passes the second time is recorded in `flakes.jsonl` and named in `node tools/flakes.mjs`, so
// a flaky test is a fact somebody can see rather than an hour somebody lost.

import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/** The folder every checkout on this machine shares: the temporary folder this process was started with. */
export function machineTemp() {
  return process.env.UNLUMINOUS_MACHINE_TEMP || os.tmpdir();
}

/** Where every run's temporary folder is made. */
export function runsRoot() {
  return path.join(machineTemp(), 'unluminous-tests');
}

/** How old a run's folder has to be, with its process gone, before a later run removes it. */
const STALE_AFTER_MS = 6 * 60 * 60 * 1000;

/**
 * Whether a process with this id is running.
 * @param pid - the process id
 */
function isRunning(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === 'EPERM';
  }
}

/**
 * Remove a folder and everything in it, trying again while a process that has just ended still
 * holds a file in it. Answers whether it is gone.
 * @param folder - the folder to remove
 */
export function removeFolder(folder) {
  for (let attempt = 0; attempt < 10; attempt++) {
    try {
      fs.rmSync(folder, { recursive: true, force: true, maxRetries: 3, retryDelay: 200 });
      return !fs.existsSync(folder);
    } catch {
      // A short wait for whichever process still holds a file to let go of it.
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
    }
  }
  return !fs.existsSync(folder);
}

/**
 * Remove the folders of earlier runs that ended without removing their own, which is a run that
 * was killed. A folder is named `<label>-<time>-<pid>`, so one whose process is still running is left.
 */
export function pruneStaleRuns() {
  const root = runsRoot();
  if (!fs.existsSync(root)) return 0;
  let removed = 0;
  for (const name of fs.readdirSync(root)) {
    const pid = Number(/-(\d+)$/.exec(name)?.[1]);
    const folder = path.join(root, name);
    let age = 0;
    try {
      age = Date.now() - fs.statSync(folder).mtimeMs;
    } catch {
      continue;
    }
    // A process id is reused, so a folder far older than any run is removed whatever answers to it.
    const stillRunning = pid && isRunning(pid) && age < 7 * 24 * 60 * 60 * 1000;
    if (stillRunning || age < STALE_AFTER_MS) continue;
    if (removeFolder(folder)) removed++;
  }
  return removed;
}

/**
 * Make this run's temporary folder, and the environment that points every test at it.
 *
 * `TMP`, `TEMP` and `TMPDIR` are what Rust's `std::env::temp_dir`, Python and PowerShell read, so
 * every test and every program a test starts writes under this folder. `UNLUMINOUS_MACHINE_TEMP`
 * names the shared folder, so a run started inside a run puts its own folder beside this one rather
 * than inside it.
 * @param label - a word for the kind of run, which starts the folder's name
 */
export function startRunTemp(label) {
  pruneStaleRuns();
  const stamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\..*$/, '');
  const folder = path.join(runsRoot(), `${label}-${stamp}-${process.pid}`);
  fs.mkdirSync(folder, { recursive: true });
  return {
    folder,
    // `RUST_BACKTRACE` so that a binary that aborts says where: an abort is rare, and the run it happened
    // in is the only chance to see it.
    environment: { TMP: folder, TEMP: folder, TMPDIR: folder, UNLUMINOUS_MACHINE_TEMP: machineTemp(), RUST_BACKTRACE: '1' },
  };
}

/**
 * Remove a run's temporary folder, and say how many entries it held, so a run that leaves a lot
 * behind is visible in its own output.
 * @param run - what `startRunTemp` returned
 */
export function endRunTemp(run) {
  let entries = 0;
  try {
    entries = fs.readdirSync(run.folder).length;
  } catch {
    return;
  }
  if (removeFolder(run.folder)) {
    console.log(`Removed this run's temporary folder (${entries} entries at the top): ${run.folder}`);
  } else {
    console.log(`Could not remove all of ${run.folder}: a program the tests started still holds a file in it. The next run removes it.`);
  }
}

/** The main checkout, found from any worktree through git's common directory. */
export function mainCheckout(repo) {
  try {
    const common = execFileSync('git', ['-C', repo, 'rev-parse', '--git-common-dir'], { encoding: 'utf8' }).trim();
    const absolute = path.isAbsolute(common) ? common : path.join(repo, common);
    return path.dirname(path.resolve(absolute));
  } catch {
    return repo;
  }
}

/** Where every run and every flake is recorded: `_agent_output/test-runs/` of the main checkout. */
export function recordsFolder(repo) {
  return path.join(mainCheckout(repo), '_agent_output', 'test-runs');
}

/**
 * Runs one cargo command, with its output on this console and in a log file, and resolves to its
 * exit code and everything it printed.
 * @param args - the cargo arguments
 * @param environment - variables added to this process's environment
 * @param log - a file the output is appended to, or null
 */
export function cargoWithOutput(args, environment, log) {
  console.log(`\n  cargo ${args.join(' ')}`);
  return new Promise((resolve) => {
    const child = spawn('cargo', args, { env: { ...process.env, ...environment }, stdio: ['ignore', 'pipe', 'pipe'] });
    const chunks = [];
    const write = (stream) => (data) => {
      chunks.push(data);
      stream.write(data);
      if (log) fs.appendFileSync(log, data);
    };
    child.stdout.on('data', write(process.stdout));
    child.stderr.on('data', write(process.stderr));
    child.on('close', (code) => resolve({ code: code ?? 1, output: Buffer.concat(chunks).toString('utf8') }));
    child.on('error', () => resolve({ code: 1, output: Buffer.concat(chunks).toString('utf8') }));
  });
}

/**
 * Reads libtest's output into the test binaries it ran: the target each `Running` line names, its
 * result line, and the tests in it that failed.
 *
 * A failure is read from `test <name> ... FAILED`, which libtest prints as each test ends, so a
 * binary that is killed before its summary still names the tests that had failed. A binary that
 * printed `Running` and no result line is reported with `crashed` set: it ended without libtest
 * finishing, which is an abort, a stack overflow or a test that hung until somebody stopped it.
 * @param output - everything cargo printed
 */
export function readLibtest(output) {
  const binaries = [];
  let current = null;
  for (const raw of output.split(/\r?\n/)) {
    const line = raw.trimEnd();
    const running = /^\s*Running (unittests )?(\S+) \((.+)\)\s*$/.exec(line);
    if (running) {
      current = { source: running[2].replaceAll('\\', '/'), unit: Boolean(running[1]), exe: running[3], failed: [], result: null };
      binaries.push(current);
      continue;
    }
    if (/^\s*Doc-tests /.test(line)) {
      current = { source: 'doctests', unit: false, exe: '', failed: [], result: null, doc: true };
      binaries.push(current);
      continue;
    }
    if (!current) continue;
    if (!current.died && /panicked at|^error: process didn't exit successfully|STATUS_[A-Z_]+/.test(line)) current.died = line.trim();
    const failed = /^test (.+?) \.\.\. FAILED$/.exec(line);
    if (failed && !current.failed.includes(failed[1])) current.failed.push(failed[1]);
    // **The summary's list is read as well (`task-2364`).** A test that starts a copy of its own binary has
    // that copy's output land between `test <name> ... ` and its `FAILED`, so the line above misses it. The
    // list after `failures:` that ends the binary names every failed test, four spaces in, on its own line.
    if (/^failures:$/.test(line)) current.inList = true;
    else if (current.inList && /^ {4}\S+$/.test(raw)) {
      const name = raw.trim();
      if (!current.failed.includes(name)) current.failed.push(name);
    } else if (current.inList && line !== '') current.inList = /^ {4}/.test(raw);
    const result = /^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored.*finished in ([0-9.]+)s/.exec(line);
    if (result) {
      current.inList = false;
      current.result = { ok: result[1] === 'ok', passed: Number(result[2]), failed: Number(result[3]), ignored: Number(result[4]), seconds: Number(result[5]) };
    }
  }
  for (const binary of binaries) binary.crashed = binary.result === null;
  return binaries;
}

/**
 * The cargo target selection that runs one binary again: `--lib`, `--bin <name>` or `--test <name>`.
 * @param binary - one entry from `readLibtest`
 */
export function targetOf(binary) {
  const stem = path.basename(binary.exe).replace(/-[0-9a-f]{8,}(\.exe)?$/, '');
  if (binary.source.startsWith('tests/') || /\/tests\/[^/]+\.rs$/.test(binary.source)) return ['--test', stem];
  if (/src\/lib\.rs$/.test(binary.source)) return ['--lib'];
  if (/src\/main\.rs$/.test(binary.source) || /src\/bin\//.test(binary.source)) return ['--bin', stem.replaceAll('_', '-')];
  return ['--test', stem];
}

/**
 * Appends one line of JSON to a file in the records folder.
 * @param repo - the checkout
 * @param name - the file, such as `flakes.jsonl`
 * @param value - what to record
 */
export function appendRecord(repo, name, value) {
  const folder = recordsFolder(repo);
  fs.mkdirSync(folder, { recursive: true });
  fs.appendFileSync(path.join(folder, name), JSON.stringify(value) + '\n');
}

/**
 * Every line of a records file, newest last.
 * @param repo - the checkout
 * @param name - the file
 */
export function readRecords(repo, name) {
  const file = path.join(recordsFolder(repo), name);
  if (!fs.existsSync(file)) return [];
  return fs
    .readFileSync(file, 'utf8')
    .split('\n')
    .filter(Boolean)
    .flatMap((line) => {
      try {
        return [JSON.parse(line)];
      } catch {
        return [];
      }
    });
}

/**
 * Runs each failed test once more, on its own and on one thread, and answers which passed.
 *
 * A binary that crashed is not retried test by test, because libtest did not say which test it was
 * in; it is run again whole. A test that passes alone is a flake, and is written to `flakes.jsonl`
 * with what it failed with, so `node tools/flakes.mjs` can name the tests that do it most.
 * @param options - `{ repo, crate, binaries, manifest, environment, log, commit, retryEnvironment }`
 */
export async function retryFailures(options) {
  const { repo, crate, binaries, manifest, environment, log, commit, retryEnvironment = {} } = options;
  const stillFailing = [];
  const flaky = [];
  for (const binary of binaries) {
    if (binary.doc) {
      if (binary.failed.length > 0 || binary.crashed) stillFailing.push(`${crate} doc tests`);
      continue;
    }
    if (binary.crashed) {
      // A binary that ended without libtest finishing aborted, so which test it was in is not known: it is
      // run once more, whole. One that passes then is recorded as a flake with the line it died on, which
      // is what the next person looking at it needs (`task-2364`: `notebooks` aborted once in a
      // `windows-core` COM call, "misaligned pointer dereference", and did not again in six runs).
      const args = ['test', '--manifest-path', manifest, '-p', crate, ...targetOf(binary), '--no-fail-fast'];
      console.log(`\n  ${crate} ${binary.source} ended without a result; running it again, whole.`);
      const again = await cargoWithOutput(args, { ...environment, ...retryEnvironment }, log);
      if (again.code === 0) {
        flaky.push(`${binary.source} (ended without a result)`);
        appendRecord(repo, 'flakes.jsonl', {
          at: new Date().toISOString(),
          commit,
          crate,
          target: binary.source,
          test: '(the whole binary ended without a result)',
          died: binary.died ?? null,
        });
      } else {
        stillFailing.push(`${crate} ${binary.source} (ended without a result, twice)`);
      }
      continue;
    }
    if (binary.failed.length === 0) continue;
    const args = ['test', '--manifest-path', manifest, '-p', crate, ...targetOf(binary), '--', '--exact', '--test-threads=1', ...binary.failed];
    console.log(`\n  Running ${binary.failed.length} failed test(s) of ${crate} ${binary.source} again, alone: ${binary.failed.join(', ')}`);
    const again = await cargoWithOutput(args, { ...environment, ...retryEnvironment }, log);
    const second = readLibtest(again.output).filter((b) => !b.doc);
    const failedAgain = new Set(second.flatMap((b) => b.failed));
    const crashedAgain = second.some((b) => b.crashed);
    for (const test of binary.failed) {
      if (again.code === 0 || (!failedAgain.has(test) && !crashedAgain)) {
        flaky.push(test);
        appendRecord(repo, 'flakes.jsonl', { at: new Date().toISOString(), commit, crate, target: binary.source, test });
      } else {
        stillFailing.push(`${crate} ${binary.source} ${test}`);
      }
    }
  }
  return { stillFailing, flaky };
}
