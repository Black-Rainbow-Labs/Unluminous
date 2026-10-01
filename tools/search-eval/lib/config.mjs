// The fixed facts the evaluation runs against: which repositories, at which commits, where the
// snapshots and the runs are written, and the measurement constants from TDD section 7.6.
//
// Everything here is part of the manifest of a run. Changing a pinned commit changes every query set's
// gold, so it is only ever done before the first gate run (TDD section 8.4).

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const HERE = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
export const REPO = path.resolve(HERE, '..', '..');

/**
 * Where snapshots, runs and the loop's records go. `SEARCH_EVAL_ROOT` wins; otherwise a folder on D:,
 * because a ticket's worktree is deleted when the ticket is retired and the loop's history has to
 * outlive it; otherwise the gitignored `_search-eval/` beside the repository.
 */
export function evalRoot() {
  if (process.env.SEARCH_EVAL_ROOT) return process.env.SEARCH_EVAL_ROOT;
  if (process.platform === 'win32' && fs.existsSync('D:/')) return 'D:/unluminous-search-eval';
  return path.join(REPO, '_search-eval');
}

/** The corpora of TDD section 7.1, each at a pinned commit. */
export const CORPORA = [
  { name: 'unluminous', tier: 'small', source: 'C:/jason/dev/unluminous', sha: '3eddcb6c303999c4502312e57ecf3139c2bee269', lang: 'rust' },
  { name: 'ai-service', tier: 'medium', source: 'C:/jason/dev/ai-service', sha: 'd8df63eb5d4e1a3be9befebee6bd8137ec3ca7bd', lang: 'typescript' },
  { name: 'inillucent', tier: 'medium', source: 'C:/jason/dev/inillucent', sha: 'e6aa248e3f88c5cdd4e3410461d4cbeb8f7be139', lang: 'rust' },
  { name: 'linux', tier: 'large', source: 'https://github.com/torvalds/linux.git', ref: 'v6.16', sha: null, lang: 'c', families: ['F1', 'F2', 'F7'] },
];

/** The folder a corpus snapshot is checked out into. */
export function snapshotDir(corpus) {
  const sha = corpus.sha || corpus.ref;
  return path.join(evalRoot(), 'corpora', `${corpus.name}@${sha.slice(0, 12)}`);
}

/**
 * The performance cores of the Core Ultra 9 285 on this machine, logical 0, 1, 10 to 13, 22 and 23
 * (TDD section 7.6). Both arms run inside this mask.
 */
export const PERFORMANCE_CORES = [0, 1, 10, 11, 12, 13, 22, 23];
export const AFFINITY_MASK = PERFORMANCE_CORES.reduce((m, c) => m | (1 << c), 0);

/** The fixed seed and resample count of the paired bootstrap (TDD section 7.6). */
export const SEED = 20260925;
export const RESAMPLES = 10000;

/** How much slower than the quiet reference the rg arm may run before a run is refused. */
export const QUIET_TOLERANCE = 0.03;

/** The goals of TDD section 2.1, as numbers. */
export const GOALS = { speed: 6.0, tokens: 0.5, accuracyRelative: 1.10 };

/** The home folder's Claude projects, where the transcripts are. */
export const TRANSCRIPTS = path.join(os.homedir(), '.claude', 'projects');
