// Checks a pull request's description against `.github/pull_request_template.md` (task-2326).
//
//     node .github/scripts/check-pull-request.mjs                  in a workflow: reads the event
//     node .github/scripts/check-pull-request.mjs --body <file>    by hand: checks a description, no files
//
// **What it enforces is what a reviewer would otherwise have to ask for.** Every section of the
// template has words of the author's own in it; at least one platform is ticked; every box in the
// checklist is ticked; a change to what the window draws carries a picture or a video under
// `Evidence`; and a change to Rust code adds a test or says why it cannot have one.
//
// **The guidance in the template is inside HTML comments, and comments are removed before anything
// is read.** So a section the author did not touch reads as empty, whatever the template says there,
// and nothing here has to know the template's wording.
//
// **It runs from `pull_request_target`, so the copy of this file that runs is the one on `main`.**
// A pull request that edits this script cannot change what checks it. Nothing from the pull request is
// checked out or executed: the description and the list of changed files are read through the API.
//
// The verdict is a pure function, `review`, so `check-pull-request.test.mjs` tests it with no network.

import fs from 'node:fs';
import { pathToFileURL } from 'node:url';

/** The marker that identifies the one comment this check keeps up to date on a pull request. */
export const MARKER = '<!-- unluminous-pull-request-check -->';

/**
 * The sections the template asks for, in its order.
 *
 * `least` is how many characters of the author's own words a section needs to count as written. It is
 * small on purpose: it catches a section left empty or answered with `-`, and it does not judge prose.
 */
export const SECTIONS = [
  { name: 'Overview', least: 20 },
  { name: 'Details', least: 20 },
  { name: 'Agent parity', least: 10 },
  { name: 'Test coverage', least: 10 },
  { name: 'Evidence', least: 10 },
  { name: 'Platforms', least: 0 },
  { name: 'AI disclosure', least: 4 },
  { name: 'Checklist', least: 0 },
];

/** Answers that are not answers. `None` is missing on purpose: it is a real answer to `AI disclosure`. */
const NOT_AN_ANSWER = new Set(['n/a', 'na', 'tbd', 'todo', 'wip', '-', '--', '.', '...', 'see above', 'see title']);

/** What a picture or a video looks like in a description: Markdown, HTML, or an uploaded file's address. */
const MEDIA = /!\[[^\]]*\]\([^)]+\)|<img\s|<video\s|https:\/\/github\.com\/user-attachments\/|https:\/\/user-images\.githubusercontent\.com\/|\.(png|jpe?g|gif|webp|mp4|mov|webm)\b/i;

/**
 * The paths whose change is a change to what somebody sees, so `Evidence` must show it.
 *
 * The drawing, the theme, the drawn icons, the accepted screenshots and the application icon. A change
 * to `services/` or to a crate with no window is not on this list, because nobody can photograph it.
 */
const SEEN = [
  /^crates\/unluminous-app\/src\/components\//,
  /^crates\/unluminous-app\/src\/theme\//,
  /^crates\/unluminous-app\/tests\/snapshots\/.*\.png$/,
  /^installer\/icon\//,
  /^design\//,
];

/**
 * Removes every HTML comment, including one the author left unclosed at the end.
 *
 * @param text - Markdown as GitHub stores it
 */
export function withoutComments(text) {
  return text.replace(/<!--[\s\S]*?(-->|$)/g, '');
}

/**
 * Splits a description into its sections, keyed by heading in lower case.
 *
 * A heading at any level from one to three opens a section, and the section runs to the next such
 * heading. Lines inside a fenced code block are never read as headings, so a Mermaid diagram or a
 * shell transcript with a `#` in it stays in the section it was written in.
 *
 * @param body - the pull request description
 */
