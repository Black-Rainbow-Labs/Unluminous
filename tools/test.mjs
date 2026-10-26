// Runs the release gate's clippy and tests for the crates whose inputs have changed, and nothing else.
//
// `task-2240`. `tools/release.ps1` ran `cargo clippy --workspace --all-targets` and `cargo test
// --workspace` on every release, and nine of the twenty releases from 0.64.2 to 0.73.0 changed no
// crate but `crates/unluminous-app`. The other nine crates were compiled, linted and tested
// again on content that had already passed, at 100% of every processor on the machine.
//
// ## What decides whether a crate runs
//
// Each crate is a **unit** with a hash of everything its tests can read:
//
//   - every tracked file under the crate's own folder and under the folder of every workspace crate
//     it depends on, read as git would commit it, so a worktree's carriage returns and the main
//     checkout's line feeds hash the same;
//   - the files every build reads: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`,
//     `.cargo/config.toml`, `rustfmt.toml` and `kittest.toml`, with the release's own version
//     number taken out, so that the version bump a release commits does not make every crate look
//     changed;
//   - for `unluminous-app`, every tracked file in the repository except the folders listed in
//     `NEVER_READ_BY_A_BUILD`. Its tests walk `crates`, `documentation`, `README.md`,
//     `sample-diagrams` and `sample`, and its build script reads `installer/icon`, so it is described
//     by what it does not read rather than by what it does. A folder added later is read until
//     somebody says otherwise, which is the safe direction for that mistake;
//   - the command and its environment, so a pass without `UNLUMINOUS_REQUIRE_KERNEL` is never taken
//     for a pass with it.
//
// When `cargo test` for that unit has passed on that exact hash before, it is not run again. The
// same holds for clippy, as a unit of its own. A change to `unluminous-core` changes the hash of
// `unluminous-core`, `unluminous-lsp` and `unluminous-app`, because those are the crates whose inputs
// include it, so the crates a change can reach are re-run and only those.
//
// ## Where a pass is recorded
//
// In `_agent_output/test-receipts/` of the **main checkout**, found through `git rev-parse
// --git-common-dir` from any worktree. A ticket that ran this in its worktree after committing has
// therefore already done the release's testing for every crate it did not touch since, and the release
// run from the main checkout reads that. A receipt is only written when the whole cargo command
// passed.
//
// Usage:
//   node tools/test.mjs                 clippy and tests for every unit that has not passed
//   node tools/test.mjs --all           every unit, whatever has passed before
//   node tools/test.mjs --dry-run       say what would run and run nothing
//   node tools/test.mjs --no-clippy     tests only
//   node tools/test.mjs --kernel-optional   the notebook kernel tests may skip with no Python (release.sh)
//
// Run it through `pwsh tools/capped.ps1 node tools/test.mjs` from a shell, or let `tools/release.ps1`
// run it, so it stays on the 80% processor share.

import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { versionless } from './versionless.mjs';
import { appendRecord, cargoWithOutput, endRunTemp, readLibtest, recordsFolder, retryFailures, startRunTemp } from './test-runs.mjs';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const APP = 'unluminous-app';

// Read by no build and no test. Everything else in the repository is an input of `unluminous-app`.
// `tools/` is here because no test reads a script; the release scripts call these tools, they do not
// compile them.
const NEVER_READ_BY_A_BUILD = [
  '.claude/',
  '.github/',
  'tasks/',
  'design/',
  'releases/',
  'tools/',
  'CHANGELOG.md',
  'CLAUDE.md',
  'AGENTS.md',
  'GEMINI.md',
  'CONTRIBUTING.md',
  'CODE_OF_CONDUCT.md',
  'SECURITY.md',
  'LICENSE-APACHE',
  'LICENSE-MIT',
  '.gitignore',
  '.gitattributes',
];

// Crates whose tests spend their time waiting on another program rather than computing, and so run
// in a lane of their own, with a target directory of their own, beside the others instead of after
// them. `unluminous-jupyter`'s `tests/kernel.rs` starts real Python and Rust kernels: measured on
// task-2240 it was 119 s of a 168 s workspace test run, and 182 s of a 202 s one with nothing to
// compile, with the machine at a mean of 32% to 37% the whole time.
//
// `unluminous-word` joined it in `task-2364`: its `word_live` and `convert` tests drive Microsoft Word
// through COM, one Word at a time, and under the release's parallel load one Word stopped answering and
// hung the release for an hour, twice (task-2359).
const WAITS_ON_OTHER_PROGRAMS = ['unluminous-jupyter', 'unluminous-word'];

