// Has the window suite been run against the code being released?
//
// `task-1928` made `tools/release.ps1` the only gate and `task-1984` T9 found the hole it left. The
// gate deliberately does not run the 639 window tests -- they need a graphics card and a person to
// open any image that changed, and a script must not be allowed to satisfy the second on its own --
// so a release could be cut from a commit whose window suite had never been run, and several were.
//
// It reads the receipts the suite leaves in `_agent_output/window-suite/` (see
// `crates/unluminous-app/tests/common/receipt.rs`) and says whether every window test binary has run,
// and run against a commit this one is built on. Since `task-2364` it also runs the suite (`--run`, see
// the section on running it below).
//
//   node tools/window-suite.mjs --check     exit 0 when every binary has a current receipt
//   node tools/window-suite.mjs             print what there is and what is missing
//   pwsh tools/capped.ps1 node tools/window-suite.mjs --run      run the suite
//
// A receipt names a commit and the time that binary started. It is written when the binary starts
// and deleted the instant anything in it panics, so a receipt present means that binary ran and
// nothing in it failed.

import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { versionless, workspaceMembersInLock } from './versionless.mjs';
import {
  appendRecord,
  cargoWithOutput,
  endRunTemp,
  readLibtest,
  readRecords,
  recordsFolder,
  retryFailures,
  startRunTemp,
} from './test-runs.mjs';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

/**
 * The main checkout, found from any worktree through git's common directory.
 *
 * `task-2240`: the receipts are kept there, by `crates/unluminous-app/tests/common/receipt.rs` as
 * well as read from there, because a ticket runs the suite in its own worktree and the release is
 * cut from the main checkout. While each checkout kept its own folder, the main checkout had no
 * receipts after a merge and the whole suite was built and run a second time there before every
 * release, on code it had already passed.
 */
function mainCheckout() {
  try {
    const common = execFileSync('git', ['-C', repo, 'rev-parse', '--git-common-dir'], { encoding: 'utf8' }).trim();
    const absolute = path.isAbsolute(common) ? common : path.join(repo, common);
    return path.dirname(path.resolve(absolute));
  } catch {
    return repo;
  }
}

const receipts = path.join(mainCheckout(), '_agent_output', 'window-suite');
const testsDir = path.join(repo, 'crates', 'unluminous-app', 'tests');

// The one binary a receipt is not asked of, and why. `agent_board`'s six tests are every one of them
// `#[ignore]`d -- they start a real agent, take minutes and cost tokens -- so `cargo test --test '*'`
// never runs any of them and the binary never builds a window. `tools/nightly.ps1` is their scheduled
// run. Asking for a receipt here would make every release wait on a nightly.
const NOT_IN_A_PLAIN_RUN = new Set(['agent_board']);

/** Every window test binary, read from the folder rather than written down twice. */
function expectedBinaries() {
  return fs
    .readdirSync(testsDir)
    .filter((name) => name.endsWith('.rs'))
    .map((name) => name.slice(0, -3))
    .filter((name) => !NOT_IN_A_PLAIN_RUN.has(name))
    .sort();
}

/**
 * Every receipt one binary has, newest first.
 *
 * `task-2285`: a receipt is `<binary>@<commit>.txt`, one per commit, because the folder is shared by
 * every worktree and one file a binary let two tickets running the suite at once overwrite each
 * other's receipts. `<binary>.txt` is the name used before, and is still read.
 */
function receiptsFor(binary) {
  if (!fs.existsSync(receipts)) return [];
  return fs
    .readdirSync(receipts)
    .filter((name) => name === `${binary}.txt` || (name.startsWith(`${binary}@`) && name.endsWith('.txt')))
    .map((name) => {
      const [commit, when] = fs.readFileSync(path.join(receipts, name), 'utf8').trim().split(/\s+/);
      return { commit, when: Number(when) || 0 };
    })
    .filter((receipt) => receipt.commit)
    .sort((left, right) => right.when - left.when);
}

/** The newest receipt for one binary, or null. */
function receiptFor(binary) {
  return receiptsFor(binary)[0] ?? null;
}

