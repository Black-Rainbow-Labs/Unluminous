/**
 * Publishes this repository's history to the public one, with the agent attribution taken out and
 * every commit under one spelling of Jason McAffee's name.
 *
 *     node tools/publish-open-source.mjs                 # rewrite, check, and say what it would push
 *     node tools/publish-open-source.mjs --push          # and push it
 *     node tools/publish-open-source.mjs --ref v0.49.2   # publish up to a tag rather than origin/main
 *
 * **What it is for.** `github.com/jasonmcaffee/unluminous` is private and is where releases are cut.
 * `github.com/Black-Rainbow-Labs/Unluminous` is the public source. The two hold the same code and the
 * same history; what differs is that 53 commits here carry `Co-Authored-By: Claude …` and
 * `Claude-Session: …` trailers, and the name on a commit is spelled three ways.
 *
 * **It is a pure function of the history, which is the property that matters.** A commit's new hash
 * depends only on its tree, its rewritten parents, its message and its author and committer — all of
 * which this reads from the commit it is rewriting — so running it again over the same history gives
 * the same hashes, and running it after a few more commits have landed leaves every published commit
 * exactly where it was and appends the new ones. **Publishing is therefore an ordinary push, never a
 * force push**, and none of the objects a force push leaves behind and readable are created.
 *
 * **Nothing dirty ever reaches GitHub.** The rewrite happens in a bare clone on this machine, the
 * checks below run against it, and the push happens only if they all pass and only with `--push`.
 *
 * It writes to no ref in this checkout and touches no file in it.
 */
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const REPOSITORY = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const PUBLIC_REMOTE = 'https://github.com/Black-Rainbow-Labs/Unluminous.git';

/** The one name and address every published commit is authored and committed under. */
const JASON = 'Jason McAffee <jasonlmcaffee@gmail.com>';

/**
 * The lines that come out of a message, each matched against a WHOLE line from its start.
 *
 * Anchoring matters here more than it usually would: this history discusses the Anthropic API,
 * `CLAUDE.md` and the `claude` program at length, and a substring match would eat sentences.
 */
