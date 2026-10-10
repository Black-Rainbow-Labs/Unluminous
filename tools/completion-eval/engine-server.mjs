#!/usr/bin/env node
/**
 * The language server alone: rust-analyzer for the Rust corpora and tsserver for the TypeScript ones,
 * each asked through `unluminous-lsp`'s `ask` example, which is the same client the window uses with
 * no editor around it. `task-2231` §8.1 names it as the third reference engine, so the scorecard can
 * say how much of Unluminous's number is the server's and how much is the merge.
 *
 * Usage:
 *   node engine-server.mjs --run <name> --ask <path to ask.exe> [--split tune|held|all] [--corpus <name>] [--limit <n>]
 *
 * Writes `D:/unluminous-completion-eval/runs/<name>/results.jsonl` in the contract the README states,
 * and resumes a run that stopped part way, as the Unluminous engine does. The control corpora (Python
 * and Go) have no server and are left out.
 */
import { spawn } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline';
import { fileURLToPath } from 'node:url';

import { copyCorpus, queryText, typedName } from './engine-unluminous.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const EVAL_ROOT = 'D:/unluminous-completion-eval';
/** The adapter each language's server is asked through. */
const ADAPTERS = { rust: 'lsp', typescript: 'tsserver' };
/** How long one question may take before it is recorded as a miss. */
const ANSWER_WITHIN = 30000;

/** The command line, as a map of `--name value` and a set of switches. */
function readArguments(argv) {
  const options = {};
  for (let i = 0; i < argv.length; i++) {
    if (!argv[i].startsWith('--')) continue;
    const name = argv[i].slice(2);
    const next = argv[i + 1];
    if (next === undefined || next.startsWith('--')) options[name] = true;
    else options[name] = argv[++i];
  }
  return options;
}

/**
 * A running `ask` process, answering one JSON line with one JSON line.
 */
class Asker {
  /**
   * Starts `ask` for a project and waits for nothing: it reads standard input only once its server is
   * ready, so the first question simply waits longer.
   * @param exe - the `ask` program
   * @param adapter - lsp or tsserver
   * @param root - the project
   */
  constructor(exe, adapter, root) {
    this.child = spawn(exe, ['--adapter', adapter, '--root', root], { stdio: ['pipe', 'pipe', 'pipe'] });
    this.waiting = new Map();
    this.child.stderr.setEncoding('utf8');
    this.child.stderr.on('data', (chunk) => process.stderr.write(`  [ask] ${chunk}`));
    readline.createInterface({ input: this.child.stdout }).on('line', (line) => {
      let reply;
      try {
        reply = JSON.parse(line);
      } catch {
        return;
      }
      const done = this.waiting.get(reply.id);
      if (done) {
        this.waiting.delete(reply.id);
        done(reply);
      }
    });
    this.exited = new Promise((resolve) => this.child.on('exit', resolve));
  }

  /**
   * Asks one question.
   * @param query - id, path, text and offset
   * @param within - how long to wait, in milliseconds
   */
  ask(query, within) {
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.waiting.delete(query.id);
        resolve({ id: query.id, labels: [], error: 'no answer in time' });
      }, within);
      this.waiting.set(query.id, (reply) => {
        clearTimeout(timer);
        resolve(reply);
      });
      this.child.stdin.write(`${JSON.stringify(query)}\n`);
    });
  }

  /** Ends the process by closing its input, which is how `ask` is told to stop its server. */
  async stop() {
    this.child.stdin.end();
    const timeout = new Promise((resolve) => setTimeout(resolve, 15000));
    await Promise.race([this.exited, timeout]);
    if (this.child.exitCode === null) this.child.kill();
  }
}

/**
 * Reads the results already written, so a resumed run asks only what is missing.
 * @param resultsPath - the run's results file
 */
function alreadyAsked(resultsPath) {
  const done = new Set();
  if (!fs.existsSync(resultsPath)) return done;
  for (const line of fs.readFileSync(resultsPath, 'utf8').split('\n').filter(Boolean)) {
    const row = JSON.parse(line);
    done.add(`${row.id}#${row.prefix}`);
  }
  return done;
}

