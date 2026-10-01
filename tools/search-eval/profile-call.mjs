// Times the parts of one index call: the whole MCP round trip, and the search time the reply reports,
// over many repetitions of one query, so a speed lever can be judged before a full run.
//
//   node tools/search-eval/profile-call.mjs <corpus folder> <query> [reps] [budget]

import { indexArm } from './lib/index-arm.mjs';
import { median } from './lib/stats.mjs';

const [dir, pattern, repsText = '50', budgetText = '0'] = process.argv.slice(2);
const arm = indexArm('index-mcp');
const query = { family: 'F2', pattern, fixed: false, word: false, ignoreCase: false, path: '', globs: [], types: [] };
await arm.time(query, dir);
const total = [], inside = [];
for (let i = 0; i < Number(repsText); i++) {
  const t = await arm.time(query, dir);
  total.push(t.ms);
  inside.push(t.answer.work || {});
}
console.log(JSON.stringify({ query: pattern, hits: (await arm.time(query, dir)).answer.hits.length, medianMs: median(total), searchMs: median(inside.map((w) => (w.micros || 0) / 1000)), gateMs: median(inside.map((w) => (w.gateMicros || 0) / 1000)), candidates: inside[0].candidates }));
await arm.close();
