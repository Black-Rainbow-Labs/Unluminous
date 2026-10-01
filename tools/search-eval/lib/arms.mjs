// The arms of TDD section 7.4. Each arm answers one query on one corpus snapshot and reports the
// answer, the text an agent would be shown, and how long it took.
//
// The rg arm answers each family the way an agent with Claude Code's Grep and Glob tools would:
// F1 and F3 are a content search for the name as a whole word, F2 is the mined pattern, F4 is the
// mined glob. Its text is what Claude Code shows, capped at the tool's default `head_limit` of 250
// lines, because that is what the agent pays tokens for.

import crypto from 'node:crypto';
import { escapeRegex, exactSet, globArgs, grepArgs, readJsonMatches, runRgSync, runRgTimed } from './rg.mjs';

const HEAD_LIMIT = 250;

/**
 * The digest of an exact answer, over its sorted `path:line` set.
 * @param hits - `{ path, line }` rows
 */
export function digestOf(hits) {
  return crypto.createHash('sha256').update(exactSet(hits).join('\n')).digest('hex').slice(0, 16);
}

/**
 * The ripgrep arguments that answer a query of a family in the rg arm.
 * @param query - a frozen query
 */
export function rgArgsFor(query) {
  if (query.family === 'F1' || query.family === 'F3') return grepArgs({ pattern: `\\b${escapeRegex(query.name)}\\b` });
  if (query.family === 'F2') return grepArgs(query);
  if (query.family === 'F4') return globArgs(query.pattern, query.path);
  throw new Error(`the rg arm has no single call for ${query.family}`);
}

/**
 * The text Claude Code shows for a content search: `path:line:text`, capped at 250 lines with the
 * note the tool adds.
 * @param hits - the match rows in rg's order
 */
export function grepText(hits) {
  const shown = hits.slice(0, HEAD_LIMIT).map((h) => `${h.path}:${h.line}:${h.text.length > 500 ? '[Omitted long matching line]' : h.text}`);
  if (hits.length > HEAD_LIMIT) shown.push(`[Showing results with pagination = limit: ${HEAD_LIMIT}]`);
  return shown.join('\n');
}

/**
 * Turns ripgrep's output for a query into an answer.
 * @param query - the frozen query
 * @param stdout - ripgrep's output
 */
export function rgAnswer(query, stdout) {
  if (query.family === 'F4') {
    const files = stdout.split('\0').filter(Boolean).map((f) => f.replace(/\\/g, '/').replace(/^\.\//, ''));
    const shown = files.slice(0, 100);
    return { hits: [], files, empty: files.length === 0, text: shown.join('\n') };
  }
  const hits = readJsonMatches(stdout);
  return { hits, files: [...new Set(hits.map((h) => h.path))], empty: hits.length === 0, text: grepText(hits) };
}

/**
 * The rg arm. `order` re-runs a content search single threaded and sorted by path, so the rank used
 * for F1's accuracy does not change with how ripgrep's threads happened to finish.
 */
export const rgArm = {
  name: 'rg',
  async time(query, dir) {
    const r = await runRgTimed(rgArgsFor(query), dir);
    return { ms: r.ms, status: r.status, answer: rgAnswer(query, r.stdout) };
  },
  order(query, dir) {
    const args = rgArgsFor(query);
    if (query.family === 'F4') return rgAnswer(query, runRgSync(args, dir).stdout);
    return rgAnswer(query, runRgSync([...args.slice(0, -1), '--sort', 'path', args[args.length - 1]], dir).stdout);
  },
};