export function sectionsOf(body) {
  const sections = new Map();
  let current = null;
  let fenced = false;
  for (const line of withoutComments(body.replace(/\r\n/g, '\n')).split('\n')) {
    if (/^\s*(```|~~~)/.test(line)) fenced = !fenced;
    const heading = fenced ? null : /^\s{0,3}#{1,3}\s+(.+?)\s*#*\s*$/.exec(line);
    if (heading) {
      current = heading[1].trim().toLowerCase();
      if (!sections.has(current)) sections.set(current, []);
      continue;
    }
    if (current) sections.get(current).push(line);
  }
  return new Map([...sections].map(([name, lines]) => [name, lines.join('\n').trim()]));
}

/**
 * Finds the section for one of the template's names, accepting a heading that starts with it, so
 * `Overview of the problem` is the `Overview` section.
 *
 * @param sections - what `sectionsOf` answered
 * @param name - the template's name for the section
 */
function sectionNamed(sections, name) {
  const wanted = name.toLowerCase();
  for (const [heading, text] of sections) if (heading === wanted || heading.startsWith(`${wanted} `)) return text;
  return undefined;
}

/**
 * The author's own words in a section, with check boxes and Markdown punctuation left out, so a
 * section holding only a bullet or an empty box does not count as written.
 *
 * @param text - one section's text
 */
function ownWords(text) {
  return text
    .split('\n')
    .filter((line) => !/^\s*[-*]\s*\[[ xX]\]/.test(line))
    .join(' ')
    .replace(/[#>*_`|-]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * The boxes in a section, ticked and not.
 *
 * @param text - one section's text
 */
function boxesIn(text) {
  const boxes = [...text.matchAll(/^\s*[-*]\s*\[([ xX])\]\s*(.*)$/gm)];
  return {
    ticked: boxes.filter((box) => box[1] !== ' ').map((box) => box[2].trim()),
    empty: boxes.filter((box) => box[1] === ' ').map((box) => box[2].trim()),
  };
}

/**
 * Whether a changed file is Rust code that is not itself a test.
 *
 * @param file - a changed file as the API lists it
 */
function isRustCode(file) {
  return file.filename.endsWith('.rs') && !/(^|\/)tests\//.test(file.filename) && file.status !== 'removed';
}

/**
 * Whether a changed file adds a test: a file under a `tests` folder, or an added line that declares
 * a test or a test module.
 *
 * @param file - a changed file as the API lists it, with its `patch` when GitHub sent one
 */
function addsATest(file) {
  if (!file.filename.endsWith('.rs') || file.status === 'removed') return false;
  if (/(^|\/)tests\//.test(file.filename)) return true;
  return /^\+.*#\[(test|cfg\(test\)|tokio::test)/m.test(file.patch ?? '');
}

/**
 * Reads a description and the files it changes and answers what is missing.
 *
 * @param body - the pull request description, or an empty string when there is none
 * @param files - the changed files as `GET /pulls/{n}/files` lists them; empty when they are not known
 */
export function review({ body, files = [] }) {
  const problems = [];
  const sections = sectionsOf(body ?? '');

  for (const { name, least } of SECTIONS) {
    const text = sectionNamed(sections, name);
    if (text === undefined) {
      problems.push(`The \`${name}\` heading is missing. Put the template's headings back and fill each one in.`);
      continue;
    }
    const words = ownWords(text);
    if (least > 0 && (words.length < least || NOT_AN_ANSWER.has(words.toLowerCase()))) {
      problems.push(`\`${name}\` has not been written. The text inside \`<!-- -->\` is guidance and does not count.`);
    }
  }

  const platforms = sectionNamed(sections, 'Platforms');
  if (platforms !== undefined && boxesIn(platforms).ticked.length === 0) {
    problems.push('`Platforms` has no box ticked. Tick every platform you built and ran this change on.');
  }

  const checklist = sectionNamed(sections, 'Checklist');
  if (checklist !== undefined) {
    const { ticked, empty } = boxesIn(checklist);
    if (ticked.length === 0 && empty.length === 0) problems.push('`Checklist` has no boxes. Put the template\'s checklist back.');
    for (const item of empty) problems.push(`\`Checklist\` item not ticked: ${item}`);
  }

  const seen = files.filter((file) => SEEN.some((pattern) => pattern.test(file.filename)));
  const evidence = sectionNamed(sections, 'Evidence') ?? '';
  if (seen.length > 0 && !MEDIA.test(evidence)) {
    problems.push(
      `This changes what the window draws (\`${seen[0].filename}\`${seen.length > 1 ? ` and ${seen.length - 1} more` : ''}), ` +
        'so `Evidence` needs a video or screenshots. Drag them into the description and GitHub uploads them.',
    );
  }

  const rust = files.filter(isRustCode);
  const testing = sectionNamed(sections, 'Test coverage') ?? '';
  if (rust.length > 0 && !files.some(addsATest) && !/^\s*[-*]?\s*no new test:/im.test(testing)) {
    problems.push(
      'This changes Rust code and adds no test. Add one, or start a line under `Test coverage` with ' +
        '`No new test:` and say why this change cannot have one.',
    );
  }

  return { problems };
}

/**
 * The comment this check keeps on a pull request: what is missing, or that nothing is.
 *
 * @param problems - what `review` answered
 */
export function commentFor(problems) {
  if (problems.length === 0) {
    return `${MARKER}\n**The description is complete.** Every section of the template is written. Thank you.`;
  }
  return [
    MARKER,
    '**The description is not complete yet**, so the `PR description` check is failing. Edit the description to fix each of these and the check runs again by itself:',
    '',
    ...problems.map((problem) => `- ${problem}`),
    '',
    'CONTRIBUTING.md says what each section is for: https://github.com/Black-Rainbow-Labs/Unluminous/blob/main/CONTRIBUTING.md#opening-a-pull-request',
  ].join('\n');
}

/**
 * Calls the GitHub API with the workflow's token and answers the parsed body.
 *
 * @param path - the path under `https://api.github.com`
 * @param options - `fetch` options; a `body` object is sent as JSON
 */
async function github(path, options = {}) {
  const response = await fetch(`https://api.github.com${path}`, {
    ...options,
    headers: {
      authorization: `Bearer ${process.env.GITHUB_TOKEN}`,
      accept: 'application/vnd.github+json',
      'x-github-api-version': '2022-11-28',
      ...(options.body ? { 'content-type': 'application/json' } : {}),
    },
    body: options.body ? JSON.stringify(options.body) : undefined,
  });
  if (!response.ok) throw new Error(`${options.method ?? 'GET'} ${path} answered ${response.status}: ${await response.text()}`);
  return response.status === 204 ? null : response.json();
}

/**
 * Every changed file of a pull request, following the pages.
 *
 * @param repository - `owner/name`
 * @param number - the pull request's number
 */
async function changedFiles(repository, number) {
  const files = [];
  for (let page = 1; ; page += 1) {
    const batch = await github(`/repos/${repository}/pulls/${number}/files?per_page=100&page=${page}`);
    files.push(...batch);
    if (batch.length < 100) return files;
  }
}

/**
 * Writes the comment, editing the one this check left before rather than adding another, so a pull
 * request edited ten times carries one comment and not ten.
 *
 * @param repository - `owner/name`
 * @param number - the pull request's number
 * @param body - the comment's text
 */
async function keepTheComment(repository, number, body) {
  const comments = await github(`/repos/${repository}/issues/${number}/comments?per_page=100`);
  const mine = comments.find((comment) => comment.body?.startsWith(MARKER));
  if (mine) {
    if (mine.body !== body) await github(`/repos/${repository}/issues/comments/${mine.id}`, { method: 'PATCH', body: { body } });
  } else {
    await github(`/repos/${repository}/issues/${number}/comments`, { method: 'POST', body: { body } });
  }
}

/** Runs the check for the pull request a workflow event names, or for a description file given by hand. */
async function main() {
  const at = process.argv.indexOf('--body');
  if (at !== -1) {
    const { problems } = review({ body: fs.readFileSync(process.argv[at + 1], 'utf8') });
    console.log(problems.length === 0 ? 'The description is complete.' : problems.map((p) => `- ${p}`).join('\n'));
    process.exit(problems.length === 0 ? 0 : 1);
  }

  const event = JSON.parse(fs.readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
  const pull = event.pull_request;
  const repository = process.env.GITHUB_REPOSITORY;
  const files = await changedFiles(repository, pull.number);
  const { problems } = review({ body: pull.body ?? '', files });

  const comment = commentFor(problems);
  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${comment.replace(MARKER, '')}\n`);
  try {
    await keepTheComment(repository, pull.number, comment);
  } catch (error) {
    // The comment is a courtesy; the verdict is the check's own status. A token without write
    // permission must not turn a complete description into a failure.
    console.log(`The comment could not be written: ${error.message}`);
  }

  for (const problem of problems) console.log(`::error title=PR description::${problem.replace(/`/g, '')}`);
  console.log(problems.length === 0 ? 'The description is complete.' : `${problems.length} thing(s) missing.`);
  process.exit(problems.length === 0 ? 0 : 1);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) await main();
