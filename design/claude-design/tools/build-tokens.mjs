// Write the design system's tokens.json and tokens.css from the values rux and Unluminous ship.
//
// Four themes:
//   light            rux light-neumorphic, transcribed from reference/neumorphic-tokens.css
//   dark             rux dark-neumorphic, reference/incognito-theme.css over the same file
//   unluminous-dark  rux dark retoned from Unluminous Dark the way theme::rux_theme does it
//   unluminous-light rux light retoned from Unluminous Light the same way
//
// Every number below is copied from crates/rux/src/theme/mod.rs (at 50b693f) or from
// crates/unluminous-app/src/theme/mod.rs. The retone is computed here by the same arithmetic the Rust
// uses, so a palette change in Unluminous is one edit here and one re-run.
//
//   node tools/build-tokens.mjs

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');

// ---- colour arithmetic, matching the Rust ------------------------------------------------------

/** @param hex - "#rrggbb" */
const rgbOf = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
/** @param rgb - three channels 0..255 */
const hexOf = (rgb) => '#' + rgb.map((c) => Math.round(c).toString(16).padStart(2, '0')).join('');
/**
 * `toward` in theme::retoned_rux: move each channel `amount` of the way from `from` to `to`.
 * @param from - the starting colour
 * @param to - the colour moved towards
 * @param amount - 0..1
 */
const toward = (from, to, amount) =>
  hexOf(rgbOf(from).map((a, i) => Math.round(a + (rgbOf(to)[i] - a) * amount)));
/**
 * A colour at an opacity, written the way the reference writes it.
 * @param hex - "#rrggbb"
 * @param opacity - 0..1
 */
const rgba = (hex, opacity) => `rgba(${rgbOf(hex).join(', ')}, ${opacity})`;

// ---- rux's two themes ------------------------------------------------------------------------

const ruxLight = {
  surface: { s0: '#e4e9f0', s1: '#eceff3', s2: '#f1f4f8', s3: '#f7f9fc', sunken: '#dce1e9' },
  shadow: { light: '#ffffff', dark: '#b8bfcc', darker: '#a8b0be' },
  ink: { 900: '#1e2530', 700: '#3a4454', 500: '#6b7686', 400: '#8a94a3', 300: '#aab2bf', 200: '#c7cdd7' },
  accent: {
    blue: '#2f6bff', 'blue-deep': '#1d4fdb', 'blue-soft': '#6f96ff', coral: '#ff6b5b',
    violet: '#8b6bff', mint: '#2fcfa6', amber: '#ffb648', rose: '#ff4f7a',
  },
  semantic: { success: '#2fb67c', warning: '#ffb648', danger: '#f25f5c' },
  interaction: {
    hover: 'rgba(0, 0, 0, 0.025)', pressed: 'rgba(0, 0, 0, 0.055)', hairline: 'rgba(0, 0, 0, 0.04)',
    scrim: 'rgba(20, 25, 36, 0.55)',
  },
  gradient: {
    primary: ['#3a78ff', '#1d4fdb'], 'primary-diagonal': ['#4a82ff', '#1d4fdb'],
    danger: ['#ff7a78', '#e2433f'], 'danger-diagonal': ['#ff8f82', '#e2433f'],
    mint: ['#3ad9af', '#1fa37e'], violet: ['#8b6bff', '#6d4fd4'], 'nav-active': ['#2a3140', '#0f131c'],
    knob: ['#ffffff', '#e3e8ef'], avatar: ['#a88cff', '#6b49e8'], 'image-well': ['#1a202d', '#0c1018'],
    progress: ['#2f6bff', '#6f96ff'], multistop: ['#2f6bff', '#8b6bff'],
  },
};

