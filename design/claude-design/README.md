# Designing Unluminous in Claude Design

Unluminous is drawn in Rust with egui, and its plugin panes with the `rux` component library. Both are
slow to iterate on visually: every change is a build and a launch. This folder holds a web copy of both,
published to Claude Design, so icons, depth, components and modals can be redesigned on the web first and
the result carried back into Rust.

Two artifacts in Claude Design are built from it:

| Artifact | What it is | Link |
|---|---|---|
| Black Rainbow Labs Rux | The design system: rux ported to React and CSS, with tokens for four themes, 34 rux components, 13 Unluminous window components, the 37 rux icons and the 73 Unluminous marks. | https://claude.ai/artifact/8UiZBtSJ1Q5vbv8cCUq7ez |
| Unluminous | The design canvas: ten artboards of the window as it is today, each built from that design system. | https://claude.ai/artifact/JUezj36TcXFSK8rkYNfhuq |

Both are private to Jason's account until shared from their Share menu.

## What is in the design system

- **Tokens** (`tools/build-tokens.mjs` writes `tokens.json`). Four themes with one set of names:
  `light` and `dark` are rux's Light and Dark Neumorphic, transcribed from `reference/neumorphic-tokens.css`
  and `reference/incognito-theme.css` in rux. `unluminous-dark` and `unluminous-light` are rux retoned
  from Unluminous's palettes, computed with the same arithmetic as `theme::rux_theme` and
  `theme::retoned_rux` in `crates/unluminous-app/src/theme/mod.rs`. Every Unluminous palette role is also
  a token, prefixed `ul-` (`ul-editor`, `ul-title-bar`, `ul-accent`, `ul-syntax-keyword`), and the
  measurements in `theme::size` are `ul-size-*`.
- **rux components** (`rux/src/components/*.jsx` without the `Ul` prefix). Each is a port of one Rust type
  in `black-rainbow-labs-rux/crates/rux/src/components/` at the commit Unluminous pins (`50b693f`), with the
  Rust's paddings, radii, sizes, type styles and elevations. Each has a guideline (`Name.md`) and a live
  preview (`Name.preview.jsx`). `rux/specs/CONVENTIONS.md` is the porting rules every component follows.
- **Unluminous window components** (`Ul*`): the title bar, the rail, the explorer, the tab strip, the
  editor, the status bar, a menu, a modal, a tile, a field, a button, the window, and `UlIcon`. These are
  drawn today by `crates/unluminous-app/src/components/`, not by rux; they are here so a redesign can
  start from the window as it is.
- **Icons.** The rux marks are read out of `crates/rux/src/icon/paths.rs` by
  `tools/extract-rux-icons.mjs`. The Unluminous marks are the exact shapes `theme::icon` paints, written as
  SVG by `cargo run -p unluminous-app --example export_icons_svg -- design/claude-design/unluminous/icons`,
  for both icon sets.

## What is on the canvas

Editing code (Unluminous Dark), Markdown side by side (Unluminous Light), the Agent-Chat pane, the
Agent-Tasks board, the ticket modal, the Settings modal, the menus and the terminal tile, the Realm canvas,
every window mark at life size and twice that, and the rux controls in both Unluminous themes. Each artboard
sets its theme on its root (`data-theme="unluminous-dark"`), so switching an artboard's theme is one
attribute.

## Rebuilding and republishing

```sh
npm install                       # once, from this folder: esbuild
npm run build                     # tokens, bundle, previews; RUX_DIR names the rux checkout
node tools/shoot.cjs ../../_agent_output/shots rux/build/previews/Button.html   # look at a preview in each theme
node tools/render-dc.cjs ../../_agent_output/canvas unluminous/canvas/project/Main.dc.html   # look at an artboard
node tools/publish-plan.mjs       # the design system's index and the list of files to send
```

Publishing is done through Claude's Artifact tool: the design system from `rux/system` (its `project/`
files, with `project/design-system.json` as the index), the canvas from `unluminous/canvas`. A changed
component means republishing `components/bundle.js` and `bundle.css` to the design system, then copying
both into the canvas at `project/ds/rux/components/`. Icon SVGs are uploads in the design system's asset
store; their ids are kept in `rux/specs/uploads-*.txt`, which `publish-plan.mjs` reads.

A rux preview is local proof only: the Design canvas runs its own runtime, and `tools/render-dc.cjs`
imitates enough of it (holes, `sc-for`, `sc-if`, `x-import`) to photograph an artboard before publishing.

## Carrying a design back into Rust

| What changed on the web | Where it goes in Rust |
|---|---|
| A rux token (surface, ink, accent, elevation, gradient, radius) | `crates/rux/src/theme/mod.rs` in `black-rainbow-labs-rux`, then bump the `rev` in Unluminous's `Cargo.toml` |
| A `ul-*` colour | `palette!` and `Palette::UNLUMINOUS_LIGHT` in `crates/unluminous-app/src/theme/mod.rs` |
| A `ul-size-*` measurement | `theme::size` in the same file |
| A rux component's look or behaviour | the component's file in `crates/rux/src/components/`, then the `rev` bump |
| An Unluminous window part (`Ul*`) | `crates/unluminous-app/src/components/<part>.rs` |
| A window mark | its function in `crates/unluminous-app/src/theme/icon.rs`; re-run `export_icons_svg` and the icon sheet test |
| The retone (how rux takes Unluminous's palette) | `theme::retoned_rux`, and `retone()` in `tools/build-tokens.mjs` to match |

After a change in Rust, run the build here again so the web copy stays the same as what ships.