/** Why a receipt does not answer for HEAD, or null when it does. */
function whyNot(binary, receipt, moved) {
  if (!isAncestorOfHead(receipt.commit)) {
    return `${binary} (last passed at ${receipt.commit.slice(0, 7)}, which HEAD is not built on)`;
  }
  if (!moved.has(receipt.commit)) moved.set(receipt.commit, whatMovedSince(receipt.commit));
  const since = moved.get(receipt.commit);
  if (since.length > 0) {
    return `${binary} (last passed at ${receipt.commit.slice(0, 7)}; ${since.length} file${
      since.length === 1 ? '' : 's'
    } a window is built from have changed since)`;
  }
  return null;
}

/** Is `commit` one HEAD is built on? A receipt from a commit that was never merged says nothing. */
function isAncestorOfHead(commit) {
  try {
    execFileSync('git', ['-C', repo, 'merge-base', '--is-ancestor', commit, 'HEAD'], {
      stdio: 'ignore',
    });
    return true;
  } catch {
    return false;
  }
}

// What a window is built from. Anything else changing between the receipt and HEAD cannot alter a
// pixel, so a documentation commit after a green suite does not send anybody back to re-run it.
//
// **Other crates' integration tests, benchmarks and examples are not among them (`task-2364`).** They are
// compiled into that crate's own test binaries and into nothing a window is built from, and a change to
// `crates/unluminous-word/tests/word_live.rs` used to send all nineteen binaries round again. The same
// holds for `unluminous-app`'s own examples and benchmarks; its `tests/` are the window suite itself.
//
// **Every crate's test fixtures and `sample-diagrams/` are inputs (`task-2364`).** The window tests open
// `unluminous-word/tests/fixtures`, `unluminous-chat/tests/streams` and the sample diagrams, and a
// changed fixture was not counted at all, so a receipt could answer for pictures of a document that had
// since changed. Markdown files are not counted, although
// `every_markdown_file_in_the_repository_renders_with_no_broken_invariant` reads all of them: counting
// them would make every ticket's design document send the suite round again.
const OTHER_CRATES_TEST_CODE = (file) =>
  file.endsWith('.rs') &&
  /^(crates\/[^/]+|unluminous-cli)\/(tests|benches|examples)\//.test(file) &&
  !/\/tests\/(fixtures|streams)\//.test(file) &&
  !file.startsWith('crates/unluminous-app/tests/');
const CHANGES_A_WINDOW = (file) =>
  !OTHER_CRATES_TEST_CODE(file) &&
  (file.endsWith('.rs') ||
    file.endsWith('.toml') ||
    file === 'Cargo.lock' ||
    file.startsWith('plugins/') ||
    file.startsWith('sample-diagrams/') ||
    /^(crates\/[^/]+|unluminous-cli)\/tests\/(fixtures|streams)\//.test(file) ||
    file.startsWith('crates/unluminous-app/tests/snapshots/'));

/** The most `.rs` files a receipt's diff may hold and still have each one asked whether only its formatting moved. */
const FORMATTING_PASS_FILES = 40;

/**
 * What has changed since `commit` that could change what a window draws.
 *
 * **`task-1984` WP5, and it is the hole an ancestor check leaves.** Asking only whether the receipt's
 * commit is an ancestor of HEAD says yes for every commit that came after it -- so a release was
 * nearly cut whose `relayout` had been rewritten two commits earlier, with a receipt that passed
 * because the rewrite was a descendant of it. An ancestor is a necessary answer and not a sufficient
 * one: what matters is whether anything a window is made of has moved since.
 */