const ruxDark = {
  surface: { s0: '#1c1f25', s1: '#23272e', s2: '#282d35', s3: '#2f353e', sunken: '#191c21' },
  shadow: { light: '#2f353e', dark: '#14171b', darker: '#0c0e11' },
  ink: { 900: '#edf0f5', 700: '#c9d0da', 500: '#939dac', 400: '#79828f', 300: '#5c6470', 200: '#3c434e' },
  accent: {
    blue: '#4c86ff', 'blue-deep': '#2a5fe6', 'blue-soft': '#7fa3ff', coral: '#ff7a6b',
    violet: '#9b80ff', mint: '#34dbb0', amber: '#ffc062', rose: '#ff6791',
  },
  semantic: { success: '#36c98a', warning: '#ffc062', danger: '#ff6b68' },
  interaction: {
    hover: 'rgba(255, 255, 255, 0.04)', pressed: 'rgba(255, 255, 255, 0.08)',
    hairline: 'rgba(255, 255, 255, 0.08)', scrim: 'rgba(8, 10, 14, 0.55)',
  },
  gradient: {
    primary: ['#4c86ff', '#2a5fe6'], 'primary-diagonal': ['#5a90ff', '#2a5fe6'],
    danger: ['#ff8a88', '#e0413e'], 'danger-diagonal': ['#ff8f82', '#e2433f'],
    mint: ['#3ad9af', '#1fa37e'], violet: ['#9b80ff', '#6d4fd4'], 'nav-active': ['#3a78ff', '#1d4fdb'],
    knob: ['#363c46', '#21252c'], avatar: ['#a88cff', '#6b49e8'], 'image-well': ['#1a202d', '#0c1018'],
    progress: ['#4c86ff', '#7fa3ff'], multistop: ['#4c86ff', '#9b80ff'],
  },
};

// ---- Unluminous's two palettes (theme/mod.rs, the palette! list and UNLUMINOUS_LIGHT) ------------

const ulDark = {
  editor: '#1a1f26', 'title-bar': '#2a313d', toolbar: '#1e222a', explorer: '#1f232a',
  'explorer-footer': '#1c2026', 'status-bar': '#101519', control: '#353b46', field: '#1d212a',
  'control-border': '#383f4b', divider: '#2a303b', menu: '#262c36', accent: '#489ff8',
  'selected-row': '#304361', unsaved: '#febc2e', 'text-selection': '#304361', 'find-match': '#4a432b',
  'code-panel': '#232933', 'code-chip': '#282f3a', 'text-strong': '#ffffff', text: '#e8ebf1',
  'text-control': '#c8cedb', 'text-dim': '#8b93a3', 'text-faint': '#78808f', 'file-markdown': '#418cd9',
  'file-text': '#7e8795', 'blame-old': '#3c7d64', 'blame-new': '#b4588c', 'git-added': '#7fca98',
  'git-modified': '#4d9dc3', 'git-untracked': '#9a8c5a', 'board-accent': '#4c6ef5', agent: '#9b7cf6',
  close: '#ff5f57', minimise: '#febc2e', maximise: '#28c840', icon: '#8b93a3', 'icon-active': '#ffffff',
  'icon-disabled': '#78808f', folder: '#8b93a3', 'folder-open': '#489ff8', 'hover-wash': '#ffffff',
  'on-accent': '#ffffff', failure: '#f07178',
};
const ulLight = {
  editor: '#fbfcfd', 'title-bar': '#e6eaf0', toolbar: '#f1f3f6', explorer: '#f1f3f7',
  'explorer-footer': '#e9ecf1', 'status-bar': '#e1e5eb', control: '#ffffff', field: '#ffffff',
  'control-border': '#c3cad5', divider: '#d5dae2', menu: '#f8f9fb', accent: '#2a63f0',
  'selected-row': '#d6e3ff', unsaved: '#d99200', 'text-selection': '#c6d8ff', 'find-match': '#f8e39c',
  'code-panel': '#f0f2f6', 'code-chip': '#e8ebf0', 'text-strong': '#10151c', text: '#1e2530',
  'text-control': '#343d4b', 'text-dim': '#5a6575', 'text-faint': '#747e8d', 'file-markdown': '#2b78d6',
  'file-text': '#8a94a3', 'blame-old': '#3c8c6c', 'blame-new': '#b44e8a', 'git-added': '#1c8448',
  'git-modified': '#1f6cb5', 'git-untracked': '#8a6a1c', 'board-accent': '#4c6ef5', agent: '#7c5ce6',
  close: '#ff5f57', minimise: '#febc2e', maximise: '#28c840', icon: '#5a6575', 'icon-active': '#10151c',
  'icon-disabled': '#a3abb7', folder: '#6b7686', 'folder-open': '#2a63f0', 'hover-wash': '#000000',
  'on-accent': '#ffffff', failure: '#c42b35',
};
// Unluminous Dark names no code colours, so each language plugin's own Dracula applies; Unluminous
// Light names One Light's nine (theme::Theme::unluminous_light).
const syntaxDark = {
  keyword: '#ff79c6', builtin: '#bd93f9', function: '#50fa7b', type: '#8be9fd', string: '#f1fa8c',
  number: '#ffb86c', comment: '#6272a4', operator: '#ff79c6', text: '#f8f8f2',
};
const syntaxLight = {
  keyword: '#a626a4', builtin: '#0174a8', function: '#3a6ce0', type: '#986400', string: '#2f7a2e',
  number: '#986801', comment: '#6e737d', operator: '#0e7c86', text: '#2c313a',
};

