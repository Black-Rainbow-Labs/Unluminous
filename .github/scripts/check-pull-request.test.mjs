// Checks for what `check-pull-request.mjs` decides, run with
// `node --test .github/scripts/check-pull-request.test.mjs` (task-2326).
//
// The untouched template has to fail and a filled one has to pass; every other test is one way a
// description can be incomplete, and fails if the check stops noticing it.

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { commentFor, MARKER, review, sectionsOf } from './check-pull-request.mjs';

const TEMPLATE = fs.readFileSync(
  path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'pull_request_template.md'),
  'utf8',
);

/** The template with every section written and every box ticked, as a careful contributor sends it. */
function filled() {
  return TEMPLATE
    .replace('## Overview\n', '## Overview\n\nThe find bar forgot its last search when a tab was closed. Closes #12.\n')
    .replace('## Details\n', '## Details\n\n`services::find` keeps the needle per window rather than per tab now.\n')
    .replace('## Agent parity\n', '## Agent parity\n\nNo new control. `editor find` already reaches the same state.\n')
    .replace('## Test coverage\n', '## Test coverage\n\n`the_needle_survives_closing_a_tab` in `services/find.rs`.\n')
    .replace('## Evidence\n', '## Evidence\n\n`cargo test -p unluminous-app --lib find` printed 14 passed.\n')
    .replace('## AI disclosure\n', '## AI disclosure\n\nNone\n')
    .replace('- [ ] Windows', '- [x] Windows')
    .replaceAll('- [ ] I ', '- [x] I ')
    .replace('- [ ] `cargo fmt', '- [x] `cargo fmt')
    .replace('- [ ] The tests', '- [x] The tests')
    .replace('- [ ] Every control', '- [x] Every control')
    .replace('- [ ] Documentation', '- [x] Documentation')
    .replace('- [ ] My changes', '- [x] My changes');
}

/** A changed file as `GET /pulls/{n}/files` lists it. */
function file(filename, patch = '', status = 'modified') {
  return { filename, patch, status };
}

test('the template as GitHub fills it in fails, naming every section', () => {
  const { problems } = review({ body: TEMPLATE });
  for (const name of ['Overview', 'Details', 'Agent parity', 'Test coverage', 'Evidence', 'AI disclosure']) {
    assert.ok(problems.some((p) => p.includes(`\`${name}\` has not been written`)), `${name} was not reported`);
  }
  assert.ok(problems.some((p) => p.includes('`Platforms` has no box ticked')));
  assert.ok(problems.some((p) => p.startsWith('`Checklist` item not ticked')));
});

test('a filled description passes', () => {
  assert.deepEqual(review({ body: filled() }).problems, []);
});

test('an empty description names every heading as missing', () => {
  const { problems } = review({ body: '' });
  assert.equal(problems.filter((p) => p.includes('heading is missing')).length, 8);
});

test('a heading that starts with the section name is that section', () => {
  const body = filled().replace('## Overview\n', '# Overview of the problem\n');
  assert.deepEqual(review({ body }).problems, []);
});

test('a placeholder answer does not count', () => {
  const body = filled().replace('`editor find` already reaches the same state.', '').replace('No new control.', 'N/A');
  assert.ok(review({ body }).problems.some((p) => p.includes('`Agent parity` has not been written')));
});

test('words left inside a comment do not count', () => {
  const body = filled().replace(
    'The find bar forgot its last search when a tab was closed. Closes #12.',
    '<!-- The find bar forgot its last search when a tab was closed. -->',
  );
  assert.ok(review({ body }).problems.some((p) => p.includes('`Overview` has not been written')));
});

test('a heading inside a fenced block stays in its section', () => {
  const sections = sectionsOf('## Details\n\n```sh\n# not a heading\n```\n\n## Evidence\n\nran it');
  assert.ok(sections.get('details').includes('# not a heading'));
  assert.equal(sections.get('evidence'), 'ran it');
});

test('an unticked checklist item is named', () => {
  const body = filled().replace('- [x] `cargo fmt', '- [ ] `cargo fmt');
  const { problems } = review({ body });
  assert.equal(problems.length, 1);
  assert.ok(problems[0].includes('cargo fmt --all -- --check'));
});

test('a change to the drawing without a picture or video fails', () => {
  const files = [file('crates/unluminous-app/src/components/find_bar.rs', '+#[test]\n+fn x() {}')];
  assert.ok(review({ body: filled(), files }).problems.some((p) => p.includes('needs a video or screenshots')));
});

test('a change to the drawing with an uploaded video passes', () => {
  const files = [file('crates/unluminous-app/src/components/find_bar.rs', '+#[test]\n+fn x() {}')];
  const body = filled().replace('printed 14 passed.', 'printed 14 passed.\n\nhttps://github.com/user-attachments/assets/3f1c9d2e-0000-4000-8000-000000000000');
  assert.deepEqual(review({ body, files }).problems, []);
});

test('a change to Rust code with no test fails unless it says why', () => {
  const files = [file('crates/unluminous-core/src/rope.rs', '+let x = 1;')];
  assert.ok(review({ body: filled(), files }).problems.some((p) => p.includes('adds no test')));
  const body = filled().replace('in `services/find.rs`.', 'in `services/find.rs`.\n\nNo new test: this only renames a private function.');
  assert.deepEqual(review({ body, files }).problems, []);
});

test('a test file or an added test attribute counts as a test', () => {
  const code = file('crates/unluminous-core/src/rope.rs', '+let x = 1;');
  assert.deepEqual(review({ body: filled(), files: [code, file('crates/unluminous-core/tests/rope.rs')] }).problems, []);
  const inline = file('crates/unluminous-core/src/rope.rs', '+    #[test]\n+    fn splits() {}');
  assert.deepEqual(review({ body: filled(), files: [inline] }).problems, []);
});

test('the comment starts with the marker it is found by', () => {
  assert.ok(commentFor([]).startsWith(MARKER));
  assert.ok(commentFor(['x']).startsWith(MARKER));
  assert.ok(commentFor(['x']).includes('- x'));
});