function whatMovedSince(commit) {
  try {
    const changed = execFileSync('git', ['-C', repo, 'diff', '--name-only', `${commit}..HEAD`], {
      encoding: 'utf8',
    })
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
      .filter(CHANGES_A_WINDOW)
      .filter((file) => !onlyTheVersionMoved(file, commit));
    // **A formatting pass is only worth asking about when few files changed (`task-2312`).** It exists for a
    // `cargo fmt` commit after the suite ran, which touches a handful of files. A receipt dozens of
    // files behind is stale whatever their whitespace says, and checking each one is two `git show`
    // calls and a `rustfmt` run, so the first file that is a real change answers for the rest.
    if (changed.filter((file) => file.endsWith('.rs')).length > FORMATTING_PASS_FILES) {
      const real = changed.find((file) => !onlyTheFormattingMoved(file, commit));
      return real ? changed : [];
    }
    return changed.filter((file) => !onlyTheFormattingMoved(file, commit));
  } catch {
    // A commit git cannot reach is already reported as stale by the ancestor check above.
    return [];
  }
}

/**
 * Whether `Cargo.toml` or `Cargo.lock` differs between `commit` and HEAD only in the release's
 * version number.
 *
 * `task-2240`. Every release commits a new version into both files and nothing else, so before this
 * every receipt went stale the moment a release was cut, and the next ticket's release asked for the
 * whole suite again even when that ticket changed nothing a window is drawn from. No accepted picture
 * draws the version: if one did, every release would have needed new pictures accepted, and none has.
 * The window tests that read `CARGO_PKG_VERSION` compare against it rather than against a written
 * number, so a version-only change passes or fails them the same way.
 * @param file - a path from `git diff --name-only`
 * @param commit - the commit the receipt names
 */
function onlyTheVersionMoved(file, commit) {
  if (file !== 'Cargo.toml' && file !== 'Cargo.lock') return false;
  const show = (revision, name) =>
    execFileSync('git', ['-C', repo, 'show', `${revision}:${name}`], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  const members = workspaceMembersInLock(show('HEAD', 'Cargo.lock'));
  return versionless(file, show(commit, file), members) === versionless(file, show('HEAD', file), members);
}

/** The workspace's Rust edition, which rustfmt needs when it is handed a file on its own. */
function workspaceEdition() {
  const manifest = fs.readFileSync(path.join(repo, 'Cargo.toml'), 'utf8');
  return manifest.match(/^\s*edition\s*=\s*"(\d{4})"/m)?.[1] ?? '2021';
}

/**
 * Whether a `.rs` file at HEAD is exactly what rustfmt makes of the same file at `commit`.
 *
 * `task-2295`. The release's `cargo fmt --check` refused about eight releases after the ticket had run the
 * window suite, and `cargo fmt` rewrites `.rs` files, so every one of those receipts went stale and the suite
 * ran again, for whitespace. Rustfmt is run on the file as it was at the receipt's commit, with the workspace's
 * own `rustfmt.toml` and edition, and the answer is compared with HEAD. A file that differs in any other way,
 * by a single character, is still a change, so this lets through a formatting commit and nothing else.
 * @param file - a path from `git diff --name-only`
 * @param commit - the commit the receipt names
 */
function onlyTheFormattingMoved(file, commit) {
  if (!file.endsWith('.rs')) return false;
  const show = (revision) =>
    execFileSync('git', ['-C', repo, 'show', `${revision}:${file}`], {
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
      stdio: ['ignore', 'pipe', 'ignore'],
    }).replace(/\r\n/g, '\n');
  try {
    const before = show(commit);
    const after = show('HEAD');
    const formatted = execFileSync(
      'rustfmt',
      ['--edition', workspaceEdition(), '--config-path', path.join(repo, 'rustfmt.toml'), '--emit', 'stdout'],
      { input: before, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, stdio: ['pipe', 'pipe', 'ignore'] },
    ).replace(/\r\n/g, '\n');
    return formatted === after;
  } catch {
    // A file that is new or deleted since the receipt, or one rustfmt cannot read, is a change.
    return false;
  }
}

// ============================================================================================
// Running the suite (`task-2364`)
//
//   pwsh tools/capped.ps1 node tools/window-suite.mjs --run
//   pwsh tools/capped.ps1 node tools/window-suite.mjs --run --only word,git --repeat 3 --load 19
//
// The suite was run by typing `cargo test -p unluminous-app --test '*' --no-fail-fast`, which leaves
// three things to whoever typed it. This does them:
//
//   - **A temporary folder of its own.** Every fixture the window tests build is a folder under
//     `%TEMP%`, and every unique folder a test made was left there. The run gets a fresh folder and
//     it is removed at the end (`tools/test-runs.mjs`), so no test depends on what other suites left
//     in `%TEMP%` and nothing is left behind.
//   - **A test that fails is run once more, on its own.** One that passes then is a flake. Its binary
//     keeps its receipt, because the rest of it passed in the same run and the test passed alone, and
//     the flake is written to `flakes.jsonl` so `node tools/flakes.mjs` can name it. Before this, one
//     flake under load took the receipt away and sent somebody round the whole 40 minute suite again.
//   - **A record of the run**: each binary's time and result, in `_agent_output/test-runs/runs.jsonl`.
//
// `--load <threads>` starts `tools/perf-smooth/load.mjs` beside the suite, which is how the suite was
// measured under the kind of load a release puts on the machine. `--repeat <n>` runs it n times and
// prints which tests failed in which runs. `--jobs <n>` builds the suite once and runs n binaries at a
// time, each in a temporary folder of its own (`runSideBySide`).


/**
 * Read a `--name value` argument, or the fallback.
 * @param name - the option without its dashes
 * @param fallback - what to answer when it was not given
 */
function option(name, fallback) {
  const at = process.argv.indexOf(`--${name}`);
  return at >= 0 && at + 1 < process.argv.length ? process.argv[at + 1] : fallback;
}

/**
 * Start the load generator with this many threads, and resolve once every thread is running.
 * @param threads - how many busy threads
 */
function startLoad(threads) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [path.join(repo, 'tools', 'perf-smooth', 'load.mjs'), '--threads', String(threads), '--tied'], {
      stdio: ['pipe', 'pipe', 'inherit'],
    });
    child.stdout.on('data', (data) => {
      if (String(data).includes('ready')) resolve(child);
    });
    child.on('exit', () => resolve(child));
  });
}

