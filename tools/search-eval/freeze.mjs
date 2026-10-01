// Freezes the query sets (TDD sections 7.2, 8.1 and 8.5): samples each family from what was mined,
// splits it 70% dev and 30% held out by file (by ticket for F5), records the gold, and writes the
// sets with a manifest of their hashes into `tools/search-eval/queries/`.
//
// Run once, before the first measured run. A run refuses to start when a set's hash no longer matches
// the manifest, so editing a set to move a score fails loudly. `--force` exists only for the case
// where nothing has been measured yet.

import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, HERE, SEED, TRANSCRIPTS, evalRoot, snapshotDir } from './lib/config.mjs';
import { exactSet, grepArgs, readJsonMatches, runRgSync } from './lib/rg.mjs';
import { seeded } from './lib/stats.mjs';
import { listTranscripts, readGrepWords, scanTranscript, shellCommands } from './lib/transcripts.mjs';

const QUERIES = path.join(HERE, 'queries');
const MINED = path.join(evalRoot(), 'mined');
const GOLD = path.join(evalRoot(), 'gold');
const HELD_OUT = 0.3;
const MAX_HITS = 20000;

/**
 * Reads a JSON lines file, or an empty list when it is missing.
 * @param file - the file
 */
export function readJsonl(file) {
  if (!fs.existsSync(file)) return [];
  return fs.readFileSync(file, 'utf8').split('\n').filter((l) => l.trim()).map((l) => JSON.parse(l));
}

/**
 * Which split a key falls in: a fixed hash of the seed and the key, so the same file is always on the
 * same side and a file's questions never sit on both.
 * @param key - the file path or ticket key the split is by
 */
export function splitOf(key) {
  const h = crypto.createHash('sha256').update(`${SEED}:${key}`).digest();
  return h.readUInt32BE(0) / 2 ** 32 < HELD_OUT ? 'heldout' : 'dev';
}

/**
 * A seeded shuffle, so sampling is the same every time it is run.
 * @param items - what to shuffle (not changed)
 * @param salt - mixed into the seed so two families do not share an order
 */
function shuffled(items, salt) {
  const random = seeded(SEED ^ salt);
  const a = [...items];
  for (let i = a.length - 1; i > 0; i--) { const j = Math.floor(random() * (i + 1)); [a[i], a[j]] = [a[j], a[i]]; }
  return a;
}

/**
 * Whether a name looks like something an agent would search for as an identifier: long enough, and
 * either mixed case, with an underscore or digit, or long.
 * @param name - the short name
 */
function isIdentifierLike(name) {
  if (name.length < 4) return false;
  return /[A-Z]/.test(name.slice(1)) || /[_0-9]/.test(name) || name.length >= 8;
}

/**
 * F1 and F3 for one corpus: identifiers with one to three definitions, stratified by how often they
 * are used, from the compiler made symbol gold.
 * @param corpus - one entry of CORPORA
 */
function symbolFamilies(corpus) {
  const gold = readJsonl(path.join(GOLD, `symbols-${corpus.name}.jsonl`));
  const buckets = [[0, 2], [3, 10], [11, 50], [51, 5000]];
  const picked = [];
  const candidates = shuffled(gold.filter((g) => isIdentifierLike(g.name) && g.defs.length >= 1 && g.defs.length <= 3), 1);
  for (const [lo, hi] of buckets) {
    picked.push(...candidates.filter((g) => g.refs.length >= lo && g.refs.length <= hi).slice(0, 40));
  }
  const f1 = picked.map((g) => ({ family: 'F1', repo: corpus.name, id: `F1:${corpus.name}:${g.name}`, name: g.name, gold: { defs: g.defs }, split: splitOf(`${corpus.name}:${g.defs[0].split(':')[0]}`) }));
  const f3 = picked.filter((g) => g.refs.length > 0).map((g) => ({ family: 'F3', repo: corpus.name, id: `F3:${corpus.name}:${g.name}`, name: g.name, gold: { refs: g.refs, defs: g.defs }, split: splitOf(`${corpus.name}:${g.defs[0].split(':')[0]}`) }));
  return { f1, f3 };
}

/**
 * Takes ripgrep's answer to an F2 query on its snapshot, which is the gold, or null when ripgrep
 * refuses the pattern.
 * @param query - the F2 query
 * @param dir - the snapshot
 */
