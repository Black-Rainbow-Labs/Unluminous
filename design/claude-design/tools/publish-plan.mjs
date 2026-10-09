// Write the design system's index and the list of files to send, ready for one publish.
//
// The icons are uploads in the system's asset store rather than files, so their ids are kept in
// rux/specs/uploads-*.txt (one line each: <group>/<file> <bytes> <id>) as they come back from the
// upload, and the index names them from there. Everything else under rux/system/project is a file.
//
//   node tools/publish-plan.mjs [--created <RFC 3339>]
//
// Writes rux/system/project/design-system.json and rux/build/publish-files.json, the `files` map of
// the publish call.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const system = path.join(root, 'rux/system');
const project = path.join(system, 'project');
const now = new Date().toISOString().replace(/\.\d+Z$/, 'Z');
const createdAt = process.argv.includes('--created') ? process.argv[process.argv.indexOf('--created') + 1] : now;

const GROUPS = { Icons: 's', UnluminousIcons: 's', UnluminousClassic: 's' };
const assetGroups = {};
for (const group of Object.keys(GROUPS)) assetGroups[group] = { name: group, tile: GROUPS[group], order: [], files: {} };
const specs = path.join(root, 'rux/specs');
for (const file of fs.readdirSync(specs).filter((f) => /^uploads-\d+\.txt$/.test(f)).sort()) {
  for (const line of fs.readFileSync(path.join(specs, file), 'utf8').split('\n').filter(Boolean)) {
    const [where, size, blob] = line.trim().split(/\s+/);
    const [group, name] = where.split('/');
    assetGroups[group].order.push(name);
    assetGroups[group].files[name] = { name, blob, size: Number(size), type: 'image/svg+xml' };
  }
}
for (const group of Object.values(assetGroups)) group.order.sort();

const index = {
  v: 3,
  layout: 'files',
  createdOnFiles: { v: 1, at: createdAt },
  title: 'Black Rainbow Labs Rux',
  namespace: 'Rux',
  libraries: [
    { name: 'react', version: '18.3.1', global: 'React', file: 'components/lib/react.production.min.js' },
    { name: 'react-dom', version: '18.3.1', global: 'ReactDOM', file: 'components/lib/react-dom.production.min.js' },
  ],
  sections: {},
  groups: Object.keys(GROUPS),
  assetGroups,
  blobs: {},
  docs: { readme: 'project/README.md', sections: [] },
  lastChange: {
    by: 'Jason',
    at: now,
    via: 'Claude Code',
    note: 'rux at 50b693f ported to React and CSS, with the Unluminous retones and window components',
  },
};
fs.writeFileSync(path.join(project, 'design-system.json'), JSON.stringify(index, null, 1) + '\n');

/**
 * Every file under a folder, as paths relative to it.
 * @param dir - the folder
 */
function walk(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(full).map((p) => `${entry.name}/${p}`) : [entry.name];
  });
}
const files = {};
for (const rel of walk(project)) {
  if (rel === 'design-system.json') continue;
  if (rel.startsWith('assets/') && rel.endsWith('.svg')) continue;
  files[`project/${rel}`] = `project/${rel}`;
}
fs.mkdirSync(path.join(root, 'rux/build'), { recursive: true });
fs.writeFileSync(path.join(root, 'rux/build/publish-files.json'), JSON.stringify(files, null, 1) + '\n');
const counts = Object.values(assetGroups).map((g) => `${g.name} ${g.order.length}`).join(', ');
console.log(`index written (${counts}); ${Object.keys(files).length} files to send`);
