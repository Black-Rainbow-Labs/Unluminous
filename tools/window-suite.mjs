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

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const receipts = path.join(repo, '_agent_output', 'window-suite');
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

/** The receipt for one binary, or null. */
function receiptFor(binary) {
  const file = path.join(receipts, `${binary}.txt`);
  if (!fs.existsSync(file)) return null;
  const [commit, when] = fs.readFileSync(file, 'utf8').trim().split(/\s+/);
  if (!commit) return null;
  return { commit, when: Number(when) || 0 };
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
    return execFileSync('git', ['-C', repo, 'diff', '--name-only', `${commit}..HEAD`], {
      encoding: 'utf8',
    })
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
      .filter(CHANGES_A_WINDOW);
  } catch {
    // A commit git cannot reach is already reported as stale by the ancestor check above.
    return [];
  }
}

const check = process.argv.includes('--check');
const binaries = expectedBinaries();
const missing = [];
const stale = [];
const moved = new Map();
for (const binary of binaries) {
  const receipt = receiptFor(binary);
  if (!receipt) {
    missing.push(binary);
    continue;
  }
  if (!isAncestorOfHead(receipt.commit)) {
    stale.push(
      `${binary} (last passed at ${receipt.commit.slice(0, 7)}, which HEAD is not built on)`,
    );
    continue;
  }
  if (!moved.has(receipt.commit)) moved.set(receipt.commit, whatMovedSince(receipt.commit));
  const since = moved.get(receipt.commit);
  if (since.length > 0) {
    stale.push(
      `${binary} (last passed at ${receipt.commit.slice(0, 7)}; ${since.length} file${
        since.length === 1 ? '' : 's'
      } a window is built from have changed since)`,
    );
  }
}

if (!check) {
  console.log(`Window suite receipts in ${path.relative(repo, receipts)}:`);
  for (const binary of binaries) {
    const receipt = receiptFor(binary);
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
console.error("  cargo test -p unluminous-app --test '*' --no-fail-fast");
process.exit(1);
