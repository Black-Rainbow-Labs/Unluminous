// Checks each corpus out at its pinned commit, into a folder of its own that nothing else edits.
//
// A local repository is cloned with `--shared`, so its objects are borrowed rather than copied, and
// checked out detached at the pinned commit. The snapshot has to be a git checkout rather than an
// exported tree, because ripgrep only honours `.gitignore` inside a git repository, and the file set
// both arms search is ripgrep's.
//
// Linux cannot be checked out on Windows at all: it has files named `aux.c` and pairs of names that
// differ only in case. So it is fetched as the release tarball, extracted with the names Windows refuses
// left out, and given an empty `.git` so its `.gitignore` files are read. Both arms search the same
// extracted tree, so the files that could not be created are missing from both.

import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, evalRoot, snapshotDir } from './config.mjs';

/**
 * Makes sure a corpus snapshot exists at its pinned commit and returns its folder.
 * @param corpus - one entry of CORPORA
 */
export function ensureSnapshot(corpus) {
  const dir = snapshotDir(corpus);
  if (fs.existsSync(path.join(dir, '.snapshot-ready'))) return dir;
  fs.mkdirSync(path.dirname(dir), { recursive: true });
  if (corpus.source.startsWith('http')) extractTarball(corpus, dir);
  else cloneAtCommit(corpus.source, corpus.sha, dir);
  fs.writeFileSync(path.join(dir, '.snapshot-ready'), JSON.stringify({ sha: corpus.sha, ref: corpus.ref || null, at: new Date().toISOString() }));
  excludeMarker(dir);
  return dir;
}

/**
 * Checks a local repository out at one commit into a new folder, borrowing its objects.
 * @param source - the repository to clone
 * @param sha - the commit
 * @param dir - the folder to create
 */
export function cloneAtCommit(source, sha, dir) {
  if (!fs.existsSync(path.join(dir, '.git'))) {
    execFileSync('git', ['clone', '--quiet', '--shared', '--no-checkout', source, dir], { stdio: 'inherit' });
  }
  execFileSync('git', ['-C', dir, '-c', 'advice.detachedHead=false', 'checkout', '--quiet', '--force', '--detach', sha], { stdio: 'inherit' });
}

/**
 * Keeps the snapshot's own marker file out of both arms' file set.
 * @param dir - the snapshot folder
 */
function excludeMarker(dir) {
  const exclude = path.join(dir, '.git', 'info', 'exclude');
  fs.mkdirSync(path.dirname(exclude), { recursive: true });
  const text = fs.existsSync(exclude) ? fs.readFileSync(exclude, 'utf8') : '';
  if (!text.includes('.snapshot-ready')) fs.appendFileSync(exclude, '\n.snapshot-ready\n');
}

/**
 * Downloads a release tarball and extracts it, skipping the names Windows cannot create.
 * @param corpus - a corpus whose source is a URL
 * @param dir - the folder to create
 */
function extractTarball(corpus, dir) {
  const archive = path.join(evalRoot(), 'downloads', `${corpus.name}-${corpus.ref}.tar.gz`);
  fs.mkdirSync(path.dirname(archive), { recursive: true });
  if (!fs.existsSync(archive)) {
    const url = corpus.source.replace(/\.git$/, '') + `/archive/refs/tags/${corpus.ref}.tar.gz`;
    execFileSync('curl', ['-sSL', '--fail', '-o', archive + '.part', url], { stdio: 'inherit' });
    fs.renameSync(archive + '.part', archive);
  }
  fs.mkdirSync(dir, { recursive: true });
  const tar = process.platform === 'win32' ? path.join(process.env.SystemRoot || 'C:/Windows', 'System32', 'tar.exe') : 'tar';
  // bsdtar reports the entries it could not create and carries on; the exit status is not a failure here.
  spawnSync(tar, ['-xzf', archive, '-C', dir, '--strip-components', '1'], { stdio: ['ignore', 'ignore', 'ignore'] });
  if (!fs.existsSync(path.join(dir, 'Makefile'))) throw new Error(`extracting ${archive} produced no tree`);
  execFileSync('git', ['init', '--quiet', dir]);
}

/** Every corpus the given families need. */
export function corporaFor(family) {
  return CORPORA.filter((c) => !c.families || c.families.includes(family));
}
