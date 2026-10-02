// Compares the code index with other ways of searching code on this machine, on the frozen held out
// queries: ripgrep as Claude Code runs it (the reference), the index over MCP, `git grep`, ugrep, and
// ugrep with its own index. Each query is timed through every tool in turn, in an order that rotates
// from one query to the next, and each tool's lines are compared with ripgrep's.
//
//   node tools/search-eval/compare-tools.mjs [--corpora a,b] [--reps 3] [--max N]
//
// F2 is the held out grep patterns agents really ran. F3's held out names are searched as whole words,
// which is the one question every tool can be asked. Zoekt does not build on Windows and is measured
// in WSL by compare-zoekt.py, against ripgrep in the same place.

import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, HERE, evalRoot, snapshotDir } from './lib/config.mjs';
import { CLI, indexArm } from './lib/index-arm.mjs';
import { machine, pinToPerformanceCores } from './lib/machine.mjs';
import { exactSet, grepArgs, grepPattern, readJsonMatches, runRgTimed } from './lib/rg.mjs';
import { bootstrap, geomean, median } from './lib/stats.mjs';

const UGREP_DIR = 'C:/Users/jason/AppData/Local/Microsoft/WinGet/Packages/Genivia.ugrep_Microsoft.Winget.Source_8wekyb3d8bbwe';
const UGREP = path.join(UGREP_DIR, 'ugrep.exe');
const UGREP_INDEXER = path.join(UGREP_DIR, 'ugrep-indexer.exe');
const VCS = ['.git', '.svn', '.hg', '.bzr', '.jj', '.sl'];

/**
 * Reads the command line.
 * @param argv - the arguments after the script
 */
function options(argv) {
  const get = (name, fallback) => { const i = argv.indexOf(`--${name}`); return i >= 0 ? argv[i + 1] : fallback; };
  return { corpora: get('corpora', CORPORA.map((c) => c.name).join(',')).split(','), reps: Number(get('reps', 3)), max: Number(get('max', 0)), tools: get('tools', ''), buildsFrom: get('builds-from', ''), inProcess: argv.includes('--in-process') };
}

/**
 * The held out queries every tool can answer: F2 as mined, and F3's names as whole word patterns.
 * @param corpus - the corpus name
 */
function queriesOf(corpus) {
  const read = (file) => fs.readFileSync(path.join(HERE, 'queries', file), 'utf8').trim().split('\n').map((l) => JSON.parse(l)).filter((q) => q.split === 'heldout' && q.repo === corpus);
  const exact = read('F2-exact.jsonl');
  const words = read('F3-references.jsonl').map((q) => ({ family: 'F3', repo: q.repo, id: q.id, pattern: q.name, fixed: true, word: true, ignoreCase: false, path: '', globs: [], types: [] }));
  return [...exact, ...words];
}

/**
 * Runs a program and times it, keeping its standard output.
 * @param program - the program
 * @param args - its arguments
 * @param cwd - the folder it runs in
 */
function timed(program, args, cwd) {
  return new Promise((resolve) => {
    const started = process.hrtime.bigint();
    const child = spawn(program, args, { cwd, windowsHide: true });
    const chunks = [];
    child.stdout.on('data', (c) => chunks.push(c));
    child.stderr.on('data', () => {});
    child.on('error', () => resolve({ ms: NaN, stdout: '', status: -1 }));
    child.on('close', (status) => resolve({ ms: Number(process.hrtime.bigint() - started) / 1e6, stdout: Buffer.concat(chunks).toString('utf8'), status }));
  });
}

/**
 * Reads `path:line` pairs out of output where each record is the path, a NUL and the line number.
 * @param stdout - the output
 */
