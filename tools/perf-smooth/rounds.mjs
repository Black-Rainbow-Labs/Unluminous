#!/usr/bin/env node
// Run the smoothness bench over several builds, interleaved, so the machine's drift lands on all of them.
//
//   node tools/perf-smooth/rounds.mjs --flow <hill climb folder> --reps 3 --loads idle,cap19,over36 \
//        <variant>=<folder with unluminous.exe>[@NAME=value,NAME=value] [<variant>=<folder> ...]
//
// Each pass runs every load once for every build, in the same order, before the next pass starts, and
// each run appends to <flow>/<variant>/results.jsonl. A build is never measured twice in a row, and no two
// benches ever run at once. `score.mjs` reads what this writes.

import { spawnSync } from 'node:child_process';
import path from 'node:path';

const args = process.argv.slice(2);
/** Read a `--name value` argument, or the fallback. */
function option(name, fallback) {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
}
const flow = path.resolve(option('flow', '_agent_output/perf-smooth/hillclimb'));
const reps = Number(option('reps', '3'));
const firstRep = Number(option('first-rep', '0'));
const loads = option('loads', 'idle,cap19,over36').split(',');
const cases = option('cases', '');
const presentmon = option('presentmon', '');
const builds = args.filter((word) => word.includes('=') && !word.startsWith('--')).map((word) => {
  const [variant, ...rest] = word.split('=');
  const [folder, settings = ''] = rest.join('=').split('@');
  return { variant, folder: path.resolve(folder), env: settings ? settings.split(',') : [] };
});
const here = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));

for (let rep = firstRep; rep < firstRep + reps; rep++) {
  for (const load of loads) {
    for (const build of builds) {
      const words = [path.join(here, 'bench.mjs'), '--bin', build.folder, '--out', path.join(flow, build.variant), '--rep', String(rep), '--load', load];
      if (cases) words.push('--cases', cases);
      for (const pair of build.env) words.push('--env', pair);
      if (presentmon) words.push('--presentmon', presentmon);
      console.log(`== rep ${rep} ${load} ${build.variant}`);
      const run = spawnSync('node', words, { stdio: 'inherit' });
      if (run.status !== 0) console.log(`!! ${build.variant} ${load} rep ${rep} exited ${run.status}`);
    }
  }
}
