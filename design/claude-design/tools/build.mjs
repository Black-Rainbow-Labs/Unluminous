// Build the Black Rainbow Labs Rux design system for Claude Design from the web port in rux/src.
//
//   npm run build            (from design/claude-design)
//
// Writes rux/system/project/: tokens.json, components/bundle.js, bundle.css, index.d.ts, and a
// README.md and preview.html for every component. Also writes rux/build/, which is not published:
// tokens.css and a local copy of every preview that loads its own stylesheets, so the library can be
// looked at in a browser before it is published.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as esbuild from 'esbuild';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const src = path.join(root, 'rux/src');
const project = path.join(root, 'rux/system/project');
const local = path.join(root, 'rux/build');
const NAMESPACE = 'Rux';

// rux itself: the icon path data and the vendored fonts come from a checkout of black-rainbow-labs-rux,
// at the commit Cargo.toml pins. RUX_DIR names it; the default is where it lives on this machine.
const ruxDir = process.env.RUX_DIR || 'C:/jason/dev/black-rainbow-labs-rux';
const pathsRs = path.join(ruxDir, 'crates/rux/src/icon/paths.rs');
if (!fs.existsSync(pathsRs)) throw new Error(`no rux checkout at ${ruxDir}; set RUX_DIR`);
const { execFileSync } = await import('node:child_process');
execFileSync(process.execPath, [path.join(here, 'extract-rux-icons.mjs'), pathsRs], { stdio: 'inherit' });
const fonts = path.join(project, 'fonts');
fs.mkdirSync(fonts, { recursive: true });
for (const file of fs.readdirSync(path.join(ruxDir, 'crates/rux/assets/fonts')).filter((f) => f.endsWith('.ttf'))) {
  fs.copyFileSync(path.join(ruxDir, 'crates/rux/assets/fonts', file), path.join(fonts, file));
}

await import('./build-tokens.mjs');

// ---- Unluminous's drawn marks, as data the UlIcon component draws ------------------------------
//
// unluminous/icons is written by the export_icons_svg example from theme::icon. The ink it drew in
// becomes currentColor, so a mark takes the colour of the words round it as it does in the window.
// The classic set keeps only the marks that differ from the material one, and a mark the classic set
// does not draw at all (the folder mark) is left out.
const ulIcons = path.join(root, 'unluminous/icons');
/**
 * The drawing inside one exported SVG, with its ink turned into currentColor.
 * @param file - the SVG's path
 */
