// The harness's own tests (TDD section 10): each way a run can be wrong must produce its documented
// failure rather than a score.
//
//   node tools/search-eval/test-harness.mjs
//
// 1. A frozen set whose bytes changed (an edited query, a corrupted gold) is refused with exit 3.
// 2. A box busier than the quiet reference is not graded: exit 4, "NOT GRADED".
// 3. An arm that crashes is a failure row scored zero, never a missing row.
// Each runs against a temporary eval root and a temporary copy of the sets, so nothing real is touched.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { HERE } from './lib/config.mjs';

const RUN = path.join(HERE, 'run.mjs');
const failures = [];

/**
 * Makes a temporary eval root that borrows the real corpora, and a copy of the frozen sets.
 */
function sandbox() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'search-eval-test-'));
  const queries = path.join(root, 'queries');
  fs.cpSync(path.join(HERE, 'queries'), queries, { recursive: true });
  const realRoot = process.env.SEARCH_EVAL_ROOT || 'D:/unluminous-search-eval';
  fs.symlinkSync(path.join(realRoot, 'corpora'), path.join(root, 'corpora'), 'junction');
  return { root, queries };
}

/**
 * Runs the runner in a sandbox and returns its exit code and output.
 * @param box - the sandbox
 * @param args - the runner's arguments
 */
function run(box, args) {
  const r = spawnSync(process.execPath, [RUN, ...args], { encoding: 'utf8', env: { ...process.env, SEARCH_EVAL_ROOT: box.root, SEARCH_EVAL_QUERIES: box.queries } });
  return { code: r.status, out: (r.stdout || '') + (r.stderr || '') };
}

/**
 * Records a check's outcome.
 * @param name - what was checked
 * @param ok - whether it held
 * @param detail - what was seen
 */
function check(name, ok, detail) {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : `: ${detail}`}`);
  if (!ok) failures.push(name);
}

const small = ['--families', 'F2', '--corpora', 'unluminous', '--max', '2', '--reps', '1', '--warmup', '0'];

{
  const box = sandbox();
  const file = path.join(box.queries, 'F2-exact.jsonl');
  fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace('"hits":', '"hits": '));
  const r = run(box, ['--arms', 'rg', ...small, '--no-quiet-check']);
  check('a changed query set is refused with exit 3', r.code === 3 && /FROZEN SET CHANGED/.test(r.out), `exit ${r.code}: ${r.out.slice(0, 200)}`);
}
{
  const box = sandbox();
  fs.writeFileSync(path.join(box.root, 'quiet-reference.json'), JSON.stringify({ median: 0.001, detail: 'a reference no real run can match' }));
  const r = run(box, ['--arms', 'rg', ...small]);
  check('a busier box is not graded, exit 4', r.code === 4 && /NOT GRADED/.test(r.out), `exit ${r.code}: ${r.out.slice(0, 200)}`);
}
{
  const box = sandbox();
  const r = run(box, ['--arms', 'rg,crash', ...small, '--no-quiet-check']);
  const dir = (r.out.match(/run folder: (.*)/) || [])[1]?.trim();
  const rows = dir ? fs.readFileSync(path.join(dir, 'per-query.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l)) : [];
  const crashed = rows.map((row) => row.arms.crash);
  check('a crashed arm is a failure row scored zero', rows.length === 2 && crashed.every((a) => a && a.failure && a.score.primary === 0), `rows ${rows.length}: ${JSON.stringify(crashed).slice(0, 200)}`);
}
console.log(failures.length ? `${failures.length} harness test(s) failed` : 'every harness test passed');
process.exitCode = failures.length ? 1 : 0;