/**
 * theme::retoned_rux: rux's theme of the same darkness with the surfaces, ink and accent of a palette.
 * @param base - ruxDark or ruxLight
 * @param ul - the Unluminous palette
 * @param dark - whether the palette is a dark one
 */
function retone(base, ul, dark) {
  // derived::board_surfaces
  const [page, lane, card, well] = dark
    ? [ul.editor, ul.explorer, ul['code-panel'], ul.field]
    : [ul.explorer, ul['explorer-footer'], ul.control, ul['code-chip']];
  const raised = dark ? toward(card, '#ffffff', 0.04) : card;
  const shadow = dark
    ? { light: toward(card, '#ffffff', 0.06), dark: toward(page, '#000000', 0.45), darker: toward(page, '#000000', 0.65) }
    : base.shadow;
  return {
    ...base,
    surface: { s0: page, s1: lane, s2: card, s3: raised, sunken: well },
    shadow,
    ink: {
      900: ul['text-strong'], 700: ul.text, 500: ul['text-control'], 400: ul['text-dim'],
      300: ul['text-faint'], 200: toward(ul['text-faint'], lane, 0.5),
    },
    accent: { ...base.accent, blue: ul.accent },
    // Theme::retoned writes the washes at 0.06 and 0.04 whatever the darkness.
    washes: [0.06, 0.04],
  };
}

const themes = [
  { id: 'light', name: 'Light Neumorphic', rux: ruxLight, ul: ulLight, syntax: syntaxLight, washes: [0.06, 0.04] },
  { id: 'dark', name: 'Dark Neumorphic', rux: ruxDark, ul: ulDark, syntax: syntaxDark, washes: [0.1, 0.07] },
  { id: 'unluminous-dark', name: 'Unluminous Dark', rux: retone(ruxDark, ulDark, true), ul: ulDark, syntax: syntaxDark },
  { id: 'unluminous-light', name: 'Unluminous Light', rux: retone(ruxLight, ulLight, false), ul: ulLight, syntax: syntaxLight },
];
for (const theme of themes) theme.washes = theme.washes ?? theme.rux.washes;

// ---- the elevations, theme::elevations ---------------------------------------------------------

/**
 * Every named elevation for one theme, as complete box-shadow values.
 * @param t - a theme record
 */
function elevationsOf(t) {
  const s = t.rux.shadow;
  const primaryTo = t.rux.gradient.primary[1];
  const navTo = t.rux.gradient['nav-active'][1];
  return {
    'e-raised-sm': `-3px -3px 6px ${s.light}, 3px 3px 6px ${s.dark}`,
    'e-raised': `-6px -6px 14px ${s.light}, 6px 6px 14px ${s.dark}`,
    'e-raised-lg': `-10px -10px 24px ${s.light}, 12px 12px 24px ${s.dark}`,
    'e-pressed-sm': `inset 2px 2px 4px ${s.dark}, inset -2px -2px 4px ${s.light}`,
    'e-pressed': `inset 4px 4px 8px ${s.dark}, inset -4px -4px 8px ${s.light}`,
    'e-pressed-deep': `inset 6px 6px 12px ${s.darker}, inset -6px -6px 12px ${s.light}`,
    'e-modal': 'inset 0 1px 0 rgba(255, 255, 255, 0.7), 0 1px 2px rgba(15, 22, 38, 0.06), 0 6px 16px rgba(15, 22, 38, 0.12), 0 18px 40px rgba(15, 22, 38, 0.2), 0 40px 80px rgba(15, 22, 38, 0.16)',
    'e-primary': `-4px -4px 10px ${s.light}, 4px 4px 14px ${rgba(primaryTo, 0.35)}, inset 1px 1px 1px rgba(255, 255, 255, 0.28), inset -1px -1px 2px rgba(0, 0, 0, 0.15)`,
    'e-primary-pressed': `inset 3px 3px 8px rgba(0, 0, 0, 0.25), inset -2px -2px 4px ${rgba(t.rux.accent['blue-soft'], 0.3)}`,
    'e-primary-sm': `-3px -3px 6px ${s.light}, 4px 4px 10px ${rgba(primaryTo, 0.35)}, inset 1px 1px 1px rgba(255, 255, 255, 0.25)`,
    'e-nav-active': `-2px -2px 5px ${s.light}, 3px 3px 8px ${rgba(navTo, 0.35)}, inset 1px 1px 2px rgba(255, 255, 255, 0.08)`,
    'e-image-well': `inset 4px 4px 10px rgba(0, 0, 0, 0.35), inset -3px -3px 8px rgba(255, 255, 255, 0.02), -3px -3px 8px ${s.light}, 3px 3px 10px ${s.dark}`,
    // select.rs opens its menu at --e-raised-lg; Unluminous asks for the smallest raised shadow
    // instead (menu_elevation in components/agent_chat), because the large one spread a dark blur over
    // the messages under it (task-2200).
    'e-menu': t.id.startsWith('unluminous')
      ? `-3px -3px 6px ${s.light}, 3px 3px 6px ${s.dark}`
      : `-10px -10px 24px ${s.light}, 12px 12px 24px ${s.dark}`,
  };
}