/**
 * The window test executables cargo builds, by binary name, read from `--message-format=json`.
 * @param output - what `cargo test --no-run --message-format=json-render-diagnostics` printed
 */
function testExecutables(output) {
  const found = new Map();
  for (const line of output.split(/\r?\n/)) {
    if (!line.startsWith('{')) continue;
    try {
      const message = JSON.parse(line);
      if (message.reason === 'compiler-artifact' && message.executable && message.target?.kind?.includes('test')) {
        found.set(message.target.name, message.executable);
      }
    } catch {
      // A line of cargo's that is not a message.
    }
  }
  return found;
}

/** How long each binary took in the last recorded window run, so the longest can start first. */
function lastSeconds() {
  const runs = readRecords(repo, 'runs.jsonl').filter((run) => run.kind === 'window');
  const seconds = new Map();
  for (const run of runs) for (const binary of run.binaries ?? []) if (binary.seconds) seconds.set(binary.binary, binary.seconds);
  return seconds;
}

/**
 * Run one test executable the way `cargo test` does, from the crate's folder, with a run folder of
 * its own under the run's, and resolve to what libtest printed.
 * @param name - the binary's name
 * @param exe - its path
 * @param temp - the run's temporary folder
 * @param log - the run's log, which the binary's whole output is appended to when it ends
 * @param threads - how many test threads it may run
 */