const innerOf = (file) =>
  fs
    .readFileSync(file, 'utf8')
    .replace(/^[\s\S]*?<!--[^>]*-->\n/, '')
    .replace(/<\/svg>\s*$/, '')
    .replace(/#1e2530/g, 'currentColor')
    .trim();
const ulData = { material: {}, classic: {} };
for (const file of fs.readdirSync(path.join(ulIcons, 'material')).sort()) ulData.material[file.slice(0, -4)] = innerOf(path.join(ulIcons, 'material', file));
for (const file of fs.readdirSync(path.join(ulIcons, 'classic')).sort()) {
  const name = file.slice(0, -4);
  const inner = innerOf(path.join(ulIcons, 'classic', file));
  if (inner && inner !== ulData.material[name]) ulData.classic[name] = inner;
}
fs.writeFileSync(path.join(src, 'ul-icons.data.json'), JSON.stringify(ulData) + '\n');
// The same marks as asset files, for the design system's Unluminous Icons groups.
for (const [set, group] of [['material', 'UnluminousIcons'], ['classic', 'UnluminousClassic']]) {
  const out = path.join(project, 'assets', group);
  fs.mkdirSync(out, { recursive: true });
  for (const name of Object.keys(ulData[set])) fs.copyFileSync(path.join(ulIcons, set, `${name}.svg`), path.join(out, `${name}.svg`));
}


// Which section of the design system each component is filed under. A component not named here is
// filed under "Components" and the build says so.
const GROUPS = {
  Actions: ['Button', 'IconButton', 'Key'],
  Forms: ['Select', 'TextInput', 'TextArea', 'Stepper', 'Checkbox', 'Switch', 'Segmented', 'DiceToggle', 'Fader'],
  Navigation: ['TopNav', 'ProjectMenu', 'ListItem', 'MenuHeading'],
  Surfaces: ['Panel', 'Well', 'SubGroup', 'Divider', 'Tile', 'Modal', 'Scrollbar', 'Instrument', 'Plate', 'Screen', 'Led', 'Readout'],
  Feedback: ['Spinner', 'Progress', 'Skeleton', 'Chip', 'Avatar', 'Meter'],
  Data: ['Chart', 'Table', 'Timeline'],
  Foundations: ['Icon'],
  Unluminous: ['UlIcon', 'UlWindow', 'UlTitleBar', 'UlActivityBar', 'UlExplorer', 'UlTabStrip', 'UlEditor', 'UlStatusBar', 'UlMenu', 'UlModal', 'UlTile', 'UlButton', 'UlField'],
};

/**
 * The section a component belongs to.
 * @param name - the component name
 */
function groupOf(name) {
  for (const [group, names] of Object.entries(GROUPS)) if (names.includes(name)) return group;
  console.warn(`  ${name} is in no group; filed under Components`);
  return 'Components';
}

/** Every component source in rux/src/components, by name. */
function components() {
  const dir = path.join(src, 'components');
  return fs
    .readdirSync(dir)
    .filter((file) => file.endsWith('.jsx') && !file.endsWith('.preview.jsx'))
    .map((file) => file.slice(0, -4))
    .sort();
}

/**
 * The names a component file exports, read from its `export function` and `export const` lines.
 * @param name - the component file's name
 */
function exportsOf(name) {
  const text = fs.readFileSync(path.join(src, 'components', `${name}.jsx`), 'utf8');
  return [...text.matchAll(/^export (?:function|const) (\w+)/gm)].map((m) => m[1]);
}

// esbuild plugins: React is the page's global, and a preview reaches components through the bundle's
// global rather than bundling a second copy of them.
const reactGlobal = {
  name: 'react-global',
  setup(build) {
    build.onResolve({ filter: /^react(-dom)?(\/client|\/jsx-runtime)?$/ }, (args) => ({ path: args.path, namespace: 'global' }));
    build.onLoad({ filter: /.*/, namespace: 'global' }, (args) => ({
      contents: args.path.startsWith('react-dom') ? 'module.exports = window.ReactDOM;' : 'module.exports = window.React;',
      loader: 'js',
    }));
  },
};
const libraryGlobal = {
  name: 'library-global',
  setup(build) {
    build.onResolve({ filter: /\.jsx$/ }, (args) => {
      if (args.kind === 'entry-point' || args.path.endsWith('.preview.jsx')) return undefined;
      return { path: args.path, namespace: 'library' };
    });
    build.onLoad({ filter: /.*/, namespace: 'library' }, () => ({ contents: `module.exports = window.${NAMESPACE};`, loader: 'js' }));
  },
};
const jsxOptions = { jsx: 'transform', jsxFactory: 'React.createElement', jsxFragment: 'React.Fragment', loader: { '.json': 'json' } };

/**
 * Refuse output that would end or escape the inline <script> a consumer puts it in.
 * @param code - compiled JavaScript
 * @param where - what it is, for the message
 */
function inlineSafe(code, where) {
  const safe = code.replace(/<\/script/gi, '<\\/script').replace(/<!--/g, '\\x3C!--');
  if (/<\/script|<!--/i.test(safe)) throw new Error(`${where} still holds a closing script tag`);
  return safe;
}

// ---- the bundle ------------------------------------------------------------------------------

const names = components();
const exported = names.flatMap((name) => exportsOf(name).map((symbol) => ({ file: name, symbol })));
const entry = [
  `import * as React from 'react';`,
  `export { Icon, ICON_NAMES } from './icon.jsx';`,
  ...names.map((name) => `export * from './components/${name}.jsx';`),
].join('\n');
const bundle = await esbuild.build({
  stdin: { contents: entry, resolveDir: src, loader: 'jsx' },
  bundle: true,
  format: 'iife',
  globalName: NAMESPACE,
  minify: true,
  write: false,
  target: 'es2019',
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [reactGlobal],
  ...jsxOptions,
});
// A component is an exported name in PascalCase; a constant in capitals or a helper is not one.
const symbols = ['Icon', ...exported.map((e) => e.symbol).filter((s) => /^[A-Z][a-z]/.test(s))];
const header = `/* @ds-bundle: ${JSON.stringify({ format: 4, namespace: NAMESPACE, components: symbols.map((name) => ({ name })) })} */`;
fs.mkdirSync(path.join(project, 'components'), { recursive: true });
const bundleJs = `${header}\n${inlineSafe(bundle.outputFiles[0].text, 'bundle.js')}window.${NAMESPACE}=${NAMESPACE};\n`;
fs.writeFileSync(path.join(project, 'components/bundle.js'), bundleJs);

const css = [fs.readFileSync(path.join(src, 'base.css'), 'utf8')]
  .concat(['Instrument', ...names.filter((n) => n !== 'Instrument')].filter((n) => fs.existsSync(path.join(src, 'components', `${n}.css`))).map((n) => `/* ---- ${n} ---- */\n${fs.readFileSync(path.join(src, 'components', `${n}.css`), 'utf8')}`))
  .join('\n');
if (/<\/style/i.test(css)) throw new Error('bundle.css holds a closing style tag');
fs.writeFileSync(path.join(project, 'components/bundle.css'), css);

// ---- the React runtime the previews load --------------------------------------------------------

const libDir = path.join(project, 'components/lib');
fs.mkdirSync(libDir, { recursive: true });
for (const file of ['react.production.min.js', 'react-dom.production.min.js']) {
  const from = path.join(root, 'rux/vendor', file);
  fs.copyFileSync(from, path.join(libDir, file));
}

// ---- types --------------------------------------------------------------------------------------

const dts = [
  '// Black Rainbow Labs Rux: the web port of the rux Rust component library.',
  '// Documentation, not type checked. Every component takes className, style and any data-* or aria-* prop.',
  "import type { ReactElement, ReactNode, CSSProperties } from 'react';",
  '',
  'export interface Common { className?: string; style?: CSSProperties; [attribute: `data-${string}` | `aria-${string}`]: unknown; }',
  '',
  `export type IconName = ${JSON.parse(fs.readFileSync(path.join(src, 'icons.data.json'), 'utf8')) && Object.keys(JSON.parse(fs.readFileSync(path.join(src, 'icons.data.json'), 'utf8'))).map((n) => `'${n}'`).join(' | ')};`,
  'export interface IconProps extends Common { name: IconName; size?: number; stroke?: number; }',
  'export declare function Icon(props: IconProps): ReactElement | null;',
  'export declare const ICON_NAMES: IconName[];',
  '',
];
/**
 * A component's props, read from the destructuring in its signature: the name, and a type guessed
 * from the default the source gives it.
 * @param text - the component file's source
 * @param symbol - the component's name
 */
function propsOf(text, symbol) {
  const start = text.indexOf(`export function ${symbol}(`);
  if (start < 0) return null;
  let depth = 0;
  let open = text.indexOf('{', start);
  if (open < 0 || open > text.indexOf(')', start)) return [];
  let end = open;
  for (; end < text.length; end++) {
    if (text[end] === '{') depth++;
    if (text[end] === '}' && --depth === 0) break;
  }
  const body = text.slice(open + 1, end);
  const parts = [];
  let level = 0;
  let current = '';
  for (const ch of body) {
    if ('{[('.includes(ch)) level++;
    if ('}])'.includes(ch)) level--;
    if (ch === ',' && level === 0) {
      parts.push(current.trim());
      current = '';
    } else current += ch;
  }
  if (current.trim()) parts.push(current.trim());
  return parts
    .filter((p) => !p.startsWith('...') && !['className', 'style'].includes(p.split('=')[0].trim()))
    .map((p) => {
      const [key, ...rest] = p.split('=');
      const value = rest.join('=').trim();
      let type = 'unknown';
      if (/^['"`]/.test(value)) type = 'string';
      else if (/^-?\d/.test(value)) type = 'number';
      else if (value === 'true' || value === 'false') type = 'boolean';
      else if (value.startsWith('[')) type = 'unknown[]';
      if (/^on[A-Z]/.test(key.trim())) type = '(...args: any[]) => void';
      if (key.trim() === 'children') type = 'ReactNode';
      return `  ${key.trim().split(':')[0]}?: ${type};${value ? ` // default ${value.replace(/\s+/g, ' ').slice(0, 60)}` : ''}`;
    });
}

for (const name of names) {
  const types = path.join(src, 'components', `${name}.d.ts`);
  if (fs.existsSync(types)) {
    dts.push(fs.readFileSync(types, 'utf8').trim(), '');
    continue;
  }
  const text = fs.readFileSync(path.join(src, 'components', `${name}.jsx`), 'utf8');
  for (const symbol of exportsOf(name).filter((s) => /^[A-Z][a-z]/.test(s))) {
    const props = propsOf(text, symbol);
    if (props === null) continue;
    dts.push(`export interface ${symbol}Props extends Common {`, ...props, '}');
    dts.push(`export declare function ${symbol}(props: ${symbol}Props): ReactElement | null;`, '');
  }
}
fs.writeFileSync(path.join(project, 'components/index.d.ts'), dts.join('\n') + '\n');

// ---- a README and a live preview per component -------------------------------------------------

/**
 * The height a preview asks for: `export const height = N` in its source, or 160.
 * @param source - the preview's source text
 */
const heightOf = (source) => Number(/export const height = (\d+)/.exec(source)?.[1] ?? 160);

/**
 * Compile one preview into the inline script its preview.html carries.
 * @param file - the preview source's path
 */
async function compilePreview(file) {
  const result = await esbuild.build({
    stdin: {
      contents: `import Preview from ${JSON.stringify('./' + path.basename(file))};\nimport { createRoot } from 'react-dom/client';\ncreateRoot(document.getElementById('root')).render(React.createElement(Preview));`,
      resolveDir: path.dirname(file),
      loader: 'jsx',
    },
    bundle: true,
    format: 'iife',
    minify: true,
    write: false,
    target: 'es2019',
    plugins: [reactGlobal, libraryGlobal],
    ...jsxOptions,
  });
  return inlineSafe(result.outputFiles[0].text, file);
}

/**
 * The preview document, in the form the design system loads it and in a local form that loads its
 * own stylesheets and scripts.
 * @param title - the component's name
 * @param group - its section
 * @param height - its row height
 * @param script - the compiled preview
 */
function previewHtml(title, group, height, script) {
  const card = `<!-- @dsCard group="${group}" height=${height} -->`;
  const body = `<div id="root"></div>\n<script>${script}</script>`;
  const published = `${card}\n<!doctype html>\n<html>\n<head><meta charset="utf-8"><title>${title} — preview</title></head>\n<body>\n${body}\n</body>\n</html>\n`;
  const head = [
    '<link rel="stylesheet" href="../tokens.css">',
    '<link rel="stylesheet" href="../../system/project/components/bundle.css">',
    '<script src="../../system/project/components/lib/react.production.min.js"></script>',
    '<script src="../../system/project/components/lib/react-dom.production.min.js"></script>',
    '<script src="../../system/project/components/bundle.js"></script>',
  ].join('\n');
  const localCopy = `<!doctype html>\n<html data-theme="light">\n<head><meta charset="utf-8"><title>${title} — preview</title>\n${head}\n</head>\n<body>\n${body}\n</body>\n</html>\n`;
  return { published, localCopy };
}

const previews = [];
fs.mkdirSync(path.join(local, 'previews'), { recursive: true });
const cards = [...names, 'Icon'];
for (const name of cards) {
  const base = name === 'Icon' ? path.join(src, 'Icon') : path.join(src, 'components', name);
  const out = path.join(project, 'components', name);
  fs.mkdirSync(out, { recursive: true });
  if (fs.existsSync(`${base}.md`)) fs.copyFileSync(`${base}.md`, path.join(out, 'README.md'));
  else console.warn(`  ${name} has no guideline`);
  const previewFile = `${base}.preview.jsx`;
  if (!fs.existsSync(previewFile)) {
    console.warn(`  ${name} has no preview`);
    continue;
  }
  const source = fs.readFileSync(previewFile, 'utf8');
  const { published, localCopy } = previewHtml(name, groupOf(name), heightOf(source), await compilePreview(previewFile));
  if (published.length > 256 * 1024) throw new Error(`${name} preview is over 256 KB`);
  fs.writeFileSync(path.join(out, 'preview.html'), published);
  fs.writeFileSync(path.join(local, 'previews', `${name}.html`), localCopy);
  previews.push(name);
}

// The cover: a preview with no component behind it, so its folder holds nothing else.
const cover = fs.readFileSync(path.join(src, 'Cover.html'), 'utf8');
fs.mkdirSync(path.join(project, 'components/Cover'), { recursive: true });
fs.writeFileSync(path.join(project, 'components/Cover/preview.html'), cover);
fs.writeFileSync(
  path.join(local, 'previews/Cover.html'),
  cover.replace('</head>', '<link rel="stylesheet" href="../tokens.css">\n</head>').replace('<html>', '<html data-theme="light">'),
);

// A local gallery of every preview in both kinds of theme, for looking before publishing.
const themes = ['light', 'dark', 'unluminous-dark', 'unluminous-light'];
const gallery = `<!doctype html><html><head><meta charset="utf-8"><title>Rux gallery</title>
<style>body{margin:0;font:13px system-ui;background:#888}h2{margin:16px;font:600 14px system-ui}
.row{display:grid;grid-template-columns:repeat(4,1fr);gap:8px;padding:0 8px 8px}iframe{width:100%;height:320px;border:0;background:#fff}</style>
<script>function themed(f){var t=f.dataset.theme;try{f.contentDocument.documentElement.dataset.theme=t}catch(e){}}</script></head><body>
${previews.map((n) => `<h2>${n}</h2><div class="row">${themes.map((t) => `<iframe data-theme="${t}" onload="themed(this)" src="previews/${n}.html"></iframe>`).join('')}</div>`).join('\n')}
</body></html>`;
fs.writeFileSync(path.join(local, 'gallery.html'), gallery);

console.log(`bundle: ${symbols.length} exports, ${(bundleJs.length / 1024).toFixed(0)} KB; css ${(css.length / 1024).toFixed(0)} KB; ${previews.length} previews`);
