// Prints the state of the index hosts of the three local corpora: passages, vectors and errors. With
// `--wait-vectors` it waits until every corpus that can embed has a vector for every chunk.
//
//   node tools/search-eval/hosts.mjs [--wait-vectors]

import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { CORPORA, evalRoot, snapshotDir } from './lib/config.mjs';
import { CLI } from './lib/index-arm.mjs';

/**
 * One corpus host's status, through the CLI.
 * @param corpus - one entry of CORPORA
 */
function statusOf(corpus) {
  const r = spawnSync(CLI, ['search', 'status', '--root', snapshotDir(corpus), '--json'], { encoding: 'utf8', windowsHide: true, env: { ...process.env, UNLUMINOUS_INDEX_CACHE: path.join(evalRoot(), 'index-cache') } });
  try { return JSON.parse(r.stdout).result; } catch { return null; }
}

const wait = process.argv.includes('--wait-vectors');
for (;;) {
  let pending = false;
  for (const corpus of CORPORA.filter((c) => c.name !== 'linux')) {
    const s = statusOf(corpus);
    console.log(`${corpus.name}: ready ${s?.ready}, passages ${s?.passages} (${s?.passagesReady ? 'built' : 'building'}), vectors ${s?.vectors}, can embed ${s?.canEmbed}${s?.storeError ? `, error ${s.storeError}` : ''}`);
    // A host that has just started can report no vectors and no embedding for a moment, so a corpus is
    // only finished when it holds a vector for every passage. Every evaluation host can embed.
    if (!s || !s.passagesReady || s.vectors < s.passages) pending = true;
  }
  if (!wait || !pending) break;
  await new Promise((r) => setTimeout(r, 60_000));
}
