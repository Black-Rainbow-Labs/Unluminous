// Runs Claude Code headless for the parts of the evaluation a model has to do: writing the F6 and F8
// questions, the agent level runs of TDD section 7.5, and the grader.
//
// Every call goes through `usageAllows` first, which reads Claude usage only through ai-service's
// `claudeUsageCache.cjs` (never the endpoint directly) and refuses to start a session above 80% of
// the weekly limit. Every call runs with no settings, no MCP servers but the ones named, and the
// tool list it is given, so nothing on this machine leaks into a session that the other arm does not
// also have (R11).

import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { CLAUDE_EXE } from './rg.mjs';

const require = createRequire(import.meta.url);
const USAGE = 'C:/jason/dev/ai-service/backend/claudeUsageCache.cjs';
export const WEEKLY_CEILING = 80;

/**
 * Whether a new session may start: the weekly utilisation is under the ceiling. When it is not, the
 * caller waits and asks again; it never reads the endpoint itself.
 */
export async function usageAllows() {
  const { getUsage } = require(USAGE);
  const reading = await getUsage();
  const weekly = reading.usage?.seven_day?.utilization;
  if (typeof weekly !== 'number') return { ok: false, weekly: null, why: reading.error || 'no usage reading' };
  return { ok: weekly < WEEKLY_CEILING, weekly, why: weekly < WEEKLY_CEILING ? null : `weekly usage ${weekly}% is at or over ${WEEKLY_CEILING}%` };
}

/**
 * Waits until usage allows another session, checking every ten minutes.
 * @param log - where to say that it is waiting
 */
export async function waitForUsage(log = console.log) {
  for (;;) {
    const allowed = await usageAllows();
    if (allowed.ok) return allowed;
    log(`waiting: ${allowed.why}`);
    await new Promise((r) => setTimeout(r, 600_000));
  }
}

/**
 * Runs one headless Claude Code session and returns its final result, its usage and its events.
 * @param options - `{ prompt, model, cwd, tools, systemPrompt, mcpConfig, timeoutMs, extraArgs }`
 */
export async function runClaude(options) {
  await waitForUsage();
  const args = ['-p', '--output-format', 'stream-json', '--verbose', '--model', options.model,
    '--setting-sources', '', '--strict-mcp-config', '--mcp-config', JSON.stringify(options.mcpConfig || { mcpServers: {} }),
    '--tools', options.tools ?? '', '--no-session-persistence', '--permission-mode', 'bypassPermissions'];
  if (options.allowedTools) args.push('--allowedTools', options.allowedTools);
  if (options.systemPrompt) args.push('--system-prompt', options.systemPrompt);
  args.push(...(options.extraArgs || []));
  return new Promise((resolve) => {
    const started = Date.now();
    const child = spawn(CLAUDE_EXE, args, { cwd: options.cwd, windowsHide: true, env: { ...process.env, CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1' } });
    let out = '', err = '';
    const timer = setTimeout(() => child.kill(), options.timeoutMs || 900_000);
    child.stdout.on('data', (c) => { out += c; });
    child.stderr.on('data', (c) => { err += c; });
    child.stdin.end(options.prompt);
    child.on('close', (status) => {
      clearTimeout(timer);
      const events = out.split('\n').filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
      const result = events.find((e) => e.type === 'result');
      resolve({ status, ms: Date.now() - started, events, result, text: result?.result ?? '', usage: totalUsage(events), stderr: err.slice(0, 2000) });
    });
  });
}

/**
 * Sums the tokens a session processed, counting cache reads and writes as input (TDD section 2.1 G2),
 * from each distinct assistant message's usage block.
 * @param events - the stream-json events of one session
 */
export function totalUsage(events) {
  const seen = new Set();
  const total = { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, turns: 0, firstInput: null };
  for (const e of events) {
    if (e.type !== 'assistant' || !e.message?.usage || seen.has(e.message.id)) continue;
    seen.add(e.message.id);
    const u = e.message.usage;
    const input = (u.input_tokens || 0) + (u.cache_read_input_tokens || 0) + (u.cache_creation_input_tokens || 0);
    if (total.firstInput === null) total.firstInput = input;
    total.input += u.input_tokens || 0;
    total.cacheRead += u.cache_read_input_tokens || 0;
    total.cacheWrite += u.cache_creation_input_tokens || 0;
    total.output += u.output_tokens || 0;
    total.turns++;
  }
  total.all = total.input + total.cacheRead + total.cacheWrite + total.output;
  return total;
}

/**
 * Pulls the last JSON object out of a model's answer, for answers that end with a JSON block.
 * @param text - the final message
 */
export function lastJson(text) {
  const fenced = [...text.matchAll(/```(?:json)?\s*([\s\S]*?)```/g)].map((m) => m[1]);
  const candidates = fenced.length ? fenced.reverse() : [text.slice(text.lastIndexOf('{', text.lastIndexOf('}')))];
  for (const c of candidates) {
    const start = c.indexOf('{');
    const end = c.lastIndexOf('}');
    if (start < 0 || end < start) continue;
    try { return JSON.parse(c.slice(start, end + 1)); } catch { /* try the next */ }
  }
  const start = text.indexOf('{');
  try { return JSON.parse(text.slice(start, text.lastIndexOf('}') + 1)); } catch { return null; }
}
