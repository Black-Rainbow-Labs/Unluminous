// Render the canvas's .dc.html artboards locally, the way the canvas mounts them, and photograph them.
//
// The Design canvas runs each artboard in its own runtime, which cannot be opened from a terminal. This
// is enough of that runtime to see an artboard: it evaluates the artboard's logic class for its values,
// fills {{holes}}, repeats <sc-for>, branches <sc-if>, and mounts <x-import> from window.Rux with the
// attributes as props. Pictures go to _agent_output.
//
//   node tools/render-dc.cjs <out folder> <artboard.dc.html> [more…]

const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'C:/jason/dev/ai-service/ui/node_modules/playwright');

const root = path.resolve(__dirname, '..');
const read = (p) => fs.readFileSync(path.join(root, p), 'utf8');

/** What runs in the page: parse the artboard, work out its values, and render it with React. */
function mount(source) {
  const doc = new DOMParser().parseFromString(source, 'text/html');
  const logic = doc.querySelector('script[data-dc-script]').textContent;
  class DCLogic {
    constructor() { this.props = {}; this.state = {}; }
    setState() {}
    forceUpdate() {}
  }
  // eslint-disable-next-line no-new-func
  const Component = new Function('DCLogic', `${logic}; return Component;`)(DCLogic);
  const values = new Component().renderVals();
  const h = React.createElement;

  const lookup = (pathText, scope) => {
    const literal = pathText.trim();
    if (literal === 'true') return true;
    if (literal === 'false') return false;
    if (/^-?\d+(\.\d+)?$/.test(literal)) return Number(literal);
    return literal.split('.').reduce((at, key) => (at == null ? undefined : at[key]), scope);
  };
  const resolve = (text, scope) => {
    const whole = /^\{\{([^}]+)\}\}$/.exec(text.trim());
    if (whole) return lookup(whole[1], scope);
    return text.replace(/\{\{([^}]+)\}\}/g, (_, p) => String(lookup(p, scope) ?? ''));
  };
  const camel = (name) => name.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  const styleOf = (text) => {
    const style = {};
    for (const rule of text.split(';')) {
      const at = rule.indexOf(':');
      if (at < 0) continue;
      const key = rule.slice(0, at).trim();
      if (!key) continue;
      style[key.startsWith('--') ? key : camel(key)] = rule.slice(at + 1).trim();
    }
    return style;
  };
  const kids = (node, scope) => Array.from(node.childNodes).map((child, i) => convert(child, scope, i)).filter((x) => x !== null);

  function convert(node, scope, key) {
    if (node.nodeType === 3) {
      const text = resolve(node.textContent, scope);
      return text === '' ? null : text;
    }
    if (node.nodeType !== 1) return null;
    const tag = node.tagName.toLowerCase();
    if (tag === 'helmet' || tag === 'script') return null;
    if (tag === 'sc-for') {
      const list = resolve(node.getAttribute('list'), scope) || [];
      const as = node.getAttribute('as');
      return h(React.Fragment, { key }, list.map((item, i) => h(React.Fragment, { key: i }, kids(node, { ...scope, [as]: item, $index: i }))));
    }
    if (tag === 'sc-if') return resolve(node.getAttribute('value'), scope) ? h(React.Fragment, { key }, kids(node, scope)) : null;
    const props = { key };
    let slotStyle = null;
    for (const attr of Array.from(node.attributes)) {
      if (attr.name === 'component-from-global-scope') continue;
      const value = resolve(attr.value, scope);
      if (attr.name === 'style') {
        if (tag === 'x-import') slotStyle = styleOf(String(value));
        else props.style = styleOf(String(value));
      } else if (attr.name === 'class') props.className = value;
      else if (attr.name === 'for') props.htmlFor = value;
      else if (tag === 'x-import') props[camel(attr.name)] = value;
      else if (attr.name.startsWith('data-') || attr.name.startsWith('aria-')) props[attr.name] = value;
      else props[camel(attr.name)] = value;
    }
    if (tag === 'x-import') {
      const target = node.getAttribute('component-from-global-scope').split('.').reduce((at, k) => at && at[k], window);
      if (!target) return h('div', { key, style: { outline: '2px solid red' } }, `missing ${node.getAttribute('component-from-global-scope')}`);
      const children = kids(node, scope);
      const element = h(target, props, ...(children.length ? children : []));
      return h('div', { key: `slot-${key}`, style: { display: 'contents', ...(slotStyle || {}) } }, element);
    }
    const children = kids(node, scope);
    return h(tag, props, ...(children.length ? children : []));
  }
  const template = doc.querySelector('x-dc');
  const tree = kids(template, values);
  ReactDOM.createRoot(document.getElementById('root')).render(h(React.Fragment, null, tree));
}

(async () => {
  const [out, ...files] = process.argv.slice(2);
  fs.mkdirSync(out, { recursive: true });
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1180, height: 740 } });
  page.on('pageerror', (e) => console.log(`  error: ${e.message}`));
  page.on('console', (m) => m.type() === 'error' && console.log(`  console: ${m.text()}`));
  const css = read('unluminous/canvas/project/ds/rux/tokens.css').split('url("fonts/').join('url("file:///' + path.join(root, 'rux/system/project/fonts/').split(path.sep).join('/'));
  for (const file of files) {
    await page.setContent(`<!doctype html><html><head><meta charset="utf-8"><style>body{margin:0}</style></head><body><div id="root"></div></body></html>`);
    await page.addStyleTag({ content: css });
    await page.addStyleTag({ content: read('rux/system/project/components/bundle.css') });
    await page.addScriptTag({ content: read('rux/system/project/components/lib/react.production.min.js') });
    await page.addScriptTag({ content: read('rux/system/project/components/lib/react-dom.production.min.js') });
    await page.addScriptTag({ content: read('rux/system/project/components/bundle.js') });
    await page.evaluate(mount, fs.readFileSync(file, 'utf8'));
    await page.waitForTimeout(400);
    const name = path.basename(file).replace('.dc.html', '');
    await page.screenshot({ path: path.join(out, `${name}.png`) });
    console.log(`shot ${name}`);
  }
  await browser.close();
})();