// ---- tokens.json ---------------------------------------------------------------------------------

const colorUsage = {
  'surface-0': 'The deepest panel base and the canvas backdrop; a recessed area of the page.',
  'surface-1': 'The page itself: the neumorphic canvas every raised and pressed surface is lit against.',
  'surface-2': 'A surface one step raised: cards, panels, a tile.',
  'surface-3': 'The most raised surface: modals, popovers, menus.',
  'surface-sunken': 'Recessed wells, slider tracks, the hairline between two sections.',
  'shadow-light': 'The lit half of every elevation: white on a light ground, a lifted grey on a dark one.',
  'shadow-dark': 'The shaded half of every elevation.',
  'shadow-darker': 'The shaded half of the deepest pressed elevation.',
  'ink-900': 'Headings and the strongest words; reads on every surface.',
  'ink-700': 'Body text and the words on a secondary button.',
  'ink-500': 'Secondary text, captions and kickers.',
  'ink-400': 'Placeholder text and quiet marks.',
  'ink-300': 'Disabled words and faint marks. Below 4.5:1 on surface-1 in both neumorphic themes, as in the reference: never body text.',
  'ink-200': 'Hairline marks and the faintest decoration; not for words.',
  'accent-blue': 'The one action colour: a hovered secondary control, a link, the focus ring, an active mark.',
  'accent-blue-deep': 'The deep end of the primary gradient and the glow under a primary button.',
  'accent-blue-soft': 'The soft blue a pressed primary button lights from inside.',
  'accent-coral': 'Danger ink on a secondary button, and a meter past its limit.',
  'accent-violet': 'The dice toggle when it is on.',
  'accent-mint': 'Save, and anything that finished well.',
  'accent-amber': 'A warning lamp, and an unsaved change.',
  'accent-rose': 'A second warm accent for charts.',
  success: 'Success state words and lamps.',
  warning: 'Warning state words and lamps.',
  danger: 'Error state words and lamps.',
  hover: 'A barely there wash over a row or ghost control under the pointer.',
  pressed: 'The same wash one step stronger, while held.',
  hairline: 'A one pixel rule between sections.',
  'accent-wash': 'An accent tinted wash on the menu row that would be chosen.',
  'accent-wash-soft': 'The quieter accent wash on the row that already is chosen.',
  scrim: 'What a modal dims the page behind it with.',
  'focus-ring': 'The ring round whatever has the keyboard: the accent at 65%, as rux draws it. Not in the reference, which has no focus styling.',
  'on-accent': 'Words and marks on a gradient fill: the primary, mint and danger buttons.',
};

const color = { themes: themes.map(({ id, name }) => ({ id, name })), tokens: [] };
/**
 * Add one colour token with a value per theme.
 * @param name - the token name
 * @param pick - a function from a theme record to its value
 * @param usage - where the token is used
 */
const addColor = (name, pick, usage) =>
  color.tokens.push({ name, value: Object.fromEntries(themes.map((t) => [t.id, pick(t)])), usage });