// What every compile reads, whichever crate it is.
const READ_BY_EVERY_BUILD = [
  'Cargo.toml',
  'Cargo.lock',
  'rust-toolchain.toml',
  '.cargo/config.toml',
  'rustfmt.toml',
  'kittest.toml',
];

/**
 * Runs git in the repository and returns its standard output as text.
 * @param args - the git arguments
 * @param input - text for git's standard input, if any
 */
function git(args, input) {
  return execFileSync('git', ['-C', repo, ...args], {
    encoding: 'utf8',
    input,
    maxBuffer: 256 * 1024 * 1024,
  });
}

/**
 * Every file the working tree holds, mapped to the blob git would commit for it.
 *
 * The index gives the committed and staged files. A file changed on disk and not staged, or not
 * tracked and not ignored, is hashed with `git hash-object`, which applies the same line ending
 * filters a commit does, so it hashes as it would once committed. A deleted file is dropped.
 */
function workingTreeBlobs() {
  const blobs = new Map();
  for (const entry of git(['ls-files', '-s', '-z']).split('\0')) {
    if (!entry) continue;
    const tab = entry.indexOf('\t');
    blobs.set(entry.slice(tab + 1), entry.split(' ')[1]);
  }
  const entries = git(['status', '--porcelain=v1', '-z', '--untracked-files=all']).split('\0');
  const present = [];
  for (let index = 0; index < entries.length; index++) {
    const entry = entries[index];
    if (entry.length <= 3) continue;
    // A rename or a copy is followed by the path it came from, as an entry of its own.
    if (entry[0] === 'R' || entry[0] === 'C') blobs.delete(entries[++index]);
    const file = entry.slice(3);
    if (fs.existsSync(path.join(repo, file))) present.push(file);
    else blobs.delete(file);
  }
  if (present.length > 0) {
    const hashes = git(['hash-object', '--stdin-paths'], present.join('\n') + '\n').trim().split('\n');
    present.forEach((file, index) => blobs.set(file, hashes[index]));
  }
  return blobs;
}

/**
 * The workspace's crates, each with its folder and the workspace crates it depends on.
 */
function workspaceCrates() {
  const metadata = JSON.parse(
    execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1', '--manifest-path', path.join(repo, 'Cargo.toml')], {
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    }),
  );
  const names = new Set(metadata.packages.map((p) => p.name));
  return metadata.packages.map((p) => ({
    name: p.name,
    folder: path.relative(repo, path.dirname(p.manifest_path)).replaceAll('\\', '/') + '/',
    dependsOn: [...new Set(p.dependencies.filter((d) => names.has(d.name)).map((d) => d.name))],
  }));
}

/**
 * A crate and every workspace crate it reaches through its dependencies, itself included.
 * @param crate - the crate to start from
 * @param byName - every crate, by name
 */
function closureOf(crate, byName) {
  const seen = new Set();
  const pending = [crate.name];
  while (pending.length > 0) {
    const name = pending.pop();
    if (seen.has(name)) continue;
    seen.add(name);
    pending.push(...byName.get(name).dependsOn);
  }
  return [...seen].map((name) => byName.get(name));
}

/**
 * The text of a file every build reads, with the workspace's own version taken out, so that the
 * version bump a release commits does not make every crate look changed. See `tools/versionless.mjs`.
 * @param file - the path, relative to the repository
 * @param members - the names of the workspace crates
 */
function versionlessText(file, members) {
  const full = path.join(repo, file);
  if (!fs.existsSync(full)) return '';
  return versionless(file, fs.readFileSync(full, 'utf8'), members);
}

/**
 * Whether a tracked file is an input of a unit: under one of its folders, or anywhere outside
 * `NEVER_READ_BY_A_BUILD` for `unluminous-app`.
 * @param file - the path, relative to the repository
 * @param folders - the folders of the unit's crate closure
 * @param isApp - whether the unit is `unluminous-app`
 * @param own - the folder of the unit's own crate
 * @param ownTestsBuilt - whether the unit builds its own crate's integration tests
 */
