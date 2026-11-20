# task-2786: Unluminous issues (find in previews and pages, link menus, pages under other things)

Eight reports. Each section says what the cause is, measured or read in the code, and what changes.

## 1. Find in the Markdown preview

**Cause.** The Find bar is drawn only by `show_editor`, over the source. In the Preview view mode the source is never drawn, so `Cmd+F` opens a `Find` that nothing draws and that searches text the person cannot see.

**Change.** When the tab that is showing is in the Preview mode, Find searches the preview's own text (`Preview::text`) with the same `services::find::Find`. The bar is drawn over the preview, every match is painted behind the words in the find match color, the current match is the preview's selection, and the preview scrolls to it. Next, Previous, `Cmd+G` and Enter step through the matches. Replace is not offered there, because the preview is worked out from the source and a byte range in it is not a byte range in the file. The same applies to a note node or a File Editor node on the Realm that shows its preview. Opening and closing Find no longer moves the keyboard to the editing area while the Realm holds it, so Find works in a File Editor node on the canvas.

## 2. A right click on a link

**Cause.** `markdown_text` already opens a link menu (chat, a ticket, a Designer note), but the Markdown preview opens its own Copy menu, a Word hyperlink offers only "Open Hyperlink" (which goes to the system browser), and an address in the source editor has nothing. A link in a chat node on the Realm never answers, because `follow_a_link` asks `Context::layer_id_at`, and a node's layer has no `AreaState`, so the answer is never the node's layer.

**Change.** One menu everywhere, `Open in New Unluminous Tab`, `Open in Default Browser`, `Copy Link Address`:
- the Markdown preview opens the link menu when the right click is on a link;
- the source editor's right click menu gains the two open rows when the right click is on an address;
- a Word hyperlink's right click menu gains the same two rows;
- `follow_a_link` accepts a press when the layer at the pointer is the node's parent layer.

## 3. Send to Realm on a browser tab, and 8. a node over a browser node

**Cause (macOS).** A page is cropped with a mask on the WKWebView's layer, but AppKit sends the pointer and the scroll wheel to a view by its frame and `hitTest:`, never by the mask. So anything egui draws over a page loses its clicks, its wheel and then its keyboard to the page: a tab's right click menu row (`Send to Realm` sits over the page), an Agent Chat node over a browser node (the wheel scrolls the page, a click in the prompt focuses the page), a File Editor node over one (a drag selects the page's text). Windows is not affected, because a window region cuts the hits with the picture.

**Change.** `hitTest:` is added to wry's `WryWebView` class at run time. It answers nil for a point outside the parts of the page that are shown, so AppKit gives the event to the view under it, which is the window's own and egui sees it. The parts are the same rectangles the mask is made of, kept per view in a table the hit test reads. On macOS, "does the page hold the keyboard" is now asked of the window's first responder, and "is the pointer over the page" of the pointer and those same parts, so a click on the chat prompt gives the keyboard back to the window.

## 4. The black box round a tab's right click menu

**Cause (macOS).** The hole cut round a popup over a page is grown by 18 points for the popup's shadow. On Windows a photograph of the page is painted there; macOS cannot photograph a page synchronously, so the editor's dark ground was painted, and the menu's dark shadow went on top of it.

**Change.** On macOS the page is asked for its own background color (the body's, then the root element's, white when both are transparent) when it loads and when a popup first covers it, and that color is painted in the hole. The shadow then falls on what looks like the page.

## 5. The Realm is slow with a file preview and two browser nodes

**Measured.** Frame trace while turning the wheel and dragging over a realm with a note node in preview and two browser nodes: 191 frames, 206 ms average, `realm-nodes` 204 ms of it. With the note in source view: 0.15 ms.

**Cause.** `Cached::stale` is cleared only when the source is laid out. A document shown as its preview alone never lays its source out, so once any font or size change set `stale`, the whole preview (markdown, pictures, diagrams, layout) was rebuilt on every frame. The same happens to a Markdown tab in a pane in Preview mode after a font change.

**Change.** The preview has its own flag, `Cached::preview_stale`, set with `stale` through `Cached::mark_stale` and cleared when the preview is built.

## 6. Find in a browser tab or node

**Change.** `Cmd+F` while a browser tab or a browser node holds the keyboard opens a Find bar over the page, drawn on a foreground layer so the page is cut round it. Typing searches the page with `window.find` (case and direction honored, wrapping), Enter and Shift+Enter step, a count of matches in the page's text is shown, and Escape closes it. `unluminous-cli browser find` is the agent's half.

## 7. Cmd+C in a browser tab or node

**Cause.** wry's child web view answers NO to every key equivalent, so `Cmd+C` goes to the Edit menu. The menu's Copy is disabled when the empty document behind a page has no selection, and when enabled it sent `execCommand('copy')` only to a browser tab, which WebKit refuses outside a user gesture.

**Change.** The Edit menu's entries are on while a page holds the keyboard, and reach the page in a browser tab or in a browser node. Copy and Cut read the page's selection (the focused field's selection, or the document's) with a script and put it on the clipboard; Cut then deletes it; Paste, Select All, Undo and Redo use `execCommand` as before.

## Tests

Window tests for: Find in the preview (matches, stepping, scroll, no Replace), the link menu from the preview, the preview not being rebuilt every frame, the Edit menu being enabled and routed for a browser node, and the page find state through the command line. The macOS hit testing and the page color are checked by driving the installed build with `unluminous-cli input` against a realm with a chat node over a browser node.
