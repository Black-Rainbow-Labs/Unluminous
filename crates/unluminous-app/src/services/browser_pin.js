// The element picker and the pin a browser node uses to show one element of a page. `task-2203`.
//
// Evaluated in the page by `services::browser`, never given a way to call Unluminous: the host asks
// `__unluminousPicker.take()` for the answer, so there is no bridge for a page to call. Defining the two
// objects is idempotent, so evaluating this file again on the same document changes nothing.
//
// `tasks/task-2203-browser-sessions-and-pinned-elements-tdd.md` §3 is the design.
(() => {
  if (window.__unluminousPicker && window.__unluminousPin) return;

  const SVG = 'http://www.w3.org/2000/svg';
  const LIMIT = 2147483647;

  // ------------------------------------------------------------------------------- the selector

  const esc = (text) => (window.CSS && CSS.escape ? CSS.escape(text) : text.replace(/[^\w-]/g, '\\$&'));

  // A class that looks generated (a hash, a long run of digits) changes between builds of a site, so it
  // is not used to find the element again.
  const steadyClass = (name) => /^[A-Za-z_-][\w-]*$/.test(name) && !/\d{3,}/.test(name) && name.length < 40;

  const findsOnly = (selector, el) => {
    try {
      const found = document.querySelectorAll(selector);
      return found.length === 1 && found[0] === el;
    } catch (_) {
      return false;
    }
  };

  // Walk up from the element, preferring an id, then classes, then the tag with :nth-of-type, and stop as
  // soon as what has been built finds the element and nothing else. uBlock Origin's picker builds its
  // selectors the same way.
  const selectorFor = (el) => {
    if (el.id && findsOnly('#' + esc(el.id), el)) return '#' + esc(el.id);
    const parts = [];
    let node = el;
    while (node && node.nodeType === 1 && node !== document.documentElement) {
      if (node !== el && node.id && findsOnly('#' + esc(node.id), node)) {
        parts.unshift('#' + esc(node.id));
      } else {
        let part = node.localName;
        const parent = node.parentElement;
        if (parent) {
          const same = Array.from(parent.children).filter((child) => child.localName === node.localName);
          const classes = Array.from(node.classList).filter(steadyClass).slice(0, 3);
          const withClasses = part + classes.map((name) => '.' + esc(name)).join('');
          if (classes.length && same.filter((child) => child.matches(withClasses)).length === 1) {
            part = withClasses;
          } else if (same.length > 1) {
            part += ':nth-of-type(' + (same.indexOf(node) + 1) + ')';
          }
        }
        parts.unshift(part);
      }
      const selector = parts.join(' > ');
      if (findsOnly(selector, el)) return selector;
      if (parts[0].startsWith('#')) break;
      node = node.parentElement;
    }
    // Nothing shorter was unique, so the whole path by position, which always is.
    const path = [];
    for (node = el; node && node !== document.documentElement; node = node.parentElement) {
      const parent = node.parentElement;
      const same = parent ? Array.from(parent.children).filter((child) => child.localName === node.localName) : [];
      path.unshift(node.localName + (same.length > 1 ? ':nth-of-type(' + (same.indexOf(node) + 1) + ')' : ''));
    }
    return 'html > ' + path.join(' > ');
  };

  // What the label above the outline calls an element: its tag, its id or its first two classes.
  const describe = (el) => {
    let text = el.localName;
    if (el.id) text += '#' + el.id;
    else {
      const classes = Array.from(el.classList).slice(0, 2);
      if (classes.length) text += '.' + classes.join('.');
    }
    return text.length > 60 ? text.slice(0, 57) + '...' : text;
  };

  // ------------------------------------------------------------------------------- the picker

  let picking = null;
  let answer = null;

  // The element under a point, skipping the picker's own overlay, the root and the body.
  const elementAt = (x, y) => {
    const found = document.elementsFromPoint(x, y);
    for (const el of found) {
      if (picking && el === picking.host) continue;
      if (el === document.documentElement || el === document.body) continue;
      return el;
    }
    return null;
  };

  // Dim the page with one path covering the whole viewport and a hole cut where the element is, filled
  // even-odd, then outline the hole and name the element above it.
  const draw = () => {
    if (!picking) return;
    const { ocean, outline, label, chip } = picking;
    const width = window.innerWidth;
    const height = window.innerHeight;
    const el = picking.el;
    if (!el || !el.isConnected) {
      ocean.setAttribute('d', `M0 0H${width}V${height}H0Z`);
      outline.setAttribute('width', '0');
      chip.style.display = 'none';
      return;
    }
    const r = el.getBoundingClientRect();
    ocean.setAttribute('d', `M0 0H${width}V${height}H0Z M${r.left} ${r.top}V${r.bottom}H${r.right}V${r.top}Z`);
    outline.setAttribute('x', r.left);
    outline.setAttribute('y', r.top);
    outline.setAttribute('width', Math.max(0, r.width));
    outline.setAttribute('height', Math.max(0, r.height));
    label.textContent = describe(el) + '  ' + Math.round(r.width) + ' × ' + Math.round(r.height);
    chip.style.display = 'block';
    const above = r.top - 26;
    chip.style.top = (above >= 4 ? above : Math.min(height - 26, r.bottom + 4)) + 'px';
    chip.style.left = Math.max(4, Math.min(width - chip.offsetWidth - 4, r.left)) + 'px';
  };

  const choose = (el) => {
    if (!picking || !el) return;
    picking.el = el;
    draw();
  };

  const stop = (result) => {
    if (!picking) return;
    for (const [type, handler] of picking.listeners) window.removeEventListener(type, handler, true);
    picking.host.remove();
    picking = null;
    answer = result;
  };

  const swallow = (event) => {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();
  };

  // Up the tree, remembering where it came from so down comes back the same way.
  const parent = () => {
    const el = picking.el;
    const up = el && el.parentElement;
    if (!up || up === document.body || up === document.documentElement) return;
    picking.below.push(el);
    choose(up);
  };

  const child = () => {
    const down = picking.below.pop() || (picking.el && picking.el.firstElementChild);
    if (down) choose(down);
  };

  const pick = () => {
    const el = picking && picking.el;
    if (!el) return;
    stop({ state: 'picked', selector: selectorFor(el), label: describe(el) });
  };

  const start = (x, y) => {
    if (picking) stop(null);
    // A pinned page has its root scaled, and the overlay is fixed to the root, so the pin goes first.
    if (window.__unluminousPin) window.__unluminousPin.clear();
    answer = null;
    const host = document.createElement('unluminous-picker');
    host.style.cssText =
      'all: initial; position: fixed; inset: 0; z-index: ' + LIMIT + '; pointer-events: none; display: block;';
    const root = host.attachShadow({ mode: 'closed' });
    const svg = document.createElementNS(SVG, 'svg');
    svg.setAttribute('style', 'position: fixed; inset: 0; width: 100vw; height: 100vh; pointer-events: none;');
    const ocean = document.createElementNS(SVG, 'path');
    ocean.setAttribute('fill', 'rgba(8, 10, 18, 0.55)');
    ocean.setAttribute('fill-rule', 'evenodd');
    const outline = document.createElementNS(SVG, 'rect');
    outline.setAttribute('fill', 'rgba(110, 168, 254, 0.10)');
    outline.setAttribute('stroke', '#6ea8fe');
    outline.setAttribute('stroke-width', '2');
    svg.append(ocean, outline);
    const chip = document.createElement('div');
    chip.setAttribute(
      'style',
      'position: fixed; display: none; padding: 3px 8px; border-radius: 6px; background: #1d2333; ' +
        'color: #e6e9f2; font: 12px/18px system-ui, sans-serif; white-space: nowrap; ' +
        'box-shadow: 0 2px 8px rgba(0,0,0,0.4); pointer-events: none;'
    );
    const label = document.createElement('span');
    chip.append(label);
    const help = document.createElement('div');
    help.textContent = 'Click to show only this element · Wheel or ↑ ↓ for the parent or child · Esc to cancel';
    help.setAttribute(
      'style',
      'position: fixed; left: 50%; bottom: 14px; transform: translateX(-50%); padding: 6px 12px; ' +
        'border-radius: 8px; background: #1d2333; color: #e6e9f2; font: 12px/18px system-ui, sans-serif; ' +
        'white-space: nowrap; box-shadow: 0 2px 10px rgba(0,0,0,0.45); pointer-events: none;'
    );
    root.append(svg, chip, help);
    document.documentElement.append(host);
    const listeners = [
      ['mousemove', (event) => {
        picking.below = [];
        choose(elementAt(event.clientX, event.clientY));
      }],
      ['mousedown', swallow],
      ['mouseup', swallow],
      ['pointerdown', swallow],
      ['pointerup', swallow],
      ['auxclick', swallow],
      ['dblclick', swallow],
      ['contextmenu', swallow],
      ['click', (event) => {
        swallow(event);
        if (event.button === 0) pick();
      }],
      ['wheel', (event) => {
        swallow(event);
        if (event.deltaY < 0) parent();
        else if (event.deltaY > 0) child();
      }],
      ['keydown', (event) => {
        swallow(event);
        if (event.key === 'Escape') stop({ state: 'cancelled' });
        else if (event.key === 'ArrowUp') parent();
        else if (event.key === 'ArrowDown') child();
        else if (event.key === 'Enter') pick();
      }],
      ['scroll', () => draw()],
      ['resize', () => draw()],
    ];
    picking = { host, ocean, outline, label, chip, el: null, below: [], listeners };
    for (const [type, handler] of listeners) {
      window.addEventListener(type, handler, { capture: true, passive: false });
    }
    const atX = typeof x === 'number' ? x : window.innerWidth / 2;
    const atY = typeof y === 'number' ? y : window.innerHeight / 2;
    choose(elementAt(atX, atY));
    if (!picking.el) draw();
    return true;
  };

  window.__unluminousPicker = {
    start,
    cancel: () => stop({ state: 'cancelled' }),
    // What the host reads: the answer once, then nothing. `picking` while it is still open.
    take: () => {
      if (picking) return { state: 'picking' };
      const given = answer;
      answer = null;
      return given;
    },
    selectorFor,
  };

  // ---------------------------------------------------------------------------------- the pin

  const PIN_STYLE = 'unluminous-pin-style';
  const ROOM = 8;
  let pinned = null;

  const opaque = (colour) => {
    const match = /rgba?\(([^)]+)\)/.exec(colour || '');
    if (!match) return colour && colour !== 'transparent';
    const parts = match[1].split(',').map((part) => part.trim());
    return parts.length < 4 || parseFloat(parts[3]) > 0;
  };

  // The colour the space around the element is painted: the first background found walking up from its
  // parent, which is what was around it on the page. The element's own background is not used, because a
  // green button would then paint the whole node green.
  const groundOf = (el) => {
    for (let node = el.parentElement; node; node = node.parentElement) {
      const colour = getComputedStyle(node).backgroundColor;
      if (opaque(colour)) return colour;
    }
    return '#ffffff';
  };

  // Hide every sibling of the element and of each of its ancestors, and take the ancestors' own ground,
  // border and shadow away. The element's own subtree is not touched, so what it hides stays hidden.
  const mark = (el) => {
    for (const old of document.querySelectorAll('[data-unluminous-pin-path]')) {
      if (!old.contains(el)) old.removeAttribute('data-unluminous-pin-path');
    }
    for (const old of document.querySelectorAll('[data-unluminous-pin-hide]')) {
      if (old.contains(el)) old.removeAttribute('data-unluminous-pin-hide');
    }
    for (let node = el; node && node !== document.documentElement; node = node.parentElement) {
      const parent = node.parentElement;
      if (!parent) break;
      if (parent !== document.documentElement && !parent.hasAttribute('data-unluminous-pin-path')) {
        parent.setAttribute('data-unluminous-pin-path', '');
      }
      for (const sibling of parent.children) {
        if (sibling !== node && !sibling.hasAttribute('data-unluminous-pin-hide')) {
          sibling.setAttribute('data-unluminous-pin-hide', '');
        }
      }
    }
  };

  const unmark = () => {
    for (const old of document.querySelectorAll('[data-unluminous-pin-path]')) old.removeAttribute('data-unluminous-pin-path');
    for (const old of document.querySelectorAll('[data-unluminous-pin-hide]')) old.removeAttribute('data-unluminous-pin-hide');
  };

  // Scale the root by the same amount in both directions so the element fills the view, centred. An element
  // so tall that fitting its height would leave it under half the width it could have is fitted to the width
  // instead and scrolled with the wheel.
  const fit = () => {
    if (!pinned || !pinned.el || !pinned.el.isConnected) return;
    const html = document.documentElement;
    html.style.setProperty('transform', 'none', 'important');
    const r = pinned.el.getBoundingClientRect();
    const width = window.innerWidth;
    const height = window.innerHeight;
    if (r.width < 1 || r.height < 1) {
      html.style.setProperty('transform', pinned.transform || 'none', 'important');
      return;
    }
    const across = (width - 2 * ROOM) / r.width;
    const whole = Math.min(across, (height - 2 * ROOM) / r.height);
    pinned.tall = whole < across * 0.5;
    // At most four times its size, because a small control blown up to fill a large node is a picture of one
    // letter rather than of the element. A tall element that is scrolled is read line by line, so it is made
    // at most twice its size: Wikipedia's infobox fitted to a wide node came out at over three times and
    // needed a screenful of wheel for each row.
    const scale = Math.max(0.05, pinned.tall ? Math.min(2, across) : Math.min(4, whole));
    const drawnHeight = r.height * scale;
    const most = Math.max(0, drawnHeight + 2 * ROOM - height);
    pinned.scroll = pinned.tall ? Math.max(0, Math.min(most, pinned.scroll)) : 0;
    const x = (width - r.width * scale) / 2 - r.left * scale;
    const y = pinned.tall
      ? ROOM - r.top * scale - pinned.scroll
      : (height - drawnHeight) / 2 - r.top * scale;
    pinned.transform = `translate(${x}px, ${y}px) scale(${scale})`;
    html.style.setProperty('transform', pinned.transform, 'important');
    pinned.scale = scale;
  };

  const take = (el) => {
    if (pinned.el === el) return;
    if (pinned.resize) pinned.resize.disconnect();
    pinned.el = el;
    if (!el) return;
    pinned.ground = groundOf(el);
    document.documentElement.style.setProperty('background', pinned.ground, 'important');
    mark(el);
    if (window.ResizeObserver) {
      pinned.resize = new ResizeObserver(() => fit());
      pinned.resize.observe(el);
    }
    fit();
  };

  const clear = () => {
    if (!pinned) return false;
    clearInterval(pinned.timer);
    if (pinned.resize) pinned.resize.disconnect();
    window.removeEventListener('resize', pinned.onResize, true);
    window.removeEventListener('wheel', pinned.onWheel, true);
    const html = document.documentElement;
    html.removeAttribute('data-unluminous-pinned');
    html.style.removeProperty('transform');
    html.style.removeProperty('background');
    if (pinned.inline) html.setAttribute('style', pinned.inline);
    else if (html.getAttribute('style') === '') html.removeAttribute('style');
    const style = document.getElementById(PIN_STYLE);
    if (style) style.remove();
    unmark();
    pinned = null;
    return true;
  };

  const apply = (selector) => {
    clear();
    const html = document.documentElement;
    pinned = {
      selector,
      el: null,
      scroll: 0,
      tall: false,
      scale: 1,
      transform: '',
      inline: html.getAttribute('style') || '',
      timer: 0,
      resize: null,
      onResize: () => fit(),
      onWheel: (event) => {
        if (!pinned || !pinned.tall) return;
        event.preventDefault();
        event.stopPropagation();
        pinned.scroll += event.deltaY;
        fit();
      },
    };
    const style = document.createElement('style');
    style.id = PIN_STYLE;
    style.textContent =
      // A table with collapsed borders paints its cells' borders itself, so a hidden cell's border is still
      // drawn unless it is made transparent as well.
      '[data-unluminous-pin-hide] { visibility: hidden !important; border-color: transparent !important; }\n' +
      '[data-unluminous-pin-hide] * { border-color: transparent !important; }\n' +
      '[data-unluminous-pin-path] { background: transparent !important; border-color: transparent !important; ' +
      'box-shadow: none !important; outline: none !important; }\n' +
      'html[data-unluminous-pinned] { overflow: hidden !important; transform-origin: 0 0 !important; ' +
      'transition: none !important; }\n' +
      'html[data-unluminous-pinned] body { overflow: visible !important; }\n';
    (document.head || html).append(style);
    html.setAttribute('data-unluminous-pinned', '');
    window.scrollTo(0, 0);
    window.addEventListener('resize', pinned.onResize, true);
    window.addEventListener('wheel', pinned.onWheel, { capture: true, passive: false });
    // A page that builds the element later, or rebuilds it, is looked at again twice a second.
    pinned.timer = setInterval(() => {
      if (!pinned) return;
      if (!pinned.el || !pinned.el.isConnected) {
        let found = null;
        try {
          found = document.querySelector(pinned.selector);
        } catch (_) {
          found = null;
        }
        if (found) take(found);
        return;
      }
      mark(pinned.el);
      fit();
    }, 500);
    let found = null;
    try {
      found = document.querySelector(selector);
    } catch (_) {
      return { state: 'bad-selector' };
    }
    take(found);
    return { state: found ? 'pinned' : 'waiting' };
  };

  window.__unluminousPin = {
    apply,
    clear,
    // What is pinned, for `realm browser <node> pinned`.
    state: () =>
      pinned
        ? { selector: pinned.selector, found: !!(pinned.el && pinned.el.isConnected), scale: pinned.scale, tall: pinned.tall }
        : null,
  };
})();
