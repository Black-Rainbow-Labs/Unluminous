// A link a person clicks opens in a new tab (`task-2270`). Put in every page a rendered tab or a browser node
// loads, before the page's own scripts, unless `browser.links` is `same-tab` or the page is a design.
//
// It asks the browser for a new window, which Unluminous's view turns into a new tab or a new node, and gives
// the page nothing to call. Left to the page: a click with a modifier or another button, a link with a
// `download` attribute, a link to a place on the same page, and an address that is not a page.
(function () {
  if (window.__unluminousLinks) {
    return;
  }
  window.__unluminousLinks = true;
  document.addEventListener(
    'click',
    function (event) {
      if (event.defaultPrevented || event.button !== 0) {
        return;
      }
      if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) {
        return;
      }
      var start = event.target;
      if (start && start.nodeType !== 1) {
        start = start.parentElement;
      }
      var link = start && start.closest ? start.closest('a[href]') : null;
      if (!link || link.hasAttribute('download')) {
        return;
      }
      var url;
      try {
        url = new URL(link.getAttribute('href'), document.baseURI);
      } catch (problem) {
        return;
      }
      if (url.protocol !== 'http:' && url.protocol !== 'https:' && url.protocol !== 'unluminous:') {
        return;
      }
      var here = new URL(window.location.href);
      var samePage =
        url.origin === here.origin && url.pathname === here.pathname && url.search === here.search;
      if (samePage && url.hash) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      window.open(url.href, '_blank');
    },
    true
  );
})();
