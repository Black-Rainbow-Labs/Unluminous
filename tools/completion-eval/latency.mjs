#!/usr/bin/env node
/**
 * G4 of `tasks/task-2231-autocomplete-intellisense-tdd.md`: the time from a keystroke to a list that
 * holds the right answer, read from a run made with `engine-unluminous.mjs --first`.
 *
 * The popup draws the structural rows at once and merges the server's rows when they arrive, so a
 * query has two moments. Its time is the first list's (`ms0`) when the expected name is in that list's
 * top ten, and the first list's plus the wait for the server (`ms0 + ms`) when it only arrives with the
 * server's rows. A query whose answer is in neither top ten has no right list and is counted apart.
 *
 * Usage: node latency.mjs --run <name> [--split held|tune|all]
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const TOP = 10;

/** The command line, as a map of `--name value`. */
function readArguments(argv) {
  const options = {};
  for (let i = 0; i < argv.length; i += 2) options[argv[i].replace(/^--/, '')] = argv[i + 1];
  return options;
}

/**
 * The value at a fraction of a sorted list.
 * @param sorted - numbers in ascending order
 * @param fraction - 0.5 for the median
 */
function quantile(sorted, fraction) {
  if (!sorted.length) return null;
  return sorted[Math.min(sorted.length - 1, Math.floor(fraction * sorted.length))];
}

/**
 * Each query's time to a right list, by language.
 * @param rows - the run's results
 * @param byId - the positions by id
 * @param split - which half is read
 */
function timesByLanguage(rows, byId, split) {
  const out = {};
  for (const row of rows) {
    const position = byId.get(row.id);
    if (!position || (split !== 'all' && position.split !== split) || row.ms0 === undefined) continue;
    const bucket = (out[position.language] ??= { times: [], fromFirst: 0, fromServer: 0, never: 0 });
    if ((row.labels0 || []).slice(0, TOP).includes(position.expected)) {
      bucket.times.push(row.ms0);
      bucket.fromFirst += 1;
    } else if ((row.labels || []).slice(0, TOP).includes(position.expected)) {
      bucket.times.push(row.ms0 + row.ms);
      bucket.fromServer += 1;
    } else {
      bucket.never += 1;
    }
  }
  return out;
}

const options = readArguments(process.argv.slice(2));
if (!options.run) {
  console.error('usage: latency.mjs --run <name> [--split held|tune|all]');
  process.exit(2);
}
const split = options.split || 'held';
const positions = JSON.parse(fs.readFileSync(path.join(HERE, 'positions.json'), 'utf8')).positions;
const byId = new Map(positions.map((p) => [p.id, p]));
const results = path.join('D:/unluminous-completion-eval/runs', options.run, 'results.jsonl');
const rows = fs.readFileSync(results, 'utf8').trim().split('\n').map((line) => JSON.parse(line));
console.log(`| Language | queries with a right list | from the first list | from the server | never | p50 ms | p95 ms |`);
console.log(`|---|---|---|---|---|---|---|`);
for (const [language, b] of Object.entries(timesByLanguage(rows, byId, split))) {
  const sorted = [...b.times].sort((x, y) => x - y);
  const p50 = quantile(sorted, 0.5);
  const p95 = quantile(sorted, 0.95);
  console.log(`| ${language} | ${sorted.length} | ${b.fromFirst} | ${b.fromServer} | ${b.never} | ${p50?.toFixed(0)} | ${p95?.toFixed(0)} |`);
}
