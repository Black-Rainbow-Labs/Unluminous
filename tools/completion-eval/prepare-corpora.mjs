#!/usr/bin/env node
// Writes the pristine, pinned copy of every corpus the completion evaluation reads
// (`task-2231` §8.1), into D:/unluminous-completion-eval/corpora/<name>.
//
// A corpus from a local repository is `git archive` of a pinned commit, so what is measured is the
// commit and never somebody's working tree. A corpus cloned from outside is a shallow clone of a tag.
// A TypeScript corpus gets its `node_modules`, either as a directory junction to the repository's own
// (read only, so nothing is downloaded) or, for a clone, by `npm install`. A corpus already present is
// left alone; pass --force to write it again.
//
//   node tools/completion-eval/prepare-corpora.mjs [--only <name>] [--force]

import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const EVAL_ROOT = process.env.COMPLETION_EVAL_ROOT || 'D:/unluminous-completion-eval';
export const CORPORA_ROOT = path.join(EVAL_ROOT, 'corpora');

/** Reads corpora.json. */
export function readCorpora() {
  return JSON.parse(fs.readFileSync(path.join(HERE, 'corpora.json'), 'utf8')).corpora;
}

/**
 * Extracts `git archive <commit>[:tree]` of a repository into a folder.
 * @param repo - the repository
 * @param commit - the commit
 * @param tree - a folder inside it, or '' for the whole tree
 * @param dest - the folder to write
 */
function archive(repo, commit, tree, dest) {
  fs.mkdirSync(dest, { recursive: true });
  const spec = tree ? `${commit}:${tree}` : commit;
  const tar = path.join(EVAL_ROOT, `archive-${process.pid}.tar`);
  execFileSync('git', ['-C', repo, 'archive', '--format=tar', '-o', tar, spec], { stdio: 'inherit' });
  // Windows' own tar: Git for Windows' GNU tar reads the `D:` of a path as a remote host.
  const tarProgram =
    process.platform === 'win32' ? path.join(process.env.SystemRoot || 'C:/Windows', 'System32', 'tar.exe') : 'tar';
  execFileSync(tarProgram, ['-xf', tar, '-C', dest], { stdio: 'inherit' });
  fs.rmSync(tar);
}

/**
 * Links a folder into the corpus as `node_modules`, with a directory junction on Windows.
 * @param target - the real node_modules
 * @param dest - the corpus root
 */
function link(target, dest) {
  const at = path.join(dest, 'node_modules');
  if (fs.existsSync(at)) return;
  fs.symlinkSync(target, at, 'junction');
}

/**
 * Writes one corpus.
 * @param name - its name
 * @param spec - its entry in corpora.json
 * @param force - whether to write it again when present
 */
function prepare(name, spec, force) {
  const dest = path.join(CORPORA_ROOT, name);
  if (fs.existsSync(dest) && !force) {
    console.log(`${name}: present`);
    return;
  }
  if (fs.existsSync(dest)) fs.rmSync(dest, { recursive: true, force: true });
  if (spec.clone) {
    execFileSync('git', ['clone', '--depth', '1', '--branch', spec.tag, spec.clone, dest], { stdio: 'inherit' });
    spec.commit = execFileSync('git', ['-C', dest, 'rev-parse', 'HEAD']).toString().trim();
    fs.rmSync(path.join(dest, '.git'), { recursive: true, force: true });
    if (spec.install) {
      const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
      const done = spawnSync(npm, ['install', '--ignore-scripts', '--no-audit', '--no-fund', '--yes', '--legacy-peer-deps'], {
        cwd: dest,
        stdio: 'inherit',
        shell: process.platform === 'win32',
      });
      if (done.status !== 0) throw new Error(`npm install failed in ${dest}`);
    }
  } else {
    archive(spec.repo, spec.commit, spec.tree || '', dest);
    if (spec.node_modules) link(spec.node_modules, dest);
  }
  // A copy with no `.git` has its `.gitignore` files read by nobody, because ripgrep's walk (and so the
  // code index's) only honours them inside a repository: zod's copy walked 3,813 files, most of them
  // `node_modules`. An empty repository is enough, and `node_modules` and `target` are excluded in it
  // as well, because a subtree such as `ai-service/ui` does not carry the `.gitignore` that names them.
  execFileSync('git', ['init', '-q', dest]);
  fs.appendFileSync(path.join(dest, '.git', 'info', 'exclude'), '\nnode_modules/\ntarget/\n');
  fs.writeFileSync(path.join(dest, '.completion-eval.json'), JSON.stringify({ name, ...spec }, null, 2));
  console.log(`${name}: written at ${spec.commit || spec.tag}`);
}

/** Runs from the command line. */
function main() {
  const args = process.argv.slice(2);
  const only = args.includes('--only') ? args[args.indexOf('--only') + 1] : null;
  const force = args.includes('--force');
  fs.mkdirSync(CORPORA_ROOT, { recursive: true });
  for (const [name, spec] of Object.entries(readCorpora())) {
    if (only && only !== name) continue;
    prepare(name, spec, force);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
