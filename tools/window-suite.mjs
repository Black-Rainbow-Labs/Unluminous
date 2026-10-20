// Has the window suite been run against the code being released?
//
// `task-1928` made `tools/release.ps1` the only gate and `task-1984` T9 found the hole it left. The
// gate deliberately does not run the 639 window tests -- they need a graphics card and a person to
// open any image that changed, and a script must not be allowed to satisfy the second on its own --
// so a release could be cut from a commit whose window suite had never been run, and several were.
//
// This does not run the suite. It reads the receipts the suite leaves in
// `_agent_output/window-suite/` (see `crates/unluminous-app/tests/common/receipt.rs`) and says
// whether every window test binary has run, and run against a commit this one is built on.
//
//   node tools/window-suite.mjs --check     exit 0 when every binary has a current receipt
//   node tools/window-suite.mjs             print what there is and what is missing
//
// A receipt names a commit and the time that binary started. It is written when the binary starts
// and deleted the instant anything in it panics, so a receipt present means that binary ran and
// nothing in it failed.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { versionless, workspaceMembersInLock } from './versionless.mjs';

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
const CHANGES_A_WINDOW = (file) =>
  file.endsWith('.rs') ||
  file.endsWith('.toml') ||
  file === 'Cargo.lock' ||
  file.startsWith('plugins/') ||
  file.startsWith('crates/unluminous-app/tests/snapshots/');

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
console.error("  pwsh tools/capped.ps1 cargo test -p unluminous-app --test '*' --no-fail-fast");
process.exit(1);