function runExecutable(name, exe, temp, log, threads) {
  const folder = path.join(temp.folder, name);
  fs.mkdirSync(folder, { recursive: true });
  const environment = {
    ...process.env,
    ...temp.environment,
    TMP: folder,
    TEMP: folder,
    TMPDIR: folder,
    CARGO_MANIFEST_DIR: path.join(repo, 'crates', 'unluminous-app'),
    RUST_TEST_THREADS: String(threads),
  };
  const started = Date.now();
  return new Promise((resolve) => {
    const child = spawn(exe, [], { cwd: path.join(repo, 'crates', 'unluminous-app'), env: environment, stdio: ['ignore', 'pipe', 'pipe'] });
    const chunks = [];
    child.stdout.on('data', (data) => chunks.push(data));
    child.stderr.on('data', (data) => chunks.push(data));
    const finish = (code) => {
      // **How it ended, written the way cargo writes it**, so a binary that dies with no word of its own (an
      // access violation prints nothing) still says how, and `readLibtest` records it with the flake. Measured
      // on task-2364: `canvas_realm` once printed `running 124 tests` and nothing else, 13 s in, under load.
      const ended = code === 0 ? '' : `\nerror: process didn't exit successfully: exit code ${code} (0x${((code ?? 1) >>> 0).toString(16)})\n`;
      const output = `     Running tests\\${name}.rs (${exe})\n` + Buffer.concat(chunks).toString('utf8') + ended;
      fs.appendFileSync(log, output);
      const seconds = Math.round((Date.now() - started) / 1000);
      console.log(`  ${code === 0 ? 'passed' : 'FAILED'}  ${name} in ${seconds} s${ended ? `, ${ended.trim().replace(/^error: /, '')}` : ''}`);
      resolve({ code: code ?? 1, output });
    };
    child.on('close', finish);
    child.on('error', () => finish(1));
  });
}

/**
 * Build the window suite once and run its binaries `jobs` at a time, the longest first.
 *
 * `cargo test --test '*'` runs the 19 binaries one after another, so the suite takes the sum of their
 * times, and a few long binaries (`panel_docking`, `plugins_board_and_chat`, `window_and_chrome`) each
 * spend minutes on a handful of long tests while most of the machine waits. Running two or three at once
 * needs each to have its own temporary folder, because every fixture is a fixed name in it, and so each
 * binary gets a folder of its own under the run's; the lock in `tests/common/turn.rs` is taken in that
 * folder too, so the binaries do not wait on each other.
 * @param only - binary names, or an empty list for all of them
 * @param jobs - how many binaries at once
 * @param temp - the run's temporary folder
 * @param log - the run's log
 */
async function runSideBySide(only, jobs, temp, log) {
  const manifest = path.join(repo, 'Cargo.toml');
  const targets = only.length > 0 ? only.flatMap((binary) => ['--test', binary]) : ['--test', '*'];
  const built = await cargoWithOutput(
    ['test', '--manifest-path', manifest, '-p', 'unluminous-app', ...targets, '--no-run', '--message-format=json-render-diagnostics'],
    temp.environment,
    null,
  );
  if (built.code !== 0) return { code: built.code, output: '' };
  const executables = testExecutables(built.output);
  const before = lastSeconds();
  const queue = [...executables.keys()].sort((left, right) => (before.get(right) ?? 1e9) - (before.get(left) ?? 1e9));
  // **The processors are shared between the binaries, not given to each** (`task-2364`). Three binaries with 19
  // test threads each took three to four times as long as each did alone; libtest's threads are what a binary
  // spends its processors on, so each gets its share of them.
  const processors = Number(process.env.RUST_TEST_THREADS) || os.availableParallelism();
  const threads = Math.max(4, Math.ceil(processors / jobs));
  console.log(`\n  Running ${queue.length} window test binaries, ${jobs} at a time with ${threads} test threads each, the longest first.`);
  const outputs = [];
  let failed = false;
  const worker = async () => {
    while (queue.length > 0) {
      const name = queue.shift();
      const result = await runExecutable(name, executables.get(name), temp, log, threads);
      outputs.push(result.output);
      if (result.code !== 0) failed = true;
    }
  };
  await Promise.all(Array.from({ length: Math.min(jobs, queue.length) }, worker));
  return { code: failed ? 1 : 0, output: outputs.join('\n') };
}

/**
 * Copy the pictures a failed attempt wrote, `*.new.png` and `*.diff.png`, beside the run's log, and answer
 * the folder, or null when there were none.
 *
 * `task-2364`. kittest removes a test's diff picture when the test passes, so a picture that differed once
 * and matched on the retry left nothing to look at, and a flaky picture could not be told from a real
 * change. The copies are what a person opens to tell which it was.
 * @param since - when the run started, in milliseconds; only pictures written since then are kept
 * @param log - the run's log, which the folder is named after
 */
