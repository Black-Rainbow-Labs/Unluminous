// Writes the F6 (concept) and F8 (unanswerable) question candidates with a model (TDD section 7.2).
//
// F6: a function's code is shown to one session, which writes a question the code answers without
// using any identifier, file name or string from it. A second session, which never saw the first,
// is shown the question and the code and says whether the code answers it. Only questions it accepts
// are kept, and the code's location is the gold.
//
// F8: a session is told what the repository is and asked for questions about things it does not do,
// each with the distinctive words code doing it would have to contain. A question is kept only when
// ripgrep finds none of those words anywhere in the corpus.

import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, evalRoot, snapshotDir } from './lib/config.mjs';
import { lastJson, runClaude } from './lib/agent.mjs';
import { escapeRegex, grepArgs, readJsonMatches, runRgSync } from './lib/rg.mjs';
import { seeded } from './lib/stats.mjs';

const MODEL = 'sonnet';
const PER_CORPUS_F6 = Number(process.env.F6_PER_CORPUS || 70);
const PER_CORPUS_F8 = Number(process.env.F8_PER_CORPUS || 40);
const out = path.join(evalRoot(), 'mined');

/**
 * Picks function definitions to write questions from: source files only, not tests, spread across
 * the corpus by a seeded shuffle.
 * @param corpus - the corpus name
 * @param count - how many to pick
 */
function pickDefinitions(corpus, count) {
  const gold = fs.readFileSync(path.join(evalRoot(), 'gold', `symbols-${corpus}.jsonl`), 'utf8').trim().split('\n').map(JSON.parse);
  const random = seeded();
  const defs = [];
  for (const g of gold) for (const d of g.defs) {
    const [file, line] = [d.slice(0, d.lastIndexOf(':')), Number(d.slice(d.lastIndexOf(':') + 1))];
    if (/(^|\/)(tests?|__tests__|e2e|examples|benches)\/|\.(test|spec)\.|_test\.rs$/.test(file)) continue;
    defs.push({ name: g.name, file, line });
  }
  for (let i = defs.length - 1; i > 0; i--) { const j = Math.floor(random() * (i + 1)); [defs[i], defs[j]] = [defs[j], defs[i]]; }
  const files = new Set();
  const picked = [];
  for (const d of defs) {
    if (files.has(d.file)) continue;
    const span = functionSpan(corpus, d);
    if (!span) continue;
    files.add(d.file);
    picked.push({ ...d, ...span });
    if (picked.length >= count) break;
  }
  return picked;
}

/**
 * The lines of a definition, from its line to the closing brace at its own depth, if it is a function
 * between 12 and 120 lines long.
 * @param corpus - the corpus name
 * @param def - `{ file, line }`
 */
function functionSpan(corpus, def) {
  const corpusDef = CORPORA.find((c) => c.name === corpus);
  let text;
  try { text = fs.readFileSync(path.join(snapshotDir(corpusDef), def.file), 'utf8'); } catch { return null; }
  const lines = text.split('\n');
  const first = lines[def.line - 1] || '';
  if (!/\b(fn|function|async|def)\b|=>|\)\s*[:{]/.test(first)) return null;
  let depth = 0, opened = false;
  for (let i = def.line - 1; i < Math.min(lines.length, def.line + 160); i++) {
    for (const ch of lines[i]) {
      if (ch === '{') { depth++; opened = true; } else if (ch === '}') depth--;
    }
    if (opened && depth <= 0) {
      const length = i - def.line + 2;
      if (length < 12 || length > 120) return null;
      return { start: def.line, end: i + 1, code: lines.slice(def.line - 1, i + 1).join('\n') };
    }
  }
  return null;
}

/**
 * Asks one session for a question the code answers, and a second session whether it does.
 * @param pick - a definition with its code
 */
