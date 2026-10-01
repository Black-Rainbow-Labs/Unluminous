// Builds the scorecard of a tool level run (TDD section 7.7): the per family table first, then the
// speed goal with its interval, both arms' absolutes beside every ratio.

import { GOALS } from './config.mjs';
import { bootstrap, describeRatio, geomean, mean, median, verdict } from './stats.mjs';

/**
 * Groups rows by a key.
 * @param rows - the rows
 * @param key - a function from a row to its group
 */
function groupBy(rows, key) {
  const out = new Map();
  for (const r of rows) { const k = key(r); if (!out.has(k)) out.set(k, []); out.get(k).push(r); }
  return out;
}

/**
 * Summarises one family on one corpus for every arm: how many queries, the primary metric, the median
 * time, the mean tokens shown, and failures.
 * @param rows - per query rows for that family and corpus
 * @param arms - the arm names
 */
export function familySummary(rows, arms) {
  const out = { n: rows.length };
  for (const arm of arms) {
    const mine = rows.map((r) => r.arms[arm]).filter(Boolean);
    out[arm] = {
      primary: mean(mine.map((a) => a.score?.primary ?? 0)),
      medianMs: median(mine.filter((a) => a.ms).map((a) => a.ms)),
      tokens: mean(mine.map((a) => a.tokens || 0)),
      failures: mine.filter((a) => a.failure).length,
    };
  }
  return out;
}

/**
 * The speed ratio of the index arm against the rg arm on F1 to F4: per query rg time over index
 * time, geometric mean, bootstrap interval.
 * @param rows - per query rows
 * @param indexArm - the index arm's name
 */
export function speed(rows, indexArm) {
  const pairs = rows.filter((r) => ['F1', 'F2', 'F3', 'F4'].includes(r.family) && r.arms.rg?.ms && r.arms[indexArm]?.ms && !r.arms[indexArm].failure);
  const ratios = pairs.map((r) => r.arms.rg.ms / r.arms[indexArm].ms);
  if (!ratios.length) return null;
  const interval = bootstrap(ratios, geomean);
  return { n: ratios.length, ...interval, verdict: verdict(interval), pass: interval.lo >= GOALS.speed, rgMedianMs: median(pairs.map((r) => r.arms.rg.ms)), indexMedianMs: median(pairs.map((r) => r.arms[indexArm].ms)) };
}

/**
 * Writes the scorecard markdown for a run.
 * @param rows - per query rows
 * @param arms - the arm names
 * @param manifest - the run's manifest
 * @param weights - the frozen family weights
 */
export function scorecardMarkdown(rows, arms, manifest, weights) {
  const lines = [`# Scorecard: ${manifest.label || manifest.runId}`, '', `Run \`${manifest.runId}\`, split \`${manifest.split}\`, unluminous \`${manifest.unluminousSha}\`, ${manifest.at}.`, ''];
  lines.push('## Per family', '', `| Corpus | Family | n | ${arms.map((a) => `${a} metric | ${a} median ms | ${a} tokens`).join(' | ')} |`, `|---|---|---|${arms.map(() => '---|---|---|').join('')}`);
  const byCorpus = groupBy(rows, (r) => `${r.repo}|${r.family}`);
  for (const key of [...byCorpus.keys()].sort()) {
    const [repo, family] = key.split('|');
    const s = familySummary(byCorpus.get(key), arms);
    lines.push(`| ${repo} | ${family} | ${s.n} | ${arms.map((a) => `${s[a].primary.toFixed(3)}${s[a].failures ? ` (${s[a].failures} failed)` : ''} | ${s[a].medianMs?.toFixed(1) ?? '-'} | ${Math.round(s[a].tokens)}`).join(' | ')} |`);
  }
  const indexArm = arms.find((a) => a !== 'rg');
  if (indexArm) {
    lines.push('', '## G1 speed (F1 to F4)', '', '| Corpus | n | rg median ms | index median ms | geomean ratio | 95% interval | reads as | verdict | pass |', '|---|---|---|---|---|---|---|---|---|');
    for (const [repo, mine] of groupBy(rows, (r) => r.repo)) {
      const s = speed(mine, indexArm);
      if (s) lines.push(`| ${repo} | ${s.n} | ${s.rgMedianMs.toFixed(1)} | ${s.indexMedianMs.toFixed(2)} | ${s.point.toFixed(2)}x | ${s.lo.toFixed(2)}x to ${s.hi.toFixed(2)}x | ${describeRatio(s.point)} | ${s.verdict} | ${s.pass ? 'yes' : 'no'} |`);
    }
  }
  lines.push('', `Family weights (frozen): ${Object.entries(weights).map(([k, v]) => `${k} ${v}`).join(', ')}.`);
  return lines.join('\n') + '\n';
}
