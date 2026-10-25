// The tests that failed and then passed when they were run again on their own (`task-2364`).
//
//   node tools/flakes.mjs              the last 14 days, most frequent first
//   node tools/flakes.mjs --days 60    a longer window
//   node tools/flakes.mjs --runs 10    also the last 10 runs of the gate and the window suite
//
// `tools/test.mjs` and `tools/window-suite.mjs --run` run a failed test once more, alone, and write one
// line to `_agent_output/test-runs/flakes.jsonl` of the main checkout for each that passes then. A flake
// does not stop a release, so this is where somebody sees them. The rule in `CLAUDE.md`: a test named
// here twice in 14 days gets a ticket, and is either fixed or marked `#[ignore = "quarantined: task-N"]`
// until it is.

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { readRecords } from './test-runs.mjs';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Read a `--name value` argument as a number, or the fallback.
 * @param name - the option without its dashes
 * @param fallback - what to answer when it was not given
 */
function number(name, fallback) {
  const at = process.argv.indexOf(`--${name}`);
  return at >= 0 ? Number(process.argv[at + 1]) : fallback;
}

/** Print the flakes of the window, most frequent first, and the last runs when asked. */
function main() {
  const days = number('days', 14);
  const since = Date.now() - days * 24 * 60 * 60 * 1000;
  const flakes = readRecords(repo, 'flakes.jsonl').filter((flake) => Date.parse(flake.at) >= since);
  const byTest = new Map();
  for (const flake of flakes) {
    const key = `${flake.crate} ${flake.target} ${flake.test}`;
    byTest.set(key, [...(byTest.get(key) ?? []), flake]);
  }
  const rows = [...byTest.entries()].sort((left, right) => right[1].length - left[1].length);
  if (rows.length === 0) console.log(`No test failed and then passed in the last ${days} days.`);
  else console.log(`Tests that failed and then passed on their own, in the last ${days} days:`);
  for (const [test, seen] of rows) {
    const last = seen.at(-1);
    const mark = seen.length >= 2 ? '  <- twice or more: file a ticket, then fix or quarantine it' : '';
    console.log(`  ${String(seen.length).padStart(3)}  ${test}  (last ${last.at.slice(0, 16)} at ${last.commit.slice(0, 8)})${mark}`);
  }
  const runs = number('runs', 0);
  if (runs > 0) {
    console.log(`\nThe last ${runs} runs:`);
    for (const run of readRecords(repo, 'runs.jsonl').slice(-runs)) {
      const verdict = run.failed?.length ? `FAILED (${run.failed.length})` : 'passed';
      const flaky = run.flaky?.length ? `, ${run.flaky.length} flaky` : '';
      const load = run.load ? ` beside ${run.load} load threads` : '';
      console.log(`  ${run.at.slice(0, 16)}  ${run.kind.padEnd(6)} ${verdict}${flaky} in ${run.seconds} s${load}`);
    }
  }
}

main();
