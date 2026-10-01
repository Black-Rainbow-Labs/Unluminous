// Copies the release CLI into the eval root, which is the copy every run uses (lib/index-arm.mjs).
//
// A running index host holds its executable open, and on Windows an open executable cannot be replaced,
// so a build while a run is in progress used to fail. The runs use this copy instead, and a build only
// ever replaces `target/release`. Staging records which commit the copy was built from.
//
//   node tools/search-eval/stage.mjs

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { REPO, evalRoot } from './lib/config.mjs';

const exe = process.platform === 'win32' ? 'unluminous-cli.exe' : 'unluminous-cli';
const from = process.argv[2] || path.join(REPO, 'target', 'release', exe);
const sha = execFileSync('git', ['-C', REPO, 'rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).trim();
const dir = path.join(evalRoot(), 'bin');
fs.mkdirSync(dir, { recursive: true });
const to = path.join(dir, `unluminous-cli-${sha}${process.platform === 'win32' ? '.exe' : ''}`);
fs.copyFileSync(from, to);
fs.writeFileSync(path.join(dir, 'current.txt'), to);
console.log(`staged ${to}`);
