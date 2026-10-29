#!/usr/bin/env node
// Copy the skills catalogue unluminous.com publishes into the binary (task-2396).
//
// `sites/unluminous/scripts/sync-skills.mjs` in black-rainbow-labs-sites publishes the catalogue and every file of
// every skill under `public/skills/`. This copies what the binary carries into `crates/unluminous-app/skills/`:
//
//   catalogue.json            the site's list, each entry marked `bundled: complete` or `bundled: partial`
//   <id>/...                  every file of a skill no larger than LIMIT altogether; for a larger one its
//                             SKILL.md and licence only, and the Skills Manager fetches the rest from the site
//                             the first time the skill is installed
//   images/<id>.jpg           the 480 wide card picture
//
// The skills Unluminous makes (`unluminous: true`) are written in this folder by hand and are left alone.
//
//   node tools/skills/snapshot.mjs [--site C:/jason/dev/black-rainbow-labs-sites/sites/unluminous]

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const LIMIT = 256 * 1024;
const here = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const at = process.argv.indexOf('--site');
const site = at > 0 ? process.argv[at + 1] : 'C:/jason/dev/black-rainbow-labs-sites/sites/unluminous';
const published = path.join(site, 'public', 'skills');
const bundle = path.join(here, 'crates', 'unluminous-app', 'skills');

const catalogue = JSON.parse(fs.readFileSync(path.join(published, 'catalogue.json'), 'utf8'));
fs.mkdirSync(path.join(bundle, 'images'), { recursive: true });
let bytes = 0;
for (const entry of catalogue.skills) {
  fs.copyFileSync(path.join(published, 'images', `${entry.id}.jpg`), path.join(bundle, 'images', `${entry.id}.jpg`));
  if (entry.unluminous) {
    entry.bundled = 'complete';
    continue;
  }
  const total = entry.files.reduce((sum, file) => sum + file.size, 0);
  const complete = total <= LIMIT;
  entry.bundled = complete ? 'complete' : 'partial';
  const folder = path.join(bundle, entry.id);
  fs.rmSync(folder, { recursive: true, force: true });
  for (const file of entry.files) {
    if (!complete && file.path !== 'SKILL.md' && !/^LICEN[CS]E/i.test(file.path)) continue;
    const target = path.join(folder, ...file.path.split('/'));
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(path.join(published, 'files', entry.id, ...file.path.split('/')), target);
    bytes += file.size;
  }
}
fs.writeFileSync(path.join(bundle, 'catalogue.json'), `${JSON.stringify(catalogue, null, 2)}\n`);
const partial = catalogue.skills.filter((entry) => entry.bundled === 'partial').map((entry) => entry.id);
console.log(`${catalogue.skills.length} skills, ${(bytes / 1024).toFixed(0)} KB of files bundled; partial: ${partial.join(', ')}`);