function keepThePictures(since, log) {
  const snapshots = path.join(repo, 'crates', 'unluminous-app', 'tests', 'snapshots');
  const found = [];
  const walk = (folder) => {
    for (const entry of fs.readdirSync(folder, { withFileTypes: true })) {
      const full = path.join(folder, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (/\.(new|diff)\.png$/.test(entry.name) && fs.statSync(full).mtimeMs >= since) found.push(full);
    }
  };
  if (fs.existsSync(snapshots)) walk(snapshots);
  if (found.length === 0) return null;
  const into = log.replace(/\.log$/, '-pictures');
  fs.mkdirSync(into, { recursive: true });
  for (const file of found) fs.copyFileSync(file, path.join(into, path.basename(file)));
  return into;
}

/**
 * One run of the window suite, or of the binaries `only` names. Answers what the record says.
 * @param only - binary names, or an empty list for all of them
 * @param load - how many load threads to run beside it, or 0
 * @param jobs - how many binaries to run at once; 1 is `cargo test --test '*'`
 */
async function runOnce(only, load, jobs) {
  const commit = execFileSync('git', ['-C', repo, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  const started = Date.now();
  const temp = startRunTemp('window');
  fs.mkdirSync(recordsFolder(repo), { recursive: true });
  const log = path.join(recordsFolder(repo), `window-${new Date().toISOString().replace(/[:.]/g, '-')}.log`);
  const manifest = path.join(repo, 'Cargo.toml');
  const targets = only.length > 0 ? only.flatMap((binary) => ['--test', binary]) : ['--test', '*'];
  const loader = load > 0 ? await startLoad(load) : null;
  let binaries = [];
  let retried = { stillFailing: [], flaky: [] };
  let code = 1;
  try {
    const result =
      jobs > 1
        ? await runSideBySide(only, jobs, temp, log)
        : await cargoWithOutput(['test', '--manifest-path', manifest, '-p', 'unluminous-app', ...targets, '--no-fail-fast'], temp.environment, log);
    code = result.code;
    binaries = readLibtest(result.output).filter((binary) => !binary.doc);
    if (code !== 0) {
      const kept = keepThePictures(started, log);
      if (kept) console.log(`\n  The pictures of the failed attempt, kept before the retry can pass and remove them: ${kept}`);
      retried = await retryFailures({
        repo,
        crate: 'unluminous-app',
        binaries,
        manifest,
        environment: temp.environment,
        log,
        commit,
        retryEnvironment: { UNLUMINOUS_RECEIPT_RETRY: '1' },
      });
    }
  } finally {
    if (loader) loader.stdin.end();
    endRunTemp(temp);
  }
  const seconds = Math.round((Date.now() - started) / 1000);
  // No binary read at all is a build that failed before any test ran.
  const passed = binaries.length > 0 && (code === 0 || retried.stillFailing.length === 0);
  const record = {
    at: new Date().toISOString(),
    kind: 'window',
    commit,
    load,
    jobs,
    seconds,
    passed,
    binaries: binaries.map((binary) => ({
      binary: path.basename(binary.source, '.rs'),
      seconds: binary.result?.seconds ?? null,
      passed: binary.result?.passed ?? 0,
      failed: binary.failed,
      crashed: binary.crashed,
    })),
    flaky: retried.flaky,
    failed: retried.stillFailing,
    log,
  };
  appendRecord(repo, 'runs.jsonl', record);
  console.log(`\nWindow suite ${passed ? 'passed' : 'FAILED'} in ${seconds} s${load ? ` beside ${load} load threads` : ''}.`);
  for (const test of retried.flaky) console.log(`  failed and then passed on its own: ${test}`);
  for (const failure of retried.stillFailing) console.error(`  failed: ${failure}`);
  if (binaries.length === 0) console.error(`  nothing ran; the build failed. The whole output: ${log}`);
  return record;
}

/** Run the suite as many times as `--repeat` says, and print which tests failed in which runs. */
async function runSuite() {
  const only = option('only', '').split(',').filter(Boolean);
  const repeat = Number(option('repeat', '1'));
  const load = Number(option('load', '0'));
  const jobs = Number(option('jobs', '2'));
  const records = [];
  for (let index = 0; index < repeat; index++) {
    if (repeat > 1) console.log(`\n==> Run ${index + 1} of ${repeat}`);
    records.push(await runOnce(only, load, jobs));
  }
  if (repeat > 1) {
    const failures = new Map();
    records.forEach((record, index) => {
      for (const binary of record.binaries) for (const test of binary.failed) failures.set(`${binary.binary} ${test}`, [...(failures.get(`${binary.binary} ${test}`) ?? []), index + 1]);
    });
    console.log(`\n${records.filter((record) => record.passed).length} of ${repeat} runs passed.`);
    for (const [test, runs] of failures) console.log(`  ${test}: failed in run ${runs.join(', ')}`);
  }
  process.exit(records.every((record) => record.passed) ? 0 : 1);
}

if (process.argv.includes('--run')) await runSuite();

/** Read the receipts and say whether every binary has one that answers for HEAD; exits with the answer. */
function checkReceipts() {
  const check = process.argv.includes('--check');
  const binaries = expectedBinaries();
  const missing = [];
  const stale = [];
  const moved = new Map();
  // The receipt that answers for HEAD, for each binary that has one.
  const answering = new Map();
  for (const binary of binaries) {
    const all = receiptsFor(binary);
    if (all.length === 0) {
      missing.push(binary);
      continue;
    }
    // Any receipt that answers for HEAD will do; when none does, the newest one says why.
    //
    // **Tried newest first, stopping at the first that answers (`task-2312`).** Every receipt used to be
    // judged, and the folder keeps about thirty a binary. An old receipt is a diff of hundreds of `.rs` files,
    // each of which `onlyTheFormattingMoved` runs through `rustfmt`, so the check took almost five minutes on
    // every release before anything was built. The newest receipt is nearly always the one that answers.
    let newestReason = null;
    let found = null;
    for (const receipt of all) {
      const why = whyNot(binary, receipt, moved);
      if (why === null) {
        found = receipt;
        break;
      }
      newestReason ??= why;
    }
    if (found) {
      answering.set(binary, found);
    } else {
      stale.push(newestReason);
    }
  }
  // Only what moved since the receipts that were reported stale is printed below.
  for (const commit of [...moved.keys()]) {
    if ([...answering.values()].some((receipt) => receipt.commit === commit)) moved.delete(commit);
  }

  if (!check) {
    console.log(`Window suite receipts in ${path.relative(repo, receipts)}:`);
    for (const binary of binaries) {
      const receipt = answering.get(binary) ?? receiptFor(binary);
      const when = receipt?.when ? new Date(receipt.when * 1000).toISOString() : '';
      console.log(
        receipt ? `  ${binary}: ${receipt.commit.slice(0, 7)} ${when}` : `  ${binary}: never`,
      );
    }
  }

  if (missing.length === 0 && stale.length === 0) {
    if (check) console.log(`The window suite has passed at this commit: ${binaries.length} binaries.`);
    process.exit(0);
  }

  console.error('The window suite has not passed against this code.');
  for (const binary of missing) console.error(`  never run, or last run failed: ${binary}`);
  for (const line of stale) console.error(`  ${line}`);
  for (const [commit, since] of moved) {
    if (since.length === 0) continue;
    console.error('');
    console.error(`  changed since ${commit.slice(0, 7)}:`);
    for (const file of since.slice(0, 12)) console.error(`    ${file}`);
    if (since.length > 12) console.error(`    and ${since.length - 12} more`);
  }
  console.error('');
  console.error('Run it and look at any image that changed:');
  console.error('  pwsh tools/capped.ps1 node tools/window-suite.mjs --run');
  process.exit(1);
}

if (!process.argv.includes('--run')) checkReceipts();