function nulRecords(stdout) {
  const hits = [];
  for (const line of stdout.split('\n')) {
    const [file, number] = line.split('\0');
    if (file && /^\d+$/.test(number || '')) hits.push({ path: file.replace(/\\/g, '/').replace(/^\.\//, ''), line: Number(number) });
  }
  return hits;
}

/**
 * Reads `path:line` pairs out of ugrep's `%f:%n` records: the number after the last colon.
 * @param stdout - the output
 */
function colonRecords(stdout) {
  const hits = [];
  for (const line of stdout.split('\n')) {
    const m = line.replace(/\r$/, '').match(/^(.*):(\d+)$/);
    if (m) hits.push({ path: m[1].replace(/\\/g, '/').replace(/^\.\//, ''), line: Number(m[2]) });
  }
  return hits;
}

/**
 * git grep's pathspecs for a query's folder and globs, written as ripgrep reads them: a glob with no
 * slash matches a file name at any depth, and one starting with `!` excludes.
 * @param query - the query
 */
function gitPathspecs(query) {
  const base = query.path || '.';
  const globs = query.globs || [];
  const specs = [];
  const prefix = base === '.' ? '' : `${base.replace(/\/$/, '')}/`;
  for (const g of globs.filter((x) => !x.startsWith('!'))) specs.push(`:(glob)${prefix}${g.includes('/') ? g : `**/${g}`}`);
  if (!specs.length) specs.push(base);
  for (const g of globs.filter((x) => x.startsWith('!'))) specs.push(`:(glob,exclude)${prefix}${g.slice(1).includes('/') ? g.slice(1) : `**/${g.slice(1)}`}`);
  return specs;
}

/**
 * The tools, each a function from a query and the corpus folders to a timed answer.
 * @param index - the index arm
 */
function tools(index) {
  const ugrepArgs = (query, extra) => {
    // ugrep refuses -P together with --index, so the index mode is asked in ugrep's own syntax, which
    // reads \b, \w and alternation the way ripgrep does.
    const args = ['-r', '-n', '-I', '--hidden', '--ignore-files', ...(extra.includes('--index') ? [] : ['-P']), '--format=%u%f:%n%~', ...extra];
    for (const dir of VCS) args.push(`--exclude-dir=${dir}`);
    if (query.ignoreCase) args.push('-i');
    for (const g of query.globs || []) args.push('-g', g);
    args.push('-e', grepPattern(query), query.path || '.');
    return args;
  };
  return {
    rg: async (query, dirs) => { const r = await runRgTimed(grepArgs(query), dirs.snapshot); return { ms: r.ms, hits: readJsonMatches(r.stdout) }; },
    index: async (query, dirs) => {
      const { ms, answer } = await index.time({ ...query, family: 'F2' }, dirs.snapshot);
      return { ms, hits: answer.hits };
    },
    'git grep': async (query, dirs) => {
      // A snapshot unpacked from a tarball has an empty repository, so git grep is told to search the
      // folder rather than a history with no commits in it.
      const git = dirs.hasCommits;
      const args = ['grep', ...(git ? [] : ['--no-index']), '-n', '-I', '-z', '--no-color', '-P', ...(query.ignoreCase ? ['-i'] : []), '-e', grepPattern(query), '--', ...gitPathspecs(query)];
      const r = await timed('git', args, dirs.snapshot);
      return { ms: r.ms, hits: nulRecords(r.stdout) };
    },
    ugrep: async (query, dirs) => { const r = await timed(UGREP, ugrepArgs(query, []), dirs.copy); return { ms: r.ms, hits: colonRecords(r.stdout) }; },
    'ugrep --index': async (query, dirs) => { const r = await timed(UGREP, ugrepArgs(query, ['--index']), dirs.copy); return { ms: r.ms, hits: colonRecords(r.stdout) }; },
  };
}

/**
 * Builds the index each indexed tool needs and records what it cost: the time, and the size on disk.
 * @param corpus - the corpus name
 * @param dirs - the snapshot and the ugrep copy
 */
function buildIndexes(corpus, dirs) {
  const out = {};
  // The code index, from nothing: a cache folder of its own, emptied first, a host started and asked
  // until ready. Each corpus has a cache folder of its own, so no host of an earlier corpus holds it.
  const cache = path.join(evalRoot(), 'compare', 'index-cache', corpus);
  fs.rmSync(cache, { recursive: true, force: true });
  process.env.UNLUMINOUS_INDEX_CACHE = cache;
  const env = { ...process.env, UNLUMINOUS_INDEX_CACHE: cache, UNLUMINOUS_EMBED: 'off' };
  let started = Date.now();
  let status = null;
  for (;;) {
    const r = spawnSync(CLI, ['search', 'status', '--root', dirs.snapshot, '--json'], { encoding: 'utf8', env, windowsHide: true });
    try { status = JSON.parse(r.stdout).result; } catch { status = null; }
    if (status?.ready) break;
    spawnSync(process.execPath, ['-e', 'setTimeout(()=>{},200)']);
  }
  // Ready means the exact index answers; the passage table is then built in the background. Timing
  // starts once it is built, so the queries are not sharing the processor with that build.
  for (;;) {
    const r = spawnSync(CLI, ['search', 'status', '--root', dirs.snapshot, '--json'], { encoding: 'utf8', env, windowsHide: true });
    let st = null;
    try { st = JSON.parse(r.stdout).result; } catch { st = null; }
    if (st?.passagesReady) { status = { ...status, passages: st.passages }; break; }
    spawnSync(process.execPath, ['-e', 'setTimeout(()=>{},500)']);
  }
  // With --in-process the host that built the index is stopped, so the MCP server the index arm starts
  // loads the index from disk and holds it itself, which is what an agent's MCP server does when no
  // window or other process already hosts the checkout. Without it, every query goes through that
  // host over a loopback socket, which is what happens when one does.
  if (opts.inProcess && status.host?.pid) spawnSync('powershell', ['-NoProfile', '-Command', `Stop-Process -Id ${status.host.pid} -Force`], { windowsHide: true });
  out.index = { buildMs: status.loadMs, withPassagesMs: Date.now() - started, origin: status.origin, files: status.files, bytes: fs.statSync(status.indexFile).size, memoryBytes: status.memoryBytes };
  if (keepUgrep) {
    out['ugrep --index'] = keepUgrep[corpus];
    return out;
  }
  // ugrep's index, written into the copy's folders, after the indexer has taken any earlier one out.
  spawnSync(UGREP_INDEXER, ['-d', '-q'], { cwd: dirs.copy, windowsHide: true, maxBuffer: 1 << 26 });
  started = Date.now();
  spawnSync(UGREP_INDEXER, ['-I', '--ignore-files', '-.', '-q'], { cwd: dirs.copy, windowsHide: true, maxBuffer: 1 << 26 });
  let bytes = 0;
  const walk = (dir) => { for (const e of fs.readdirSync(dir, { withFileTypes: true })) { const p = path.join(dir, e.name); if (e.isDirectory()) walk(p); else if (e.name.startsWith('._UG#_Store')) bytes += fs.statSync(p).size; } };
  walk(dirs.copy);
  out['ugrep --index'] = { buildMs: Date.now() - started, bytes };
  console.log(`${corpus}: index ${out.index.buildMs} ms ${(out.index.bytes / 2 ** 20).toFixed(1)} MB, ugrep index ${out['ugrep --index'].buildMs} ms ${(bytes / 2 ** 20).toFixed(1)} MB`);
  return out;
}

const opts = options(process.argv.slice(2));
pinToPerformanceCores();
process.env.UNLUMINOUS_EMBED = 'off';
const index = indexArm('index-mcp');
const all = tools(index);
const runner = opts.tools ? Object.fromEntries(opts.tools.split(',').map((t) => [t, all[t]])) : all;
if (opts.inProcess && runner.index) { runner['index, in process'] = runner.index; delete runner.index; }
const names = Object.keys(runner);
const keepUgrep = opts.buildsFrom ? JSON.parse(fs.readFileSync(path.join(evalRoot(), 'runs', opts.buildsFrom, 'summary.json'), 'utf8')).builds : null;
if (keepUgrep) for (const c of Object.keys(keepUgrep)) keepUgrep[c] = keepUgrep[c]['ugrep --index'];
const rows = [];
const builds = {};
for (const name of opts.corpora) {
  const corpus = CORPORA.find((c) => c.name === name);
  const dirs = { snapshot: snapshotDir(corpus), copy: path.join(evalRoot(), 'compare', 'ugrep', path.basename(snapshotDir(corpus))) };
  dirs.hasCommits = spawnSync('git', ['-C', dirs.snapshot, 'rev-parse', '--verify', '-q', 'HEAD'], { windowsHide: true }).status === 0;
  builds[name] = buildIndexes(name, dirs);
  let queries = queriesOf(name);
  if (opts.max) queries = queries.slice(0, opts.max);
  for (const [n, query] of queries.entries()) {
    const order = names.map((_, i) => names[(i + n) % names.length]);
    const row = { id: query.id, family: query.family, repo: name, tools: {} };
    for (const tool of order) {
      const times = [];
      let hits = [];
      // A pattern a tool refuses, such as a \n that ripgrep and the index both reject without
      // multiline mode, is an empty answer with no time, so it counts against the tool's lines and not
      // its speed.
      for (let rep = 0; rep <= opts.reps; rep++) {
        let r;
        try { r = await runner[tool](query, dirs); } catch { r = { ms: NaN, hits: [] }; }
        if (rep > 0 && Number.isFinite(r.ms)) times.push(r.ms);
        hits = r.hits;
      }
      row.tools[tool] = { ms: times.length ? median(times) : NaN, set: exactSet(hits) };
    }
    const reference = row.tools.rg.set.join('\n');
    for (const tool of names) {
      row.tools[tool].same = row.tools[tool].set.join('\n') === reference;
      row.tools[tool].hits = row.tools[tool].set.length;
      delete row.tools[tool].set;
    }
    rows.push(row);
  }
  console.log(`${name}: ${queries.length} queries`);
}
await index.close();

const summary = [];
for (const name of opts.corpora) {
  const mine = rows.filter((r) => r.repo === name);
  for (const tool of names) {
    const buildKey = tool === 'index, in process' ? 'index' : tool;
    const timedRows = mine.filter((r) => Number.isFinite(r.tools[tool].ms));
    const ratios = mine.map((r) => r.tools.rg.ms / r.tools[tool].ms).filter((x) => Number.isFinite(x) && x > 0);
    const ci = bootstrap(ratios, geomean);
    summary.push({ corpus: name, tool, n: mine.length, medianMs: +median(timedRows.map((r) => r.tools[tool].ms)).toFixed(2), speedVsRg: +geomean(ratios).toFixed(2), lo: +ci.lo.toFixed(2), hi: +ci.hi.toFixed(2), sameAsRg: mine.filter((r) => r.tools[tool].same).length, build: builds[name][buildKey] || null });
  }
}
const out = path.join(evalRoot(), 'runs', `compare-${Math.floor(Date.now() / 1000)}`);
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, 'per-query.jsonl'), rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
fs.writeFileSync(path.join(out, 'summary.json'), JSON.stringify({ at: new Date().toISOString(), machine: machine(), reps: opts.reps, summary, builds }, null, 2));
for (const s of summary) console.log(`${s.corpus.padEnd(11)} ${s.tool.padEnd(14)} median ${String(s.medianMs).padStart(8)} ms  ${String(s.speedVsRg).padStart(7)}x (${s.lo} to ${s.hi})  same lines ${s.sameAsRg}/${s.n}`);
console.log(`run folder: ${out}`);
