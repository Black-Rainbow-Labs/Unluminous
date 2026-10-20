// The agent study's cases, each with a check that reads the answer back from Unluminous and the disk.
//
// `scenarios.json` says what to ask. This file says how to tell whether it was done, which is what
// turns the study from transcripts a person reads into a number that can be compared between two
// builds (`task-2314`). A check never believes what the agent said it did, with one exception: where
// the job is to *tell* the person something (a value, a list, a count), the check reads the agent's
// final words for the fact, and the fact itself was worked out here from the project.
//
// A check returns `{ pass, why }`. `why` is one short sentence saying what was found, so a failing
// row in the report can be read without opening the transcript.
//
// `native` is the second rule. Where a case exists to see whether an agent uses Unluminous for a job
// rather than doing it round the window (writing the file itself, grepping, shelling out to git),
// the check fails when the job was done round it, even if the end state is right. A case says which
// of its jobs that applies to; a `cargo check` in the agent's own shell is not a miss.
import fs from 'node:fs';
import path from 'node:path';

/** Every scenario in `scenarios.json` plus the cases added for `task-2314`, by id. */
export function loadCases(here) {
  const scenarios = JSON.parse(fs.readFileSync(path.join(here, 'scenarios.json'), 'utf8'));
  const all = [...scenarios, ...EXTRA];
  return all.map(s => ({ ...s, check: NOT_GRADED[s.id] ? null : CHECKS[s.id] ?? null }));
}

/**
 * Cases the eval leaves out, because a fresh settings folder cannot do them whatever an agent does:
 * the failure would be the machine's, and it would sit in every variant's score as noise.
 */
const NOT_GRADED = {
  's34-members-and-signature': 'needs rust-analyzer, which `editor.servers` leaves off by default',
  's36-dictation': 'needs the speech model, which a fresh settings folder has not downloaded',
};

/** True for a call to one of Unluminous's tools. */
export const isUnluminous = call => /unluminous/i.test(call.tool ?? '');

/** The verb a call to an Unluminous area tool named, or the tool's own last word for a narrow tool. */
export function verbOf(call) {
  const input = call.input ?? {};
  if (typeof input.command === 'string') return input.command;
  const m = /unluminous_(?:[a-z]+_)?([a-z_]+)$/.exec(call.tool ?? '');
  return m ? m[1] : '';
}

/** A call to `tool` (a regular expression over the tool name) whose verb matches `verb`, that worked. */
function used(ctx, tool, verb = /.*/) {
  return stepsOf(ctx.calls).some(c => tool.test(c.tool ?? '') && verb.test(verbOf(c)) && !c.error);
}

/**
 * Every call, with each step of an `unluminous_batch` call as a call of its own to the tool its first
 * word names (`task-2314`), so a command sent in a batch counts the same as one sent alone.
 */
export function stepsOf(calls) {
  return calls.flatMap(c => {
    // The root tool takes `<area> <verb>` as its command and sends the same request the area tool
    // would, so `{"command":"terminal send"}` to `unluminous` is a call to `unluminous_terminal`.
    const named = /(^|_)unluminous$/.test(c.tool ?? '') && typeof c.input?.command === 'string'
      && /^[a-z-]+ [a-z-]+$/.test(c.input.command.trim());
    if (named) {
      const [area, verb] = c.input.command.trim().split(' ');
      return [c, { ...c, tool: `unluminous_${area}`, input: { ...c.input, command: verb } }];
    }
    if (!/unluminous_batch$/.test(c.tool ?? '') || c.error) return [c];
    const steps = c.input?.steps ?? c.input?.arguments?.steps ?? [];
    const lines = Array.isArray(steps) ? steps : String(steps).split('\n');
    return [c, ...lines.map(line => {
      const [area, verb] = String(line).trim().replace(/^unluminous-cli\s+/, '').split(/\s+/);
      return { tool: `unluminous_${area}`, input: { command: verb, line }, error: null };
    })];
  });
}