/**
 * Asks every chosen position of one corpus, at each prefix length, and appends the answers.
 * @param asker - the running `ask`
 * @param copy - the run's copy of the corpus
 * @param chosen - the positions
 * @param done - what was already asked
 * @param resultsPath - where answers go
 */
async function askTheCorpus(asker, copy, chosen, done, resultsPath) {
  let asked = 0;
  let file = null;
  let original = null;
  for (const position of chosen) {
    const letters = [...position.expected];
    for (const p of [0, 1, 2, 3]) {
      if (p > letters.length || done.has(`${position.id}#${p}`)) continue;
      if (file !== position.path) {
        file = position.path;
        original = Buffer.from(fs.readFileSync(path.join(copy, file)).toString('utf8').replaceAll('\r\n', '\n'), 'utf8');
      }
      const { text, caret } = queryText(original, position, letters.slice(0, p).join(''));
      const id = `${position.id}#${p}`;
      const reply = await asker.ask({ id, prefix: p, path: path.join(copy, file).replaceAll('\\', '/'), text, offset: caret }, ANSWER_WITHIN);
      const row = { id: position.id, prefix: p, labels: [...new Set((reply.labels || []).map(typedName))].slice(0, 50), ms: reply.ms ?? null };
      if (reply.error) row.error = reply.error;
      fs.appendFileSync(resultsPath, `${JSON.stringify(row)}\n`);
      asked += 1;
    }
  }
  return asked;
}

async function main() {
  const options = readArguments(process.argv.slice(2));
  if (!options.run || !options.ask) {
    console.error('usage: engine-server.mjs --run <name> --ask <ask.exe> [--split tune|held|all] [--corpus <name>] [--limit <n>]');
    process.exit(2);
  }
  const positionsPath = options.positions || path.join(HERE, 'positions.json');
  const positionsBytes = fs.readFileSync(positionsPath);
  const frozen = JSON.parse(positionsBytes.toString('utf8'));
  const split = options.split || 'tune';
  const runFolder = path.join(EVAL_ROOT, 'runs', options.run);
  fs.mkdirSync(runFolder, { recursive: true });
  const resultsPath = path.join(runFolder, 'results.jsonl');
  const done = alreadyAsked(resultsPath);
  const runPath = path.join(runFolder, 'run.json');
  const previous = fs.existsSync(runPath) ? JSON.parse(fs.readFileSync(runPath, 'utf8')) : null;
  const run = {
    engine: 'server',
    ask: options.ask,
    split,
    positions: positionsPath,
    positionsSha256: crypto.createHash('sha256').update(positionsBytes).digest('hex'),
    started: previous?.started || new Date().toISOString(),
    corpora: previous?.corpora || {},
  };
  for (const [name, spec] of Object.entries(frozen.corpora)) {
    if (options.corpus && options.corpus !== name) continue;
    const adapter = ADAPTERS[spec.language];
    if (!adapter || spec.control) continue;
    let chosen = frozen.positions.filter((p) => p.corpus === name && (split === 'all' || p.split === split));
    if (options.limit) chosen = chosen.slice(0, Number(options.limit));
    if (!chosen.length) continue;
    const copy = copyCorpus(name, path.join(runFolder, 'corpora', name));
    console.log(`${name}: ${adapter} on ${copy}`);
    const asker = new Asker(options.ask, adapter, copy);
    const began = Date.now();
    try {
      const asked = await askTheCorpus(asker, copy, chosen, done, resultsPath);
      run.corpora[name] = { adapter, queries: (run.corpora[name]?.queries || 0) + asked, seconds: Math.round((Date.now() - began) / 1000) };
    } finally {
      await asker.stop();
    }
    run.ended = new Date().toISOString();
    fs.writeFileSync(runPath, JSON.stringify(run, null, 2));
  }
  run.ended = new Date().toISOString();
  fs.writeFileSync(runPath, JSON.stringify(run, null, 2));
}

main();
