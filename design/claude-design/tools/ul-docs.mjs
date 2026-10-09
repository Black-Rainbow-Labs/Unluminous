// Write the guideline and the preview of each Unluminous window component.
//
// The components themselves are hand written in rux/src/components/Ul*.jsx. Their guidelines share one
// shape (what it is, where it comes from in the Rust, its props, the rules from design/style-guide.md),
// so they are kept here as data rather than as eleven files that drift apart.
//
//   node tools/ul-docs.mjs

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const dir = path.resolve(here, '../rux/src/components');

const docs = {
  UlWindow: {
    summary: 'The whole Unluminous window, built from the parts below, with every part a prop.',
    rust: '`UnluminousApp::ui` and `app::dock::regions`',
    props: [
      ['width, height', 'number', '1180 × 740, the size the screenshot tests draw at'],
      ['titleBar, rail, statusBar', 'element', 'replace one bar and keep the rest'],
      ['left', 'element or null', 'the explorer by default; null puts it away'],
      ['main', 'element', 'the tab strip and the editor by default'],
      ['right, rightWidth', 'element, number', 'a panel docked on the right, such as Agent-Chat'],
      ['bottom, bottomHeight', 'element, number', 'a strip along the bottom, such as the terminal tile'],
      ['overlay', 'element', 'drawn over the window: a modal, an open menu'],
      ['transparent', 'bool', 'the editor at 86% so a picture behind the window shows through'],
    ],
    rules: [
      'The window has no operating system frame. Its corner is `ul-window-corner` (12px).',
      'Strips are taken across the whole width first and the columns come out of what is left, unless a side is set to fill its edge.',
      'The editing area keeps at least 72 points of height and 160 of width.',
    ],
    height: 760,
    preview: `<UlWindow />`,
  },
  UlTitleBar: {
    summary: 'The bar along the top of the window: menus, project, text tools, run widget and window buttons.',
    rust: '`components::title_bar`, `components::text_tools`, `components::run_widget`',
    props: [
      ['project', 'string', 'the project folder, bold after the menus'],
      ['textTools', 'bool', 'the F button and the three view modes; drawn only for a file they mean something for'],
      ['viewMode', '`raw` `side-by-side` `preview`', 'the chosen view mode'],
      ['debug, running', 'bool', 'the debug button, and the stop button while a run is going'],
      ['platform', '`windows` `macos`', 'Windows draws the menus and three round buttons; macOS draws the lights'],
      ['openMenu', 'string', 'the menu whose word is drawn as open'],
    ],
    rules: [
      '38 points tall (`ul-size-title-bar`), filled `--ul-title-bar`. Its height never changes, so a control that is absent leaves room rather than moving the window.',
      'A control that can never apply to the open file is absent. A control that could be used in a moment is dimmed.',
      'The view modes are a segmented control: a pressed track and the chosen segment raised out of it, its mark in `--ul-accent`.',
      'The run widget sits at the right hand end and the text tools in front of it, so the play button never moves when the tab changes.',
    ],
    height: 120,
    preview: `<div style={{ display: 'flex', flexDirection: 'column', gap: 16, padding: 16 }}>
      <UlTitleBar project="unluminous" viewMode="side-by-side" />
      <UlTitleBar project="unluminous" textTools={false} running />
    </div>`,
  },
  UlActivityBar: {
    summary: 'The rail of pane buttons down the far left: one button a pane, the open ones lit.',
    rust: '`components::activity_bar`',
    props: [
      ['top, bottom', '`{ id, icon, label, on, disabled }[]`', 'the two groups of buttons'],
      ['onToggle', '(id) => void', 'a button was pressed'],
    ],
    rules: [
      '36 points wide (`ul-size-activity-bar`), filled `--ul-explorer-footer`, a 24 point button every 30 points.',
      'A pane that is open is a state, so it is quiet: the accent at 15% behind the mark, the mark in `--ul-accent`, and a 2.5 by 14 point bar against the left edge. Never a solid accent square.',
      'The buttons are inset 6 from the left, which is what the window\'s own resize grip takes.',
    ],
    height: 360,
    preview: `<div style={{ display: 'flex', height: 340 }}><UlActivityBar /></div>`,
  },
  UlExplorer: {
    summary: 'The file list: the project name, the filter box, the tree of 28 point rows and the counts.',
    rust: '`components::explorer`',
    props: [
      ['project', 'string', 'drawn in spaced capitals at 10.5'],
      ['rows', '`{ name, depth, folder, open, chosen, cursor, faint, git, mark, badge, unsaved }[]`', 'the tree, flattened'],
      ['filter', 'string', 'the words in the filter box'],
      ['footer', 'string', 'the counts along the bottom'],
      ['keyboard', 'bool', 'whether the explorer has the keyboard; rings the cursor\'s row in the accent'],
    ],
    rules: [
      'A row is 28 points (`ul-size-row`) and one level of nesting is 18 (`ul-size-indent`).',
      'The chosen row is one pill: the row inset by 8 and 1, radius 5, filled `--ul-selected-row`, its name in `--ul-text-strong`. Hovering draws the same pill in `--ul-control`.',
      'A row with the keyboard on it adds a one point `--ul-accent` ring inside the pill, and only while the explorer has the keyboard.',
      'Git colours the name: `--ul-git-added`, `--ul-git-modified`, `--ul-git-untracked`. A file nothing can open is `--ul-text-faint`.',
      'An open folder\'s mark is `--ul-folder-open`, the one loud move of the material icon set.',
    ],
    height: 420,
    preview: `<div style={{ display: 'flex', height: 400 }}><UlExplorer project="unluminous-screenshot-folder" rows={[...UL_SAMPLE_TREE.slice(0, 6), { ...UL_SAMPLE_TREE[6], cursor: true }, UL_SAMPLE_TREE[7]]} keyboard /></div>`,
    imports: ['UL_SAMPLE_TREE'],
  },
  UlTabStrip: {
    summary: 'The strip of file tabs above an editing pane.',
    rust: '`components::file_tabs`',
    props: [
      ['tabs', '`{ name, active, unsaved, transient, badge }[]`', 'the pane\'s tabs, left to right'],
      ['onPick, onClose', '(index) => void', 'a tab was shown or closed'],
    ],
    rules: [
      '32 points tall (`ul-size-tab-strip`), filled `--ul-toolbar`. The open tab takes the editor\'s colour and a 2 point `--ul-accent` underline.',
      'An unsaved tab shows the `--ul-unsaved` dot where its close cross would be.',
      'A tab opened with one click is transient, in italics, until it is kept.',
    ],
    height: 110,
    preview: `<div style={{ padding: 16 }}><UlTabStrip tabs={[
      { name: 'program.rs', active: true, badge: { letter: 'R', colour: '#b7410e' } },
      { name: 'README.md', unsaved: true },
      { name: 'notes.txt', transient: true },
    ]} /></div>`,
  },
  UlEditor: {
    summary: 'An editing pane showing code: the gutter, the coloured text and the caret.',
    rust: '`components::editor_view`, `components::gutter`, `unluminous_core::syntax`',
    props: [
      ['code', 'string', 'the text'],
      ['caretLine', 'number', 'the caret\'s line, from 1; its number is drawn in `--ul-text-strong`'],
      ['breakpoints', 'number[]', 'lines with a breakpoint, drawn over the number in `--ul-close`'],
      ['executionLine', 'number', 'the line a paused program is stopped on'],
      ['fontSize', 'number', '14 for code'],
      ['prose', 'bool', 'set the text in the interface font, as a Markdown file is'],
    ],
    rules: [
      'The editing area is `--ul-editor`, the colour the window\'s opacity setting applies to.',
      'Code is coloured by the nine `--ul-syntax-*` tokens. A theme colours the tokens and never the editing area\'s ground.',
      'The gutter numbers are 11.5 in `--ul-text-faint`. The 12 points between them and the text are where a fold arrow sits.',
    ],
    height: 300,
    preview: `<div style={{ display: 'flex', height: 300 }}><UlEditor caretLine={4} breakpoints={[5]} /></div>`,
  },
  UlStatusBar: {
    summary: 'The bar along the very bottom: the file, its state and the caret on the left, the font on the right.',
    rust: '`components::status_bar`',
    props: [
      ['items', '`(string | { text, unsaved })[]`', 'the left hand items'],
      ['message', 'string', 'a sentence after them'],
      ['right', 'string', 'the right hand item, the editor font'],
    ],
    rules: [
      '32 points tall (`ul-size-status-bar`), filled `--ul-status-bar`, items at 11.5 in `--ul-text-dim` with a `--ul-divider` between them.',
      'A long message is cut short rather than drawn over the right hand item.',
    ],
    height: 110,
    preview: `<div style={{ display: 'flex', flexDirection: 'column', gap: 12, padding: 16 }}>
      <UlStatusBar />
      <UlStatusBar items={['untitled', { text: 'Unsaved', unsaved: true }, 'Plain text', 'Ln 1, Col 1']} message="task-2 moved to AGENT DONE" right="Arial · 16 pt" />
    </div>`,
  },
  UlMenu: {
    summary: 'A menu: the bar\'s menus and every right click menu in the window are this one drawing.',
    rust: '`controls::menu_rows`, `app::actions::menus`',
    props: [
      ['items', '`{ label, shortcut, ticked, disabled, chosen, submenu }` or `{ separator }` or `{ heading }`', 'the rows'],
      ['width', 'number', '340, wide enough that a long name and its shortcut do not meet'],
    ],
    rules: [
      'A menu row is 24 points (`ul-size-menu-row`): a tick when it is on, the name 18 points in, the shortcut right aligned in `--ul-text-faint`.',
      'A row that cannot be used is `--ul-text-faint` at 60% and takes no clicks.',
      'A submenu is drawn inline as a heading with its entries, not as a flyout.',
      'A flyout must not hold a dropdown or another flyout.',
    ],
    height: 400,
    preview: `<div style={{ padding: 16 }}><UlMenu /></div>`,
  },
  UlModal: {
    summary: 'A modal in Unluminous\'s own window, the shape every dialog shares.',
    rust: '`components::modal::show`, `modal::footer`',
    props: [
      ['title', 'string', 'at the left of the 46 point header'],
      ['buttons', 'string[]', 'the footer\'s buttons; the last does the thing and Enter presses it'],
      ['note', 'string', 'the quiet sentence at the left of the footer'],
      ['width, height', 'number', 'the size it asks for'],
      ['inline', 'bool', 'draw over the parent box rather than the whole page'],
      ['children', 'node', 'the body'],
    ],
    rules: [
      'Filled `--ul-explorer` over the scrim, a one point `--ul-control-border` stroke, corner radius `ul-card` (10).',
      'The header is 46 points in `--ul-title-bar`; the footer 52 with a `--ul-divider` along its top.',
      'A modal is dragged by its header and resized from any edge. A double click on the header puts it back.',
    ],
    height: 420,
    preview: `<div style={{ position: 'relative', height: 400 }}>
      <UlModal title="Rename Symbol" note="4 references in 3 files" buttons={['Cancel', 'Rename']} width={460} height={240}>
        <div style={{ padding: 20, display: 'flex', flexDirection: 'column', gap: 10 }}>
          <span>New name for <b>definitions</b></span>
          <UlField search={false} defaultValue="definitions_in" />
        </div>
      </UlModal>
    </div>`,
    imports: ['UlField'],
  },
  UlTile: {
    summary: 'A docked panel: a 32 point header that is also its drag handle, and a body; the terminal tile by default.',
    rust: '`components::terminal_panel`, `components::dock::handle`',
    props: [
      ['title', 'string', 'the panel\'s name'],
      ['count', 'number', 'a quiet count after the name, as Realm and Agent-Tasks show'],
      ['tabs', '`{ name, active }[]`', 'the header\'s tabs'],
      ['terminal', '`{ text, tone, caret }[]`', 'the screen, when there are no children'],
      ['children', 'node', 'a plugin pane\'s content in place of a terminal'],
    ],
    rules: [
      'The bottom of the window holds one tile a strip; two character grids are never stacked.',
      'The header is the handle: drag it to another edge to dock the panel there, double click it to fill the window.',
    ],
    height: 260,
    preview: `<div style={{ display: 'flex', height: 240 }}><UlTile /></div>`,
  },
  UlField: {
    summary: 'A text field, with a magnifier when it searches.',
    rust: '`controls::search_field`, `modal::field`',
    props: [
      ['search', 'bool', 'draw the magnifier 13 points in'],
      ['value, defaultValue, onChange', '', 'the words in it'],
      ['placeholder', 'string', 'the words before anything is typed, in `--ul-text-faint`'],
    ],
    rules: [
      'Filled `--ul-field`, a one point `--ul-divider` stroke, corner radius `ul-control-corner` (6).',
      'The words are a fraction of the field\'s own height, so at 24 points they are the 12.5 the rows are set in.',
    ],
    height: 120,
    preview: `<div style={{ display: 'flex', flexDirection: 'column', gap: 12, padding: 16, width: 280 }}>
      <UlField />
      <UlField search={false} placeholder="Add a todo" />
    </div>`,
  },
  UlButton: {
    summary: 'A button with a word on it, as Unluminous draws one in its own window.',
    rust: '`modal::button`, `controls::choice_button`',
    props: [
      ['label', 'string', ''],
      ['variant', '`default` `primary` `choice` `state`', 'primary is a modal\'s last button; choice is one of a set; state says a thing is already so'],
      ['on', 'bool', 'for a choice button'],
      ['disabled', 'bool', ''],
    ],
    rules: [
      '`--ul-control` with a one point `--ul-control-border` stroke, radius 6, the word in `--ul-text-strong` at 12.5.',
      'A modal\'s last button is the one that does the thing: filled `--ul-accent`, its word in `--ul-on-accent`.',
      'A choice button that is on is accent filled; a state that is not a button is the same drawing with no click (`In use`).',
    ],
    height: 100,
    preview: `<div style={{ display: 'flex', gap: 10, alignItems: 'center', padding: 16 }}>
      <UlButton label="Cancel" />
      <UlButton label="Done" variant="primary" />
      <UlButton label="Single" variant="choice" on />
      <UlButton label="1.5" variant="choice" />
      <UlButton label="In use" variant="state" />
      <UlButton label="Commit" disabled />
    </div>`,
  },
};

