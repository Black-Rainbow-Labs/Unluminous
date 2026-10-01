// The index arms of TDD section 7.4: `index-mcp`, a `tools/call` over stdio to `unluminous-cli mcp serve
// --areas search`, which hosts the index in its own process the way it does for an agent; and
// `index-cli`, `unluminous-cli search ...` started once per call, reported beside it and not gated.
//
// One MCP server is started per corpus, with its working folder the snapshot, and it is not timed until
// `search status` says the index is ready: G1 is a warm cache measurement, and the cold build is the
// cost budget of section 2.2, reported separately.

import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { REPO, evalRoot } from './config.mjs';

/**
 * The CLI the runs use: `SEARCH_EVAL_CLI`, or the copy `stage.mjs` last put in the eval root, or the
 * release build when nothing has been staged.
 */
function stagedCli() {
  if (process.env.SEARCH_EVAL_CLI) return process.env.SEARCH_EVAL_CLI;
  const current = path.join(evalRoot(), 'bin', 'current.txt');
  if (fs.existsSync(current)) return fs.readFileSync(current, 'utf8').trim();
  return path.join(REPO, 'target', 'release', process.platform === 'win32' ? 'unluminous-cli.exe' : 'unluminous-cli');
}
export const CLI = stagedCli();
const CACHE = () => path.join(evalRoot(), 'index-cache');

/**
 * One MCP server over stdio, with requests matched to responses by id.
 */
class McpServer {
  /**
   * Starts the server in a corpus folder.
   * @param dir - the corpus snapshot
   */
  constructor(dir) {
    this.dir = dir;
    this.child = spawn(CLI, ['mcp', 'serve', '--areas', 'search'], { cwd: dir, windowsHide: true, env: { ...process.env, UNLUMINOUS_INDEX_CACHE: CACHE(), CLAUDE_PROJECT_DIR: dir } });
    this.pending = new Map();
    this.nextId = 1;
    let buffer = '';
    this.child.stdout.on('data', (chunk) => {
      buffer += chunk;
      let at;
      while ((at = buffer.indexOf('\n')) >= 0) {
        const line = buffer.slice(0, at);
        buffer = buffer.slice(at + 1);
        if (!line.trim()) continue;
        const message = JSON.parse(line);
        const waiting = this.pending.get(message.id);
        if (waiting) { this.pending.delete(message.id); waiting(message); }
      }
    });
    this.child.stderr.on('data', () => {});
  }

  /**
   * Sends one request and resolves with its response and the time from writing it to reading the answer.
   * @param method - the JSON-RPC method
   * @param params - its parameters
   */
  request(method, params) {
    const id = this.nextId++;
    return new Promise((resolve) => {
      const started = process.hrtime.bigint();
      this.pending.set(id, (message) => resolve({ message, ms: Number(process.hrtime.bigint() - started) / 1e6 }));
      this.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
    });
  }

  /**
   * Calls the search tool with a verb and its arguments.
   * @param verb - the search verb
   * @param args - the verb's arguments
   */
  call(verb, args) {
    return this.request('tools/call', { name: 'unluminous_search', arguments: { command: verb, arguments: args } });
  }

  close() {
    this.child.kill();
  }
}

/**
 * The tool call that answers a query of a family in the index arm.
 * @param query - a frozen query
 */
export function indexCallFor(query) {
  if (query.family === 'F2') {
    const pattern = query.fixed ? query.pattern.replace(/[\\^$.|?*+()[\]{}]/g, '\\$&') : query.pattern;
    return ['find', { query: query.word ? `\\b(?:${pattern})\\b` : pattern, mode: 'regex', path: query.path || '', glob: (query.globs || []).join(' '), type: (query.types || [])[0] || '', 'ignore-case': !!query.ignoreCase, budget: 0 }];
  }
  // F1 asks where a name is defined and F3 asks for every use: the two verbs that answer exactly that.
  if (query.family === 'F1') return ['def', { name: query.name, limit: 10 }];
  if (query.family === 'F3') return ['refs', { name: query.name, budget: 0 }];
  if (query.family === 'F4') return ['files', { query: query.pattern, path: query.path || '', limit: 100 }];
  throw new Error(`no index call for ${query.family}`);
}

