// `Ctrl`/`Cmd` with the wheel, and `Ctrl` with plus, minus and zero, zoom the page through Unluminous
// (`task-2336`). Put in every page a rendered tab or a browser node loads, before the page's own scripts.
//
// The engine would zoom the page itself, and Unluminous would not know: a browser node's page is zoomed by
// the node's own zoom times the canvas's, so the next move of the canvas put it back. So the gesture is
// stopped here and posted to Unluminous as `unluminous-zoom:in`, `out` or `reset`, and Unluminous zooms the
// node or the tab the way the same gesture over its toolbar does.
//
// Listened for after the page's own handlers, and left alone when one of them already took it: a map that
// zooms itself on the modifier wheel keeps doing so. On macOS the menu bar takes `Cmd` with plus before the
// page sees it, so only the wheel is caught there.
(function () {
  if (window.__unluminousZoom || !window.ipc || typeof window.ipc.postMessage !== 'function') {
    return;
  }
  window.__unluminousZoom = true;
  var mac = /Mac/.test(navigator.platform || '');
  // One notch of a mouse wheel is 100 units of `deltaY`; a trackpad sends many small ones, which are added
  // up so that a pinch does not step on every event.
  var NOTCH = 100;
  var gathered = 0;
  var lastAt = 0;
  var lastDirection = 0;
  function post(which) {
    try {
      window.ipc.postMessage('unluminous-zoom:' + which);
    } catch (problem) {}
  }
  window.addEventListener(
    'wheel',
    function (event) {
      if (event.defaultPrevented || !(event.ctrlKey || event.metaKey)) {
        return;
      }
      event.preventDefault();
      var delta = event.deltaY;
      if (event.deltaMode === 1) {
        delta *= 33;
      } else if (event.deltaMode === 2) {
        delta *= NOTCH;
      }
      // The first movement of a gesture is a step however small it is, so a trackpad answers at once; the
      // rest of the gesture is added up. A pause or a change of direction starts a new gesture.
      var now = Date.now();
      var direction = delta < 0 ? -1 : 1;
      if (now - lastAt > 250 || direction !== lastDirection) {
        gathered = direction * NOTCH;
      } else {
        gathered += delta;
      }
      lastAt = now;
      lastDirection = direction;
      while (Math.abs(gathered) >= NOTCH) {
        post(gathered < 0 ? 'in' : 'out');
        gathered -= gathered < 0 ? -NOTCH : NOTCH;
      }
    },
    { passive: false }
  );
  window.addEventListener('keydown', function (event) {
    if (mac || event.defaultPrevented || !event.ctrlKey || event.altKey || event.metaKey) {
      return;
    }
    var key = event.key;
    var which = key === '=' || key === '+' ? 'in' : key === '-' || key === '_' ? 'out' : key === '0' ? 'reset' : null;
    if (!which) {
      return;
    }
    event.preventDefault();
    post(which);
  });
})();