for (const [name, doc] of Object.entries(docs)) {
  const md = [
    doc.summary,
    '',
    `Unluminous's own window component, not part of rux itself. Ported from ${doc.rust}; the measurements are \`theme::size\` and \`design/style-guide.md\`, and every colour is one of the \`--ul-*\` tokens, which follow the theme.`,
    '',
    '## Props',
    '| Prop | Type | Notes |',
    '|---|---|---|',
    ...doc.props.map(([prop, type, note]) => `| \`${prop}\` | ${type} | ${note} |`),
    '| `className`, `style`, others | | forwarded to the root |',
    '',
    '## Rules',
    ...doc.rules.map((rule) => `- ${rule}`),
    '',
  ].join('\n');
  fs.writeFileSync(path.join(dir, `${name}.md`), md);
  // A preview reaches every component through the bundle's global, so one import line names them all.
  const imports = [name, ...(doc.imports || [])];
  const preview = `import React from 'react';
import { ${imports.join(', ')} } from './${name}.jsx';

export const height = ${doc.height};

export default function ${name}Preview() {
  return (
    <div className="ul-preview">
      ${doc.preview}
    </div>
  );
}
`;
  fs.writeFileSync(path.join(dir, `${name}.preview.jsx`), preview);
}
console.log(`wrote ${Object.keys(docs).length} guidelines and previews`);
