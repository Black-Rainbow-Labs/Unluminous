// Reads a SCIP index, the compiler made index `rust-analyzer scip` and `scip-typescript` write, into
// the gold for F1 (where is X defined) and F3 (where is X used).
//
// A symbol is keyed by its full SCIP name, so two functions called `new` in different types stay
// apart, and the question an agent asks is by its short name, so the gold for a name is every
// definition of every symbol with that short name. Local symbols (a variable inside a function) are
// left out, because "where is X" is not asked about them.

import fs from 'node:fs';
import path from 'node:path';
import protobuf from 'protobufjs';

const DEFINITION = 0x1;

/**
 * The short name a person would type for a SCIP symbol: its last descriptor, without the method
 * disambiguator or the punctuation SCIP uses to say what kind of descriptor it is.
 * @param symbol - the full SCIP symbol string
 */
export function shortName(symbol) {
  if (symbol.startsWith('local ')) return null;
  const parts = symbol.split(' ');
  const descriptors = parts.slice(4).join(' ').replace(/\([^)]*\)/g, '').replace(/`([^`]*)`/g, '$1');
  const names = descriptors.match(/[A-Za-z_$][\w$]*/g);
  return names ? names[names.length - 1] : null;
}

/**
 * Loads one SCIP file and adds its definitions and references to `into`, keyed by short name.
 * @param file - the `.scip` file
 * @param prefix - the folder the index's paths are relative to, inside the corpus ('' or 'backend/')
 * @param protoPath - the `scip.proto` schema
 * @param into - `Map<name, { defs: Set<string>, refs: Set<string> }>`, locations as `path:line`
 */
export async function readScip(file, prefix, protoPath, into) {
  const root = await protobuf.load(protoPath);
  const Index = root.lookupType('scip.Index');
  const index = Index.decode(fs.readFileSync(file));
  for (const doc of index.documents) {
    const rel = (prefix + doc.relativePath).replace(/\\/g, '/');
    for (const occurrence of doc.occurrences) {
      const name = shortName(occurrence.symbol || '');
      if (!name) continue;
      const line = (occurrence.range?.[0] ?? 0) + 1;
      if (!into.has(name)) into.set(name, { defs: new Set(), refs: new Set(), symbols: new Set() });
      const entry = into.get(name);
      entry.symbols.add(occurrence.symbol);
      if (occurrence.symbolRoles & DEFINITION) entry.defs.add(`${rel}:${line}`);
      else entry.refs.add(`${rel}:${line}`);
    }
  }
  return into;
}

/**
 * Writes the symbol gold for one corpus as one JSON line per name that has at least one definition.
 * @param map - the map `readScip` filled
 * @param out - the `.jsonl` file to write
 */
export function writeSymbolGold(map, out) {
  const lines = [];
  for (const [name, entry] of [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
    if (!entry.defs.size) continue;
    lines.push(JSON.stringify({ name, defs: [...entry.defs].sort(), refs: [...entry.refs].sort(), symbols: entry.symbols.size }));
  }
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, lines.join('\n') + '\n');
  return lines.length;
}
