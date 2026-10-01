// F7, freshness (TDD section 7.2): a scripted sequence of changes to a scratch clone of a corpus, each
// followed at once by the same searches through ripgrep and through the index. The family's metric is
// how many results were stale or missing; it must be zero.
//
//   node tools/search-eval/run-freshness.mjs [corpus] [--keep]
//
// The clone is made from the snapshot with `git clone --shared` into the eval root's scratch folder, so
// the snapshot every other family reads is never written to. The steps: write a new file, edit a line,
// rename a file, delete a file, write 5,000 files in one burst, switch the branch, and write and search
// with no pause at all, five times over.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, evalRoot, snapshotDir } from './lib/config.mjs';
import { indexArm } from './lib/index-arm.mjs';
import { exactSet, grepArgs, readJsonMatches, runRgSync } from './lib/rg.mjs';
import { pinToPerformanceCores } from './lib/machine.mjs';

const corpusName = process.argv[2] && !process.argv[2].startsWith('--') ? process.argv[2] : 'unluminous';
const corpus = CORPORA.find((c) => c.name === corpusName);
const scratch = path.join(evalRoot(), 'scratch', `freshness-${corpusName}-${Date.now()}`);
pinToPerformanceCores();
execFileSync('git', ['clone', '--quiet', '--shared', snapshotDir(corpus), scratch]);
execFileSync('git', ['-C', scratch, 'checkout', '--quiet', '-b', 'freshness-a']);
const arm = indexArm('index-mcp');
const results = [];

/**
 * Searches both arms for a pattern right now and records any difference.
 * @param step - the step's name
 * @param pattern - the regex
 */
async function compare(step, pattern) {
  const query = { family: 'F2', pattern, fixed: false, word: false, ignoreCase: false, path: '', globs: [], types: [] };
  const index = await arm.time(query, scratch);
  const rg = exactSet(readJsonMatches(runRgSync(grepArgs(query), scratch).stdout));
  const ours = exactSet(index.answer.hits);
  const missing = rg.filter((x) => !ours.includes(x));
  const stale = ours.filter((x) => !rg.includes(x));
  results.push({ step, pattern, rg: rg.length, index: ours.length, missing: missing.length, stale: stale.length, ms: index.ms, examples: [...missing, ...stale].slice(0, 3) });
}

/**
 * Writes a file inside the scratch clone, creating its folder.
 * @param rel - the path
 * @param text - the content
 */
function write(rel, text) {
  fs.mkdirSync(path.dirname(path.join(scratch, rel)), { recursive: true });
  fs.writeFileSync(path.join(scratch, rel), text);
}

await compare('start', 'fn main');
write('fresh/new_file.rs', 'pub fn freshness_token_alpha() {}\n');
await compare('write a new file', 'freshness_token_alpha');
write('fresh/new_file.rs', 'pub fn freshness_token_beta() {}\n');
await compare('edit a line (new text)', 'freshness_token_beta');
await compare('edit a line (old text)', 'freshness_token_alpha');
fs.renameSync(path.join(scratch, 'fresh/new_file.rs'), path.join(scratch, 'fresh/renamed_file.rs'));
await compare('rename a file', 'freshness_token_beta');
fs.unlinkSync(path.join(scratch, 'fresh/renamed_file.rs'));
await compare('delete a file', 'freshness_token_beta');
for (let i = 0; i < 5000; i++) write(`fresh/storm/file_${i}.txt`, `storm line ${i} freshness_storm_${i % 50}\n`);
await compare('5,000 files in one burst', 'freshness_storm_7\\b');
await compare('5,000 files in one burst (count)', 'storm line');
execFileSync('git', ['-C', scratch, 'add', '-A']);
execFileSync('git', ['-C', scratch, '-c', 'user.name=eval', '-c', 'user.email=eval@local', 'commit', '--quiet', '-m', 'freshness storm']);
execFileSync('git', ['-C', scratch, 'checkout', '--quiet', '-b', 'freshness-b', 'HEAD~1']);
await compare('switch the branch (files gone)', 'freshness_storm_7\\b');
await compare('switch the branch (files gone, count)', 'storm line');
execFileSync('git', ['-C', scratch, 'checkout', '--quiet', 'freshness-a']);
await compare('switch the branch back', 'freshness_storm_7\\b');
for (let round = 0; round < 5; round++) {
  write(`fresh/fast_${round}.rs`, `const FAST_${round}_TOKEN: u32 = ${round};\n`);
  await compare(`write then search at once, round ${round + 1}`, `FAST_${round}_TOKEN`);
}
await arm.close();
const wrong = results.reduce((n, r) => n + r.missing + r.stale, 0);
const out = path.join(evalRoot(), 'runs', `freshness-${corpusName}-${Date.now()}.json`);
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, JSON.stringify({ corpus: corpusName, wrong, results }, null, 2));
for (const r of results) console.log(`${r.missing + r.stale ? 'WRONG' : 'ok   '} ${r.step}: rg ${r.rg}, index ${r.index}, ${r.ms.toFixed(1)} ms ${r.examples.join(' ')}`);
console.log(`F7 ${corpusName}: ${wrong} stale or missing results (${out})`);
process.exitCode = wrong ? 1 : 0;
