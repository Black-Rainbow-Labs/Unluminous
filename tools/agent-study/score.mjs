// Prints the status table of an eval flow: one row a variant, the numbers recomputed from every
// variant's `results.jsonl`, never read from a summary.
//
//   node tools/agent-study/score.mjs <flow>
//   node tools/agent-study/score.mjs <flow> --cases baseline,v1      # which cases moved between two
//
// `success` and `clean` are rates over the (case, rep) rows of each split; the counts are means a
// case. The 95% interval on a rate is the normal approximation, which is what tells a real change
// from a few cases flipping.
import fs from 'node:fs';
import path from 'node:path';

const flow = path.resolve(process.argv[2] ?? die('the flow folder is required'));
const compare = process.argv.includes('--cases') ? process.argv[process.argv.indexOf('--cases') + 1].split(',') : null;

const variants = fs.readdirSync(flow)
  .filter(d => /^(baseline|v\d+)$/.test(d) && fs.existsSync(path.join(flow, d, 'results.jsonl')))
  .sort((a, b) => (a === 'baseline' ? -1 : b === 'baseline' ? 1 : Number(a.slice(1)) - Number(b.slice(1))));

/** Every row of a variant, the last row of each (case, rep) winning, so a re-run case replaces itself. */
export function rows(variant) {
  const keyed = new Map();
  for (const line of fs.readFileSync(path.join(flow, variant, 'results.jsonl'), 'utf8').split('\n').filter(Boolean)) {
    const r = JSON.parse(line);
    keyed.set(`${r.prompt_id}#${r.rep}`, r);
  }
  return [...keyed.values()];
}

const mean = xs => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : NaN);
const rate = xs => {
  const p = mean(xs);
  const half = 1.96 * Math.sqrt((p * (1 - p)) / Math.max(1, xs.length));
  return `${(p * 100).toFixed(0)}% ±${(half * 100).toFixed(0)}`;
};

if (compare) {
  const [a, b] = compare.map(v => new Map(rows(v).map(r => [r.prompt_id, r])));
  for (const [id, ra] of a) {
    const rb = b.get(id);
    if (!rb) continue;
    const moved = ra.grade.success !== rb.grade.success || ra.grade.clean !== rb.grade.clean;
    if (moved || ra.grade.refused !== rb.grade.refused) {
      console.log(`${id.padEnd(28)} ${ra.split ?? ''}  success ${ra.grade.success}->${rb.grade.success}  clean ${ra.grade.clean}->${rb.grade.clean}  refused ${ra.grade.refused}->${rb.grade.refused}  own ${ra.grade.own}->${rb.grade.own}`);
    }
  }
  process.exit(0);
}

/**
 * What a case's saved trace says beyond its grade: whether the first Unluminous call was accepted,
 * and how many of the agent's own calls read the project and how many wrote to it. A read behind the
 * window is a missed chance; a write behind it is the fault the product exists to prevent, so the two
 * are counted apart.
 */
export function traced(variant, row) {
  const file = path.join(flow, variant, 'traces', `${row.prompt_id}_rep${row.rep}.json`);
  if (!fs.existsSync(file)) return { firstRight: null, ownReads: 0, ownWrites: 0 };
  const turns = JSON.parse(fs.readFileSync(file, 'utf8'));
  let firstRight = null, ownReads = 0, ownWrites = 0;
  for (let i = 0; i < turns.length; i++) {
    const t = turns[i];
    if (t.role !== 'tool_call') continue;
    const answer = String(turns[i + 1]?.content ?? '');
    if (/unluminous/i.test(t.name ?? '')) {
      if (firstRight === null) firstRight = answer.startsWith('REFUSED') ? 0 : 1;
      continue;
    }
    if (/^(todowrite|todoread|skill|question|invalid)$/.test(t.name ?? '')) continue;
    const input = String(t.content ?? '');
    const writes = /^(write|edit|patch|multiedit|apply_patch)$/i.test(t.name ?? '')
      || (/^bash$/i.test(t.name ?? '') && /(>|sed -i|mv |cp |rm |mkdir|touch|tee |Set-Content|Out-File|New-Item)/.test(input));
    if (writes) ownWrites += 1; else ownReads += 1;
  }
  return { firstRight, ownReads, ownWrites };
}

console.log('| variant | n | success (test) | success (train) | clean (all) | first call right | refused/case | discovery/case | own reads/case | own writes/case | calls/case | s/case | in tok/case |');
console.log('|---|---|---|---|---|---|---|---|---|---|---|---|---|');
for (const v of variants) {
  const all = rows(v);
  const extra = all.map(r => traced(v, r));
  const firstRight = extra.filter(e => e.firstRight !== null).map(e => e.firstRight);
  const test = all.filter(r => r.split === 'test');
  const train = all.filter(r => r.split !== 'test');
  const g = k => mean(all.map(r => r.grade[k]));
  const desc = fs.existsSync(path.join(flow, v, 'change.md'))
    ? fs.readFileSync(path.join(flow, v, 'change.md'), 'utf8').split('\n').find(l => l.trim() && !l.startsWith('#')) ?? ''
    : '';
  console.log(`| ${v} | ${all.length} | ${rate(test.map(r => r.grade.success))} | ${rate(train.map(r => r.grade.success))} | ${rate(all.map(r => r.grade.clean))} | ${rate(firstRight)} | ${g('refused').toFixed(2)} | ${g('discovery').toFixed(2)} | ${mean(extra.map(e => e.ownReads)).toFixed(2)} | ${mean(extra.map(e => e.ownWrites)).toFixed(2)} | ${g('calls').toFixed(1)} | ${mean(all.map(r => r.latency_s)).toFixed(0)} | ${Math.round(mean(all.map(r => r.usage?.input_tokens ?? 0)))} |${desc ? ` ${desc.slice(0, 70)}` : ''}`);
}

function die(m) { console.error(m); process.exit(2); }
