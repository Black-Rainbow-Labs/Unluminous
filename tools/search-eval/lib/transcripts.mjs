// Reads the local Claude Code transcripts and pulls out every search an agent made against one of the
// evaluation's repositories: Grep tool calls, `grep` and `rg` in Bash, Glob calls and `find -name`.
//
// This is where the F2 (exact) and F4 (files) query sets come from, so the patterns are the ones agents
// really typed rather than ones written for the test. Each search is attributed to a repository from the
// session's working folder and the path it searched, and a worktree of a repository counts as that
// repository.

import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline';

/**
 * Lists every transcript file under the projects folder, including sub agent transcripts.
 * @param root - the `~/.claude/projects` folder
 */
export function listTranscripts(root) {
  const out = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.name.endsWith('.jsonl')) out.push(full);
    }
  };
  walk(root);
  return out.sort();
}

/**
 * Works out which evaluation repository a path belongs to, treating a worktree of a repository as the
 * repository, and returns the path relative to that repository's root.
 * @param absolute - an absolute Windows or POSIX path
 * @param repos - `{ name, root }` for each repository, roots as absolute Windows paths
 */
export function attributePath(absolute, repos) {
  if (!absolute) return null;
  const original = absolute.replace(/\//g, '\\').replace(/^\\([a-z])\\/i, '$1:\\');
  const norm = original.toLowerCase();
  for (const repo of repos) {
    const root = repo.root.replace(/\//g, '\\').toLowerCase().replace(/\\$/, '');
    if (norm !== root && !norm.startsWith(root + '\\')) continue;
    let rest = original.slice(root.length).replace(/^\\/, '');
    const worktree = rest.match(/^\.claude\\worktrees\\[^\\]+\\?(.*)$/);
    if (worktree) rest = worktree[1];
    return { repo: repo.name, rel: rest.replace(/\\/g, '/') };
  }
  return null;
}

/**
 * Splits a shell command line into words the way a POSIX shell would for simple commands: quotes are
 * honoured, and anything after a pipe, `&&`, `;` or a redirection ends the command. Returns the words of
 * each simple command.
 * @param command - the Bash command text
 */
export function shellCommands(command) {
  const commands = [];
  let words = [];
  let word = '';
  let inWord = false;
  let i = 0;
  const end = () => { if (inWord) { words.push(word); word = ''; inWord = false; } };
  const endCommand = () => { end(); if (words.length) commands.push(words); words = []; };
  while (i < command.length) {
    const ch = command[i];
    if (ch === "'") {
      const close = command.indexOf("'", i + 1);
      if (close < 0) return commands;
      word += command.slice(i + 1, close); inWord = true; i = close + 1; continue;
    }
    if (ch === '"') {
      let j = i + 1;
      while (j < command.length && command[j] !== '"') {
        if (command[j] === '\\' && '"\\$`'.includes(command[j + 1])) { word += command[j + 1]; j += 2; continue; }
        if (command[j] === '$' || command[j] === '`') { word += '\u0000'; }
        word += command[j]; j++;
      }
      inWord = true; i = j + 1; continue;
    }
    if (ch === '\\' && i + 1 < command.length) { word += command[i + 1]; inWord = true; i += 2; continue; }
    if (ch === ' ' || ch === '\t') { end(); i++; continue; }
    if (ch === '\n' || ch === ';' || ch === '|' || ch === '&') { endCommand(); i++; continue; }
    if (ch === '>' || ch === '<') { if (inWord && /^\d+$/.test(word)) { word = ''; inWord = false; } endCommand(); while (i < command.length && !'\n;|&'.includes(command[i])) i++; continue; }
    if (ch === '$' || ch === '`') { word += '\u0000'; }
    word += ch; inWord = true; i++;
  }
  endCommand();
  return commands;
}

/**
 * Converts a POSIX basic regular expression, as plain `grep` reads one, into the syntax ripgrep reads.
 * In a BRE `\|`, `\(`, `\)`, `\{`, `\}`, `\+` and `\?` are the operators and the bare characters are
 * literal, which is the other way round from ripgrep. Returns null for a backreference, which ripgrep
 * cannot express.
 * @param pattern - the pattern as typed after `grep`
 */
export function basicToRust(pattern) {
  let out = '';
  for (let i = 0; i < pattern.length; i++) {
    const ch = pattern[i];
    if (ch === '\\' && i + 1 < pattern.length) {
      const next = pattern[++i];
      if ('|(){}+?'.includes(next)) out += next;
      else if (/[1-9]/.test(next)) return null;
      else out += '\\' + next;
      continue;
    }
    if ('|(){}+?'.includes(ch)) { out += '\\' + ch; continue; }
    out += ch;
  }
  return out;
}

/**
 * Reads one grep or rg command's words into a search, or null if it is not a search over files that the
 * evaluation can replay (no pattern, a pattern built from a shell variable, a backreference, a search
 * of standard input).
 * @param words - the words of one simple command
 */
export function readGrepWords(words) {
  const tool = path.basename(words[0] || '').replace(/\.exe$/, '');
  if (tool !== 'grep' && tool !== 'rg' && tool !== 'egrep') return null;
  let ignoreCase = false, fixed = false, extended = tool === 'egrep', recursive = tool === 'rg', word = false;
  let pattern = null; const paths = []; const globs = []; const types = [];
  const takesValue = new Set(['-A', '-B', '-C', '-m', '--max-count', '-g', '--glob', '--include', '--exclude',
    '--exclude-dir', '-t', '--type', '-T', '--type-not', '-e', '--regexp', '-f', '--max-columns', '-M', '--context',
    '--after-context', '--before-context', '-j', '--threads', '--color', '--colors', '--sort', '--sortr', '-r', '--replace']);
  for (let i = 1; i < words.length; i++) {
    const w = words[i];
    if (w.includes('\u0000')) return null;
    if (w === '--') { if (pattern === null) pattern = words[++i]; else paths.push(...words.slice(i + 1)); break; }
    if (w.startsWith('--')) {
      const [flag, inline] = w.split('=', 2);
      const value = inline ?? (takesValue.has(flag) ? words[++i] : undefined);
      if (flag === '--ignore-case') ignoreCase = true;
      else if (flag === '--fixed-strings') fixed = true;
      else if (flag === '--extended-regexp') extended = true;
      else if (flag === '--recursive') recursive = true;
      else if (flag === '--word-regexp') word = true;
      else if (flag === '--regexp') pattern = value;
      else if (flag === '--include' || flag === '--glob') globs.push(value);
      else if (flag === '--type') types.push(value);
      else if (flag === '--exclude' || flag === '--exclude-dir' || flag === '--type-not' || flag === '--pcre2' || flag === '--multiline') return null;
      continue;
    }
    if (w.startsWith('-') && w.length > 1 && !/^-\d/.test(w)) {
      const letters = w.slice(1);
      for (let k = 0; k < letters.length; k++) {
        const f = letters[k];
        const valueFlag = '-' + f;
        if (takesValue.has(valueFlag) && !(tool !== 'rg' && 'rgtT'.includes(f))) {
          const value = letters.slice(k + 1) || words[++i];
          if (f === 'e') pattern = value;
          else if (f === 'g') globs.push(value);
          else if (f === 't') types.push(value);
          else if (f === 'T') return null;
          else if (f === 'r' && tool === 'rg') return null;
          break;
        }
        if (f === 'i') ignoreCase = true;
        else if (f === 'F') fixed = true;
        else if (f === 'E') extended = true;
        else if (f === 'r' || f === 'R') recursive = true;
        else if (f === 'w') word = true;
        else if (f === 'P' || f === 'U' || f === 'v' || f === 'z') return null;
      }
      continue;
    }
    if (pattern === null) pattern = w; else paths.push(w);
  }
  if (!pattern || !recursive) return null;
  let regex = pattern;
  if (tool !== 'rg' && !fixed && !extended) regex = basicToRust(pattern);
  if (regex === null) return null;
  return { pattern: regex, fixed, ignoreCase, word, paths, globs, types };
}

/**
 * Streams one transcript and calls back for every search tool call, every Glob and find call, and every
 * Read, in order, with the session's working folder.
 * @param file - a transcript `.jsonl` file
 * @param visit - `(event) => void`, where event is `{ kind, cwd, input, session, index }`
 */
export async function scanTranscript(file, visit) {
  const stream = fs.createReadStream(file, { encoding: 'utf8' });
  const lines = readline.createInterface({ input: stream, crlfDelay: Infinity });
  let index = 0;
  for await (const line of lines) {
    if (!line.includes('"tool_use"')) continue;
    let record;
    try { record = JSON.parse(line); } catch { continue; }
    if (record.type !== 'assistant' || !Array.isArray(record.message?.content)) continue;
    for (const block of record.message.content) {
      if (block.type !== 'tool_use') continue;
      const base = { cwd: record.cwd, session: file, index: index++, input: block.input || {} };
      if (block.name === 'Grep') visit({ ...base, kind: 'grep-tool' });
      else if (block.name === 'Glob') visit({ ...base, kind: 'glob' });
      else if (block.name === 'Read') visit({ ...base, kind: 'read' });
      else if (block.name === 'Bash' && typeof block.input?.command === 'string') visit({ ...base, kind: 'bash' });
    }
  }
}