for (const k of ['s0', 's1', 's2', 's3', 'sunken']) {
  const name = k === 'sunken' ? 'surface-sunken' : `surface-${k.slice(1)}`;
  addColor(name, (t) => t.rux.surface[k], colorUsage[name]);
}
for (const k of ['light', 'dark', 'darker']) addColor(`shadow-${k}`, (t) => t.rux.shadow[k], colorUsage[`shadow-${k}`]);
for (const k of [900, 700, 500, 400, 300, 200]) addColor(`ink-${k}`, (t) => t.rux.ink[k], colorUsage[`ink-${k}`]);
for (const k of Object.keys(ruxLight.accent)) addColor(`accent-${k}`, (t) => t.rux.accent[k], colorUsage[`accent-${k}`]);
for (const k of ['success', 'warning', 'danger']) addColor(k, (t) => t.rux.semantic[k], colorUsage[k]);
for (const k of ['hover', 'pressed', 'hairline']) addColor(k, (t) => t.rux.interaction[k], colorUsage[k]);
addColor('accent-wash', (t) => rgba(t.rux.accent.blue, t.washes[0]), colorUsage['accent-wash']);
addColor('accent-wash-soft', (t) => rgba(t.rux.accent.blue, t.washes[1]), colorUsage['accent-wash-soft']);
addColor('scrim', (t) => t.rux.interaction.scrim, colorUsage.scrim);
addColor('focus-ring', (t) => rgba(t.rux.accent.blue, 0.65), colorUsage['focus-ring']);
addColor('on-accent', () => '#ffffff', colorUsage['on-accent']);
for (const k of Object.keys(ruxLight.gradient)) {
  addColor(`grad-${k}-from`, (t) => t.rux.gradient[k][0], `The start of the ${k} gradient.`);
  addColor(`grad-${k}-to`, (t) => t.rux.gradient[k][1], `The end of the ${k} gradient.`);
}

const ulUsage = {
  editor: 'Behind the text. The window opacity setting applies its alpha to this.',
  'title-bar': 'The bar with the menus, the run widget and the window buttons; a modal header.',
  toolbar: 'The file tab strip and the terminal tile.',
  explorer: 'The file list; a modal body and its list column.',
  'explorer-footer': 'The strip counting the files under the explorer.',
  'status-bar': 'The bar along the very bottom of the window.',
  control: 'Inside a button or a dropdown that is not active; a hovered row.',
  field: 'Inside anything typed into.',
  'control-border': 'The line round a control, a menu and a modal.',
  divider: 'Between two panels, and under a section heading.',
  menu: 'Behind a menu, darker than a control so a control on it stands out.',
  accent: 'Anything switched on: the caret, an active button, the underline on the open tab.',
  'selected-row': 'The pill behind the chosen row in any list.',
  unsaved: 'The dot meaning there are changes not written to disk.',
  'text-selection': 'Behind selected text.',
  'find-match': 'Behind every Find match that is not the current one.',
  'code-panel': 'Behind a code block, a table and front matter in the Markdown preview.',
  'code-chip': 'Behind one piece of inline code.',
  'text-strong': 'Headings and the file name in the title bar.',
  text: 'Ordinary text in the editor.',
  'text-control': 'A label on a control and a name in the file list.',
  'text-dim': 'A heading in the explorer, its footer counts and the status bar.',
  'text-faint': 'Placeholder words in an empty field, a shortcut in a menu.',
  'file-markdown': 'The mark in front of a Markdown file.',
  'file-text': 'The mark in front of a plain text file.',
  'blame-old': 'The oldest commit in the blame column.',
  'blame-new': 'The newest commit in the blame column.',
  'git-added': 'A file or line git does not have yet; also the green that starts a run.',
  'git-modified': 'A file or line that differs from git.',
  'git-untracked': 'A file git is not tracking.',
  'board-accent': 'The Agent-Tasks board\'s own periwinkle, used only on the board.',
  agent: 'The colour an agent wears on the board.',
  close: 'The close window button; also the red that means something went wrong.',
  minimise: 'The minimise window button.',
  maximise: 'The maximise window button.',
  icon: 'A drawn icon at rest.',
  'icon-active': 'An icon whose pane is open or whose button is on.',
  'icon-disabled': 'An icon that cannot be used.',
  folder: 'A folder\'s arrow and mark.',
  'folder-open': 'An open folder\'s mark.',
  'hover-wash': 'The wash painted over a control on hover, used at a low strength (7%).',
  'on-accent': 'Words on an accent fill in Unluminous.',
  failure: 'A notebook cell that raised.',
};
for (const [k, usage] of Object.entries(ulUsage)) addColor(`ul-${k}`, (t) => t.ul[k], `Unluminous: ${usage}`);
for (const k of Object.keys(syntaxDark)) {
  addColor(`ul-syntax-${k}`, (t) => t.syntax[k], `Unluminous code colour for ${k} tokens (Dracula in the dark themes, One Light in the light ones).`);
}
const highlights = { yellow: '#febc2e', green: '#7fca98', blue: '#489ff8', pink: '#b4588c' };
for (const [k, hex] of Object.entries(highlights)) {
  addColor(`ul-highlight-${k}`, () => rgba(hex, 0.35), `Unluminous: a passage marked ${k}. The same in every theme, because a mark keeps the colour it was made in.`);
}

