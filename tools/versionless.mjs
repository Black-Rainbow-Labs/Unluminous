// The two files a release changes, read with the release's version number taken out.
//
// `task-2240`. `tools/release.ps1` commits a new version into `[workspace.package]` in `Cargo.toml`
// and into each workspace crate's entry in `Cargo.lock`, and nothing else changes in that commit. A
// tool that asks "has anything a build reads changed since the suite last passed" would otherwise
// answer yes after every release, for every crate and every window test binary, when the only change
// is the number a release is made of. `tools/test.mjs` and `tools/window-suite.mjs` both read the
// files through this, so the two cannot disagree about what a version-only change is.

/**
 * The text of `Cargo.toml` or `Cargo.lock` with the workspace's version replaced by `x`, and line
 * endings made line feeds so a worktree's carriage returns read the same as the main checkout.
 * Any other file comes back with only its line endings changed.
 * @param file - the path, relative to the repository, with forward slashes
 * @param text - the file's contents
 * @param members - the names of the workspace crates, whose `Cargo.lock` entries carry the version
 */
export function versionless(file, text, members) {
  const lines = text.replaceAll('\r\n', '\n');
  if (file === 'Cargo.toml') {
    return lines.replace(/^(\s*version\s*=\s*")[0-9]+\.[0-9]+\.[0-9]+(")/m, '$1x$2');
  }
  if (file === 'Cargo.lock') {
    return lines
      .split('\n[[package]]\n')
      .map((block) => {
        const name = /^name = "([^"]+)"/m.exec(block)?.[1];
        const local = !/^source = /m.test(block);
        return local && members.has(name) ? block.replace(/^version = ".*"$/m, 'version = "x"') : block;
      })
      .join('\n[[package]]\n');
  }
  return lines;
}

/**
 * The names of the workspace crates, read from a `Cargo.lock`: every package with no `source` line
 * is a path dependency, which in this repository means a workspace crate.
 * @param lock - the contents of `Cargo.lock`
 */
export function workspaceMembersInLock(lock) {
  const names = new Set();
  for (const block of lock.replaceAll('\r\n', '\n').split('\n[[package]]\n')) {
    const name = /^name = "([^"]+)"/m.exec(block)?.[1];
    if (name && !/^source = /m.test(block)) names.add(name);
  }
  return names;
}
