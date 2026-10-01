// Each family's declared primary metric (TDD section 7.2), computed from one arm's answer to one query.
// An arm's answer is `{ hits: [{ path, line }], files: [path], empty }` in the order the arm ranked it.

/**
 * Accuracy at 1 for F1: whether the arm's first hit is one of the gold definitions.
 * @param answer - the arm's answer
 * @param gold - `{ defs: ['path:line'] }`
 */
export function scoreIdentifier(answer, gold) {
  const first = answer.hits[0];
  const correct = !!first && gold.defs.includes(`${first.path}:${first.line}`);
  return { primary: correct ? 1 : 0, rank: answer.hits.findIndex((h) => gold.defs.includes(`${h.path}:${h.line}`)) + 1 || null };
}

/**
 * Exact set equality for F2, by digest of the sorted `path:line` set.
 * @param digest - the arm's digest
 * @param gold - `{ digest }`
 */
export function scoreExact(digest, gold) {
  return { primary: digest === gold.digest ? 1 : 0 };
}

/**
 * Recall of the reference locations for F3, then precision against references and definitions.
 * @param answer - the arm's answer
 * @param gold - `{ refs: ['path:line'], defs: ['path:line'] }`
 */
export function scoreReferences(answer, gold) {
  const found = new Set(answer.hits.map((h) => `${h.path}:${h.line}`));
  const refs = new Set(gold.refs);
  const relevant = new Set([...gold.refs, ...gold.defs]);
  let hit = 0, precise = 0;
  for (const r of refs) if (found.has(r)) hit++;
  for (const f of found) if (relevant.has(f)) precise++;
  return { primary: refs.size ? hit / refs.size : 1, precision: found.size ? precise / found.size : 0 };
}

/**
 * F4: whether the file the agent then read is in the arm's top three.
 * @param answer - the arm's answer
 * @param gold - `{ read }`
 */
export function scoreFiles(answer, gold) {
  const top = answer.files.slice(0, 3).map((f) => f.toLowerCase());
  return { primary: top.includes(gold.read.toLowerCase()) ? 1 : 0, rank: answer.files.findIndex((f) => f.toLowerCase() === gold.read.toLowerCase()) + 1 || null };
}

/**
 * F5: file recall at 5 against the files the ticket's commits changed.
 * @param answer - the arm's answer
 * @param gold - `{ files: [path] }`
 */
export function scoreLocalisation(answer, gold) {
  const top = new Set(answer.files.slice(0, 5));
  return { primary: gold.files.filter((f) => top.has(f)).length / gold.files.length };
}

/**
 * F6: nDCG at 10 at chunk level. A hit is relevant when its line falls inside the gold span.
 * @param answer - the arm's answer, hits ranked
 * @param gold - `{ file, start, end }`
 */
export function scoreConcept(answer, gold) {
  const relevant = (h) => h.path === gold.file && h.line >= gold.start && h.line <= gold.end;
  let dcg = 0;
  const seen = new Set();
  answer.hits.slice(0, 10).forEach((h, i) => {
    if (relevant(h) && !seen.has('gold')) { dcg += 1 / Math.log2(i + 2); seen.add('gold'); }
  });
  return { primary: dcg };
}

/**
 * F8: an abstention is correct when the arm returned nothing.
 * @param answer - the arm's answer
 */
export function scoreUnanswerable(answer) {
  return { primary: answer.empty || answer.hits.length === 0 ? 1 : 0 };
}

/** The rough token count of a text, at the fixed rate of 3.6 characters a token used until calibrated. */
export function approxTokens(text) {
  return Math.ceil(text.length / 3.6);
}
