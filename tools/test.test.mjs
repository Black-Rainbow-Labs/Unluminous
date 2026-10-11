// Checks for what `tools/test.mjs` decides, run with `node --test tools/test.test.mjs` (task-2240).
//
// What the release skips rests on two rules: a version bump is not a change, and a crate's inputs
// are its own folder, its workspace dependencies' folders, and for `unluminous-app` everything but
// the folders no build reads. Each test here fails if one of those rules is broken.

import assert from 'node:assert/strict';
import test from 'node:test';
import { closureOf, isInput, unitHash } from './test.mjs';
import { versionless, workspaceMembersInLock } from './versionless.mjs';

const LOCK = [
  'version = 4',
  '',
  '[[package]]',
  'name = "serde"',
  'version = "1.0.200"',
  'source = "registry+https://github.com/rust-lang/crates.io-index"',
  '',
  '[[package]]',
  'name = "unluminous-core"',
  'version = "0.73.0"',
  '',
].join('\n');

test('a release version bump reads the same as the version before it', () => {
  const members = workspaceMembersInLock(LOCK);
  assert.deepEqual([...members], ['unluminous-core']);
  const bumped = LOCK.replace('version = "0.73.0"', 'version = "0.74.0"');
  assert.equal(versionless('Cargo.lock', bumped, members), versionless('Cargo.lock', LOCK, members));
  const manifest = '[workspace.package]\nversion = "0.73.0"\nedition = "2021"\n';
  assert.equal(versionless('Cargo.toml', manifest.replace('0.73.0', '0.74.0'), members), versionless('Cargo.toml', manifest, members));
});

test('a dependency upgrade in Cargo.lock is still a change', () => {
  const members = workspaceMembersInLock(LOCK);
  const upgraded = LOCK.replace('version = "1.0.200"', 'version = "1.0.201"');
  assert.notEqual(versionless('Cargo.lock', upgraded, members), versionless('Cargo.lock', LOCK, members));
});

test('carriage returns read the same as line feeds', () => {
  const members = workspaceMembersInLock(LOCK);
  assert.equal(versionless('Cargo.lock', LOCK.replaceAll('\n', '\r\n'), members), versionless('Cargo.lock', LOCK, members));
});

const crates = [
  { name: 'unluminous-core', folder: 'crates/unluminous-core/', dependsOn: [] },
  { name: 'unluminous-lsp', folder: 'crates/unluminous-lsp/', dependsOn: ['unluminous-core'] },
  { name: 'unluminous-git', folder: 'crates/unluminous-git/', dependsOn: [] },
  { name: 'unluminous-app', folder: 'crates/unluminous-app/', dependsOn: ['unluminous-lsp', 'unluminous-git'] },
];
const byName = new Map(crates.map((crate) => [crate.name, crate]));

test('a crate reads its own folder and every workspace crate it reaches', () => {
  const folders = closureOf(byName.get('unluminous-lsp'), byName).map((crate) => crate.folder).sort();
  assert.deepEqual(folders, ['crates/unluminous-core/', 'crates/unluminous-lsp/']);
  assert.ok(isInput('crates/unluminous-core/src/lib.rs', folders, false));
  assert.ok(!isInput('crates/unluminous-git/src/lib.rs', folders, false));
});

test('unluminous-app reads everything except the folders no build reads', () => {
  assert.ok(isInput('sample-diagrams/flow.mmd', [], true));
  assert.ok(isInput('documentation/editing.md', [], true));
  assert.ok(isInput('installer/icon/unluminous.ico', [], true));
  assert.ok(isInput('a-folder-added-later/file.txt', [], true));
  assert.ok(!isInput('tasks/task-2240-release-speed-tdd.md', [], true));
  assert.ok(!isInput('CHANGELOG.md', [], true));
  assert.ok(!isInput('tools/release.ps1', [], true));
});

test('a change to unluminous-core changes the hash of every crate that reads it and no other', () => {
  const blobs = new Map([
    ['crates/unluminous-core/src/lib.rs', 'a'],
    ['crates/unluminous-lsp/src/lib.rs', 'b'],
    ['crates/unluminous-git/src/lib.rs', 'c'],
    ['crates/unluminous-app/src/main.rs', 'd'],
  ]);
  const hashes = (map) =>
    Object.fromEntries(
      crates.map((crate) => {
        const unit = { kind: 'test', crate: crate.name, closure: closureOf(crate, byName), command: [], environment: {} };
        return [crate.name, unitHash(unit, map, 'shared').hash];
      }),
    );
  const before = hashes(blobs);
  const after = hashes(new Map(blobs).set('crates/unluminous-core/src/lib.rs', 'e'));
  assert.notEqual(after['unluminous-core'], before['unluminous-core']);
  assert.notEqual(after['unluminous-lsp'], before['unluminous-lsp']);
  assert.notEqual(after['unluminous-app'], before['unluminous-app']);
  assert.equal(after['unluminous-git'], before['unluminous-git']);
});