function isInput(file, folders, isApp, own = null, ownTestsBuilt = true) {
  if (READ_BY_EVERY_BUILD.includes(file)) return false;
  if (isTestCodeNotBuilt(file, own, folders, ownTestsBuilt)) return false;
  if (isApp) return !NEVER_READ_BY_A_BUILD.some((skip) => (skip.endsWith('/') ? file.startsWith(skip) : file === skip));
  return folders.some((folder) => file.startsWith(folder));
}

/**
 * Whether a file is the Rust source of integration tests, benchmarks or examples the unit does not build.
 *
 * `task-2364`. Those files are compiled into that crate's own test binaries and into nothing a
 * dependent crate builds, so a change to `crates/unluminous-word/tests/word_live.rs` cannot change
 * what `unluminous-app`'s tests see, and it used to send them round again. Only the `.rs` files are
 * left out: `unluminous-app`'s tests read other crates' fixtures (`unluminous-word/tests/fixtures`,
 * `unluminous-chat/tests/streams`), and those stay inputs. `unluminous-app`'s unit runs `--lib --bins`,
 * so its own `tests/*.rs`, which are the window suite, are not that unit's inputs either: the window
 * suite has receipts of its own (`tools/window-suite.mjs`). Clippy builds `--all-targets`, so a clippy
 * unit's own tests are always its inputs.
 * @param file - the path, relative to the repository
 * @param own - the folder of the unit's own crate, whose tests are always its inputs
 * @param folders - the folders of the unit's crate closure, or empty for `unluminous-app`
 * @param ownTestsBuilt - whether the unit builds its own crate's integration tests
 */