/**
 * Turns a tool result into an answer.
 * @param query - the frozen query
 * @param message - the JSON-RPC response
 */
export function indexAnswer(query, message) {
  const result = message.result;
  if (!result || result.isError) throw new Error(result?.content?.[0]?.text || JSON.stringify(message.error || message).slice(0, 300));
  const value = result.structuredContent || {};
  const text = result.content?.map((c) => c.text || '').join('\n') || '';
  if (query.family === 'F4') return { hits: [], files: value.files || [], empty: !(value.files || []).length, text };
  if (query.family === 'F1') {
    const defs = (value.definitions || []).map((d) => ({ path: d.path, line: d.line, text: d.signature }));
    return { hits: defs, files: [...new Set(defs.map((h) => h.path))], empty: defs.length === 0, text };
  }
  const hits = (value.hits || []).map(([p, line, t]) => ({ path: p, line, text: t }));
  return { hits, files: [...new Set(hits.map((h) => h.path))], empty: hits.length === 0, text, index: value.index, work: value.work };
}

/**
 * Starts an MCP server in a folder and waits until its index is ready, and its passage table too when
 * asked; a warm up search follows so the first timed call is not the first call.
 * @param dir - the corpus folder
 * @param passages - whether to wait for the passage table as well
 */
export async function openServer(dir, passages = false) {
  const server = new McpServer(dir);
  await server.request('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'search-eval', version: '1' } });
  for (let i = 0; i < 4800; i++) {
    const { message } = await server.call('status', {});
    const st = message.result?.structuredContent;
    // A question in plain English is answered from the vectors once they exist, so a host that can
    // embed is asked only after every chunk has one: an agent working in a checkout meets it warm.
    if (st?.ready && (!passages || (st.passagesReady && (!st.canEmbed || st.vectors >= st.passages)))) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  await server.call('find', { query: 'warm', budget: 1 });
  return server;
}

/**
 * An index arm by name: `index-mcp` or `index-cli`.
 * @param name - the arm's name
 */
export function indexArm(name) {
  const servers = new Map();
  const serverFor = async (dir) => {
    if (!servers.has(dir)) servers.set(dir, await openServer(dir));
    return servers.get(dir);
  };
  if (name === 'index-cli') {
    return {
      name,
      async time(query, dir) {
        const [verb, args] = indexCallFor(query);
        const words = ['search', verb, String(args.query), '--json', '--root', dir];
        for (const [k, v] of Object.entries(args)) if (k !== 'query' && v !== '' && v !== false) words.push(`--${k}`, ...(v === true ? [] : [String(v)]));
        const started = process.hrtime.bigint();
        const r = spawnSync(CLI, words, { cwd: dir, encoding: 'utf8', maxBuffer: 1 << 28, windowsHide: true, env: { ...process.env, UNLUMINOUS_INDEX_CACHE: CACHE() } });
        const ms = Number(process.hrtime.bigint() - started) / 1e6;
        const reply = JSON.parse(r.stdout || '{}');
        return { ms, answer: indexAnswer(query, { result: { structuredContent: reply.result, content: [{ text: reply.result?.text || '' }], isError: !reply.ok } }) };
      },
      async order(query, dir) { return (await this.time(query, dir)).answer; },
    };
  }
  return {
    name,
    async time(query, dir) {
      const server = await serverFor(dir);
      const [verb, args] = indexCallFor(query);
      const { message, ms } = await server.call(verb, { ...args, structured: true });
      return { ms, answer: indexAnswer(query, message) };
    },
    async order(query, dir) { return (await this.time(query, dir)).answer; },
    async close() { for (const s of servers.values()) s.close(); },
  };
}
