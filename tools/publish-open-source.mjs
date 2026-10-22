/**
 * Publishes this repository's history to the public one, with the agent attribution taken out and
 * every commit under one spelling of Jason McAffee's name, except that a commit landed from a pull
 * request keeps its contributor as its author.
 *
 *     node tools/publish-open-source.mjs                 # rewrite, check, and say what it would push
 *     node tools/publish-open-source.mjs --push          # and push it
 *     node tools/publish-open-source.mjs --ref v0.49.2   # publish up to a tag rather than origin/main
 *     node tools/publish-open-source.mjs --ref HEAD --check-push   # would a push be a fast forward
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

/** The one name and address every commit of Jason McAffee's is published under. */
const JASON = 'Jason McAffee <jasonlmcaffee@gmail.com>';

/**
 * The trailer `tools/land-pull-request.mjs` writes on a commit it lands from a pull request.
 *
 * **A contributor's name is kept (`task-2326`).** A pull request to the public repository is landed in
 * this one with its author's own name on the commit, and a publish that rewrote every author to Jason
 * would take the credit for that work away from the person who did it. So the author of a commit that
 * carries this trailer is published as it is; its committer, who is whoever landed it, is still
 * rewritten.
 *
 * It is keyed on the trailer rather than on the name because every commit already published must
 * rewrite to exactly the hash it was published as. One of them was committed by `GitHub
 * <noreply@github.com>` through the web editor; a rule that kept every name that is not Jason's would
 * keep that one, change its hash and every hash after it, and make the next push a refused one.
 */
const LANDED = /^Landed-from: \S+#\d+\s*$/m;

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
      if (header.startsWith('author ') && LANDED.test(message)) return header;
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
 * Jason McAffee's apart from the authors of landed pull requests, and that no message still carries a
 * banned line. A publish that cannot say all three is a publish that should not happen.
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

  // A landed commit's author is the contributor and is meant to survive, so it is left out of the
  // identities that must all be Jason's and is reported on a line of its own.
  const rows = git(where, 'log', '--format=%an <%ae>%x09%cn <%ce>%x09%(trailers:key=Landed-from,valueonly,separator=%x20)', head)
    .trim().split('\n').map((row) => row.split('\t'));
  const identities = [...new Set(rows.flatMap(([author, committer, landed]) => (landed?.trim() ? [committer] : [author, committer])))];
  for (const identity of identities) if (identity !== JASON) problems.push(`an identity survived: ${identity}`);
  const contributors = [...new Set(rows.filter(([, , landed]) => landed?.trim()).map(([author]) => author))];

  const messages = git(where, 'log', '--format=%B', head).split('\n');
  for (const line of messages) {
    if (BANNED_LINE.some((pattern) => pattern.test(line.trim()))) problems.push(`a banned line survived: ${line}`);
  }

  return { problems, identities, contributors };
}