async function writeConceptQuestion(pick) {
  const writer = await runClaude({ model: MODEL, cwd: evalRoot(), systemPrompt: 'You write evaluation questions for a code search tool. Reply with one JSON object and nothing else.',
    prompt: `Here is a function from a software project.\n\n\`\`\`\n${pick.code}\n\`\`\`\n\nWrite one question a developer working in this project might ask, in plain English, whose answer is this function. Rules: do not use any identifier, function name, type name, file name, or string literal that appears in the code; describe the behaviour instead. One sentence, under 25 words.\n\nReply as {"question": "..."}.` });
  const question = lastJson(writer.text)?.question;
  if (!question) return null;
  const checker = await runClaude({ model: MODEL, cwd: evalRoot(), systemPrompt: 'You check evaluation questions. Reply with one JSON object and nothing else.',
    prompt: `Question: ${question}\n\nCode:\n\`\`\`\n${pick.code}\n\`\`\`\n\nIs this code a direct and correct answer to the question, such that a developer who asked it would be satisfied to be shown this code? Reply as {"answers": true|false, "why": "..."}.` });
  const verdict = lastJson(checker.text);
  return { question, accepted: verdict?.answers === true, why: verdict?.why || '', tokens: writer.usage.all + checker.usage.all };
}

/**
 * Asks for questions the corpus cannot answer, then keeps those whose distinctive words ripgrep does
 * not find anywhere in it.
 * @param corpus - one entry of CORPORA
 * @param count - how many to keep at most
 */
async function writeUnanswerable(corpus, count) {
  const dir = snapshotDir(corpus);
  const readme = ['README.md', 'readme.md', 'CLAUDE.md'].map((f) => path.join(dir, f)).find((f) => fs.existsSync(f));
  const about = readme ? fs.readFileSync(readme, 'utf8').slice(0, 3000) : '';
  const tops = fs.readdirSync(dir).filter((f) => !f.startsWith('.')).join(', ');
  const answer = await runClaude({ model: MODEL, cwd: evalRoot(), systemPrompt: 'You write evaluation questions for a code search tool. Reply with one JSON object and nothing else.',
    prompt: `This is the start of a software repository's documentation:\n\n${about}\n\nIts top level folders: ${tops}\n\nWrite ${count * 2} questions a developer might plausibly ask about this codebase, phrased like the others they would ask ("where do we ...", "how does ... handle ..."), but each about a behaviour or feature this codebase almost certainly does NOT implement. For each, list 2 or 3 distinctive words or short phrases that code implementing it would have to contain (for example "kubernetes", "oauth2", "pagerduty"). Avoid generic words.\n\nReply as {"questions": [{"question": "...", "terms": ["...", "..."]}]}.` });
  const kept = [];
  for (const q of lastJson(answer.text)?.questions || []) {
    if (!q.question || !Array.isArray(q.terms) || !q.terms.length) continue;
    const found = q.terms.some((term) => {
      const r = runRgSync(grepArgs({ pattern: escapeRegex(term), ignoreCase: true }), dir);
      return readJsonMatches(r.stdout).length > 0;
    });
    if (!found) kept.push({ repo: corpus.name, question: q.question, terms: q.terms });
    if (kept.length >= count) break;
  }
  return kept;
}

fs.mkdirSync(out, { recursive: true });
const only = process.argv.slice(2);
for (const corpus of CORPORA.filter((c) => !c.source.startsWith('http') && (!only.length || only.includes(c.name)))) {
  const conceptFile = path.join(out, `concept-${corpus.name}.jsonl`);
  const done = new Set(fs.existsSync(conceptFile) ? fs.readFileSync(conceptFile, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l).file) : []);
  let tokens = 0;
  for (const pick of process.env.F8_ONLY ? [] : pickDefinitions(corpus.name, PER_CORPUS_F6)) {
    if (done.has(pick.file)) continue;
    const q = await writeConceptQuestion(pick);
    if (!q) continue;
    tokens += q.tokens;
    fs.appendFileSync(conceptFile, JSON.stringify({ repo: corpus.name, file: pick.file, start: pick.start, end: pick.end, name: pick.name, ...q }) + '\n');
  }
  // With F8_ONLY set, new unanswerable questions are added to the ones already kept, without repeats.
  const unanswerableFile = path.join(out, `unanswerable-${corpus.name}.jsonl`);
  const before = process.env.F8_ONLY && fs.existsSync(unanswerableFile) ? fs.readFileSync(unanswerableFile, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l)) : [];
  const fresh = (await writeUnanswerable(corpus, PER_CORPUS_F8)).filter((q) => !before.some((b) => b.question === q.question));
  const unanswerable = [...before, ...fresh].slice(0, PER_CORPUS_F8);
  fs.writeFileSync(unanswerableFile, unanswerable.map((q) => JSON.stringify(q)).join('\n') + '\n');
  console.log(`${corpus.name}: concept questions written, ${unanswerable.length} unanswerable kept, ${tokens} tokens`);
}