function exactGold(query, dir) {
  const key = JSON.stringify([dir, grepArgs(query)]);
  if (goldCache.has(key)) return goldCache.get(key);
  const r = runRgSync(grepArgs(query), dir);
  let gold = null;
  // Exit 2 means ripgrep met an error, which is either a pattern it refused or a file it could not
  // read (Linux's symlinked folders on Windows); the second still gives a full answer over everything
  // readable, which is what both arms search, so only the first drops the query.
  const refused = r.status === 2 && /regex parse error|error parsing|unrecognized/i.test(r.stderr);
  if (r.status === 0 || r.status === 1 || (r.status === 2 && !refused && !r.tooLarge)) {
    const set = exactSet(readJsonMatches(r.stdout));
    // An answer of more than MAX_HITS lines is not a search anybody reads, and timing it measures the
    // printing. Such a query is dropped, on every corpus alike.
    if (set.length > MAX_HITS) { goldCache.set(key, null); fs.appendFileSync(GOLD_CACHE, JSON.stringify({ key, gold: null, why: 'too many hits' }) + '\n'); return null; }
    gold = { hits: set.length, files: new Set(set.map((s) => s.slice(0, s.lastIndexOf(':')))).size, digest: crypto.createHash('sha256').update(set.join('\n')).digest('hex').slice(0, 16) };
  }
  goldCache.set(key, gold);
  fs.appendFileSync(GOLD_CACHE, JSON.stringify({ key, gold }) + '\n');
  return gold;
}

// ripgrep's answers, kept so a freeze that is stopped part way does not have to ask again. The key is
// the snapshot folder and the exact arguments, so a cached answer is the answer to the same question.
const GOLD_CACHE = path.join(GOLD, 'f2-rg-cache.jsonl');
const goldCache = new Map(readJsonl(GOLD_CACHE).map((e) => [e.key, e.gold]));

/**
 * F2 for one corpus: real patterns agents typed against it, whose scope exists in the snapshot.
 * @param corpus - one entry of CORPORA
 * @param mined - every mined exact query
 */
function exactFamily(corpus, mined) {
  const dir = snapshotDir(corpus);
  const pool = corpus.name === 'linux'
    ? [...new Map(mined.map((q) => [JSON.stringify([q.pattern, q.fixed, q.ignoreCase, q.word]), { ...q, repo: 'linux', path: '', globs: [], types: [] }])).values()]
    : mined.filter((q) => q.repo === corpus.name && fs.existsSync(path.join(dir, q.path || '.')) && !/(^|\/)(_agent_output|target|node_modules)(\/|$)/.test(q.path));
  const out = [];
  for (const q of shuffled(pool, 2)) {
    const query = { pattern: q.pattern, fixed: q.fixed, ignoreCase: q.ignoreCase, word: q.word, path: q.path, globs: q.globs, types: q.types };
    const gold = exactGold(query, dir);
    if (!gold) continue;
    out.push({ family: 'F2', repo: corpus.name, id: `F2:${corpus.name}:${out.length}`, ...query, gold, split: splitOf(`${corpus.name}:${q.path || '.'}:${q.pattern}`) });
    if (out.length >= 300) break;
  }
  return out;
}

/**
 * F4: file searches agents made, whose gold read exists in the snapshot.
 * @param mined - the mined file searches
 */
