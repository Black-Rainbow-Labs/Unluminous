// The rg arm: the ripgrep Claude Code itself runs, with the arguments its Grep and Glob tools pass.
//
// Claude Code 2.1.286 is one native executable with ripgrep 14.1.1 inside it. Its Grep tool starts
// that same executable with the program name `rg` and `--no-config` as the first argument, so this
// does exactly that. The flags are copied from the tool's own source (TDD section 0, R10): `--hidden`,
// six version control folders excluded, `--max-columns 500`, and `--json -n` in content mode.
//
// The Grep tool has no fixed string switch and no whole word switch, so a mined `grep -F` pattern is
// escaped into a regex and a `grep -w` pattern is wrapped in word boundaries, which is what an agent
// using the tool would have to write.

import { spawn, spawnSync } from 'node:child_process';
import os from 'node:os';
import path from 'node:path';

export const CLAUDE_EXE = process.env.SEARCH_EVAL_CLAUDE_EXE || path.join(os.homedir(), '.local', 'bin', 'claude.exe');
const VCS = ['.git', '.svn', '.hg', '.bzr', '.jj', '.sl'];

/**
 * Escapes a string so ripgrep reads it as literal text.
 * @param text - the literal
 */
export function escapeRegex(text) {
  return text.replace(/[\\^$.|?*+()[\]{}]/g, '\\$&');
}

/**
 * The regex an agent would hand the Grep tool for a mined query.
 * @param query - an F2 query: `{ pattern, fixed, word }`
 */
export function grepPattern(query) {
  let pattern = query.fixed ? escapeRegex(query.pattern) : query.pattern;
  if (query.word) pattern = `\\b(?:${pattern})\\b`;
  return pattern;
}

/**
 * The arguments Claude Code's Grep tool passes in content mode.
 * @param query - `{ pattern, fixed, word, ignoreCase, globs, types, path }`
 */
export function grepArgs(query) {
  const args = ['--no-config', '--hidden'];
  for (const dir of VCS) args.push('--glob', `!${dir}`);
  args.push('--max-columns', '500');
  if (query.ignoreCase) args.push('-i');
  args.push('--json', '-n');
  const pattern = grepPattern(query);
  if (pattern.startsWith('-')) args.push('-e', pattern); else args.push(pattern);
  for (const type of query.types || []) args.push('--type', type);
  for (const glob of query.globs || []) args.push('--glob', glob);
  args.push(query.path || '.');
  return args;
}

/**
 * The arguments Claude Code's Glob tool passes.
 * @param pattern - the glob
 * @param scope - the folder searched, relative to the corpus
 */
export function globArgs(pattern, scope) {
  return ['--no-config', '--files', '--null', '--glob', pattern, '--sort=modified', '--no-ignore', '--hidden', scope || '.'];
}

/**
 * Reads ripgrep's `--json` output into match rows and the text Claude Code would show.
 * @param stdout - the JSON lines
 */
export function readJsonMatches(stdout) {
  const hits = [];
  for (const line of stdout.split('\n')) {
    if (!line.startsWith('{"type":"match"')) continue;
    const event = JSON.parse(line);
    const file = (event.data.path.text ?? Buffer.from(event.data.path.bytes, 'base64').toString('latin1')).replace(/\\/g, '/').replace(/^\.\//, '');
    const text = event.data.lines.text ?? '';
    hits.push({ path: file, line: event.data.line_number, text: text.replace(/\r?\n$/, '') });
  }
  return hits;
}

/**
 * Runs the embedded ripgrep once, synchronously, in a corpus folder.
 * @param args - ripgrep's arguments, starting with `--no-config`
 * @param cwd - the corpus snapshot
 */
export function runRgSync(args, cwd, maxBytes = 1 << 28) {
  const r = spawnSync(CLAUDE_EXE, args, { cwd, argv0: 'rg', maxBuffer: maxBytes, windowsHide: true });
  if (r.error && r.error.code === 'ENOBUFS') return { status: null, tooLarge: true, stdout: '', stderr: '' };
  return { status: r.status, stdout: r.stdout ? r.stdout.toString('utf8') : '', stderr: r.stderr ? r.stderr.toString('utf8') : '' };
}

/**
 * Runs the embedded ripgrep and measures the time from spawning it to having all of its output.
 * @param args - ripgrep's arguments, starting with `--no-config`
 * @param cwd - the corpus snapshot
 */
export function runRgTimed(args, cwd) {
  return new Promise((resolve) => {
    const started = process.hrtime.bigint();
    const child = spawn(CLAUDE_EXE, args, { cwd, argv0: 'rg', windowsHide: true });
    const chunks = [];
    child.stdout.on('data', (c) => chunks.push(c));
    child.stderr.on('data', () => {});
    child.on('close', (status) => {
      const ms = Number(process.hrtime.bigint() - started) / 1e6;
      resolve({ status, ms, stdout: Buffer.concat(chunks).toString('utf8') });
    });
  });
}

/**
 * The exact result of a content search as a sorted list of `path:line`, which is what parity and the
 * result digest are taken over.
 * @param hits - rows from readJsonMatches
 */
export function exactSet(hits) {
  return [...new Set(hits.map((h) => `${h.path}:${h.line}`))].sort();
}
