/**
 * Moves Cargo.lock to the newest commit on Termoil's `main` branch (task-2524).
 *
 *   node tools/take-newest-termoil.mjs [--check]
 *
 * The three Termoil crates (`termoil-vt`, `termoil-pty`, `termoil-input`) follow Termoil's `main` branch in
 * the workspace Cargo.toml, and Cargo.lock holds the commit they were last taken at. Both release scripts run
 * this before the suite, so every release builds and tests against the newest Termoil.
 *
 * It is not `cargo update -p termoil-vt -p termoil-pty -p termoil-input`, because that re-resolves more than
 * the three crates: on 2026-11-06 it also moved `gpu-allocator` from `windows` 0.62 to 0.61 and six other
 * crates' `windows-sys`, and the vendored `wgpu-hal`, which is built against `windows` 0.62, no longer
 * compiled. This rewrites only the three `source` lines and the three `version` lines, then asks
 * `cargo metadata --locked` whether Cargo accepts the lock as it is, and fails when it does not.
 *
 * `--check` prints what it would change and changes nothing.
 */
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const LOCK = path.join(REPO, 'Cargo.lock');
const MANIFEST = path.join(REPO, 'Cargo.toml');
const TERMOIL = 'https://github.com/Black-Rainbow-Labs/Termoil.git';
const CRATES = ['termoil-vt', 'termoil-pty', 'termoil-input'];

/**
 * The commit at the head of Termoil's main branch, asked of GitHub with git ls-remote.
 */
function newestCommit() {
  const out = execFileSync('git', ['ls-remote', TERMOIL, 'refs/heads/main'], { encoding: 'utf8' });
  const sha = out.split(/\s+/)[0];
  if (!/^[0-9a-f]{40}$/.test(sha)) throw new Error(`git ls-remote ${TERMOIL} answered ${JSON.stringify(out)}`);
  return sha;
}

/**
 * The workspace version written in Termoil's Cargo.toml at a commit, which every Termoil crate inherits.
 * @param sha - the commit
 */
async function versionAt(sha) {
  const url = `https://raw.githubusercontent.com/Black-Rainbow-Labs/Termoil/${sha}/Cargo.toml`;
  const answer = await fetch(url, { headers: { 'User-Agent': 'unluminous-release' } });
  if (!answer.ok) throw new Error(`${url} answered ${answer.status}`);
  const match = (await answer.text()).match(/\[workspace\.package\][^[]*?\bversion\s*=\s*"([^"]+)"/);
  if (!match) throw new Error(`${url} has no [workspace.package] version`);
  return match[1];
}

/**
 * The lock text with each Termoil crate's version and source moved to the commit. Throws when a crate is
 * missing from the lock.
 * @param text - Cargo.lock
 * @param sha - the commit
 * @param version - the crates' version at that commit
 */
export function rewriteLock(text, sha, version) {
  let out = text;
  for (const name of CRATES) {
    const entry = new RegExp(`(\\[\\[package\\]\\]\\r?\\nname = "${name}"\\r?\\nversion = )"[^"]+"(\\r?\\nsource = "git\\+${TERMOIL.replace(/[.]/g, '\\.')}\\?branch=main#)[0-9a-f]{40}"`);
    if (!entry.test(out)) throw new Error(`Cargo.lock has no ${name} from Termoil's main branch`);
    out = out.replace(entry, `$1"${version}"$2${sha}"`);
  }
  return out;
}

/**
 * Moves the lock and checks Cargo accepts it.
 */
async function main() {
  const check = process.argv.includes('--check');
  const sha = newestCommit();
  const version = await versionAt(sha);
  const before = fs.readFileSync(LOCK, 'utf8');
  const after = rewriteLock(before, sha, version);
  if (after === before) {
    console.log(`Cargo.lock already has Termoil ${version} at ${sha.slice(0, 8)}`);
    return;
  }
  console.log(`${check ? 'would move' : 'moving'} Cargo.lock to Termoil ${version} at ${sha.slice(0, 8)}`);
  if (check) return;
  fs.writeFileSync(LOCK, after);
  try {
    execFileSync('cargo', ['metadata', '--locked', '--format-version', '1', '--manifest-path', MANIFEST], { stdio: ['ignore', 'ignore', 'inherit'] });
  } catch (error) {
    fs.writeFileSync(LOCK, before);
    throw new Error(`cargo does not accept the moved lock, so Cargo.lock was put back: ${error.message}`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`take-newest-termoil: ${error.message}`);
    process.exit(1);
  });
}