/** The agent's own tools that changed a file in the project: write, edit, patch, or a shell that wrote. */
function ownWrites(ctx, file) {
  const touches = text => !file || String(text ?? '').replace(/\\/g, '/').includes(file);
  return ctx.calls.filter(c => {
    if (isUnluminous(c)) return false;
    const tool = c.tool ?? '';
    const input = c.input ?? {};
    if (/^(write|edit|patch|multiedit|apply_patch)$/i.test(tool)) return touches(input.filePath ?? input.path ?? JSON.stringify(input));
    if (/^bash$/i.test(tool)) {
      const cmd = String(input.command ?? '');
      return /(>|sed -i|mv |cp |rm |mkdir|touch|tee |Set-Content|Out-File|New-Item)/.test(cmd) && touches(cmd);
    }
    return false;
  });
}

/** The agent's own tools that read or searched the project instead of asking Unluminous. */
function ownLooks(ctx, re = /.*/) {
  return ctx.calls.filter(c => !isUnluminous(c) && /^(grep|glob|read|list|bash)$/i.test(c.tool ?? '') && re.test(JSON.stringify(c.input ?? {})));
}

const lower = s => String(s ?? '').toLowerCase();
const says = (ctx, ...words) => words.every(w => (w instanceof RegExp ? w.test(ctx.text) : lower(ctx.text).includes(lower(w))));
const ok = why => ({ pass: true, why });
const no = why => ({ pass: false, why });
const tabs = async ctx => (await ctx.cli('tab', 'list'))?.tabs ?? [];
const tabFor = (list, file) => list.find(t => String(t.path ?? '').replace(/\\/g, '/').endsWith(file));

/** The cases `task-2314` added, aimed at the commands an agent has to find rather than guess. */
const EXTRA = [
  { id: 'n01-goto-line', area: 'editor', name: 'Go to a line',
    expect: 'Opens main.rs and puts the caret on line 30 with `editor caret --line 30`.',
    prompts: ['Take me to line 30 of src/main.rs in the editor.'] },
  { id: 'n02-close-others', area: 'tab', name: 'Close every tab but one',
    expect: 'Opens three files, then closes every tab except shapes.rs.',
    prompts: ['Open src/main.rs, src/shapes.rs and web/app.js. Then close every tab except shapes.rs.'] },
  { id: 'n03-dock-terminal', area: 'panel', name: 'Move the terminal to the right',
    expect: 'Docks the terminal on the right with `panel dock terminal right` and shows it.',
    prompts: ['Move the terminal panel over to the right-hand side of the window and make sure it is showing.'] },
  { id: 'n04-explorer-width', area: 'explorer', name: 'Make the explorer wider',
    expect: 'Sets the explorer to about 400 points with `explorer width 400` or `panel size explorer --width 400`.',
    prompts: ['The file explorer is too narrow. Make it 400 points wide.'] },
  { id: 'n05-duplicate-line', area: 'editor', name: 'Duplicate a line',
    expect: 'Puts the caret on the `let total` line of main.rs and runs `editor lines duplicate`, leaving it unsaved.',
    prompts: ['In src/main.rs, duplicate the line `let total = total_area(&rects);` so it appears twice. Do not save.'] },
  { id: 'n06-open-folder', area: 'tab', name: 'Open every file in a folder',
    expect: 'Opens app.js, index.html and styles.css as tabs.',
    prompts: ['Open every file in the web folder as a tab.'] },
  { id: 'n07-highlight-many', area: 'highlight', name: 'Highlight several lines at once',
    expect: 'Highlights the three lines of main.rs that call area_of, with `highlight add` three times or one `highlight apply`.',
    prompts: ['Highlight every line in src/main.rs that calls area_of.'] },
  { id: 'n08-indent', area: 'settings', name: 'Indent with spaces',
    expect: 'Sets `editor.indent` to four spaces through settings.',
    prompts: ['Make the Tab key insert four spaces instead of a tab character in the editor.'] },
  { id: 'n09-side-preview', area: 'editor/view', name: 'Source and preview side by side',
    expect: 'Opens README.md and sets the view to `side`.',
    prompts: ['Open README.md and show me the markdown source and the rendered version side by side.'] },
  { id: 'n10-reopen', area: 'tab', name: 'Bring back a closed tab',
    before: { setup: ['tab', 'open', 'src/shapes.rs'], close: ['tab', 'close'] },
    expect: 'Uses `tab reopen` to bring shapes.rs back.',
    prompts: ['I just closed src/shapes.rs by accident. Bring that tab back.'] },
  { id: 'n11-keymap', area: 'settings', name: 'Use the VS Code keyboard shortcuts',
    expect: 'Sets `keymap.preset` to vscode.',
    prompts: ['I am used to VS Code. Switch the editor keyboard shortcuts to the VS Code ones.'] },
  { id: 'n12-move-file', area: 'explorer', name: 'Move a file to another folder',
    expect: 'Moves docs/notes.md into web/ with `explorer move`.',
    prompts: ['Move docs/notes.md into the web folder.'] },
  { id: 'n13-word-search', area: 'search', name: 'Which files mention a word',
    expect: 'Searches the project with Unluminous and names web/app.js.',
    prompts: ['Which files in this project mention collatz? Just tell me the file names.'] },
  { id: 'n14-line-numbers-off', area: 'settings', name: 'Turn a setting off by its plain name',
    expect: 'Turns the gutter line numbers off, through a setting or the View menu action.',
    prompts: ['Hide the line numbers in the editor.'] },
  { id: 'n15-terminal-output', area: 'terminal', name: 'Run a command and report a number from its output',
    expect: 'Runs `git log --oneline` in an Unluminous terminal tab and reports one commit.',
    prompts: ['In an Unluminous terminal, run git log --oneline and tell me how many commits there are.'] },
  { id: 'n16-select-copy-line-count', area: 'editor', name: 'Count lines of a file',
    expect: 'Reads the line count from `editor status` or `tab open`, and reports 41.',
    prompts: ['How many lines long is src/main.rs?'] },
  { id: 'n17-save-all', area: 'tab', name: 'Edit two files and save both',
    expect: 'Adds a comment line to the top of both files through the editor and saves each tab.',
    prompts: ['Add the line `// reviewed` to the very top of src/main.rs and of src/shapes.rs, and save both files.'] },
];

