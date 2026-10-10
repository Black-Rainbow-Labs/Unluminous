#!/usr/bin/env node
// Chooses the cursor positions the completion evaluation asks every engine about, and freezes them in
// positions.json (`task-2231` §8.1). The README beside this file describes the format and the classes.
//
// It reads the token dump `completion_tokens` writes for each corpus (the code index's own tokeniser and
// definition reading, so "not in a comment or a string" is the index's own answer) and the files
// themselves. Sampling is seeded, so the same corpora give the same positions. A positions file that
// already exists is never overwritten without --force: the frozen file is what makes a held out number
// mean something.
//
//   node tools/completion-eval/gen-positions.mjs [--per-corpus 2000] [--seed 2231] [--force] [--out <path>]

import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { CORPORA_ROOT, EVAL_ROOT, readCorpora } from './prepare-corpora.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const CLASSES = ['member', 'path', 'local', 'global', 'type', 'import', 'needs-import', 'keyword'];

/** A small seeded random number generator (mulberry32). */
function seeded(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Shuffles a list in place with a seeded generator. */
function shuffle(list, random) {
  for (let i = list.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [list[i], list[j]] = [list[j], list[i]];
  }
  return list;
}

/** Whether a position id is in the held out half: a stable hash of the id, so a split never moves. */
export function splitOf(id) {
  const digest = crypto.createHash('sha256').update(id).digest();
  return digest[0] % 2 === 0 ? 'tune' : 'held';
}

/** The token dump of a corpus, written by the `completion_tokens` example when it is missing. */
function tokensOf(name, language) {
  const out = path.join(EVAL_ROOT, 'tokens', `${name}.jsonl`);
  if (!fs.existsSync(out)) {
    const exe = process.env.COMPLETION_TOKENS_EXE ||
      'D:/agent-worktrees/cargo-target/unluminous-task-2232/release/examples/completion_tokens.exe';
    fs.mkdirSync(path.dirname(out), { recursive: true });
    fs.writeFileSync(out, execFileSync(exe, [path.join(CORPORA_ROOT, name), language], { maxBuffer: 1 << 30 }));
  }
  return fs.readFileSync(out, 'utf8').split('\n').filter(Boolean).map((line) => JSON.parse(line));
}

/** What each language's statements look like, as far as choosing positions needs. */
const SHAPES = {
  rust: { imports: ['use'], members: ['.'], paths: ['::'], types: [':', '->', 'impl', 'dyn', 'for', 'as'] },
  typescript: { imports: ['import'], members: ['?.', '.'], paths: [], types: [':', 'extends', 'implements', 'as', 'keyof', 'typeof'] },
  python: { imports: ['import', 'from'], members: ['.'], paths: [], types: [':', '->'] },
  go: { imports: ['import'], members: ['.'], paths: [], types: [] },
};

/**
 * The import statements of a file as byte ranges, each with the whole lines it covers.
 * @param file - the token dump of one file
 * @param text - the file's text
 * @param shape - the language's shape
 */
function importStatements(file, text, shape) {
  const statements = [];
  const quietEnds = file.quiet;
  for (const [start, wordEnd, kind] of file.words) {
    if (kind !== 'k') continue;
    const word = text.slice(start, wordEnd);
    if (!shape.imports.includes(word)) continue;
    // A statement starts a line, after only white space or `export`/`pub` style keywords.
    const lineStart = text.lastIndexOf('\n', start - 1) + 1;
    const lead = text.slice(lineStart, start).trim();
    if (lead !== '' && !/^(pub(\([a-z]+\))?|export)$/.test(lead)) continue;
    if (shape === SHAPES.python && word === 'import' && /^from\b/.test(text.slice(lineStart).trim())) continue;
    let end;
    if (shape === SHAPES.rust) {
      end = text.indexOf(';', start);
    } else if (shape === SHAPES.typescript) {
      // `import … from '…'` ends at the specifier; `import '…'` likewise.
      const quote = quietEnds.find(([s]) => s > start);
      end = quote ? quote[1] : text.indexOf('\n', start);
      if (text[end] === ';') end += 1;
    } else if (shape === SHAPES.go) {
      const open = text.indexOf('(', start);
      const newline = text.indexOf('\n', start);
      end = open >= 0 && open < newline ? text.indexOf(')', open) + 1 : newline;
    } else {
      const open = text.indexOf('(', start);
      const newline = text.indexOf('\n', start);
      end = open >= 0 && open < newline ? text.indexOf(')', open) + 1 : newline;
    }
    if (end < 0) end = text.length;
    let lineEnd = text.indexOf('\n', end);
    lineEnd = lineEnd < 0 ? text.length : lineEnd + 1;
    const names = new Set(
      // The names a statement brings into scope, which in a path import are not the segments on the
      // way: `use std::path::PathBuf` imports `PathBuf`, not `path`.
      file.words
        .filter(([s2, e2]) => s2 >= start && s2 < end && !text.slice(e2, e2 + 2).startsWith('::'))
        .map(([s2, e2]) => text.slice(s2, e2)),
    );
    statements.push({ start, end, names, lines: { start: lineStart, end: lineEnd } });
  }
  return statements;
}

/**
 * What comes before a byte in the code: the text since the previous line break, with trailing white
 * space removed.
 */
function before(text, at) {
  const lineStart = text.lastIndexOf('\n', at - 1) + 1;
  return text.slice(lineStart, at).trimEnd();
}

/** A name read from the one-character-a-byte view, as the UTF-8 text it is. */
function decoded(word) {
  return Buffer.from(word, 'latin1').toString('utf8');
}

/** The enclosing function's parameter list holds this name. */
function isParameter(text, blocks, line, name) {
  const enclosing = blocks
    .filter((b) => b.kind === 'function' && b.line <= line && b.end >= line && b.end > b.line)
    .sort((a, b) => b.line - a.line)[0];
  if (!enclosing) return false;
  const lines = text.split('\n');
  const head = lines.slice(enclosing.line - 1, Math.min(enclosing.line + 4, lines.length)).join('\n');
  const open = head.indexOf('(');
  const close = head.indexOf(')', open);
  if (open < 0 || close < 0) return false;
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(`\\b${escaped}\\b`).test(head.slice(open, close));
}

/**
 * Every candidate position of one corpus, by class.
 * @param name - the corpus
 * @param language - its language
 * @param files - its token dump
 */
function candidates(name, language, files) {
  const shape = SHAPES[language];
  const root = path.join(CORPORA_ROOT, name);
  const texts = new Map();
  const frequency = new Map();
  const definedIn = new Map(); // name -> Set(path), for names another file could use
  const typeNames = new Set();
  for (const file of files) {
    // One character a byte, so every offset the token dump gives is an index here. A name is turned back
    // into UTF-8 with `decoded` before it is written down.
    const text = fs.readFileSync(path.join(root, file.path)).toString('latin1').replaceAll('\r\n', '\n');
    texts.set(file.path, text);
    for (const [s, e] of file.words) {
      const word = text.slice(s, e);
      frequency.set(word, (frequency.get(word) || 0) + 1);
    }
    for (const d of file.definitions) {
      const word = text.slice(d.start, d.end);
      if (d.kind === 'type') typeNames.add(word);
      if (d.kind === 'variable' && !d.exported) continue;
      if (!definedIn.has(word)) definedIn.set(word, new Set());
      definedIn.get(word).add(file.path);
    }
  }
  const byClass = Object.fromEntries(CLASSES.map((c) => [c, []]));
  for (const file of files) {
    const text = texts.get(file.path);
    const statements = importStatements(file, text, shape);
    const definitionStarts = new Set(file.definitions.map((d) => d.start));
    const localDefinitions = file.definitions.filter((d) => d.kind === 'variable');
    const lineStarts = [0];
    for (let i = 0; i < text.length; i++) if (text.charCodeAt(i) === 10) lineStarts.push(i + 1);
    const lineOf = (at) => {
      let lo = 0, hi = lineStarts.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (lineStarts[mid] <= at) lo = mid; else hi = mid - 1;
      }
      return lo + 1;
    };
    const topBlockOf = (line) =>
      file.blocks.find((b) => b.depth === 0 && b.line <= line && b.end >= line);
    let previous = null;
    for (const [start, end, kind] of file.words) {
      const word = text.slice(start, end);
      const prior = previous;
      previous = [start, end, kind];
      if ([...word].length < 2 || (frequency.get(word) || 0) < 2) continue;
      if (definitionStarts.has(start)) continue;
      // Two words with only spaces between them, the first not a keyword, is prose: the text of a JSX
      // element, which the tokeniser reads as words. Code puts an operator or a keyword between names.
      if (prior && prior[2] !== 'k' && /^[ \t]+$/.test(text.slice(prior[1], start))) continue;
      const lead = before(text, start);
      const after = text.slice(end, end + 3);
      // A name being declared rather than used: a parameter, a field, an object key.
      if (/^\s*:(?!:)/.test(after) && /(^|[(,{|])$/.test(lead.trimEnd())) continue;
      const position = { path: file.path, start, end, expected: decoded(word), remove: [] };
      const inImport = statements.find((s) => s.start <= start && start < s.end);
      let cls = null;
      if (kind === 'k') {
        cls = inImport ? null : 'keyword';
      } else if (inImport) {
        cls = 'import';
      } else if (shape.members.some((m) => lead.endsWith(m) && !lead.endsWith('..'))) {
        cls = 'member';
      } else if (shape.paths.some((p) => lead.endsWith(p))) {
        cls = 'path';
      } else if (
        shape.types.some((t) => (/^[a-z]+$/.test(t) ? new RegExp(`\\b${t}$`).test(lead) : lead.endsWith(t) && !lead.endsWith('::'))) &&
        (kind === 't' || kind === 'b' || typeNames.has(word)) &&
        // `frames: Vec::new()` is a struct literal's value, not a type.
        !/^\s*(::|\()/.test(after)
      ) {
        cls = 'type';
      } else {
        const elsewhere = definedIn.get(word);
        const definedHere = elsewhere?.has(file.path);
        const importing = statements.filter((s) => s.names.has(word));
        const line = lineOf(start);
        const block = topBlockOf(line);
        const boundBefore = localDefinitions.some((d) => {
          if (d.start >= start || text.slice(d.start, d.end) !== word) return false;
          const dl = lineOf(d.start);
          return block ? dl >= block.line && dl <= block.end : false;
        });
        // Followed by `.` in Rust it is a value, so a local that shares a module's name.
        const aValue = language === 'rust' && /^\s*\.(?!\.)/.test(after);
        // A word the reader cannot tell from a local by its spelling alone is only taken as a project
        // name when its shape says so: a call, a capitalised name, a constant, or a module before `::`.
        const projectShaped =
          kind === 'f' || kind === 't' || /^[A-Z][A-Z0-9_]+$/.test(word) || /^\s*::/.test(after);
        if (elsewhere && !definedHere && importing.length === 1 && !boundBefore && !aValue) {
          cls = 'needs-import';
          position.remove = [importing[0].lines];
        } else if ((kind === 'w' || kind === 'f') && (boundBefore || isParameter(text, file.blocks, line, word))) {
          cls = 'local';
        } else if (elsewhere && projectShaped && !boundBefore) {
          cls = 'global';
        }
      }
      if (!cls) continue;
      position.class = cls;
      byClass[cls].push(position);
    }
  }
  return byClass;
}

/** Runs from the command line. */
function main() {
  const args = process.argv.slice(2);
  const option = (flag, fallback) => (args.includes(flag) ? args[args.indexOf(flag) + 1] : fallback);
  const perCorpus = Number(option('--per-corpus', 2000));
  const seed = Number(option('--seed', 2231));
  const out = option('--out', path.join(HERE, 'positions.json'));
  if (fs.existsSync(out) && !args.includes('--force')) {
    console.error(`${out} exists and is frozen; pass --force to write it again`);
    process.exit(2);
  }
  const corpora = readCorpora();
  const positions = [];
  const summary = {};
  const corporaOut = {};
  for (const [name, spec] of Object.entries(corpora)) {
    const random = seeded(seed + [...name].reduce((a, c) => a + c.charCodeAt(0), 0));
    const byClass = candidates(name, spec.language, tokensOf(name, spec.language));
    const per = Math.floor(perCorpus / CLASSES.length);
    summary[name] = {};
    const chosen = [];
    for (const cls of CLASSES) {
      const taken = shuffle(byClass[cls], random).slice(0, per);
      summary[name][cls] = `${taken.length} of ${byClass[cls].length}`;
      chosen.push(...taken);
    }
    chosen.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : a.start - b.start));
    chosen.forEach((p, i) => {
      const id = `${name}:${String(i).padStart(5, '0')}`;
      positions.push({ id, corpus: name, language: spec.language, ...p, split: splitOf(id) });
    });
    corporaOut[name] = { language: spec.language, commit: spec.commit, control: Boolean(spec.control) };
  }
  const file = { version: 1, seed, created: new Date().toISOString(), corpora: corporaOut, positions };
  fs.writeFileSync(out, JSON.stringify(file, null, 1));
  const hash = crypto.createHash('sha256').update(fs.readFileSync(out)).digest('hex');
  console.log(JSON.stringify(summary, null, 2));
  console.log(`${positions.length} positions written to ${out}, sha256 ${hash}`);
}

main();
