#!/usr/bin/env node
// Every relative link and every path named in the written documentation resolves to something that
// is really there.
//
//   node tools/documentation/check-links.mjs
//
// `task-1994` split a 1,106 line README into a front door and thirteen pages, which is a great many
// new relative links and a great many sentences naming a file by its path. A link that goes nowhere
// is the kind of fault nobody finds by reading, because a reader who does not click it cannot tell —
// so this is a script rather than a reading.
//
// Two things it checks, and it is deliberately strict about the first and lenient about the second:
//
//   * **Every markdown link whose target is not a URL** is resolved against the file it is in, and
//     has to exist. An anchor is checked too, against the headings of the page it points into.
//   * **Every backticked path** that looks like one — it has a slash and an extension this
//     repository uses — has to resolve against the repository root **or against one of the crate
//     roots**, because the house style names a file by its path inside the crate it lives in:
//     `app/cli.rs` and `examples/frame_cost.rs` are how those two are written everywhere. A path
//     inside a fenced block is skipped, because a fence is very often an example rather than a
//     claim.
//
// It exits non-zero on the first kind of fault and prints every one of them, because the person
// reading the output has just moved a file and wants the whole list.

import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, dirname, resolve, relative, extname, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

/** The files this walks: the front door, the documentation folder, and the pages beside them. */
const WALKED = ['README.md', 'CONTRIBUTING.md', 'AGENTS.md', 'documentation', 'design'];

/** Not walked: a build, the accepted pictures, and the scratch a task left behind. */
const SKIPPED = new Set(['target', 'node_modules', '_agent_output', 'snapshots', '.git']);

/**
 * Where a named path may be relative to. The repository root first, then every crate's source and
 * its own folder, because a sentence in a design document says `app/cli.rs` rather than
 * `crates/unluminous-app/src/app/cli.rs` and should go on being allowed to.
 */
function roots() {
  const found = [root];
  const crates = join(root, 'crates');
  if (existsSync(crates)) {
    for (const crate of readdirSync(crates)) {
      found.push(join(crates, crate), join(crates, crate, 'src'));
    }
  }
  found.push(join(root, 'unluminous-cli'), join(root, 'unluminous-cli', 'src'));
  found.push(join(root, 'design'), join(root, 'documentation'), join(root, 'tools'));
  return found;
}

/** The extensions a backticked word has to end in before it is treated as a path. */
const PATHY = new Set([
  '.rs', '.md', '.toml', '.conf', '.txt', '.json', '.ps1', '.sh', '.mjs', '.js', '.ts',
  '.png', '.jpg', '.css', '.html', '.mmd', '.sql', '.py', '.yaml', '.yml', '.plist',
]);

/**
 * Every markdown file under a path this walks.
 * @param at - a file or a folder, absolute
 */
function markdownUnder(at) {
  if (!existsSync(at)) return [];
  if (statSync(at).isFile()) return at.endsWith('.md') ? [at] : [];
  return readdirSync(at).flatMap((entry) =>
    SKIPPED.has(entry) ? [] : markdownUnder(join(at, entry)),
  );
}

/**
 * The anchors a markdown page offers, in the form a link writes them.
 * @param text - the whole page
 */
function anchorsOf(text) {
  const anchors = new Set();
  for (const line of text.split('\n')) {
    const heading = /^#{1,6}\s+(.*?)\s*$/.exec(line);
    if (!heading) continue;
    anchors.add(
      heading[1]
        .toLowerCase()
        .replace(/`/g, '')
        .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
        .replace(/[^\w\s-]/g, '')
        .trim()
        .replace(/\s+/g, '-'),
    );
  }
  return anchors;
}

/**
 * The page's text with every fenced block blanked out, so an example in a fence is not read as a
 * claim about a file that exists.
 * @param text - the whole page
 */
function withoutFences(text) {
  let fenced = false;
  return text
    .split('\n')
    .map((line) => {
      if (/^\s*```/.test(line)) {
        fenced = !fenced;
        return '';
      }
      return fenced ? '' : line;
    })
    .join('\n');
}

const pages = WALKED.flatMap((entry) => markdownUnder(join(root, entry)));
const anchors = new Map();
for (const page of pages) anchors.set(page, anchorsOf(readFileSync(page, 'utf8')));

const mayBeUnder = roots();
const faults = [];
let links = 0;
let paths = 0;

for (const page of pages) {
  const raw = readFileSync(page, 'utf8');
  const prose = withoutFences(raw);
  const where = relative(root, page).split(sep).join('/');

  for (const match of prose.matchAll(/\[[^\]]*\]\(([^)\s]+)\)/g)) {
    const target = match[1];
    if (/^(https?|mailto):/.test(target) || target.startsWith('#')) continue;
    links += 1;
    const [path, anchor] = target.split('#');
    const at = resolve(dirname(page), decodeURIComponent(path));
    if (!existsSync(at)) {
      faults.push(`${where}: link to ${target} — nothing at ${relative(root, at)}`);
      continue;
    }
    if (anchor && at.endsWith('.md')) {
      if (!anchors.has(at)) anchors.set(at, anchorsOf(readFileSync(at, 'utf8')));
      if (!anchors.get(at).has(anchor)) {
        faults.push(`${where}: link to ${target} — that page has no heading called that`);
      }
    }
  }

  for (const match of prose.matchAll(/`([^`\n]+)`/g)) {
    const word = match[1].trim();
    if (!word.includes('/') || word.includes(' ') || word.startsWith('http')) continue;
    if (word.includes('*') || word.includes('<') || word.includes('..')) continue;
    // `~/.claude.json` is a file in somebody's home folder and `react-icons@5.5.0/ci/index.mjs` is
    // inside a package. Neither is a claim about this checkout.
    if (word.startsWith('~') || word.startsWith('.') || word.includes('@')) continue;
    if (!PATHY.has(extname(word))) continue;
    paths += 1;
    if (!mayBeUnder.some((from) => existsSync(join(from, word)))) {
      faults.push(`${where}: names \`${word}\`, which is not in the checkout`);
    }
  }
}

console.log(`${pages.length} pages, ${links} relative links, ${paths} named paths`);
if (faults.length === 0) {
  console.log('every one of them resolves');
  process.exit(0);
}
console.log(`\n${faults.length} do not:`);
for (const fault of faults) console.log(`  ${fault}`);
process.exit(1);
