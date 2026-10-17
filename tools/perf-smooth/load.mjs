#!/usr/bin/env node
// Load the processors the way a build or the window suite does, for a smoothness measurement (`task-2288`).
//
//   node tools/perf-smooth/load.mjs --threads <n> [--seconds <s>] [--megabytes <per thread>]
//
// Each thread runs at normal priority and does what a compiler does to a processor: integer work over a
// buffer larger than the cache it runs next to, so it takes the core and evicts what was in the cache.
// It prints `ready <threads>` once every thread is running, and stops at the deadline, or with `--tied`
// when its standard input closes, so a harness that dies does not leave the machine loaded.
//
// It sets no affinity of its own. `pwsh tools/capped.ps1 node tools/perf-smooth/load.mjs --threads 19`
// gives it the window suite's mask, which is the fastest 19 processors, so every performance core is busy
// and only efficiency cores are free.

import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads';

if (isMainThread) {
  const args = process.argv.slice(2);
  /** Read a `--name value` argument, or the fallback. */
  const option = (name, fallback) => {
    const at = args.indexOf(`--${name}`);
    return at >= 0 ? Number(args[at + 1]) : fallback;
  };
  const threads = option('threads', 19);
  const seconds = option('seconds', 3600);
  const megabytes = option('megabytes', 8);
  let running = 0;
  const workers = [];
  for (let index = 0; index < threads; index++) {
    const worker = new Worker(new URL(import.meta.url), { workerData: { megabytes } });
    worker.on('message', () => {
      running += 1;
      if (running === threads) console.log(`ready ${threads}`);
    });
    workers.push(worker);
  }
  const stop = () => {
    for (const worker of workers) worker.terminate();
    process.exit(0);
  };
  setTimeout(stop, seconds * 1000);
  // Only when asked: a shell that starts this in the background hands it a closed standard input.
  if (args.includes('--tied')) {
    process.stdin.on('end', stop);
    process.stdin.on('close', stop);
    process.stdin.resume();
  }
} else {
  const words = new Uint32Array((workerData.megabytes * 1024 * 1024) / 4);
  for (let at = 0; at < words.length; at++) words[at] = at * 2654435761;
  parentPort.postMessage('running');
  let hash = 0;
  // A stride that is not a power of two walks the whole buffer without settling into a few cache sets.
  for (let at = 0; ; at = (at + 4099) % words.length) {
    hash = (hash ^ words[at]) * 16777619;
    words[at] = hash;
  }
}
