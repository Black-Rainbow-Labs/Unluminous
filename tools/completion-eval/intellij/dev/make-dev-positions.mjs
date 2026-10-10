// Builds a small hand written positions file for the two dev corpora, so the IntelliJ engine can be
// proven before the real generator exists. Usage: node make-dev-positions.mjs <out.json>
import fs from 'node:fs';
import path from 'node:path';

const ROOT = 'D:/unluminous-completion-eval/corpora';
const rust = 'ripgrep-dev';
const ts = 'ts-dev';

/** Each entry finds the first occurrence of `before + ident` and makes `ident` the position. */
const entries = [
  [rust, 'rust', 'crates/searcher/src/searcher/mod.rs', 'let mut config = self.', 'config', 'member'],
  [rust, 'rust', 'crates/searcher/src/searcher/mod.rs', '.encoding(self.config.', 'encoding', 'member'],
  [rust, 'rust', 'crates/searcher/src/searcher/mod.rs', 'self.config.encoding.as_ref().', 'map', 'member'],
  [rust, 'rust', 'crates/searcher/src/searcher/mod.rs', 'SearcherBuilder { config: Config::', 'default', 'path'],
  [rust, 'rust', 'crates/searcher/src/searcher/mod.rs', 'let mut decode_builder = DecodeReaderBytesBuilder::', 'new', 'path'],
  [rust, 'rust', 'crates/searcher/src/searcher/mod.rs', 'pub fn build(&self) -> Searcher {\n        ', 'let', 'keyword'],
  [rust, 'rust', 'crates/searcher/src/searcher/mod.rs', 'if ', 'config', 'local'],
  [rust, 'rust', 'crates/searcher/src/searcher/mod.rs', 'pub fn build(&self) -> ', 'Searcher', 'type'],
  [rust, 'rust', 'crates/core/search.rs', 'ignore::overrides::', 'Override', 'path'],
  [rust, 'rust', 'crates/core/search.rs', 'binary_implicit: grep::searcher::', 'BinaryDetection', 'path'],
  [ts, 'typescript', 'src/database.ts', '    this.affected.', 'push', 'member'],
  [ts, 'typescript', 'src/database.ts', 'const count = Number(changed[0]);\n    this.', 'affected', 'member'],
  [ts, 'typescript', 'src/database.ts', 'if (status !== Status.', 'Ok', 'member'],
  [ts, 'typescript', 'src/database.ts', 'if (status !== Status.Ok) this.#free();\n    ', 'check', 'global'],
  [ts, 'typescript', 'src/database.ts', 'const status = calls().', 'txn_execute', 'member'],
  [ts, 'typescript', 'src/database.ts', '    this.affected.push(count);\n    ', 'return', 'keyword'],
  [ts, 'typescript', 'src/database.ts', 'this.#handle, sql, changed, error) as number;\n    if (', 'status', 'local'],
];

const positions = entries.map(([corpus, language, rel, before, ident, cls], i) => {
  const text = fs.readFileSync(path.join(ROOT, corpus, rel), 'utf8').replace(/\r\n/g, '\n');
  const at = text.indexOf(before + ident);
  if (at < 0) throw new Error(`not found: ${rel}: ${before + ident}`);
  const startChars = at + before.length;
  const start = Buffer.byteLength(text.slice(0, startChars));
  const end = start + Buffer.byteLength(ident);
  return { id: `${corpus}:${String(i).padStart(5, '0')}`, corpus, language, path: rel, start, end, expected: ident, class: cls, split: i % 2 ? 'held' : 'tune', remove: [] };
});

fs.writeFileSync(process.argv[2], JSON.stringify({ version: 1, seed: 0, corpora: { [rust]: { language: 'rust' }, [ts]: { language: 'typescript' } }, positions }, null, 1));
console.log(`wrote ${positions.length} positions`);