const shadow = {
  note: 'Complete box-shadow values. Light comes from the top left. Only a press changes elevation; a hover never does.',
  tokens: Object.keys(elevationsOf(themes[0])).map((name) => ({
    name,
    value: Object.fromEntries(themes.map((t) => [t.id, elevationsOf(t)[name]])),
    usage: {
      'e-raised-sm': 'Small controls: a secondary button, a chip, a select trigger.',
      'e-raised': 'A raised button for the one action a section is about, a panel.',
      'e-raised-lg': 'The largest raised surfaces: a page card, the nav bar.',
      'e-pressed-sm': 'A small control held down; a small well.',
      'e-pressed': 'A field, a text area, a segmented track.',
      'e-pressed-deep': 'The deepest well: a prompt box, a canvas.',
      'e-modal': 'A modal floating over the scrim.',
      'e-primary': 'The large primary button.',
      'e-primary-pressed': 'The primary button held down.',
      'e-primary-sm': 'A smaller primary button.',
      'e-nav-active': 'The chosen top nav item.',
      'e-image-well': 'A dark picture well, pressed in and raised off the page.',
      'e-menu': 'An open dropdown menu.',
    }[name],
  })),
};

const radius = {
  tokens: [
    ['r-sm', '8px', 'Small wells, chips inside a field, a menu row.'],
    ['r-md', '12px', 'Fields, selects, the dashed add button, a tile.'],
    ['r-lg', '18px', 'Panels and cards.'],
    ['r-xl', '24px', 'Large cards and the modal.'],
    ['r-2xl', '32px', 'The largest page cards.'],
    ['r-pill', '999px', 'Buttons, the nav bar, a switch: every pill.'],
    ['ul-window-corner', '12px', 'Unluminous: the window\'s own rounded corner.'],
    ['ul-control-corner', '6px', 'Unluminous: a control, a field, a menu.'],
    ['ul-row-pill', '5px', 'Unluminous: the pill behind a chosen or hovered row.'],
    ['ul-card', '10px', 'Unluminous: a modal and a card in a list.'],
  ].map(([name, value, usage]) => ({ name, value, usage })),
};

const spacing = {
  note: 'rux lays out with each rule\'s own padding and gap rather than a spacing scale; these are Unluminous\'s closed measurements from theme::size.',
  tokens: [
    ['ul-size-title-bar', '38px', 'Height of the title bar.'],
    ['ul-size-activity-bar', '36px', 'Width of the rail of pane buttons (a 24px button, 6px either side).'],
    ['ul-size-status-bar', '32px', 'Height of the status bar.'],
    ['ul-size-explorer', '248px', 'Width of the explorer (150 to 620 by dragging).'],
    ['ul-size-explorer-footer', '28px', 'Height of the strip counting the files.'],
    ['ul-size-row', '28px', 'One row in any list.'],
    ['ul-size-menu-row', '24px', 'One row in a menu.'],
    ['ul-size-tab-strip', '32px', 'Height of a file tab strip and a tile header.'],
    ['ul-size-indent', '18px', 'One level of nesting in a tree.'],
    ['ul-size-editor-padding-x', '43px', 'Between the text and the left edge of the editing area.'],
    ['ul-size-editor-padding-y', '36px', 'Between the text and the top of the editing area.'],
  ].map(([name, value, usage]) => ({ name, value, usage })),
};

const fontFiles = [
  ['Inter', 'Inter-Regular.ttf', '400'], ['Inter', 'Inter-Medium.ttf', '500'],
  ['Inter', 'Inter-SemiBold.ttf', '600'], ['Inter', 'Inter-Bold.ttf', '700'],
  ['JetBrains Mono', 'JetBrainsMono-Regular.ttf', '400'], ['JetBrains Mono', 'JetBrainsMono-Medium.ttf', '500'],
  ['JetBrains Mono', 'JetBrainsMono-SemiBold.ttf', '600'],
];
/**
 * One type style record.
 * @param name - the style name
 * @param fontSize - px
 * @param fontWeight - 400..700
 * @param extra - lineHeight, letterSpacing, family, usage and sample
 */
