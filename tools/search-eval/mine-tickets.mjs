// Mines the F5 (localisation) candidates: Tasks board tickets whose commits are in one of the
// evaluation repositories. The question is the ticket's title and description; the gold is the set of
// source files the ticket's commits changed; the corpus is checked out at the parent of the ticket's
// first commit, so the answer is not in the tree being searched.
//
// Docs, lock files and generated snapshots are not gold, and a ticket whose commits changed more than
// 25 files is dropped (TDD section 7.2).

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { CORPORA, evalRoot } from './lib/config.mjs';

const API = process.env.TASKS_API || 'http://localhost:8091/tasks';
const TOKEN = process.env.CLAUDE_SKIP_TOKEN;
const MAX_FILES = 25;

/**
 * Whether a changed path counts as gold: source code, not docs, lock files, pictures or test snapshots.
 * @param file - a path relative to the repository
 */
export function isGoldPath(file) {
  if (/\.(md|txt|png|jpe?g|gif|svg|ico|lock|snap|webp|mp4|wav|json)$/i.test(file)) return false;
  if (/(^|\/)(package-lock\.json|Cargo\.lock|CHANGELOG)/i.test(file)) return false;
  if (/(^|\/)(docs?|tasks|_agent_output|snapshots|releases)\//i.test(file)) return false;
  return true;
}

/**
 * Lists the ticket commits reachable from a corpus's pinned commit, grouped by ticket key, oldest first.
 * @param corpus - one entry of CORPORA
 */
function ticketCommits(corpus) {
  const log = execFileSync('git', ['-C', corpus.source, 'log', '--no-merges', '--reverse', '--format=%H%x09%P%x09%s', corpus.sha], { encoding: 'utf8', maxBuffer: 1 << 28 });
  const byTicket = new Map();
  for (const line of log.split('\n')) {
    const [sha, parents, subject] = line.split('\t');
    const key = subject?.match(/^(task-\d+)\b/)?.[1];
    if (!key) continue;
    if (!byTicket.has(key)) byTicket.set(key, { key, parent: parents.split(' ')[0], commits: [] });
    byTicket.get(key).commits.push(sha);
  }
  return byTicket;
}

/**
 * The files a ticket's commits changed, without deletions.
 * @param repo - the repository
 * @param commits - the ticket's commits
 */
function changedFiles(repo, commits) {
  const files = new Set();
  for (const sha of commits) {
    const out = execFileSync('git', ['-C', repo, 'show', '--no-renames', '--diff-filter=AM', '--name-only', '--format=', sha], { encoding: 'utf8' });
    for (const f of out.split('\n')) if (f.trim()) files.add(f.trim());
  }
  return [...files];
}

/**
 * Reads one ticket's title and description from the board.
 * @param key - the ticket key
 */
async function readTicket(key) {
  const res = await fetch(`${API}/by-key/${key}`, { headers: { 'x-skip-token': TOKEN } });
  if (!res.ok) return null;
  const t = await res.json();
  return { title: t.title || '', description: t.description || '', project: t.agentProjectId || null };
}

const out = [];
for (const corpus of CORPORA.filter((c) => !c.source.startsWith('http'))) {
  let kept = 0, dropped = 0;
  for (const ticket of ticketCommits(corpus).values()) {
    const all = changedFiles(corpus.source, ticket.commits);
    const gold = all.filter(isGoldPath);
    if (!gold.length || all.length > MAX_FILES) { dropped++; continue; }
    const text = await readTicket(ticket.key);
    if (!text || text.description.length < 80) { dropped++; continue; }
    out.push({ repo: corpus.name, key: ticket.key, parent: ticket.parent, commits: ticket.commits, title: text.title, description: text.description, gold });
    kept++;
  }
  console.log(`${corpus.name}: kept ${kept}, dropped ${dropped}`);
}
const dir = path.join(evalRoot(), 'mined');
fs.mkdirSync(dir, { recursive: true });
fs.writeFileSync(path.join(dir, 'tickets.jsonl'), out.map((t) => JSON.stringify(t)).join('\n') + '\n');
