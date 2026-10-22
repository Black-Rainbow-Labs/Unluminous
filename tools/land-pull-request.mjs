/**
 * Lands a pull request from the public repository in this one, with its contributor as the author.
 *
 *     node tools/land-pull-request.mjs 12 --dry-run   # what it would do, and whether it applies cleanly
 *     node tools/land-pull-request.mjs 12             # apply it to the current branch and commit it
 *     node tools/land-pull-request.mjs 12 --close     # once published: say where it landed and close it
 *
 * **Why a pull request is landed rather than merged (`task-2326`).** `github.com/Black-Rainbow-Labs/Unluminous`
 * is published from this private repository by `tools/publish-open-source.mjs`, by an ordinary push of a
 * history that is a pure function of this one. A pull request merged on GitHub puts a commit on the
 * public `main` that this repository has never had, so the next publish is not a fast forward and is
 * refused. So the change comes here instead: it is applied to the branch being worked on as one commit,
 * released, and published, and the pull request is closed with the commit it became.
 *
 * **The contributor keeps the credit.** The commit is authored by whoever wrote the pull request's last
 * commit, every other author in it is named in a `Co-authored-by` trailer, and the commit carries
 * `Landed-from: Black-Rainbow-Labs/Unluminous#<n>`, which is what tells the publisher to leave its author
 * as it is.
 *
 * **It lands only a pull request whose checks passed**: the description, the formatting and the headless
 * tests. `--anyway` lands one regardless, for a maintainer who has read it and decided.
 *
 * The pull request's commits are fetched into `refs/landing/`, never into a branch, and those refs are
 * deleted when it finishes. Nothing is pushed.
 */
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const REPOSITORY = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const PUBLIC = 'Black-Rainbow-Labs/Unluminous';
const PUBLIC_REMOTE = `https://github.com/${PUBLIC}.git`;
/** The checks a pull request has to pass to be landed: `.github/workflows/`'s three jobs, by name. */
const CHECKS = ['PR description', 'Format', 'Headless tests'];

/**
 * Runs git in this checkout and answers its standard output.
 *
 * @param args - the git arguments
 * @param input - text for its standard input, when it reads one
 */
function git(args, input) {
  return execFileSync('git', ['-C', REPOSITORY, ...args], { encoding: 'utf8', input, maxBuffer: 1 << 28 });
}

/**
 * The token git already holds for the public repository, which is the one `git push` uses. It is read
 * the way `tools/release.sh` reads it and is never printed.
 */
function token() {
  if (process.env.GH_TOKEN) return process.env.GH_TOKEN;
  const answer = execFileSync('git', ['credential', 'fill'], {
    input: `protocol=https\nhost=github.com\npath=${PUBLIC}.git\n\n`,
    encoding: 'utf8',
  });
  const found = /^password=(.*)$/m.exec(answer)?.[1];
  if (!found) throw new Error(`git holds no credential for ${PUBLIC}`);
  return found;
}

/**
 * Calls the GitHub API and answers the parsed body.
 *
 * @param route - the path under `https://api.github.com`
 * @param options - `fetch` options; a `body` object is sent as JSON
 */
async function github(route, options = {}) {
  const response = await fetch(`https://api.github.com${route}`, {
    ...options,
    headers: {
      authorization: `Bearer ${token()}`,
      accept: 'application/vnd.github+json',
      'x-github-api-version': '2022-11-28',
      ...(options.body ? { 'content-type': 'application/json' } : {}),
    },
    body: options.body ? JSON.stringify(options.body) : undefined,
  });
  if (!response.ok) throw new Error(`${options.method ?? 'GET'} ${route} answered ${response.status}: ${await response.text()}`);
  return response.json();
}

/**
 * The first section of a description, which the template calls `Overview`, without the guidance in
 * comments. It becomes the body of the commit message.
 *
 * @param body - the pull request description
 */
function overviewOf(body) {
  const text = (body ?? '').replace(/\r\n/g, '\n').replace(/<!--[\s\S]*?(-->|$)/g, '');
  const match = /^#{1,3}\s+Overview[^\n]*\n([\s\S]*?)(?=^#{1,3}\s|$(?![\s\S]))/m.exec(text);
  return (match?.[1] ?? '').trim();
}

/** Reads the pull request number and the switches. */
function readTheArguments(argv) {
  const options = { number: 0, dryRun: false, close: false, anyway: false };
  for (const argument of argv) {
    if (argument === '--dry-run') options.dryRun = true;
    else if (argument === '--close') options.close = true;
    else if (argument === '--anyway') options.anyway = true;
    else if (/^\d+$/.test(argument)) options.number = Number(argument);
    else throw new Error(`unknown argument ${argument}`);
  }
  if (!options.number) throw new Error('name the pull request: node tools/land-pull-request.mjs <number>');
  return options;
}

/**
 * Fetches the pull request's head and the public `main` into `refs/landing/`.
 *
 * @param number - the pull request's number
 */
function fetchThePullRequest(number) {
  git(['fetch', '--quiet', PUBLIC_REMOTE, `+refs/pull/${number}/head:refs/landing/pull-${number}`, '+refs/heads/main:refs/landing/public-main']);
  return { head: `refs/landing/pull-${number}`, main: 'refs/landing/public-main' };
}