const BANNED_LINE = [
  /^co-authored-by:.*(claude|anthropic)/i,
  /^claude-session:/i,
  /^\u{1F916}\s*generated with/iu,
  /^generated with \[?claude/i,
];

/** Runs a git command in a repository and returns its standard output. */
function git(where, ...args) {
  return execFileSync('git', ['-C', where, ...args], { encoding: 'utf8', maxBuffer: 1 << 28 });
}

/** Runs a git command in a repository, writing `input` to it, and returns its standard output. */
function gitWithInput(where, input, ...args) {
  return execFileSync('git', ['-C', where, ...args], { input, encoding: 'utf8', maxBuffer: 1 << 28 });
}

/**
 * Takes the agent attribution out of a message.
 *
 * A message with none of it in comes back UNTOUCHED rather than rebuilt from its own lines, so that
 * "only the trailers changed" is a claim a reader can check rather than take on trust.
 *
 * @param message - the commit or tag message, exactly as git stores it
 */
function cleaned(message) {
  const lines = message.split('\n');
  const kept = lines.filter((line) => !BANNED_LINE.some((pattern) => pattern.test(line.trim())));
  if (kept.length === lines.length) return message;
  return `${kept.join('\n').replace(/\n+$/, '')}\n`;
}

/**
 * Splits a raw commit object into its headers and its message.
 *
 * @param raw - what `git cat-file commit` printed
 */
function readCommit(raw) {
  const blank = raw.indexOf('\n\n');
  const headers = raw.slice(0, blank).split('\n');
  return { headers, message: raw.slice(blank + 2) };
}

/**
 * Rewrites one identity line — `author`, `committer` or `tagger` — keeping its timestamp exactly.
 *
 * The timestamp is everything after the closing angle bracket, and it is kept byte for byte: a
 * published history whose dates had moved would be a different history wearing the same messages.
 *
 * @param line - the header line, such as `author Jason <j@example.com> 1758000000 -0700`
 */
function underOneName(line) {
  const [keyword] = line.split(' ', 1);
  const when = line.slice(line.lastIndexOf('>') + 1);
  return `${keyword} ${JASON}${when}`;
}

/**
 * Rewrites every commit reachable from `head`, parents first, and returns the old-to-new map.
 *
 * The signature on a signed commit is dropped rather than kept: it was made over the bytes of the
 * commit being replaced, so carrying it forward would attach a signature that does not verify.
 *
 * @param where - the bare repository being rewritten
 * @param head - the commit to rewrite up to
 */
function rewriteHistory(where, head) {
  const order = git(where, 'rev-list', '--reverse', '--topo-order', head).trim().split('\n');
  const map = new Map();
  let messagesChanged = 0;

  for (const old of order) {
    const { headers, message } = readCommit(git(where, 'cat-file', 'commit', old));
    const rebuilt = withoutFoldedSignature(headers).map((header) => {
      if (header.startsWith('parent ')) return `parent ${map.get(header.slice(7))}`;
      if (header.startsWith('author ') || header.startsWith('committer ')) return underOneName(header);
      return header;
    });
    const message_ = cleaned(message);
    if (message_ !== message) messagesChanged += 1;
    const object = `${rebuilt.join('\n')}\n\n${message_}`;
    map.set(old, gitWithInput(where, object, 'hash-object', '-t', 'commit', '-w', '--stdin').trim());
  }
  return { map, order, messagesChanged };
}

/**
 * Drops the continuation lines of a multi-line header, which is what a signature is.
 *
 * `gpgsig` is followed by the rest of the armour indented by one space; those lines have to go with
 * the header they belong to, and nothing else in a commit object is folded that way.
 *
 * @param headers - the header lines of a raw commit or tag object
 */
function withoutFoldedSignature(headers) {
  const kept = [];
  let inside = false;
  for (const header of headers) {
    if (header.startsWith('gpgsig')) { inside = true; continue; }
    if (inside && header.startsWith(' ')) continue;
    inside = false;
    kept.push(header);
  }
  return kept;
}

/**
 * Points every tag at the rewritten commit it named, rebuilding an annotated tag's own object.
 *
 * A tag naming a commit that was not rewritten — one on another branch — is left out rather than
 * published, because the public repository holds one line of history and a tag off it would name a
 * commit nothing there can reach.
 *
 * @param where - the bare repository being rewritten
 * @param map - old commit to new commit
 */
function rewriteTags(where, map) {
  const rows = git(where, 'for-each-ref', '--format=%(refname)\t%(objecttype)\t%(objectname)', 'refs/tags')
    .trim().split('\n').filter(Boolean);
  const moved = [];

  for (const row of rows) {
    const [refname, kind, objectname] = row.split('\t');
    if (kind === 'commit') {
      if (!map.has(objectname)) continue;
      moved.push([refname, map.get(objectname)]);
      continue;
    }
    const { headers, message } = readCommit(git(where, 'cat-file', 'tag', objectname));
    const target = headers.find((header) => header.startsWith('object '))?.slice(7);
    if (!map.has(target)) continue;
    const rebuilt = withoutFoldedSignature(headers).map((header) => {
      if (header.startsWith('object ')) return `object ${map.get(target)}`;
      if (header.startsWith('tagger ')) return underOneName(header);
      return header;
    });
    const object = `${rebuilt.join('\n')}\n\n${cleaned(message)}`;
    moved.push([refname, gitWithInput(where, object, 'hash-object', '-t', 'tag', '-w', '--stdin').trim()]);
  }
  return moved;
}

/**
 * Checks the rewritten history against the one it came from and returns what it found.
 *
 * Three things are asserted rather than assumed: that not one file changed, that every commit is
 * Jason McAffee's, and that no message still carries a banned line. A publish that cannot say all
 * three is a publish that should not happen.
 *
 * @param where - the bare repository holding both histories
 * @param map - old commit to new commit
 * @param order - the old commits, parents first
 */
function check(where, map, order) {
  const problems = [];
  const head = map.get(order[order.length - 1]);

  // Two walks rather than two lookups per commit: `git log` prints the whole history in one process,
  // and a rewrite of a few hundred commits otherwise spends most of its time starting git.
  const trees = (of) => new Map(git(where, 'log', '--format=%H %T', of).trim().split('\n').map((row) => row.split(' ')));
  const before = trees(order[order.length - 1]);
  const after = trees(head);
  for (const old of order) {
    if (before.get(old) !== after.get(map.get(old))) problems.push(`${old.slice(0, 10)} the tree changed`);
  }

  const identities = [...new Set(git(where, 'log', '--format=%an <%ae>%n%cn <%ce>', head).trim().split('\n'))];
  for (const identity of identities) if (identity !== JASON) problems.push(`an identity survived: ${identity}`);

  const messages = git(where, 'log', '--format=%B', head).split('\n');
  for (const line of messages) {
    if (BANNED_LINE.some((pattern) => pattern.test(line.trim()))) problems.push(`a banned line survived: ${line}`);
  }

  return { problems, identities };
}

/** Reads the flags, with `--ref` and `--remote` taking a value and the rest being switches. */
function readTheArguments(argv) {
  const options = { ref: 'origin/main', remote: PUBLIC_REMOTE, push: false, work: '' };
  for (let at = 0; at < argv.length; at += 1) {
    if (argv[at] === '--push') options.push = true;
    else if (argv[at] === '--ref') options.ref = argv[++at];
    else if (argv[at] === '--remote') options.remote = argv[++at];
    else if (argv[at] === '--work') options.work = argv[++at];
    else throw new Error(`unknown argument ${argv[at]}`);
  }
  return options;
}

function main() {
  const options = readTheArguments(process.argv.slice(2));
  // `^{commit}` rather than the bare ref: an annotated tag names a tag object, and everything below
  // this line is about commits.
  const head = git(REPOSITORY, 'rev-parse', `${options.ref}^{commit}`).trim();
  const work = options.work || fs.mkdtempSync(path.join(os.tmpdir(), 'unluminous-publish-'));

  console.log(`source     ${REPOSITORY}`);
  console.log(`ref        ${options.ref} = ${head}`);
  console.log(`remote     ${options.remote}`);
  console.log(`work       ${work}\n`);

  const mirror = path.join(work, 'rewrite.git');
  if (!fs.existsSync(mirror)) {
    execFileSync('git', ['clone', '--mirror', REPOSITORY, mirror], { stdio: 'inherit' });
  }

  const { map, order, messagesChanged } = rewriteHistory(mirror, head);
  const moved = rewriteTags(mirror, map);
  const { problems, identities } = check(mirror, map, order);

  console.log(`commits    ${order.length} rewritten, ${messagesChanged} whose message changed`);
  console.log(`tags       ${moved.length} moved`);
  console.log(`identity   ${identities.join(' | ')}`);
  console.log(`head       ${head.slice(0, 10)} -> ${map.get(head).slice(0, 10)}`);
  console.log(`trees      ${problems.some((p) => p.includes('tree')) ? 'CHANGED' : 'every one identical'}\n`);

  if (problems.length > 0) {
    for (const problem of problems.slice(0, 20)) console.error(`  ${problem}`);
    console.error(`\n${problems.length} problem(s). Nothing was pushed.`);
    process.exit(1);
  }

  if (!options.push) {
    console.log('Checks passed. Run again with --push to publish.');
    return;
  }

  const refspecs = [`${map.get(head)}:refs/heads/main`, ...moved.map(([refname, object]) => `${object}:${refname}`)];
  execFileSync('git', ['-C', mirror, 'push', options.remote, ...refspecs], { stdio: 'inherit' });
  console.log(`\npushed to ${options.remote}`);
}

main();
