// The Designer's side of a design: outlines an element under the pointer, chooses one, drops a note on it,
// and draws the pins of the notes that are already there. `task-2254`.
//
// Evaluated in the page by `services::browser`, after `browser_pin.js`, and never given a way to call
// Unluminous: the host asks `__unluminousDesigner.take()` for what happened, so there is no bridge for a page
// to call. Defining the object is idempotent, so evaluating this file again on the same document changes
// nothing. Everything drawn is inside a closed shadow root on one fixed element, so the page's own CSS
// cannot reach the overlay and the overlay's cannot reach the page.
//
// `tasks/task-2254-designer-plugin-tdd.md` §7 is the design.
(() => {
  if (window.__unluminousDesigner) return;
  const selectorFor = window.__unluminousSelectorFor;
  const describe = window.__unluminousDescribe;
  if (typeof selectorFor !== 'function' || typeof describe !== 'function') return;

  const LIMIT = 2147483647;
  const PROPS = [
    'font-family', 'font-size', 'font-weight', 'color',
    'background-color', 'padding', 'margin', 'border-radius',
  ];
  const SCROLL_KEY = '__unluminousDesigner:' + location.pathname;
  const CURSOR_STYLE = 'unluminous-designer-cursor';

  let mode = 'browse';
  let pins = [];
  let pinEls = new Map();
  let chosen = null;
  let hovered = null;
  let flash = null;
  let reports = [];
  let listeners = [];
  let frame = 0;

  const push = (report) => {
    reports.push(report);
  };

  // ------------------------------------------------------------------------------------- the overlay

  const host = document.createElement('unluminous-designer');
  host.style.cssText =
    'all: initial; position: fixed; inset: 0; z-index: ' + LIMIT + '; pointer-events: none; display: block;';
  const root = host.attachShadow({ mode: 'closed' });
  const style = document.createElement('style');
  style.textContent = `
    :host { --u: 1; }
    .outline { position: fixed; display: none; box-sizing: border-box; border: calc(2px * var(--u)) solid #6ea8fe;
      background: rgba(110, 168, 254, 0.10); border-radius: 2px; pointer-events: none; }
    .outline.steady { background: rgba(110, 168, 254, 0.06); }
    .chip { position: fixed; display: none; padding: calc(3px * var(--u)) calc(8px * var(--u));
      border-radius: calc(6px * var(--u)); background: #1d2333; color: #e6e9f2;
      font: calc(12px * var(--u)) / calc(18px * var(--u)) system-ui, sans-serif; white-space: nowrap;
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4); pointer-events: none; }
    .pin { position: fixed; width: calc(22px * var(--u)); height: calc(22px * var(--u));
      margin: calc(-11px * var(--u)) 0 0 calc(-11px * var(--u)); border-radius: 50%;
      background: #5b8def; color: #fff; font: 600 calc(11px * var(--u)) / calc(22px * var(--u)) system-ui, sans-serif;
      text-align: center; box-shadow: 0 2px 6px rgba(0, 0, 0, 0.45), inset 0 1px 0 rgba(255, 255, 255, 0.35);
      pointer-events: auto; cursor: pointer; user-select: none; }
    .pin.active { width: calc(28px * var(--u)); height: calc(28px * var(--u));
      margin: calc(-14px * var(--u)) 0 0 calc(-14px * var(--u)); line-height: calc(28px * var(--u));
      box-shadow: 0 0 0 3px rgba(91, 141, 239, 0.35), 0 3px 8px rgba(0, 0, 0, 0.5); }
    .pin.detached { opacity: 0.5; background: #8a93a6; }
    .pin.pulse { animation: pulse 0.9s ease-out 2; }
    @keyframes pulse { 0% { transform: scale(1); } 50% { transform: scale(1.5); } 100% { transform: scale(1); } }
  `;
  const outline = document.createElement('div');
  outline.className = 'outline';
  const chip = document.createElement('div');
  chip.className = 'chip';
  const layer = document.createElement('div');
  root.append(style, outline, chip, layer);
  document.documentElement.append(host);

  // ------------------------------------------------------------------------------------- the elements

  const find = (selector) => {
    try {
      return selector ? document.querySelector(selector) : null;
    } catch (_) {
      return null;
    }
  };

  const oneLine = (text, most) => {
    const flat = String(text || '').replace(/\s+/g, ' ').trim();
    return flat.length > most ? flat.slice(0, most - 1) + '…' : flat;
  };

  // The start of an element's markup, cut on a tag boundary where there is one near the end.
  const cutHtml = (html) => {
    if (html.length <= 1200) return html;
    const cut = html.lastIndexOf('>', 1200);
    return cut > 600 ? html.slice(0, cut + 1) : html.slice(0, 1200);
  };

  const details = (el) => {
    const r = el.getBoundingClientRect();
    const computed = getComputedStyle(el);
    const styles = {};
    for (const property of PROPS) styles[property] = computed.getPropertyValue(property);
    return {
      selector: selectorFor(el),
      tag: describe(el),
      excerpt: oneLine(el.innerText || el.textContent, 120),
      html: cutHtml(el.outerHTML || ''),
      styles,
      rect: { x: r.left + scrollX, y: r.top + scrollY, w: r.width, h: r.height },
    };
  };

  // The element under a point, skipping the root and the body.
  const elementAt = (x, y) => {
    for (const el of document.elementsFromPoint(x, y)) {
      if (el === host || el === document.documentElement || el === document.body) continue;
      return el;
    }
    return null;
  };

  const overAPin = (x, y) => {
    for (const { dot } of pinEls.values()) {
      const r = dot.getBoundingClientRect();
      if (x >= r.left - 3 && x <= r.right + 3 && y >= r.top - 3 && y <= r.bottom + 3) return true;
    }
    return false;
  };

  // ------------------------------------------------------------------------------------ drawing

  const drawOutline = () => {
    const now = Date.now();
    if (flash && flash.until < now) flash = null;
    const el = (flash && flash.el) || hovered || find(chosen);
    if (!el || !el.isConnected) {
      outline.style.display = 'none';
      chip.style.display = 'none';
      return;
    }
    const r = el.getBoundingClientRect();
    outline.style.cssText =
      'display: block; left: ' + r.left + 'px; top: ' + r.top + 'px; width: ' + r.width + 'px; height: ' + r.height + 'px;';
    outline.className = hovered ? 'outline' : 'outline steady';
    chip.textContent = describe(el) + '  ' + Math.round(r.width) + ' × ' + Math.round(r.height);
    chip.style.display = 'block';
    const above = r.top - 26;
    chip.style.top = (above >= 4 ? above : Math.min(window.innerHeight - 26, r.bottom + 4)) + 'px';
    chip.style.left = Math.max(4, Math.min(window.innerWidth - chip.offsetWidth - 4, r.left)) + 'px';
  };

  // Where a pin is drawn, and whether its element was found.
  const placePin = (entry) => {
    const { dot, data } = entry;
    const el = find(data.selector);
    let x;
    let y;
    if (el) {
      const r = el.getBoundingClientRect();
      x = r.left + data.fx * r.width;
      y = r.top + data.fy * r.height;
    } else {
      x = (data.page ? data.page.x : 0) - scrollX;
      y = (data.page ? data.page.y : 0) - scrollY;
    }
    entry.found = !!el;
    entry.x = x + scrollX;
    entry.y = y + scrollY;
    dot.style.left = Math.max(11, Math.min(window.innerWidth - 11, x)) + 'px';
    dot.style.top = Math.max(11, Math.min(window.innerHeight - 11, y)) + 'px';
    dot.classList.toggle('detached', !el || !!data.detached);
  };

  const tick = () => {
    frame = 0;
    for (const entry of pinEls.values()) placePin(entry);
    drawOutline();
    if (pinEls.size || hovered || chosen || flash) frame = requestAnimationFrame(tick);
  };

  const wake = () => {
    if (!frame) frame = requestAnimationFrame(tick);
  };

  // ----------------------------------------------------------------------------------------- pins

  const reportAnchors = () => {
    const found = [];
    const missing = [];
    for (const [id, entry] of pinEls) {
      if (!entry.found) {
        missing.push(id);
        continue;
      }
      const el = find(entry.data.selector);
      found.push({ id, x: entry.x, y: entry.y, element: el ? details(el) : null });
    }
    push({ kind: 'anchors', found, missing });
  };

  const setPins = (list) => {
    pins = Array.isArray(list) ? list : [];
    layer.replaceChildren();
    pinEls = new Map();
    for (const data of pins) {
      const dot = document.createElement('div');
      dot.className = 'pin' + (data.active ? ' active' : '');
      dot.textContent = String(data.n === undefined ? data.id : data.n);
      dot.addEventListener('click', (event) => {
        event.preventDefault();
        event.stopPropagation();
        push({ kind: 'pin', id: data.id });
      });
      layer.append(dot);
      pinEls.set(data.id, { dot, data, found: false, x: 0, y: 0 });
    }
    for (const entry of pinEls.values()) placePin(entry);
    reportAnchors();
    wake();
  };

  // ---------------------------------------------------------------------------------------- modes

  const swallow = (event) => {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();
  };

  const choose = (el) => {
    chosen = selectorFor(el);
    hovered = null;
    push({ kind: 'selected', element: details(el) });
    wake();
  };

  const up = () => {
    const el = find(chosen);
    const parent = el && el.parentElement;
    if (parent && parent !== document.body && parent !== document.documentElement) choose(parent);
  };

  const down = () => {
    const el = find(chosen);
    const child = el && el.firstElementChild;
    if (child) choose(child);
  };

  const onClick = (event) => {
    if (overAPin(event.clientX, event.clientY)) return;
    swallow(event);
    if (event.button !== 0) return;
    const el = elementAt(event.clientX, event.clientY);
    if (!el) return;
    // Comment is the one mode that listens for clicks: it chooses the element and opens a note on it.
    const r = el.getBoundingClientRect();
    const fx = r.width > 0 ? (event.clientX - r.left) / r.width : 1;
    const fy = r.height > 0 ? (event.clientY - r.top) / r.height : 0;
    chosen = selectorFor(el);
    hovered = null;
    push({ kind: 'comment', element: details(el), fx: Math.max(0, Math.min(1, fx)), fy: Math.max(0, Math.min(1, fy)) });
    wake();
  };

  const onKey = (event) => {
    if (event.key === 'Escape') {
      swallow(event);
      chosen = null;
      hovered = null;
      push({ kind: 'cleared' });
      wake();
    } else if (event.altKey && event.key === 'ArrowUp') {
      swallow(event);
      up();
    } else if (event.altKey && event.key === 'ArrowDown') {
      swallow(event);
      down();
    }
  };

  const blocked = (event) => {
    if (!overAPin(event.clientX, event.clientY)) swallow(event);
  };

  const setMode = (wanted) => {
    for (const [type, handler] of listeners) window.removeEventListener(type, handler, true);
    listeners = [];
    mode = wanted === 'comment' ? 'comment' : 'browse';
    const old = document.getElementById(CURSOR_STYLE);
    if (old) old.remove();
    hovered = null;
    if (mode !== 'browse') {
      const cursor = document.createElement('style');
      cursor.id = CURSOR_STYLE;
      cursor.textContent = 'html, html * { cursor: crosshair !important; }';
      (document.head || document.documentElement).append(cursor);
      listeners = [
        ['pointermove', (event) => {
          hovered = overAPin(event.clientX, event.clientY) ? null : elementAt(event.clientX, event.clientY);
          wake();
        }],
        ['pointerdown', blocked],
        ['pointerup', blocked],
        ['mousedown', blocked],
        ['mouseup', blocked],
        ['auxclick', blocked],
        ['dblclick', blocked],
        ['contextmenu', blocked],
        ['click', onClick],
        ['keydown', onKey],
      ];
      for (const [type, handler] of listeners) {
        window.addEventListener(type, handler, { capture: true, passive: false });
      }
    }
    wake();
  };

  // ---------------------------------------------------------------------------------- the host's calls

  const select = (selector) => {
    const el = find(selector);
    if (!el) return false;
    const r = el.getBoundingClientRect();
    if (r.bottom < 0 || r.top > window.innerHeight) el.scrollIntoView({ block: 'center' });
    chosen = selector;
    hovered = null;
    push({ kind: 'selected', element: details(el) });
    wake();
    return true;
  };

  const focus = (id) => {
    const entry = pinEls.get(id);
    if (!entry) return false;
    const el = find(entry.data.selector);
    if (el) {
      el.scrollIntoView({ block: 'center' });
      flash = { el, until: Date.now() + 1500 };
    }
    entry.dot.classList.remove('pulse');
    void entry.dot.offsetWidth;
    entry.dot.classList.add('pulse');
    wake();
    return !!el;
  };

  const clear = () => {
    chosen = null;
    hovered = null;
    flash = null;
    outline.style.display = 'none';
    chip.style.display = 'none';
    wake();
  };

  const take = () => {
    const out = reports;
    reports = [];
    return { reports: out };
  };

  // --------------------------------------------------------------------------- the scroll across a reload

  const saveScroll = () => {
    try {
      sessionStorage.setItem(SCROLL_KEY, String(window.scrollY));
    } catch (_) {
      // A page that forbids storage just goes back to the top.
    }
  };

  let saving = 0;
  window.addEventListener('pagehide', saveScroll);
  window.addEventListener('scroll', () => {
    clearTimeout(saving);
    saving = setTimeout(saveScroll, 200);
  }, { passive: true });

  try {
    const saved = Number(sessionStorage.getItem(SCROLL_KEY));
    if (saved > 0) window.scrollTo(0, saved);
  } catch (_) {
    // As above.
  }

  // How much bigger the overlay is drawn than a CSS pixel, so a pin is the same size on the screen whatever
  // zoom the host gave the page to fit a device into the tab. The host sends one over the page's zoom.
  const setScale = (scale) => {
    const u = Number(scale);
    host.style.setProperty('--u', String(Number.isFinite(u) && u > 0 ? Math.min(u, 4) : 1));
  };

  window.__unluminousDesigner = { setMode, setPins, setScale, select, focus, clear, take, parent: up, child: down };
  push({ kind: 'loaded', url: location.href, scrollY: window.scrollY });
})();
