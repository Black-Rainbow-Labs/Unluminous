# task-2203: browser sessions that come back, and a node that shows one element

The ticket:

> For our web browser node and tabs, we don't seem to be retaining session history. We want that to
> be retained between restarts and shared across projects, survive upgrades of Unluminous, etc.
>
> For the realm browser node, we want to be able to right click the page, see a pop up to select
> element which has element outlines similar to how ublock ad remover does it, and when the element is
> selected only that element is displayed in the node.

Two features. This document says what was measured, what was chosen and what was left out.

## 1. What was actually lost, measured

The browser profile is already in the right place. `UnluminousApp::use_store` sets it to
`<settings folder>/browser`, which is `%APPDATA%\Unluminous\browser` on Windows. That folder is per
person, not per project, and nothing an installer writes touches it. Reading it on this machine:

| What | Where | State |
|---|---|---|
| Persistent cookies | `EBWebView/Default/Network/Cookies` | 9 rows, all with an expiry. Kept. |
| Session cookies (no expiry) | same table | **0 rows.** Never written. |
| The engine's own history | `EBWebView/Default/History` | 26 visited addresses. Kept, and nothing in Unluminous reads it. |
| A tab's back and forward list | `BrowserTab::history` | **Memory only.** |
| Browser tabs in the editing area | `ProjectState` | **Not written at all.** A project reopens without them. |
| A browser node's address | the realm file, `node.<id>.url` | Kept, but a local page is written as `unluminous://tab-3/page.html`, and `BrowserLocation::parse` refuses the `unluminous` scheme, so **a node on a local page comes back empty.** |

So four things are lost on a restart, and each needs its own fix:

1. **Session cookies.** A cookie with no `Expires` lives as long as the browser process. Most sign in
   pages that offer "remember me" set a persistent cookie, and the ones that do not set a session
   cookie, which is gone the moment the last Unluminous window closes. Chrome and Edge keep them only
   when "Continue where you left off" is on: their cookie store has a `restore_old_session_cookies`
   switch that writes session cookies to disk and reads them back
   ([Eric Lawrence, "Undead Session Cookies"](https://textslashplain.com/2019/06/24/surprise-undead-session-cookies/)).
   WebView2 has no such switch. The supported route is the cookie manager:
   `ICoreWebView2CookieManager::GetCookies` to read them and `AddOrUpdateCookie` to put them back
   ([ICoreWebView2CookieManager](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2cookiemanager)).
   `wry` 0.56 wraps both as `WebView::cookies` and `WebView::set_cookie`, on WebView2 and WKWebView.
2. **Each tab's history.** Back and forward are answered from `BrowserTab::history`, which nothing
   writes down.
3. **The browser tabs in the editing area.** `project_state` skips any tab with no path.
4. **A local page in a node.** The written address names the tab id the page had in the last run.

A fifth thing is also worth fixing because the ticket says *"shared across projects"*: Unluminous has
no record of where a person has been. The engine keeps one, but in a format Unluminous does not read.
Unluminous keeps its own list, in the settings folder, and offers it under the address field.

Chromium writes its cookie database every 30 seconds, after 512 changes, or when asked to flush
([sqlite_persistent_cookie_store.h](https://chromium.googlesource.com/chromium/src/+/HEAD/net/extras/sqlite/sqlite_persistent_cookie_store.h)).
A persistent cookie set a few seconds before the window closes can therefore be lost as well. That
case is not covered here: the snapshot below keeps session cookies only, and the engine is left to
write its own.

## 2. Keeping a session

### 2.1 Session cookies

`services::browser_session` is a new module with no engine in it, so its tests need no window.

- **What is kept**: every cookie the engine reports with no expiry, with its name, value, domain,
  path, `Secure`, `HttpOnly` and `SameSite`. Persistent cookies are left to the engine, which already
  keeps them.
- **When it is read**: two seconds after a page finishes loading, and every 30 seconds while the view
  exists, from `BrowserHost::reconcile`. That runs in `raw_input_hook`, before the egui pass, which is
  the only place `CLAUDE.md` allows a call that pumps messages (`WebView::cookies` waits on a
  completion with `webview2_com::wait_with_pump`). It is also read once more just before the view is
  dropped, which is what closing the last rendered tab does. It is not read from `on_exit`, because a
  nested message pump inside the window's own shutdown is the shape that hung the window on
  `task-1756`.
- **Where it is written**: `<settings folder>/browser/session-cookies`, written to a temporary file
  and renamed, so a window that dies mid write leaves the previous file. All Unluminous windows share
  one browser process (one user data folder and the same options; see
  [the WebView2 process model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model)),
  so they share one cookie jar, and whichever window writes last writes the whole set. No merge is
  needed.
- **How it is protected**: on Windows the file is encrypted with `CryptProtectData` for the current
  user, which is what Chromium does with its own cookie values. On macOS it is written with mode
  `0600` inside the user's own Library folder. A file that will not decrypt is treated as no file.
- **When it is put back**: when the view is created, **before** its first navigation. The view is now
  built with no address, given the cookies, and then sent to the tab's address. Building it with the
  address would start the request before the cookies were there.

### 2.2 A tab's history

`BrowserTab` gains `history()` and `restore(history, position)`. The second clamps the position and
rewrites every local address to the new tab's id, because `unluminous://tab-<id>/` is how a page asks
for its own files and the id is handed out again on every run.

- **A node** writes its history and position to the realm's **sidecar**, beside the camera and the
  terminal sessions. Where one person had been is not part of what the realm is.
- **A tab in the editing area** is written to the project's `.unluminous/browser-tabs.conf`: its pane,
  its position and its list. On restore the tabs are opened after the files and put in their panes.
- `BrowserLocation::parse` accepts `unluminous://tab-<id>/<path>` and reads it as the file at `<path>`
  in the project, so a node written by an earlier build on a local page also comes back.

### 2.3 Where a person has been, across projects

`services::browser_session::Visits` is `<settings folder>/browser-history.conf`: the address, the
title, how many times and when, for the 500 most recent addresses. A page that finishes loading adds
one. Local `unluminous://` pages are not kept, because they name a tab and a project.

The address field offers up to six of them while it is being typed into, matched on the address and the
title, the most visited first. `Up` and `Down` choose, `Enter` goes, `Escape` puts the list away.
`unluminous-cli browser history [text]` reads the same list.

### 2.4 Upgrades

Everything above is in the settings folder. The Inno Setup script deletes nothing under `%APPDATA%`
on an upgrade or an uninstall, which was checked, and the macOS installer replaces the bundle only.

## 3. Pinning one element of a page in a node

### 3.1 What uBlock Origin does

uBlock's element picker ([epicker.js](https://github.com/gorhill/uBlock/blob/master/src/js/scriptlets/epicker.js))
runs a script in the page and draws its overlay as an SVG: one path for the whole viewport (the
"ocean") and one for the hovered elements (the "islands"), so the page is dimmed everywhere except the
element under the pointer. It builds a selector by walking up from the element, preferring an id,
then classes, then the tag with `:nth-of-type`, and checks the result with `querySelectorAll`. The
Firefox add-on Isolator, which shows one picked element on its own, moves to the parent with the wheel
up and back with the wheel down.

Unluminous copies all of that: the dimmed page with a hole and an outline, a label naming the element
and its size, the wheel and the arrow keys to move up and down the tree, and a selector built the same
way.

### 3.2 How the picker starts

- **On Windows, from the page's own right click menu.** WebView2 raises `ContextMenuRequested`
  (`ICoreWebView2_11`, runtime 1.0.1185 or later), and a host can add items to the default menu with
  `ICoreWebView2Environment9::CreateContextMenuItem`
  ([docs](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_11)).
  Unluminous adds `Select Element` to the top of the menu, and `Show Whole Page` when the node is
  pinned. The menu's `Location` is in the page's own coordinates, so the picker opens with the element
  under the pointer already chosen. The items appear only for a browser **node**, never for a tab in
  the editing area.
- **On every platform, from the node's toolbar**: a crosshair button beside the address field. macOS
  has no host hook into WKWebView's own menu that `wry` exposes, so this is the way in there.
- **From the command line**: `realm browser <node> pick`.

### 3.3 How the answer comes back, without a bridge

`CLAUDE.md` says a page gets **no JavaScript host bridge**, and this keeps that rule. The picker
script holds its answer in a closure. The host asks for it with `evaluate_script_with_callback` every
150 milliseconds while a pick is open, which is a call the host makes, not one the page can make. A
page could forge an answer to that question, but the only thing a forged answer can do is pin an
element of the page itself.

### 3.4 Showing only the element

The pin is the address it was picked on and the selector, written to the **realm file** as
`node.<id>.pin` and `node.<id>.pin.url`, because a node that shows one element is part of what the
realm is. It applies while the node is on that address, ignoring the fragment. Following a link out of
it shows the whole of the next page, and `Back` shows the element again.

The pin script:

1. Marks every sibling of the element and of each of its ancestors `visibility: hidden`. The element's
   own subtree is not touched, so anything it hides on purpose stays hidden. A rule of the form
   `body * { visibility: hidden }` followed by `el * { visibility: visible }` would show every closed
   menu inside it.
2. Clears the background, border and shadow of each ancestor, and paints the root with the first
   background colour found walking up from the element's **parent**, so the space around the element
   is the colour that was around it on the page. Starting at the element itself was tried first and
   painted a whole node green around a green button.
3. Sets `overflow: hidden` on the root, scrolls to the top, and measures the element.
4. Scales the root with `transform: translate(...) scale(s)` and `transform-origin: 0 0`, where `s`
   is the **same in both directions**, so nothing is stretched. The element is centred with 8 pixels
   of room around it.
   - `s = min(width / w, height / h)` fits the whole element, and is never more than 4: a small button
     at 8 times its size was measured and read as a picture of two words rather than of a button.
   - If fitting the height would make the element less than half the width it would have at the
     width's own fit, the element is taller than it is useful to shrink (a long article), so it is
     fitted to the width, at most twice its own size, and the wheel scrolls it instead. Wikipedia's
     infobox fitted to a wide node with no limit came out at over three times its size.
5. Fits again when the page resizes, when the element changes size (`ResizeObserver`), and every half
   second looks the selector up again, so a page that rebuilds the element still shows it.

The page keeps the layout width the node gives it. The scale only changes how large the result is
drawn, so text does not reflow. A transform on the root makes it the containing block for
`position: fixed` descendants
([MDN](https://developer.mozilla.org/en-US/docs/Web/CSS/Containing_block)), which is wanted here: a
fixed element that is picked is measured and scaled with everything else.

Unpinning removes every attribute, style and listener the script added, so the page is back as it was
without a reload.

The pin is applied after each page load that lands on the pinned address, and when the shared view
moves back to the node.

### 3.5 Commands

| Command | What it does |
|---|---|
| `realm browser <node> pick` | Opens the picker in the node's page. |
| `realm browser <node> pin --selector <css>` | Pins an element without picking it, which is what an agent does. |
| `realm browser <node> unpin` | Shows the whole page again. |
| `realm browser <node> pinned` | Says what is pinned, on which address, and whether it is showing now. |
| `browser history [text]` | The visited addresses kept across projects. |

`pin` and `unpin` change the realm file, so they are in `CHANGES_THE_FILE` and are refused while a
realm is read only.

## 4. Left out

- **A native menu item on macOS.** `wry`'s `WryWebView` would have to be subclassed or swizzled to
  answer `willOpenMenu:withEvent:`. The toolbar button covers it.
- **The engine's own `History` database.** It is the engine's file, in the engine's format, and can
  change between runtime versions. Unluminous keeps its own list.
- **Pinning in the editing area.** The ticket asks for the node. A pinned tab would be a smaller
  version of a feature the editing area does not need.
- **Restoring form contents and scroll position inside a page.** That is the engine's session restore,
  which WebView2 does not offer.

## 5. How it is checked

- Unit tests with no engine: the cookie filter and file format, the history round trip including the
  tab id rewrite, `parse` on an old `unluminous://tab-N/` address, the visit list's order, cap and
  matching, the sidecar and realm file round trips for history and pins, and the project state round
  trip for editing area tabs.
- The picker and pin scripts are checked in a real WebView2 against local pages with a wide table, a
  tall article, a small button, a picture and a fixed header, and against public pages, and each
  result is photographed with `tools/capture-window.ps1`, because a page is a native child that
  `window screenshot` does not contain.
- A session cookie set by a page is read back after Unluminous is closed and started again.
