// Read rux's icon set out of its Rust source and write it as data and as SVG files.
//
// rux keeps the 37 `SbIcon` marks in `crates/rux/src/icon/paths.rs` as verbatim path data plus typed
// circles and rects. This parses that file, so the web port draws exactly the marks the Rust draws,
// and a mark changed in Rust is one re-run away from the design system.
//
//   node tools/extract-rux-icons.mjs <path to paths.rs>
//
// Writes rux/src/icons.data.json (what the Icon component reads) and
// rux/system/project/assets/Icons/<name>.svg (what the design system shows as assets).

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const source = process.argv[2];
if (!source) {
  console.error('usage: node tools/extract-rux-icons.mjs <path to rux/crates/rux/src/icon/paths.rs>');
  process.exit(2);
}
const text = fs.readFileSync(source, 'utf8');

/**
 * The SbIcon name for each Rust variant, read from the `Icon::X => "name"` arms of `Icon::name`.
 * @param src - the paths.rs source
 */
function namesOf(src) {
  const names = {};
  for (const m of src.matchAll(/Icon::(\w+) => "(\w+)"/g)) names[m[1]] = m[2];
  return names;
}

/**
 * Split the `parts` match into one source string per variant.
 * @param src - the paths.rs source
 */
function armsOf(src) {
  const start = src.indexOf('pub fn parts(self)');
  const body = src.slice(start);
  const arms = {};
  const re = /Icon::(\w+) => const \{/g;
  let m;
  const hits = [];
  while ((m = re.exec(body))) hits.push({ name: m[1], at: m.index + m[0].length });
  hits.forEach((hit, i) => {
    const end = i + 1 < hits.length ? hits[i + 1].at : body.indexOf('pub const DEFAULT_STROKE');
    arms[hit.name] = body.slice(hit.at, end);
  });
  return arms;
}

/**
 * Turn one arm's Rust constructors into plain part records.
 * @param arm - the text of one `const { &[ … ] }` block
 */
function partsOf(arm) {
  const parts = [];
  const re = /(Part::solid\(Shape::Path\("([^"]+)"\)\))|(\bp\("([^"]+)"\))|(\b(dot|circle)\(([^)]+)\))|(\b(solid_rect|rect)\(([^)]+)\))/g;
  let m;
  while ((m = re.exec(arm))) {
    if (m[1]) parts.push({ kind: 'path', d: m[2], fill: true, stroke: false });
    else if (m[3]) parts.push({ kind: 'path', d: m[4], fill: false, stroke: true });
    else if (m[5]) {
      const [cx, cy, r] = m[7].split(',').map(Number);
      parts.push({ kind: 'circle', cx, cy, r, fill: m[6] === 'dot', stroke: true });
    } else if (m[8]) {
      const [x, y, w, h, rx] = m[10].split(',').map(Number);
      const solid = m[9] === 'solid_rect';
      parts.push({ kind: 'rect', x, y, w, h, rx, fill: solid, stroke: !solid });
    }
  }
  return parts;
}

/**
 * Write one icon as a standalone SVG in `ink`, the way SbIcon draws it.
 * @param parts - the icon's part records
 * @param ink - the colour to draw in, since an <img> cannot inherit currentColor
 */
function svgOf(parts, ink) {
  const children = parts.map((p) => {
    const paint = `${p.fill ? ` fill="${ink}"` : ''}${p.stroke ? '' : ' stroke="none"'}`;
    if (p.kind === 'path') return `<path d="${p.d}"${paint}/>`;
    if (p.kind === 'circle') return `<circle cx="${p.cx}" cy="${p.cy}" r="${p.r}"${paint}/>`;
    return `<rect x="${p.x}" y="${p.y}" width="${p.w}" height="${p.h}" rx="${p.rx}"${paint}/>`;
  });
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="${ink}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${children.join('')}</svg>\n`;
}

const names = namesOf(text);
const arms = armsOf(text);
const icons = {};
for (const [variant, arm] of Object.entries(arms)) icons[names[variant]] = partsOf(arm);
const count = Object.keys(icons).length;
if (count !== 37) throw new Error(`expected 37 icons, read ${count}`);
for (const [name, parts] of Object.entries(icons)) {
  if (parts.length === 0) throw new Error(`${name} has no parts`);
}

fs.writeFileSync(path.join(root, 'rux/src/icons.data.json'), JSON.stringify(icons, null, 1) + '\n');
const out = path.join(root, 'rux/system/project/assets/Icons');
fs.mkdirSync(out, { recursive: true });
// Ink-900 of the light theme: the darkest ink, which reads on every light ground the assets view shows.
for (const [name, parts] of Object.entries(icons)) fs.writeFileSync(path.join(out, `${name}.svg`), svgOf(parts, '#1E2530'));
console.log(`wrote ${count} icons`);