const style = (name, fontSize, fontWeight, extra = {}) => ({
  name: name.startsWith('ul-') ? `ul-t-${name.slice(3)}` : `rux-t-${name}`,
  fontSize: `${fontSize}px`,
  fontWeight,
  ...extra,
});
const type = {
  fonts: fontFiles.map(([family, file, weight]) => ({ family, file: `fonts/${file}`, weight, style: 'normal' })),
  families: {
    sans: '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    mono: '"JetBrains Mono", ui-monospace, "SF Mono", Menlo, monospace',
    ui: '"Segoe UI", -apple-system, BlinkMacSystemFont, "Helvetica Neue", Helvetica, Arial, sans-serif',
    code: 'Consolas, Menlo, "JetBrains Mono", ui-monospace, monospace',
  },
  groups: [
    {
      name: 'rux display', family: 'sans', styles: [
        style('display', 48, 600, { lineHeight: 1.05, letterSpacing: '-0.03em', usage: 'One headline a page.', sample: 'Current Sprint' }),
        style('h1', 32, 600, { lineHeight: 1.2, letterSpacing: '-0.02em', usage: 'A page heading.', sample: 'Image Creator' }),
        style('h2', 24, 600, { lineHeight: 1.3, letterSpacing: '-0.015em', usage: 'A section heading.', sample: 'Release build time' }),
        style('h3', 18, 600, { lineHeight: 1.35, letterSpacing: '-0.01em', usage: 'A card or modal heading.', sample: 'Plugin architecture for UI' }),
      ],
    },
    {
      name: 'rux text', family: 'sans', styles: [
        style('body', 15, 400, { lineHeight: 1.5, usage: 'Running text.' }),
        style('base', 14, 400, { lineHeight: 1.5, letterSpacing: '-0.005em', usage: 'What the page inherits.' }),
        style('label', 13, 500, { lineHeight: 1.4, usage: 'A label beside a control.' }),
        style('prose', 13, 400, { lineHeight: 1.6, usage: 'Running text in a well: a prompt, a text area.' }),
        style('control', 12, 500, { letterSpacing: '-0.005em', usage: 'The words on a control: a nav item, a select, a small button.' }),
        style('control-lg', 13, 500, { letterSpacing: '-0.01em', usage: 'The words on a larger control and a list row.' }),
        style('button', 14, 600, { letterSpacing: '-0.005em', usage: 'The word on a primary button.' }),
      ],
    },
    {
      name: 'rux mono', family: 'mono', styles: [
        style('caption', 11, 500, { lineHeight: 1.4, letterSpacing: '0.14em', usage: 'The uppercase kicker over a section heading.', sample: 'CONTROLS' }),
        style('panel-title', 10.5, 500, { letterSpacing: '0.16em', usage: 'A panel\'s own uppercase heading.', sample: 'PROMPT' }),
        style('field-label', 9.5, 400, { letterSpacing: '0.14em', usage: 'The uppercase label over a control.', sample: 'GENRE' }),
        style('ctrl-label', 9, 400, { letterSpacing: '0.14em', usage: 'The same in a two column grid.', sample: 'INDOOR' }),
        style('number', 12, 600, { usage: 'A number: a count, an elapsed time.', sample: '1920 × 1080' }),
      ],
    },
    {
      name: 'Unluminous interface', family: 'ui', styles: [
        style('ul-modal-breadcrumb', 13.5, 400, { usage: 'Unluminous: a modal\'s breadcrumb.' }),
        style('ul-modal-title', 13, 400, { usage: 'Unluminous: a modal\'s title.' }),
        style('ul-ordinary', 12.5, 400, { usage: 'Unluminous: a menu row, a list row, a control label, a section heading.', sample: 'program.rs' }),
        style('ul-tab', 12, 400, { usage: 'Unluminous: a terminal tab and a tile heading.' }),
        style('ul-hint', 11.5, 400, { usage: 'Unluminous: a shortcut in a menu, a line of explanation.' }),
        style('ul-menu-heading', 11, 400, { usage: 'Unluminous: a heading inside a menu.' }),
        style('ul-explorer-heading', 10.5, 400, { letterSpacing: '0.2em', usage: 'Unluminous: the explorer\'s uppercase heading and its footer counts.', sample: 'UNLUMINOUS' }),
      ],
    },
    {
      name: 'Unluminous editor', family: 'code', styles: [
        style('ul-code', 14, 400, { lineHeight: 1.45, usage: 'Unluminous: code in the editor, set in the machine\'s monospace (Consolas, Menlo).', sample: 'fn main() {}' }),
        style('ul-gutter', 11.5, 400, { usage: 'Unluminous: line numbers in the gutter.', sample: '42' }),
      ],
    },
  ],
};