function isTestCodeNotBuilt(file, own, folders, ownTestsBuilt) {
  if (!file.endsWith('.rs')) return false;
  const match = /^(.*\/)(tests|benches|examples)\//.exec(file);
  if (!match) return false;
  // A source file inside a fixture is data a test reads, not a test.
  if (/\/tests\/(fixtures|streams)\//.test(file)) return false;
  const crate = match[1];
  if (own && crate === own && ownTestsBuilt) return false;
  // Only a workspace crate's folder: a crate's folder is where its `Cargo.toml` is.
  return crate.startsWith('crates/') || crate === 'unluminous-cli/' || folders.includes(crate);
}

/**
 * The hash of one unit: its command, its environment, the shared build files and its input files.
 * @param unit - the unit, with its crate closure and command
 * @param blobs - the working tree's files and blobs
 * @param shared - the versionless text of the files every build reads
 */
function unitHash(unit, blobs, shared) {
  const hash = crypto.createHash('sha256');
  hash.update(`${unit.kind}\n${unit.command.join(' ')}\n${JSON.stringify(unit.environment)}\n`);
  hash.update(shared);
  const folders = unit.closure.map((crate) => crate.folder);
  const own = unit.closure.find((crate) => crate.name === unit.crate)?.folder ?? null;
  const ownTestsBuilt = !(unit.crate === APP && unit.kind === 'test');
  const files = [...blobs.keys()].filter((file) => isInput(file, folders, unit.crate === APP, own, ownTestsBuilt)).sort();
  for (const file of files) hash.update(`${file} ${blobs.get(file)}\n`);
  return { hash: hash.digest('hex'), files: files.length };
}

/** Where passes are recorded: the main checkout's `_agent_output/test-receipts`. */
function receiptsFolder() {
  const common = git(['rev-parse', '--git-common-dir']).trim();
  const absolute = path.isAbsolute(common) ? common : path.join(repo, common);
  return path.join(path.dirname(path.resolve(absolute)), '_agent_output', 'test-receipts');
}

/**
 * The earlier pass of this unit at this hash, or null.
 * @param unit - the unit, with its hash
 */
function earlierPass(unit) {
  const file = path.join(receiptsFolder(), `${unit.kind}-${unit.crate}.jsonl`);
  if (!fs.existsSync(file)) return null;
  const lines = fs.readFileSync(file, 'utf8').split('\n').filter(Boolean);
  for (const line of lines.reverse()) {
    try {
      const receipt = JSON.parse(line);
      if (receipt.hash === unit.hash) return receipt;
    } catch {
      // A line written by a run that was killed halfway is not a receipt.
    }
  }
  return null;
}

/**
 * Records that this unit passed at its hash. The file keeps the last fifty passes, so a release
 * that follows a revert still finds the pass from before the change that was reverted.
 * @param unit - the unit, with its hash
 */
function recordPass(unit) {
  const folder = receiptsFolder();
  fs.mkdirSync(folder, { recursive: true });
  const file = path.join(folder, `${unit.kind}-${unit.crate}.jsonl`);
  const earlier = fs.existsSync(file) ? fs.readFileSync(file, 'utf8').split('\n').filter(Boolean) : [];
  const commit = git(['rev-parse', 'HEAD']).trim();
  const line = JSON.stringify({ hash: unit.hash, commit, at: new Date().toISOString(), checkout: repo });
  fs.writeFileSync(file, [...earlier.slice(-49), line].join('\n') + '\n');
}

/**
 * Every unit: clippy and the tests for each crate, with the command `tools/release.ps1` ran before.
 *
 * The tests run with UNLUMINOUS_REQUIRE_KERNEL=1, so a machine with no Python that has ipykernel
 * fails the notebook kernel tests rather than skipping them (task-2220), unless `kernelOptional` is
 * set, which `tools/release.sh` does because the Mac it runs on was never held to that. The variable
 * is part of each unit's hash, so a pass without it is never taken for a pass with it.
 * @param crates - the workspace crates
 * @param kernelOptional - whether the kernel tests may skip on a machine with no kernel
 */
function allUnits(crates, kernelOptional = false) {
  const byName = new Map(crates.map((crate) => [crate.name, crate]));
  const testEnvironment = kernelOptional ? {} : { UNLUMINOUS_REQUIRE_KERNEL: '1' };
  // The Word plugin's tests against Word itself (`word_live`, `convert`) and against its fonts
  // (`pagination`) fail rather than skip in the Windows gate, which is where Word is (task-2359).
  const wordEnvironment = !kernelOptional && process.platform === 'win32' ? { UNLUMINOUS_REQUIRE_WORD: '1' } : {};
  const units = [];
  for (const crate of crates) {
    const closure = closureOf(crate, byName);
    units.push({ kind: 'clippy', crate: crate.name, closure, command: ['--all-targets', '--', '-D', 'warnings'], environment: {} });
    const targets = crate.name === APP ? ['--lib', '--bins'] : [];
    const environment = crate.name === 'unluminous-word' ? { ...testEnvironment, ...wordEnvironment } : testEnvironment;
    units.push({ kind: 'test', crate: crate.name, closure, command: targets, environment });
  }
  return units;
}

/**
 * The cargo commands for the units that have to run.
 *
 * Clippy for every selected crate is one command. The tests are a build and then **one `cargo test` a
 * crate** (`task-2364`): every selected crate is compiled together with `--no-run`, and then each crate's
 * tests run in a command of their own, so a test that fails in one crate leaves every other crate
 * recorded as passed. Run as one command, one failure left all of them unrecorded, and task-2359's
 * release ran every crate's tests again because `word_live` had hung once.
 *
 * Clippy builds into a target directory of its own, `<target>/clippy`, so it runs at the same time
 * as the test build instead of waiting for cargo's lock on the shared one. The two never share an
 * artifact: clippy writes metadata and a test build writes code. The crates in
 * `WAITS_ON_OTHER_PROGRAMS` get a third lane and `<target>/waiting`: they are compiled beside the
 * other two lanes, and their tests **run after the other lanes have finished** (`after: 'others'`),
 * because they drive Word through COM and start real Python and Rust kernels, and under the release's
 * parallel load a Word stopped answering and hung the release for an hour twice (task-2359).
 * @param units - the units to run
 * @param manifest - the workspace Cargo.toml
 */
function commandsFor(units, manifest) {
  const target = process.env.CARGO_TARGET_DIR || path.join(repo, 'target');
  const packages = (list) => list.flatMap((unit) => ['-p', unit.crate]);
  const clippy = units.filter((unit) => unit.kind === 'clippy');
  const waiting = units.filter((unit) => unit.kind === 'test' && WAITS_ON_OTHER_PROGRAMS.includes(unit.crate));
  const tests = units.filter((unit) => unit.kind === 'test' && unit.crate !== APP && !waiting.includes(unit));
  const app = units.filter((unit) => unit.kind === 'test' && unit.crate === APP);
  const testRun = (unit) => ({
    units: [unit],
    crate: unit.crate,
    environment: unit.environment,
    args: ['test', '--manifest-path', manifest, '-p', unit.crate, ...unit.command, '--no-fail-fast'],
  });
  const groups = [];
  if (waiting.length > 0) {
    groups.push({
      name: waiting.map((unit) => unit.crate).join(', '),
      lane: 'waiting',
      after: 'others',
      builds: [['test', '--manifest-path', manifest, ...packages(waiting), '--no-run']],
      runs: waiting.map(testRun),
      environment: { CARGO_TARGET_DIR: path.join(target, 'waiting') },
    });
  }
  if (clippy.length > 0) {
    groups.push({
      name: 'clippy',
      lane: 'clippy',
      after: null,
      builds: [],
      runs: [
        {
          units: clippy,
          crate: null,
          environment: {},
          args: ['clippy', '--manifest-path', manifest, ...packages(clippy), '--all-targets', '--', '-D', 'warnings'],
        },
      ],
      environment: { CARGO_TARGET_DIR: path.join(target, 'clippy') },
    });
  }
  if (tests.length + app.length > 0) {
    // `unluminous-app`'s target filter cannot be given to the others without dropping their integration
    // tests, so its lib and bins are built by a second `--no-run`.
    const builds = [];
    if (tests.length > 0) builds.push(['test', '--manifest-path', manifest, ...packages(tests), '--no-run']);
    if (app.length > 0) builds.push(['test', '--manifest-path', manifest, '-p', APP, '--lib', '--bins', '--no-run']);
    groups.push({
      name: 'tests',
      lane: 'test',
      after: null,
      builds,
      runs: [...tests, ...app].map(testRun),
      environment: { CARGO_TARGET_DIR: target },
    });
  }
  return groups;
}

/**
 * Runs one lane: its builds, then, once the lanes it waits for have finished, each of its runs.
 * A failed test is run once more on its own; one that passes then is a flake, and its crate is
 * recorded as passed. Everything that ran is added to `context.report`.
 * @param group - the lane's group from `commandsFor`
 * @param context - `{ temp, log, commit, report, othersDone }`
 */
async function runLane(group, context) {
  const environment = { ...context.temp.environment, ...group.environment };
  for (const build of group.builds) {
    const built = await cargoWithOutput(build, environment, context.log);
    if (built.code !== 0) {
      context.report.failed.push(`${group.name}: the build failed (exit ${built.code})`);
      return;
    }
  }
  if (group.after === 'others') {
    console.log(`\n  ${group.name}: built, and its tests start once the other lanes have finished.`);
    await context.othersDone;
  }
  for (const run of group.runs) {
    const started = Date.now();
    const runEnvironment = { ...environment, ...run.environment };
    const result = await cargoWithOutput(run.args, runEnvironment, context.log);
    let passed = result.code === 0;
    if (!passed && run.crate) {
      const binaries = readLibtest(result.output);
      const retried = await retryFailures({
        repo,
        crate: run.crate,
        binaries,
        manifest: path.join(repo, 'Cargo.toml'),
        environment: runEnvironment,
        log: context.log,
        commit: context.commit,
      });
      context.report.flaky.push(...retried.flaky.map((test) => `${run.crate} ${test}`));
      // No binary read at all is a command that did not get as far as running a test.
      passed = binaries.length > 0 && retried.stillFailing.length === 0;
      if (!passed) {
        context.report.failed.push(...(retried.stillFailing.length > 0 ? retried.stillFailing : [`${run.crate} (exit ${result.code})`]));
      }
    } else if (!passed) {
      context.report.failed.push(`${group.name} (exit ${result.code})`);
    }
    context.report.ran.push({ what: run.crate ?? group.name, seconds: Math.round((Date.now() - started) / 100) / 10, passed });
    if (passed) run.units.forEach(recordPass);
  }
}

/**
 * Whether the workspace is formatted the way the release's `cargo fmt --all -- --check` asks.
 *
 * `task-2295`. Formatting was checked only by the release, after a ticket had run the window suite, and it
 * refused about eight releases. Fixing it rewrote `.rs` files, so the suite ran again: 25 minutes for
 * whitespace. This is the step a ticket runs after committing and before the window suite, so the check is
 * here, first, and takes about a second.
 */
function formatted() {
  try {
    execFileSync('cargo', ['fmt', '--manifest-path', path.join(repo, 'Cargo.toml'), '--all', '--', '--check'], {
      stdio: ['ignore', 'ignore', 'inherit'],
    });
    console.log('Formatting: as rustfmt writes it.');
    return true;
  } catch {
    console.error('\nThe workspace is not formatted, and the release refuses it. Run this, commit, and run this script again:');
    console.error(`  cargo fmt --manifest-path ${path.join(repo, 'Cargo.toml')} --all`);
    console.error('Do it before the window suite, so the suite runs on the code the release will ship.');
    return false;
  }
}

/** Selects the units to run, prints the plan, runs the lanes at the same time and sets the exit code. */
async function main() {
  const all = process.argv.includes('--all');
  const dryRun = process.argv.includes('--dry-run');
  const noClippy = process.argv.includes('--no-clippy');
  const kernelOptional = process.argv.includes('--kernel-optional');
  const crates = workspaceCrates();
  const members = new Set(crates.map((crate) => crate.name));
  const blobs = workingTreeBlobs();
  const shared = READ_BY_EVERY_BUILD.map((file) => `${file}\n${versionlessText(file, members)}\n`).join('');

  if (!dryRun && !formatted()) process.exit(1);

  const units = allUnits(crates, kernelOptional).filter((unit) => !(noClippy && unit.kind === 'clippy'));
  const toRun = [];
  console.log('Clippy and tests, by what each crate can read:');
  for (const unit of units) {
    Object.assign(unit, unitHash(unit, blobs, shared));
    const before = all ? null : earlierPass(unit);
    const label = `${unit.kind.padEnd(6)} ${unit.crate.padEnd(22)} ${unit.hash.slice(0, 10)}`;
    if (before) console.log(`  ${label}  passed at ${before.commit.slice(0, 8)} ${before.at}`);
    else {
      console.log(`  ${label}  runs (${unit.files} files)`);
      toRun.push(unit);
    }
  }

  const manifest = path.join(repo, 'Cargo.toml');
  const groups = commandsFor(toRun, manifest);
  if (groups.length === 0) {
    console.log('\nEvery unit has passed on exactly this content before. Nothing to run.');
    return;
  }
  if (dryRun) {
    for (const group of groups) {
      for (const build of group.builds) console.log(`\n  would build: cargo ${build.join(' ')}`);
      for (const run of group.runs) console.log(`  would run${group.after ? ' after the other lanes' : ''}: cargo ${run.args.join(' ')}`);
    }
    return;
  }

  const started = Date.now();
  const commit = git(['rev-parse', 'HEAD']).trim();
  // Every test and every program a test starts writes into this run's own temporary folder, which is
  // removed when the run ends (`tools/test-runs.mjs`).
  const temp = startRunTemp('gate');
  fs.mkdirSync(recordsFolder(repo), { recursive: true });
  const log = path.join(recordsFolder(repo), `gate-${new Date().toISOString().replace(/[:.]/g, '-')}.log`);
  const report = { failed: [], flaky: [], ran: [] };
  let othersFinished;
  const othersDone = new Promise((resolve) => (othersFinished = resolve));
  const context = { temp, log, commit, report, othersDone };
  const first = groups.filter((group) => group.after !== 'others');
  const last = groups.filter((group) => group.after === 'others');
  try {
    await Promise.all([
      Promise.all(first.map((group) => runLane(group, context))).then(() => othersFinished()),
      ...last.map((group) => runLane(group, context)),
    ]);
  } finally {
    endRunTemp(temp);
  }
  const seconds = Number(((Date.now() - started) / 1000).toFixed(1));
  appendRecord(repo, 'runs.jsonl', { at: new Date().toISOString(), kind: 'gate', commit, seconds, failed: report.failed, flaky: report.flaky, ran: report.ran, log });
  for (const flake of report.flaky) console.log(`  failed and then passed on its own: ${flake}`);
  if (report.flaky.length > 0) {
    console.log('  Each one is recorded in _agent_output/test-runs/flakes.jsonl, and `node tools/flakes.mjs` names the tests that do this most.');
  }
  if (report.failed.length > 0) {
    console.error(`\nFailed after ${seconds} s:`);
    for (const failure of report.failed) console.error(`  ${failure}`);
    console.error(`The whole output: ${log}`);
    process.exit(1);
  }
  console.log(`\nPassed in ${seconds} s: ${toRun.length} of ${units.length} units ran.`);
}

// Run only when started as a program, so `tools/test.test.mjs` can import the selection without running it.
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) await main();

export { closureOf, isInput, unitHash, allUnits, commandsFor };