const CHECKS = {
  's00-discover': async ctx => {
    const areas = ['tab', 'terminal', 'git', 'debug', 'search', 'fold', 'run', 'setting', 'theme', 'split', 'highlight', 'browser', 'database', 'screenshot', 'rename'];
    const named = areas.filter(a => lower(ctx.text).includes(a));
    return named.length >= 5 ? ok(`named ${named.length} areas`) : no(`named only ${named.join(', ') || 'nothing'}`);
  },
  's01-tabs': async ctx => {
    const t = await tabs(ctx);
    const both = tabFor(t, 'src/main.rs') && tabFor(t, 'web/app.js');
    if (!both) return no('main.rs and app.js are not both open');
    return says(ctx, 'main.rs') && says(ctx, 'app.js') ? ok('both open and named') : no('open but not both named in the answer');
  },
  's02-create': async ctx => {
    const body = ctx.file('src/util.rs');
    if (!body || !/fn\s+double/.test(body) || !/i64/.test(body)) return no('src/util.rs missing or has no fn double(i64)');
    if (ownWrites(ctx, 'util.rs').length) return no('written with the agent\'s own tools');
    return ok('util.rs written through Unluminous');
  },
  's03-edit': async ctx => {
    if (ctx.file('src/shapes.rs') !== ctx.original('src/shapes.rs')) return no('shapes.rs was saved or written on disk');
    const t = tabFor(await tabs(ctx), 'src/shapes.rs');
    if (!t?.modified) return no('shapes.rs is not open with unsaved changes');
    await ctx.cli('tab', 'show', String(t.index));
    const text = (await ctx.cli('editor', 'text'))?.text;
    const lines = String(text ?? '').split(/\r?\n/);
    const at = lines.findIndex(l => /fn same_size/.test(l));
    const above = lines.slice(Math.max(0, at - 4), at).filter(l => /^\s*(\/\/\/|\/\/)/.test(l));
    return at > 0 && above.length >= 1 && text !== ctx.original('src/shapes.rs') ? ok('comment above same_size, unsaved') : no('no new comment above same_size');
  },
  's04-fold': async ctx => {
    const f = await ctx.cli('fold', 'list');
    const regions = f?.regions ?? [];
    const collapsed = regions.filter(r => r.collapsed);
    // A region is named by the line its block starts on, and total_area's body starts on line 7.
    const total = regions.find(r => Number(r.line) === 7);
    if (!collapsed.length) return no('nothing is collapsed');
    if (!total || total.collapsed) return no('total_area is not open');
    return ok(`${collapsed.length} collapsed, total_area open`);
  },
  's05-split': async ctx => {
    const p = await ctx.cli('pane', 'list');
    if ((p?.count ?? 0) < 2) return no('one pane');
    const t = await tabs(ctx);
    const m = tabFor(t, 'src/main.rs'), s = tabFor(t, 'src/shapes.rs');
    if (!m || !s) return no('main.rs or shapes.rs not open');
    return m.pane !== s.pane && m.pane < s.pane ? ok('main.rs left, shapes.rs right') : no(`panes ${m.pane} and ${s.pane}`);
  },
  's06-terminal': async ctx => {
    const t = await ctx.cli('terminal', 'list');
    const named = (t?.tabs ?? []).find(x => /checks/i.test(x.name ?? x.title ?? JSON.stringify(x)));
    if (!named) return no('no terminal tab called checks');
    if (!used(ctx, /unluminous_terminal/, /send/)) return no('cargo check was not sent to an Unluminous terminal');
    return says(ctx, /pass|succe|finish|compil|no error|clean|ok/i) ? ok('named tab, ran, reported') : no('did not say whether it passed');
  },
  's07-run': async ctx => {
    const r = await ctx.cli('run', 'list');
    const started = (r?.configurations ?? []).some(c => c.started) || (r?.runs ?? []).length > 0;
    if (!started) return no('no run configuration was started');
    return says(ctx, /total area|largest/i) ? ok('ran and reported the output') : no('did not report what it printed');
  },
  's08-debug': async ctx => {
    const b = await ctx.cli('debug', 'breakpoint', 'list');
    const onLine = (b?.breakpoints ?? []).some(x => Number(x.line) === 37 && /main\.rs/.test(x.path ?? x.file ?? ''));
    if (!onLine) return no('no breakpoint on main.rs line 37');
    // 3 x 4 + 5 x 2 + 1.5 x 9.
    return /\b35\.5\b/.test(ctx.text) ? ok('breakpoint set, total 35.5 reported') : no('did not report total = 35.5');
  },
  's09-git': async ctx => {
    if (!used(ctx, /unluminous_git/, /status|action|diff/)) return no('git status was not asked of Unluminous');
    return says(ctx, 'shapes.rs') && says(ctx, 'NOTES') ? ok('named the modified and the untracked file') : no('did not name shapes.rs and NOTES.txt');
  },
  's10-search': async ctx => {
    const viaUnluminous = used(ctx, /unluminous/, /references|find|refs/);
    if (!viaUnluminous) return no('did not search with Unluminous');
    return says(ctx, 'main.rs') && says(ctx, 'shapes.rs') ? ok('found uses in both files') : no('did not name both files');
  },
  's11-definition': async ctx => {
    const e = await ctx.cli('editor', 'status');
    const at = String(e?.path ?? '').replace(/\\/g, '/');
    return at.endsWith('src/shapes.rs') && Number(e?.caret?.line) === 2 ? ok('caret on Rect in shapes.rs') : no(`caret at ${at}:${e?.caret?.line}`);
  },
  's12-rename': async ctx => {
    const main = ctx.file('src/main.rs') ?? '', shapes = ctx.file('src/shapes.rs') ?? '';
    const tabsNow = await tabs(ctx);
    // The rename may have left open tabs modified rather than saved, which is the documented behaviour.
    let text = main + shapes;
    for (const t of tabsNow.filter(x => x.modified)) { await ctx.cli('tab', 'show', String(t.index)); text += (await ctx.cli('editor', 'text'))?.text ?? ''; }
    if (/area_of/.test(main + shapes) && !/fn area\(/.test(text)) return no('area_of is still there');
    if (!used(ctx, /unluminous/, /rename/)) return no('renamed without editor rename');
    return ok('renamed through editor rename');
  },
  's13-highlight': async ctx => {
    const h = await ctx.cli('highlight', 'list', 'src/main.rs');
    const marks = h?.highlights ?? h?.marks ?? [];
    // Line 36 (`let total = total_area(...)`) and line 11 (`total += area_of(r)`) both work out the total.
    const covers = line => marks.some(m => Number(m.fromLine ?? m.line ?? m.from?.line) <= line && Number(m.toLine ?? m.line ?? m.to?.line ?? 0) >= line);
    return covers(36) || covers(11) ? ok('the total line highlighted') : no(`highlights: ${JSON.stringify(marks).slice(0, 120)}`);
  },
  's14-explorer': async ctx => {
    if (!fs.existsSync(path.join(ctx.project, 'tests', 'smoke.rs'))) return no('tests/smoke.rs does not exist');
    if (ownWrites(ctx, 'smoke').length || ownWrites(ctx, 'tests').length) return no('made with the agent\'s own tools');
    return ok('made through the explorer');
  },
  's15-preview': async ctx => {
    const e = await ctx.cli('editor', 'status');
    if (!String(e?.path ?? '').endsWith('README.md') || e?.viewMode !== 'preview' && e?.view !== 'preview') return no(`view ${e?.viewMode ?? e?.view} on ${e?.name}`);
    return says(ctx, 'total_area') || says(ctx, 'largest') ? ok('preview shown, diagram described') : no('did not describe the diagram');
  },
  's16-settings': async ctx => String((await ctx.cli('settings', 'get', 'appearance.font.size'))?.value) === '20' ? ok('font 20') : no('font is not 20'),
  's17-screenshot': async ctx => used(ctx, /unluminous_window/, /screenshot/) ? ok('took a screenshot') : no('no screenshot'),
  's18-modal': async ctx => {
    if (!used(ctx, /unluminous_modal/, /open|type|choose|accept/)) return no('did not use the go to file dialog');
    return tabFor(await tabs(ctx), 'web/styles.css') ? ok('styles.css opened from the dialog') : no('styles.css is not open');
  },
  's19-actions': async ctx => {
    const p = await ctx.cli('panel', 'list');
    const explorer = (p?.panels ?? []).find(x => x.name === 'explorer' || x.panel === 'explorer');
    if (!explorer?.showing && !explorer?.visible) return no('the explorer was left hidden');
    // `explorer hide` names the explorer in its tool, `action run toggle-explorer` in its arguments.
    const toggles = ctx.calls.filter(c => isUnluminous(c) && !c.error && /explorer|toggle/i.test(`${c.tool} ${JSON.stringify(c.input ?? {})}`) && /run|hide|show|toggle/.test(verbOf(c)));
    return toggles.length >= 2 && says(ctx, /split|zoom|preview|explorer/i) ? ok('listed View and toggled twice') : no(`${toggles.length} toggles`);
  },
  's20-project': async ctx => says(ctx, 'scratch-project') || says(ctx, 'project') && says(ctx, /rust/i) && says(ctx, /css|javascript|html/i)
    ? ok('named the folder and languages') : no('did not name the folder and the languages'),
  's21-complete': async ctx => {
    if (!used(ctx, /unluminous/, /complete/)) return no('did not ask editor complete');
    if (ctx.file('src/main.rs') !== ctx.original('src/main.rs')) return no('main.rs changed on disk');
    return says(ctx, 'area_of') ? ok('offered area_of') : no('did not name area_of');
  },
  's22-multifile': async ctx => {
    const body = ctx.file('src/shapes.rs') ?? '';
    if (!/fn\s+perimeter/.test(body)) return no('no perimeter function saved');
    if (ownWrites(ctx, 'shapes.rs').length) return no('shapes.rs written with the agent\'s own tools');
    if (!used(ctx, /unluminous_terminal/, /send/)) return no('cargo check not run in an Unluminous terminal');
    return ok('added, saved and checked');
  },
  's23-replace': async ctx => {
    if (!used(ctx, /unluminous/, /replace/)) return no('did not use editor replace');
    const t = tabFor(await tabs(ctx), 'src/shapes.rs');
    let text = ctx.file('src/shapes.rs') ?? '';
    if (t?.modified) { await ctx.cli('tab', 'show', String(t.index)); text = (await ctx.cli('editor', 'text'))?.text ?? text; }
    return !/width/.test(text) && /span/.test(text) && /\b2\b|two/i.test(ctx.text) ? ok('replaced and counted') : no('width still there or count wrong');
  },
  's24-browser': async ctx => {
    if (!used(ctx, /unluminous_browser/, /open/)) return no('did not open a browser tab');
    return says(ctx, /unluminous:\/\/|index\.html/i) ? ok('reported the address') : no('did not report the address');
  },
  's25-database': async ctx => {
    // The plugin's own tool (`unluminous_database`, `task-2314`) or `plugins run database`.
    if (!used(ctx, /unluminous_plugins/, /run/) && !used(ctx, /unluminous_database/)) return no('did not use the database plugin');
    if (ownLooks(ctx, /sqlite3/).length) return no('shelled out to sqlite3');
    return /\b7\b|seven/i.test(ctx.text) && says(ctx, 'album') ? ok('album, 7 rows') : no('wrong or missing row count (album has 7)');
  },
  's26-chat': async ctx => (used(ctx, /unluminous_plugins/, /pane|run|view/) || used(ctx, /unluminous_agent_chat/)) && says(ctx, /claude|codex|provider/i) ? ok('opened the pane and named providers') : no('did not drive the chat pane'),
  's27-space': async ctx => {
    const n = await ctx.cli('realm', 'nodes');
    const nodes = n?.nodes ?? [];
    const browser = nodes.find(x => x.kind === 'browser');
    if (!browser || !nodes.some(x => x.kind === 'terminal')) return no('no terminal and browser nodes');
    const c = await ctx.cli('realm', 'connections');
    if (!(c?.connections ?? c?.edges ?? []).length) return no('not connected');
    return used(ctx, /unluminous_realm/, /address|browser/) && says(ctx, 'example.com') ? ok('wired and drove the browser') : no('did not drive the browser as the node');
  },
  's28-comment': async ctx => {
    if (!used(ctx, /unluminous/, /comment/)) return no('did not use editor comment');
    const t = tabFor(await tabs(ctx), 'src/shapes.rs');
    let text = ctx.file('src/shapes.rs') ?? '';
    if (t) { await ctx.cli('tab', 'show', String(t.index)); text = (await ctx.cli('editor', 'text'))?.text ?? text; }
    return text.replace(/\r/g, '') === ctx.original('src/shapes.rs').replace(/\r/g, '') ? ok('commented and put back') : no('the file is not back the way it was');
  },
  's29-palette': async ctx => {
    if (!used(ctx, /unluminous_action/, /find|list/)) return no('did not look the action up');
    const s = await ctx.cli('settings', 'get', 'editor.line_numbers');
    return /false|off|hidden/i.test(String(s?.value)) ? ok('line numbers off') : no(`line numbers ${s?.value}`);
  },
  's30-board': async ctx => {
    const lanes = (await ctx.cli('plugins', 'view', 'agent-tasks'))?.lanes ?? [];
    const card = lanes.flatMap(l => l.cards.map(c => ({ ...c, lane: l.status }))).find(c => /gutter alignment/i.test(c.title));
    if (!card) return no('no ticket of that name');
    const todos = JSON.stringify((await ctx.cli('plugins', 'run', 'agent-tasks', 'task', card.key))?.todos ?? []);
    if (!/144 points/i.test(todos)) return no('no todo on it');
    return card.lane === 'in_progress' ? ok('ticket in progress with its todo') : no(`ticket is in ${card.lane}`);
  },
  's31-theme': async ctx => {
    const v = String((await ctx.cli('settings', 'get', 'appearance.theme'))?.value ?? '');
    // `theme set` and `settings set appearance.theme` are the same change.
    return /monokai/i.test(v) ? ok('Monokai Pro set') : no(`theme is ${v || 'default'}`);
  },
  's32-chat-node': async ctx => {
    const nodes = (await ctx.cli('realm', 'nodes'))?.nodes ?? [];
    if (!nodes.some(x => x.kind === 'chat')) return no('no chat node');
    // `realm chat`, or the Agent-Chat tool given the node (`task-2314`).
    const asked = used(ctx, /unluminous_realm/, /chat/) || ctx.calls.some(c => /unluminous_agent_chat/.test(c.tool ?? '') && !c.error && /"node"/.test(JSON.stringify(c.input ?? {})));
    return asked ? ok('asked the node itself') : no('asked the panel, not the node');
  },
  's33-code-search': async ctx => {
    if (!used(ctx, /unluminous/, /find|def|refs|fragment|definition|references/)) return no('did not use the index');
    return says(ctx, 'collatzLength') ? ok('found the definition and the Collatz function') : no('did not name collatzLength');
  },
  's34-members-and-signature': async ctx => {
    if (ctx.file('src/main.rs') !== ctx.original('src/main.rs')) return no('main.rs changed on disk');
    return used(ctx, /unluminous/, /complete/) && used(ctx, /unluminous/, /signature/) ? ok('completion and signature asked') : no('did not use complete and signature');
  },
  's35-report-an-issue': async ctx => {
    const r = await ctx.cli('report', 'status');
    return r?.kind === 'bug' && r?.sent ? ok('bug report written') : no(`report kind ${r?.kind}, sent ${JSON.stringify(r?.sent)}`);
  },
  's36-dictation': async ctx => {
    const v = await ctx.cli('voice', 'status');
    if (!used(ctx, /unluminous_voice/, /transcribe/)) return no('did not use voice transcribe');
    return /time/i.test(ctx.text) && /listen.*(on|true)|"listening":\s*true/i.test(JSON.stringify(v ?? {})) ? ok('transcribed and listening') : no('not transcribed or not listening');
  },
  'n01-goto-line': async ctx => {
    const e = await ctx.cli('editor', 'status');
    return String(e?.path ?? '').replace(/\\/g, '/').endsWith('src/main.rs') && Number(e?.caret?.line) === 30 ? ok('caret on main.rs:30') : no(`caret at ${e?.name}:${e?.caret?.line}`);
  },
  'n02-close-others': async ctx => {
    const t = (await tabs(ctx)).filter(x => x.path);
    return t.length === 1 && tabFor(t, 'src/shapes.rs') ? ok('only shapes.rs is open') : no(`open: ${t.map(x => x.name).join(', ')}`);
  },
  'n03-dock-terminal': async ctx => {
    const p = await ctx.cli('panel', 'list');
    const term = (p?.panels ?? []).find(x => (x.name ?? x.panel) === 'terminal');
    return term?.side === 'right' && (term?.showing ?? term?.visible) ? ok('terminal on the right, showing') : no(`terminal ${term?.side} ${term?.showing ?? term?.visible}`);
  },
  'n04-explorer-width': async ctx => {
    const p = await ctx.cli('panel', 'list');
    const ex = (p?.panels ?? []).find(x => (x.name ?? x.panel) === 'explorer');
    const w = Number(ex?.width ?? ex?.area?.width);
    return Math.abs(w - 400) <= 8 ? ok(`explorer ${w}`) : no(`explorer ${w}`);
  },
  'n05-duplicate-line': async ctx => {
    if (ctx.file('src/main.rs') !== ctx.original('src/main.rs')) return no('main.rs was saved');
    const t = tabFor(await tabs(ctx), 'src/main.rs');
    if (!t) return no('main.rs not open');
    await ctx.cli('tab', 'show', String(t.index));
    const text = (await ctx.cli('editor', 'text'))?.text ?? '';
    const n = (text.match(/let total = total_area\(&rects\);/g) ?? []).length;
    return n === 2 ? ok('line appears twice, unsaved') : no(`line appears ${n} times`);
  },
  'n06-open-folder': async ctx => {
    const t = await tabs(ctx);
    const want = ['web/app.js', 'web/index.html', 'web/styles.css'];
    const missing = want.filter(f => !tabFor(t, f));
    return missing.length ? no(`missing ${missing.join(', ')}`) : ok('all three open');
  },
  'n07-highlight-many': async ctx => {
    const h = await ctx.cli('highlight', 'list', 'src/main.rs');
    const marks = h?.highlights ?? h?.marks ?? [];
    const lines = new Set();
    for (const m of marks) for (let l = Number(m.fromLine ?? m.from?.line); l <= Number(m.toLine ?? m.to?.line); l++) lines.add(l);
    const want = [11, 23, 39];
    const missing = want.filter(l => !lines.has(l));
    return missing.length ? no(`lines not marked: ${missing.join(', ')}`) : ok('all three calls marked');
  },
  'n08-indent': async ctx => /spaces:?\s*4|4 spaces/i.test(String((await ctx.cli('settings', 'get', 'editor.indent'))?.value)) ? ok('four spaces') : no('indent unchanged'),
  'n09-side-preview': async ctx => {
    const e = await ctx.cli('editor', 'status');
    return String(e?.path ?? '').endsWith('README.md') && (e?.viewMode ?? e?.view) === 'side' ? ok('side by side') : no(`view ${e?.viewMode ?? e?.view} on ${e?.name}`);
  },
  // `tab reopen` and opening the file again leave the same tab, so either is the job done.
  'n10-reopen': async ctx => tabFor(await tabs(ctx), 'src/shapes.rs') ? ok(used(ctx, /unluminous/, /reopen/) ? 'reopened' : 'opened again') : no('shapes.rs is not open'),
  'n11-keymap': async ctx => /vscode/i.test(String((await ctx.cli('settings', 'get', 'keymap.preset'))?.value)) ? ok('vscode keymap') : no('keymap unchanged'),
  'n12-move-file': async ctx => {
    if (!fs.existsSync(path.join(ctx.project, 'web', 'notes.md'))) return no('web/notes.md does not exist');
    return used(ctx, /unluminous_explorer/, /move/) ? ok('moved with the explorer') : no('moved round Unluminous');
  },
  'n13-word-search': async ctx => {
    if (!used(ctx, /unluminous/, /find|search|files/)) return no('did not search with Unluminous');
    return says(ctx, 'app.js') ? ok('named app.js') : no('did not name app.js');
  },
  'n14-line-numbers-off': async ctx => /false|off|hidden/i.test(String((await ctx.cli('settings', 'get', 'editor.line_numbers'))?.value)) ? ok('line numbers off') : no('line numbers on'),
  'n15-terminal-output': async ctx => used(ctx, /unluminous_terminal/, /send/) && /\b1\b|one/i.test(ctx.text) ? ok('ran it and counted one') : no('not run in a terminal or wrong count'),
  // 41 lines of text; the editor counts the empty line after the last line break too.
  'n16-select-copy-line-count': async ctx => /\b4[12]\b/.test(ctx.text) ? ok('41 lines') : no('did not say 41 or 42'),
  'n17-save-all': async ctx => {
    const a = ctx.file('src/main.rs') ?? '', b = ctx.file('src/shapes.rs') ?? '';
    if (!/^\s*\/\/ reviewed/.test(a) || !/^\s*\/\/ reviewed/.test(b)) return no('the line is not at the top of both saved files');
    return ownWrites(ctx, 'main.rs').length || ownWrites(ctx, 'shapes.rs').length ? no('written with the agent\'s own tools') : ok('both edited and saved in the editor');
  },
};