const tokens = {
  name: 'Black Rainbow Labs Rux',
  version: 1,
  meta: {
    source: 'github', repo: 'Black-Rainbow-Labs/black-rainbow-labs-rux', ref: 'main@50b693f',
    paths: { tokens: ['crates/rux/src/theme/mod.rs', 'reference/neumorphic-tokens.css', 'reference/incognito-theme.css'], fonts: ['crates/rux/assets/fonts'], assets: ['crates/rux/src/icon/paths.rs'] },
    unluminous: { repo: 'jasonmcaffee/unluminous', paths: ['crates/unluminous-app/src/theme/mod.rs'] },
    synced: new Date().toISOString().slice(0, 10),
  },
  color,
  type,
  spacing,
  radius,
  shadow,
};

const out = path.join(root, 'rux/system/project');
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, 'tokens.json'), JSON.stringify(tokens, null, 1) + '\n');

// ---- tokens.css, laid out as the design system compiles it -----------------------------------

/**
 * The declarations for one theme.
 * @param id - the theme id
 */
function themeBlock(id) {
  const lines = color.tokens.map((t) => `  --${t.name}: ${t.value[id]};`);
  for (const t of shadow.tokens) lines.push(`  --${t.name}: ${t.value[id]};`);
  return lines.join('\n');
}
const fontFaces = type.fonts
  .map((f) => `@font-face { font-family: "${f.family}"; src: url("../system/project/${f.file}") format("truetype"); font-weight: ${f.weight}; font-style: normal; font-display: swap; }`)
  .join('\n');
const scalars = [...spacing.tokens, ...radius.tokens].map((t) => `  --${t.name}: ${t.value};`);
for (const [key, stack] of Object.entries(type.families)) scalars.push(`  --font-${key}: ${stack};`);
const styleClasses = type.groups
  .flatMap((g) =>
    g.styles.map((s) => {
      const decl = [`font-family: var(--font-${s.family ?? g.family})`, `font-size: ${s.fontSize}`, `font-weight: ${s.fontWeight}`];
      if (s.lineHeight) decl.push(`line-height: ${s.lineHeight}`);
      if (s.letterSpacing) decl.push(`letter-spacing: ${s.letterSpacing}`);
      return `.${s.name} { ${decl.join('; ')}; }`;
    }),
  )
  .join('\n');
const css = [
  `/* ${tokens.name} — generated from tokens.json */`,
  `:root, [data-theme="${themes[0].id}"] {\n${themeBlock(themes[0].id)}\n}`,
  ...themes.slice(1).map((t) => `[data-theme="${t.id}"] {\n${themeBlock(t.id)}\n  color-scheme: ${t.id.includes('dark') ? 'dark' : 'light'};\n}`),
  `:root {\n${scalars.join('\n')}\n}`,
  styleClasses,
  fontFaces,
].join('\n\n');
// The design system compiles its own tokens.css from tokens.json, so this copy is never published.
// It is for the local gallery and for the Unluminous design canvas, which loads it beside the bundle.
fs.mkdirSync(path.join(root, 'rux/build'), { recursive: true });
fs.writeFileSync(path.join(root, 'rux/build/tokens.css'), css + '\n');
// The Unluminous design canvas carries its own copy of the system under ds/rux/, with the fonts beside
// it, so the font paths there are relative to that folder.
const canvasDs = path.join(root, 'unluminous/canvas/project/ds/rux');
fs.mkdirSync(canvasDs, { recursive: true });
fs.writeFileSync(path.join(canvasDs, 'tokens.css'), css.split('../system/project/fonts/').join('fonts/') + '\n');
console.log(`tokens: ${color.tokens.length} colours, ${shadow.tokens.length} shadows, ${radius.tokens.length} radii, ${spacing.tokens.length} spacing, 4 themes`);
