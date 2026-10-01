// Mines the F2 (exact) and F4 (files) query candidates out of the local transcripts.
//
// F2: every Grep tool call, and every recursive `grep` or `rg` in Bash, that searched one of the
// evaluation repositories, deduplicated by pattern, flags and scope. F4: every Glob call and every
// `find -name`, paired with the file the agent read next inside the same repository, which is the gold.
//
// Output goes to `<eval root>/mined/`, which `freeze.mjs` then samples, splits and hashes into the
// committed query sets. Mining is not part of a run: once frozen, the sets are never re-mined.

import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, TRANSCRIPTS, evalRoot } from './lib/config.mjs';
import { attributePath, listTranscripts, readGrepWords, scanTranscript, shellCommands } from './lib/transcripts.mjs';

const repos = CORPORA.filter((c) => !c.source.startsWith('http')).map((c) => ({ name: c.name, root: c.source }));

/**
 * Resolves the folder a search ran over, relative to the repository it belongs to.
 * @param cwd - the session's working folder
 * @param target - the path the search named, or undefined for the working folder
 */
function resolveScope(cwd, target) {
  if (!cwd) return null;
  let abs = cwd;
  if (target) {
    const t = target.replace(/\//g, '\\');
    abs = /^[a-z]:\\/i.test(t) || /^\\[a-z]\\/i.test(t) ? t : path.win32.join(cwd, t);
  }
  return attributePath(path.win32.normalize(abs), repos);
}

const exact = new Map();

/**
 * Whether a file the agent read is one its file search could have found, so an unrelated read after a
 * search is not taken as the answer to it.
 * @param rel - the file read, relative to the repository
 * @param scope - the folder searched, relative to the repository
 * @param pattern - the glob or `-name` pattern
 */
function nameMatches(rel, scope, pattern) {
  const lower = rel.toLowerCase();
  const prefix = scope ? scope.toLowerCase().replace(/\/$/, '') + '/' : '';
  const within = prefix && lower.startsWith(prefix) ? lower.slice(prefix.length) : lower;
  const glob = pattern.toLowerCase();
  try {
    return path.posix.matchesGlob(within, glob) || path.posix.matchesGlob(path.posix.basename(lower), glob) || path.posix.matchesGlob(within, '**/' + glob);
  } catch { return false; }
}

const files = [];

/**
 * Records one exact search if it targeted an evaluation repository.
 * @param search - the parsed search
 * @param cwd - the session's working folder
 * @param source - where it came from, for the manifest
 */
function addExact(search, cwd, source) {
  const targets = search.paths.length ? search.paths : [undefined];
  for (const target of targets) {
    const scope = resolveScope(cwd, target);
    if (!scope) continue;
    if (/[*?[]/.test(scope.rel)) continue;
    const query = {
      repo: scope.repo, pattern: search.pattern, fixed: !!search.fixed, ignoreCase: !!search.ignoreCase,
      word: !!search.word, path: scope.rel, globs: search.globs || [], types: search.types || [],
    };
    const key = JSON.stringify(query);
    const seen = exact.get(key);
    if (seen) seen.count++; else exact.set(key, { ...query, count: 1, source });
  }
}

/**
 * Reads the Glob and `find -name` calls of one session and pairs each with the next Read inside the
 * same repository.
 * @param events - the session's events in order
 */
function pairFileSearches(events) {
  for (let i = 0; i < events.length; i++) {
    const e = events[i];
    let name = null, scope = null;
    if (e.kind === 'glob' && typeof e.input.pattern === 'string') {
      name = e.input.pattern; scope = resolveScope(e.cwd, e.input.path);
    } else if (e.kind === 'bash') {
      for (const words of shellCommands(e.input.command)) {
        if (path.basename(words[0] || '') !== 'find') continue;
        const at = words.findIndex((w) => w === '-name' || w === '-iname');
        if (at < 0 || !words[at + 1] || words[at + 1].includes('\u0000')) continue;
        name = words[at + 1]; scope = resolveScope(e.cwd, words[1] && !words[1].startsWith('-') ? words[1] : undefined);
      }
    }
    if (!name || !scope) continue;
    for (let j = i + 1; j < Math.min(events.length, i + 6); j++) {
      const r = events[j];
      if (r.kind !== 'read') continue;
      const read = attributePath(r.input.file_path, repos);
      if (read && read.repo === scope.repo && nameMatches(read.rel, scope.rel, name)) {
        files.push({ repo: scope.repo, pattern: name, path: scope.rel, read: read.rel, source: path.basename(e.session) });
        break;
      }
    }
  }
}

const transcripts = listTranscripts(TRANSCRIPTS);
let grepTool = 0, bashGrep = 0;
for (const file of transcripts) {
  const events = [];
  await scanTranscript(file, (e) => {
    events.push(e);
    if (e.kind === 'grep-tool' && typeof e.input.pattern === 'string') {
      grepTool++;
      addExact({ pattern: e.input.pattern, fixed: false, ignoreCase: !!e.input['-i'], word: false,
        paths: e.input.path ? [e.input.path] : [], globs: e.input.glob ? [e.input.glob] : [], types: e.input.type ? [e.input.type] : [] },
      e.cwd, 'grep-tool');
    } else if (e.kind === 'bash') {
      for (const words of shellCommands(e.input.command)) {
        const search = readGrepWords(words);
        if (search) { bashGrep++; addExact(search, e.cwd, 'bash'); }
      }
    }
  });
  pairFileSearches(events);
}

const out = path.join(evalRoot(), 'mined');
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, 'exact.jsonl'), [...exact.values()].map((q) => JSON.stringify(q)).join('\n') + '\n');
fs.writeFileSync(path.join(out, 'files.jsonl'), files.map((q) => JSON.stringify(q)).join('\n') + '\n');
const byRepo = {};
for (const q of exact.values()) byRepo[q.repo] = (byRepo[q.repo] || 0) + 1;
console.log(JSON.stringify({ transcripts: transcripts.length, grepTool, bashGrep, distinctExact: exact.size, byRepo, fileSearches: files.length }, null, 1));