/** Deletes what `fetchThePullRequest` wrote, so landing leaves no ref behind. */
function forgetTheLanding(number) {
  for (const ref of [`refs/landing/pull-${number}`, 'refs/landing/public-main']) {
    try {
      git(['update-ref', '-d', ref]);
    } catch {
      // Already gone.
    }
  }
}

/**
 * How each required check ended on the pull request's head commit: the newest run of each, or
 * `not run` for one that never ran, which is what a first time contributor's CI is until a
 * maintainer approves it.
 *
 * @param sha - the head commit
 */
async function requiredChecks(sha) {
  const { check_runs: runs } = await github(`/repos/${PUBLIC}/commits/${sha}/check-runs?per_page=100`);
  return CHECKS.map((name) => {
    const newest = runs.filter((run) => run.name === name).sort((a, b) => (b.started_at ?? '').localeCompare(a.started_at ?? ''))[0];
    return { name, conclusion: newest?.conclusion ?? (newest ? newest.status : 'not run') };
  });
}

/**
 * The authors of the pull request's own commits, the newest first, without repeats.
 *
 * @param base - where the pull request branched from the public `main`
 * @param head - its head
 */
function authorsOf(base, head) {
  const rows = git(['log', '--format=%an <%ae>', `${base}..${head}`]).trim().split('\n').filter(Boolean);
  return [...new Set(rows)];
}

/**
 * Applies the pull request to the current branch as one commit with the contributor as its author.
 *
 * @param pull - the pull request as the API answers it
 * @param options - the switches
 */
async function land(pull, options) {
  if (git(['status', '--porcelain']).trim()) throw new Error('the checkout has uncommitted changes; commit or set them aside first');
  if (pull.state !== 'open') throw new Error(`#${pull.number} is ${pull.state}`);

  const checks = await requiredChecks(pull.head.sha);
  for (const { name, conclusion } of checks) console.log(`check      ${name}: ${conclusion}`);
  const failing = checks.filter((check) => check.conclusion !== 'success');
  if (failing.length > 0 && !options.anyway) {
    throw new Error(`${failing.map((check) => check.name).join(', ')} did not pass; ask for it to be fixed, or pass --anyway`);
  }

  const { head, main } = fetchThePullRequest(pull.number);
  try {
    const base = git(['merge-base', head, main]).trim();
    const [author, ...others] = authorsOf(base, head);
    const patch = git(['diff', '--binary', base, head]);
    console.log(`author     ${author}`);
    for (const other of others) console.log(`co-author  ${other}`);
    console.log(`files      ${git(['diff', '--stat', base, head]).trim().split('\n').pop().trim()}`);

    git(['apply', '--check', '--3way'], patch);
    if (options.dryRun) {
      console.log('\nIt applies to the current branch. Run again without --dry-run to commit it.');
      return;
    }
    git(['apply', '--3way', '--index'], patch);

    const message = [
      pull.title,
      '',
      overviewOf(pull.body) || `From ${pull.html_url}.`,
      '',
      `Landed-from: ${PUBLIC}#${pull.number}`,
      ...others.map((other) => `Co-authored-by: ${other}`),
      '',
    ].join('\n');
    git(['commit', '--quiet', `--author=${author}`, '--file=-'], message);
    console.log(`\ncommitted  ${git(['rev-parse', '--short', 'HEAD']).trim()} on ${git(['branch', '--show-current']).trim()}`);
    console.log(`\nNext: release it, then run node tools/land-pull-request.mjs ${pull.number} --close`);
  } finally {
    forgetTheLanding(pull.number);
  }
}

/**
 * Finds the published commit that landed the pull request, says so on it and closes it.
 *
 * @param pull - the pull request as the API answers it
 */
async function close(pull) {
  const { main } = fetchThePullRequest(pull.number);
  try {
    const found = git(['log', '--format=%H', '--fixed-strings', `--grep=Landed-from: ${PUBLIC}#${pull.number}`, main]).trim().split('\n')[0];
    if (!found) throw new Error(`no commit on the public main says Landed-from: ${PUBLIC}#${pull.number}; publish it first`);
    const body = [
      `Landed as ${found}, with you as its author.`,
      '',
      'Unluminous is published from a private repository, so a pull request is landed there and published rather than merged here. That is why this one shows as closed. Thank you.',
    ].join('\n');
    if (pull.state === 'open') {
      await github(`/repos/${PUBLIC}/issues/${pull.number}/comments`, { method: 'POST', body: { body } });
      await github(`/repos/${PUBLIC}/pulls/${pull.number}`, { method: 'PATCH', body: { state: 'closed' } });
    }
    console.log(`#${pull.number} landed as ${found.slice(0, 10)}; ${pull.state === 'open' ? 'closed with a comment' : 'it was already closed'}.`);
  } finally {
    forgetTheLanding(pull.number);
  }
}

async function main() {
  const options = readTheArguments(process.argv.slice(2));
  const pull = await github(`/repos/${PUBLIC}/pulls/${options.number}`);
  console.log(`pull       #${pull.number} ${pull.title} by ${pull.user.login}`);
  if (options.close) await close(pull);
  else await land(pull, options);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