/** Reads the flags, with `--ref` and `--remote` taking a value and the rest being switches. */
function readTheArguments(argv) {
  const options = { ref: 'origin/main', remote: PUBLIC_REMOTE, push: false, checkPush: false, work: '' };
  for (let at = 0; at < argv.length; at += 1) {
    if (argv[at] === '--push') options.push = true;
    else if (argv[at] === '--check-push') options.checkPush = true;
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
  const { problems, identities, contributors } = check(mirror, map, order);

  console.log(`commits    ${order.length} rewritten, ${messagesChanged} whose message changed`);
  console.log(`tags       ${moved.length} moved`);
  console.log(`identity   ${identities.join(' | ')}`);
  console.log(`landed     ${contributors.length === 0 ? 'no pull requests' : contributors.join(' | ')}`);
  console.log(`head       ${head.slice(0, 10)} -> ${map.get(head).slice(0, 10)}`);
  console.log(`trees      ${problems.some((p) => p.includes('tree')) ? 'CHANGED' : 'every one identical'}\n`);

  if (problems.length > 0) {
    for (const problem of problems.slice(0, 20)) console.error(`  ${problem}`);
    console.error(`\n${problems.length} problem(s). Nothing was pushed.`);
    process.exit(1);
  }

  // **Whether the push would be refused, asked before a release builds anything (`task-2312`).** The public
  // `main` is the rewrite of some earlier private commit, so a push is a fast forward only when that commit is in
  // the history being published. 1.3.0's push was refused after both GitHub releases were made, because another
  // ticket had reached `main` while the release ran and had been published first.
  if (options.checkPush) {
    const theirs = execFileSync('git', ['ls-remote', options.remote, 'refs/heads/main'], { encoding: 'utf8' }).split('	')[0].trim();
    if (theirs) {
      let known = true;
      try {
        execFileSync('git', ['-C', mirror, 'cat-file', '-e', `${theirs}^{commit}`], { stdio: 'ignore' });
      } catch {
        known = false;
      }
      let ahead = false;
      if (known) {
        try {
          execFileSync('git', ['-C', mirror, 'merge-base', '--is-ancestor', theirs, map.get(head)], { stdio: 'ignore' });
          ahead = true;
        } catch {
          ahead = false;
        }
      }
      if (!ahead) {
        console.error(`The public main is ${theirs.slice(0, 10)}, which is not in the history of ${options.ref}, so a push would be refused.`);
        console.error('Merge origin/main into this branch and release again.');
        process.exit(2);
      }
    }
    console.log(`The push to ${options.remote} would be a fast forward.`);
    if (!options.push) return;
  }

  if (!options.push) {
    console.log('Checks passed. Run again with --push to publish.');
    return;
  }

  // **A tag the public repository already has is left alone.** A push without force refuses to move
  // one anyway, and a single refusal fails the whole release. `task-2096` found `v0.55.0` there as a
  // lightweight tag on the right commit, which `gh release create` makes when it gets there first,
  // while the rewrite builds an annotated one: same commit, different object, and every later
  // release stopped on it.
  const published = new Set(
    execFileSync('git', ['ls-remote', '--tags', options.remote], { encoding: 'utf8' })
      .split('\n')
      .map((row) => row.split('\t')[1])
      .filter((refname) => refname && !refname.endsWith('^{}')),
  );
  const fresh = moved.filter(([refname]) => !published.has(refname));
  console.log(`tags       ${fresh.length} not yet on ${options.remote}`);
  // **A public `main` that already holds this history is left where it is (`task-2312`).** A release that is
  // carried on after a failure publishes its tag again, and by then `main` may have moved past it; pushing the
  // older commit to `main` would be refused as a step backwards and stop the release a second time.
  const theirs = execFileSync('git', ['ls-remote', options.remote, 'refs/heads/main'], { encoding: 'utf8' }).split('	')[0].trim();
  let mainHasIt = false;
  if (theirs) {
    try {
      // The mirror holds only the rewrite of the history up to this ref, so the commit the public `main` is on
      // is fetched first. The rewrite is the same every time, so everything up to here arrives already present.
      execFileSync('git', ['-C', mirror, 'fetch', '--quiet', options.remote, 'refs/heads/main'], { stdio: 'ignore' });
      execFileSync('git', ['-C', mirror, 'merge-base', '--is-ancestor', map.get(head), theirs], { stdio: 'ignore' });
      mainHasIt = true;
    } catch {
      mainHasIt = false;
    }
  }
  const refspecs = [
    ...(mainHasIt ? [] : [`${map.get(head)}:refs/heads/main`]),
    ...fresh.map(([refname, object]) => `${object}:${refname}`),
  ];
  if (refspecs.length === 0) {
    console.log(`
${options.remote} already has ${options.ref} and every tag; nothing to push`);
    return;
  }
  execFileSync('git', ['-C', mirror, 'push', options.remote, ...refspecs], { stdio: 'inherit' });
  console.log(`\npushed to ${options.remote}`);
}

main();
