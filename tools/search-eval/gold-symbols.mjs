// Turns the SCIP indexes in `<eval root>/gold/` into the F1 and F3 gold files, one per corpus.
// `rust-analyzer scip` made the two Rust ones and `scip-typescript` the ai-service ones; the commands
// are in `tools/search-eval/README.md`.

import path from 'node:path';
import { evalRoot } from './lib/config.mjs';
import { readScip, writeSymbolGold } from './lib/scip.mjs';

const gold = path.join(evalRoot(), 'gold');
const proto = path.join(evalRoot(), 'tools', 'scip.proto');
const sources = {
  unluminous: [['unluminous.scip', '']],
  inillucent: [['inillucent.scip', '']],
  'ai-service': [['ai-service-backend.scip', 'backend/'], ['ai-service-ui.scip', 'ui/']],
};
for (const [corpus, files] of Object.entries(sources)) {
  const map = new Map();
  for (const [file, prefix] of files) await readScip(path.join(gold, file), prefix, proto, map);
  const count = writeSymbolGold(map, path.join(gold, `symbols-${corpus}.jsonl`));
  console.log(`${corpus}: ${count} names with a definition`);
}
