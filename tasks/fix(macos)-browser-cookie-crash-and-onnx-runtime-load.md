fix(macos): stop the browser cookie crash and let the code index load ONNX Runtime

## Summary

Two macOS faults, both found on 2026-10-14 after updating to 0.84.4.

- **Unluminous closed itself within a second of starting.** It happened on every start of a project that had a browser tab open with saved sign in cookies. Going back to 0.84.3 did not help: it has the same code, and 0.78.0 crashed the same way once on 2026-10-12.
- **The code index never computed embeddings.** Embeddings are the number vectors the code index uses to answer plain English searches. In every Developer ID build, the first search in plain English wrote a panic to `crash.log` on the index thread, and search used word matching alone.

Neither fault is new in 0.84.4. That release only redrew the logo.

### Background

- [`wry`](https://github.com/tauri-apps/wry) is the library Unluminous uses to put a system browser view in a tab. On macOS that view is a WebKit `WKWebView`.
- [`winit`](https://github.com/rust-windowing/winit) is the library that owns the window and hands it events, such as "redraw now". It does not allow a second event to arrive while it is still handing one over, and it panics if one does.
- Session cookies are cookies with no expiry date, such as the one a sign in page sets. WebKit forgets them when the app closes. Unluminous saves them in `browser/session-cookies` in the settings folder (added by task-2203) and gives them back to WebKit the next time a browser view is created.
- The hardened runtime is a set of macOS restrictions that notarization requires. One of them, library validation, refuses to load a shared library unless it is signed by the same developer team as the app, or by Apple.
- ONNX Runtime is Microsoft's machine learning runtime. The code index uses it, through the [`ort`](https://github.com/pykeio/ort) crate, to compute embeddings. `ort` loads `libonnxruntime.dylib` while the app is running, and `inillucent setup-embeddings` installs that file under `~/Library/Application Support/inillucent/runtime/`.

### Fix 1: Unluminous closed itself on start when a browser tab had saved cookies

**What was seen.** The window opened and closed again, four times in a row. The crash reports said `EXC_CRASH (SIGABRT)`, and `crash.log` had this pair of entries each time:

**Evidence: `~/Library/Application Support/Unluminous/crash.log`, written by 0.84.4.**
```
on   the main thread
in   .../winit-0.30.13/src/platform_impl/macos/event_handler.rs:135:17
said tried to handle event while another event is currently being handled
  ...
  13: wry::wkwebview::wait_for_blocking_operation
  14: wry::wkwebview::InnerWebView::set_cookie
  15: unluminous_app::services::browser::BrowserHost::reconcile
  16: <unluminous_app::app::UnluminousApp as eframe::epi::App>::raw_input_hook
  ...
  24: winit::platform_impl::macos::app_state::ApplicationDelegate::handle_redraw
---
said panic in a function that cannot unwind
```

**Why it happened.** `BrowserHost::reconcile` runs while `winit` is handing the app a redraw. When it creates the browser view, it gives WebKit each saved cookie with `wry`'s `set_cookie`. On macOS, `set_cookie` waits up to one second for WebKit's answer, and it waits by running the main run loop by hand:

**Source:** [`wry` 0.56.1, `src/wkwebview/mod.rs` lines 1467 to 1493](https://github.com/tauri-apps/wry/blob/wry-v0.56.1/src/wkwebview/mod.rs#L1467-L1493)
```rust
unsafe fn wait_for_blocking_operation<T>(rx: std::sync::mpsc::Receiver<T>) -> Result<T> {
  ...
  loop {
    if let Ok(response) = rx.recv_timeout(interval) {
      return Ok(response);
    }
    ...
    // Go progress the event loop if we didn't get the result
    let rl = objc2_foundation::NSRunLoop::mainRunLoop();
    ...
    rl.acceptInputForMode_beforeDate(mode, &limit_date);
  }
}
```

That lets other work queued on the main thread run in the middle of the redraw. When that work delivers a second event to `winit`, `winit` panics. The panic happens inside an Objective C block, where Rust cannot unwind, so Rust aborts the process. With fifteen cookies saved there are fifteen of these waits in a row, so a second event almost always arrives. The cookie read every 30 seconds (`view.webview.cookies()`) waits the same way, so it could close the window at any time a browser tab was open, not only at start.

**What changed.** On macOS, Unluminous no longer calls `wry`'s cookie functions. It reaches WebKit's cookie store directly through `objc2-web-kit`, which is already in the build through `wry`, and nothing waits:

- When a browser view is created, every saved cookie is handed to `WKHTTPCookieStore` at once. Each completion block only counts down and asks for a frame. The view is created with no address, and the first page loads on a later frame, once WebKit has stored every cookie or after two seconds (`FIRST_PAGE_WAITS`). Waiting for the cookies keeps what task-2203 did: the first request of the first page already carries the sign in.
- The periodic read is split in two. `ask_for_session_cookies` asks WebKit, and `take_session_cookies` collects the answer on a later frame. The answer is kept on the host, not on the view, so the final read made just before the last browser tab closes is still written to disk.
- A newer address, from `navigate` or from switching tabs, replaces the first page that is waiting, so a page that loads late can never override a newer one.

Windows is unchanged. WebView2 pumps its own messages, and the Windows path calls the same `set_cookie` and `cookies` at the same moment as before.

**Source:** [`unluminous`, `crates/unluminous-app/src/services/browser.rs` lines 2330 to 2513](https://github.com/jasonmcaffee/unluminous/blob/c235cf3f6999efd84e21b9d08c5d8680a076a7ab/crates/unluminous-app/src/services/browser.rs#L2330-L2513) is the new `cookies` module, and [lines 854 to 880](https://github.com/jasonmcaffee/unluminous/blob/c235cf3f6999efd84e21b9d08c5d8680a076a7ab/crates/unluminous-app/src/services/browser.rs#L854-L880) are `keep_the_session` asking and then taking.

### Fix 2: the code index could not load ONNX Runtime in a Developer ID build

**What was seen.** `crash.log` had 36 entries with the same panic, from 0.59.0 through 0.84.4:

**Evidence: `~/Library/Application Support/Unluminous/crash.log`.**
```
on   the atrius-index-store thread
in   .../ort-2.0.0-rc.13/src/lib.rs:239:14
said `OrtGetApiBase` must be present in ONNX Runtime dylib: DlSym { source: "dlsym(0x0, OrtGetApiBase): invalid handle" }
```

The panic stays on the index thread, so the window kept running. Search in plain English used word matching only.

**Why it happened.** Inillucent found the installed runtime and handed its path to `ort`, and `dlopen` refused it. A small test program signed the way `build.sh` signs Unluminous showed the reason the panic message leaves out:

**Evidence: `dlopen` of the installed `libonnxruntime.dylib` from a program signed with the Developer ID and `--options runtime`.**
```
code signature in '.../libonnxruntime.1.22.0.dylib' not valid for use in process:
mapping process and mapped file (non-platform) have different Team IDs
```

Microsoft's release of the library is signed ad hoc, so its signature names no team. `build.sh` signs both programs with the hardened runtime and no entitlements, which leaves library validation on.

**What changed.** A new file, [`installer/macos/Unluminous.entitlements`](https://github.com/jasonmcaffee/unluminous/blob/c235cf3f6999efd84e21b9d08c5d8680a076a7ab/installer/macos/Unluminous.entitlements), has one entry: `com.apple.security.cs.disable-library-validation`. Both `unluminous` and `unluminous-cli` are signed with it, by [`installer/macos/build.sh`](https://github.com/jasonmcaffee/unluminous/blob/c235cf3f6999efd84e21b9d08c5d8680a076a7ab/installer/macos/build.sh#L289-L299) and by [`installer/macos/build-on-windows.ps1`](https://github.com/jasonmcaffee/unluminous/blob/c235cf3f6999efd84e21b9d08c5d8680a076a7ab/installer/macos/build-on-windows.ps1). `unluminous-cli` needs it as well because `unluminous-cli search serve` hosts the index when no window is running. The same test program loaded the library with this entitlement.

The cost: with library validation off, Unluminous can load any validly signed library on the system, not only libraries signed by this team. Apple's notarization accepts this entitlement. The alternative would be to ship ONNX Runtime inside the bundle and sign it with the team's identity. That adds about 70 MB to the download and duplicates something Inillucent already installs.

## Verification Steps or Test Evidence

Run on this Mac (macOS 26.6.2, Apple silicon) with a build from `installer/macos/build.sh --install --no-dmg`, signed with the Developer ID and not notarized.

Browser cookie crash:
1. Started the build on the `alle-experience-bfe` project, which had a restored browser tab and fifteen cookies in `browser/session-cookies`, and brought the window to the front.
2. Opened `https://httpbin.org/cookies/set?unluminous_check=1` with `unluminous-cli browser open`.
3. 🧪 `browser status --json` said `"showing": true`. That means the web view existed, so the cookie handover had run.
4. 🧪 The process kept running, there was no new crash report, and `crash.log` had no new entry.
5. 🧪 About 30 seconds later, `browser/session-cookies` had been rewritten (3078 bytes to 3202) and contained `unluminous_check@httpbin.org`. That shows the saved cookies were given to WebKit and the read came back.

ONNX Runtime:
1. Restarted the window and the search host on the new build.
2. Ran `unluminous-cli search find "where are offer eligibility rules checked"` in `alle-experience-bfe`.
3. 🧪 The answer was ranked by meaning, `crash.log` had no new entry, and `vmmap` on the window's process listed `libonnxruntime.1.22.0.dylib` as loaded.

Checks:
- `cargo build -p unluminous-app` and `cargo fmt --check` pass.
- `cargo test -p unluminous-app --lib browser`: 35 passed.
- `cargo clippy -p unluminous-app --lib -- -D warnings` reports two errors, and both are in code this PR does not touch. One is `characters_to_read`, a function in `unluminous-terminal/src/foreground.rs` that only Windows uses, so it is dead code on a Mac. The other is a redundant closure in `services/system_files.rs`.
- The window suite and a Windows build were not run. The Windows code path for cookies is unchanged, but `build-on-windows.ps1` now passes `--entitlements-xml-file` to `rcodesign`, and I have not run that script.

No automated test covers either fix. The cookie crash needs a real `WKWebView` and a run loop, and the window suite renders offscreen through `egui_kittest`, so it creates no web view. Library validation only applies to a signed bundle.

## What is not in this PR

- No version bump and no release. `tools/release.sh` was not run.
- The crash on quit seen once while testing is not addressed. A window that had loaded ONNX Runtime aborted inside `exit()` with a C++ `std::terminate` during static destructors (`__cxa_finalize_ranges`). It happened once in four quits, has not been reproduced, and needs a ticket of its own.
- Windows still calls `wry`'s blocking cookie functions. That has not been reported to crash there.

## Reaches users

Both fixes take effect in the next macOS build made from this branch. There is no setting or flag. A person who updates will see two differences: Unluminous stays open on a project with a browser tab and saved cookies, and plain English search uses embeddings.