function fileFamily(mined) {
  const seen = new Set();
  const out = [];
  for (const q of mined) {
    const corpus = CORPORA.find((c) => c.name === q.repo);
    if (!fs.existsSync(path.join(snapshotDir(corpus), q.read))) continue;
    const key = `${q.repo}|${q.pattern}|${q.path}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ family: 'F4', repo: q.repo, id: `F4:${q.repo}:${out.length}`, pattern: q.pattern, path: q.path, gold: { read: q.read }, split: splitOf(`${q.repo}:${q.read}`) });
  }
  return out;
}

/** F5: tickets with their parent commit and the files their commits changed, 45 a corpus at most. */
function ticketFamily() {
  const tickets = readJsonl(path.join(MINED, 'tickets.jsonl'));
  const out = [];
  for (const corpus of CORPORA.filter((c) => !c.source.startsWith('http'))) {
    for (const t of shuffled(tickets.filter((x) => x.repo === corpus.name), 3).slice(0, 45)) {
      out.push({ family: 'F5', repo: t.repo, id: `F5:${t.key}`, key: t.key, parent: t.parent, title: t.title, description: t.description, gold: { files: t.gold }, split: splitOf(t.key) });
    }
  }
  return out;
}

/** F6 and F8: the model written questions the checker accepted, and the confirmed unanswerable ones. */
function writtenFamilies() {
  const f6 = [], f8 = [];
  for (const corpus of CORPORA.filter((c) => !c.source.startsWith('http'))) {
    for (const q of readJsonl(path.join(MINED, `concept-${corpus.name}.jsonl`)).filter((x) => x.accepted)) {
      f6.push({ family: 'F6', repo: q.repo, id: `F6:${q.repo}:${f6.length}`, question: q.question, gold: { file: q.file, start: q.start, end: q.end }, split: splitOf(`${q.repo}:${q.file}`) });
    }
    for (const q of readJsonl(path.join(MINED, `unanswerable-${corpus.name}.jsonl`))) {
      f8.push({ family: 'F8', repo: q.repo, id: `F8:${q.repo}:${f8.length}`, question: q.question, gold: { terms: q.terms, empty: true }, split: splitOf(`${q.repo}:${q.question}`) });
    }
  }
  return { f6, f8 };
}

/**
 * The share of each kind of search in the transcripts, which weights F1 to F4 (TDD section 7.2). A
 * Grep for a bare identifier counts as an identifier search: as F1 when written with a definition
 * keyword in front of it, as F3 otherwise. Every other pattern is F2, and Glob and `find -name` are F4.
 * F5, F6 and F8 are questions answered by a sequence of calls, which the transcripts cannot count, so
 * they have fixed weights, with F6 under the 20% cap of TDD section 7.3.
 */
async function familyWeights() {
  const counts = { F1: 0, F2: 0, F3: 0, F4: 0 };
  const classify = (pattern) => {
    if (/^(fn|struct|enum|trait|impl|class|def|function|interface|type|const|let|pub fn|pub struct)\s+\\?\(?[A-Za-z_]\w*$/.test(pattern)) counts.F1++;
    else if (/^\\b?[A-Za-z_][A-Za-z0-9_]*(\\b)?$/.test(pattern)) counts.F3++;
    else counts.F2++;
  };
  for (const file of listTranscripts(TRANSCRIPTS)) {
    await scanTranscript(file, (e) => {
      if (e.kind === 'grep-tool' && typeof e.input.pattern === 'string') classify(e.input.pattern);
      else if (e.kind === 'glob') counts.F4++;
      else if (e.kind === 'bash') for (const words of shellCommands(e.input.command)) {
        const s = readGrepWords(words);
        if (s) classify(s.pattern);
        else if (words[0] === 'find' && words.some((w) => w === '-name' || w === '-iname')) counts.F4++;
      }
    });
  }
  const total = Object.values(counts).reduce((a, b) => a + b, 0);
  const share = 0.7;
  const weights = Object.fromEntries(Object.entries(counts).map(([k, v]) => [k, +(share * v / total).toFixed(4)]));
  Object.assign(weights, { F5: 0.15, F6: 0.10, F8: 0.05 });
  return { counts, weights };
}

/**
 * Writes one family's set and returns its hash.
 * @param name - the file name
 * @param rows - the queries
 */
function writeSet(name, rows) {
  const text = rows.map((r) => JSON.stringify(r)).join('\n') + '\n';
  fs.writeFileSync(path.join(QUERIES, name), text);
  return { file: name, count: rows.length, heldout: rows.filter((r) => r.split === 'heldout').length, sha256: crypto.createHash('sha256').update(text).digest('hex') };
}

if (fs.existsSync(path.join(QUERIES, 'manifest.json')) && !process.argv.includes('--force')) {
  console.error('The query sets are already frozen. They are not rebuilt once a run has used them (TDD section 8.5).');
  process.exit(2);
}
fs.mkdirSync(QUERIES, { recursive: true });
const minedExact = readJsonl(path.join(MINED, 'exact.jsonl'));
const sets = {};
const f1 = [], f3 = [], f2 = [];
for (const corpus of CORPORA) {
  if (corpus.name !== 'linux') { const s = symbolFamilies(corpus); f1.push(...s.f1); f3.push(...s.f3); }
  f2.push(...exactFamily(corpus, minedExact));
}
sets.F1 = writeSet('F1-identifier.jsonl', f1);
sets.F2 = writeSet('F2-exact.jsonl', f2);
sets.F3 = writeSet('F3-references.jsonl', f3);
sets.F4 = writeSet('F4-files.jsonl', fileFamily(readJsonl(path.join(MINED, 'files.jsonl'))));
sets.F5 = writeSet('F5-localisation.jsonl', ticketFamily());
const written = writtenFamilies();
sets.F6 = writeSet('F6-concept.jsonl', written.f6);
sets.F8 = writeSet('F8-unanswerable.jsonl', written.f8);
const weights = await familyWeights();
const manifest = {
  frozenAt: new Date().toISOString(), seed: SEED, heldOutShare: HELD_OUT,
  corpora: CORPORA.map((c) => ({ name: c.name, tier: c.tier, sha: c.sha, ref: c.ref || null })),
  sets, weights: weights.weights, transcriptCounts: weights.counts,
  note: 'F7 (freshness) is a scripted sequence in run-freshness.mjs, so it has no query file.',
};
fs.writeFileSync(path.join(QUERIES, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
console.log(JSON.stringify(manifest, null, 1));
