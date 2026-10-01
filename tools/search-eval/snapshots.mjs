// Creates every corpus snapshot at its pinned commit. Safe to run again: a finished snapshot is kept.
import { CORPORA } from './lib/config.mjs';
import { ensureSnapshot } from './lib/corpora.mjs';

const only = process.argv.slice(2);
for (const corpus of CORPORA) {
  if (only.length && !only.includes(corpus.name)) continue;
  const started = Date.now();
  const dir = ensureSnapshot(corpus);
  console.log(`${corpus.name}: ${dir} (${((Date.now() - started) / 1000).toFixed(1)} s)`);
}
