# Porting rux to the web: conventions every component follows

`rux` is a Rust component library drawn with egui and vello. This folder is its web port, written so
the components can be designed in Claude Design and the result carried back into Rust. The Rust source
at the commit Unluminous pins (`50b693f`) is the truth. Copy its numbers exactly: paddings, radii,
heights, gaps, type sizes, letter spacing. 5px stays 5px. Where the Rust names a rule in
`reference/image-creator.css`, read that rule too, and prefer the CSS declaration when the two agree.

## Files

For each component `Name` (PascalCase, the Rust type name):

- `src/components/Name.jsx` exports the React component(s) (React 18, function components, hooks
  allowed). JSX is compiled by esbuild with `React.createElement`. Import shared pieces from
  `../icon.jsx` (`import { Icon } from '../icon.jsx'`). Do not import anything else from npm.
- `src/components/Name.css` holds every style the component needs. Nothing inline except computed
  geometry (a progress width, a fader position).
- `src/components/Name.md` is the guideline: first sentence is a one line summary. Then: when to use
  it, the props (a table), the variants and states, what the consumer provides, and do / do not. Name
  tokens in backticks. Say which Rust type and which reference rule it came from.
- `src/components/Name.preview.jsx` default exports a function component that renders the component
  in a few telling states (variants, sizes, disabled, on/off) on a `rux-stage` wrapper. Use realistic
  labels from the reference and from Unluminous (e.g. `Create`, `Prompts`, `Save`, `claude`,
  `Start Work`), never lorem ipsum.

One file per Rust type is the default, but a Rust file that defines several public types (toggle.rs:
`Segmented`, `DiceToggle`, `Switch`) gets one file per type.

## Class names

- Block: `rux-<kebab name>` (`rux-button`, `rux-icon-button`, `rux-text-input`).
- Variant and size modifiers: `rux-button--primary`, `rux-button--lg`.
- Parts: `rux-select__trigger`, `rux-select__menu`.
- State a caller controls: `is-on`, `is-chosen`, `is-open`, `is-active`, `is-disabled`.
- Pointer states use real pseudo classes: `:hover`, `:active`, `:disabled`. A press moves the control
  `translateY(0.5px)` where the Rust does (`Visual::Active`).
- Keyboard focus: `:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }`.
- Disabled: `opacity: 0.4` (the Rust `Visual::fade`), `cursor: default`, no hover change.
- Cursors are the stylesheet's: `pointer` on anything pressable, `text` in a field, `grab` on a handle.

## Tokens (CSS custom properties, already defined per theme by tokens.css)

Never write a colour literal in a component's CSS. Use these:

Surfaces: `--surface-0` `--surface-1` `--surface-2` `--surface-3` `--surface-sunken`
Shadow tones: `--shadow-light` `--shadow-dark` `--shadow-darker`
Ink: `--ink-900` `--ink-700` `--ink-500` `--ink-400` `--ink-300` `--ink-200`
Accents: `--accent-blue` `--accent-blue-deep` `--accent-blue-soft` `--accent-coral` `--accent-violet`
`--accent-mint` `--accent-amber` `--accent-rose`
Semantic: `--success` `--warning` `--danger`
Interaction: `--hover` `--pressed` `--hairline` `--accent-wash` `--accent-wash-soft` `--scrim`
`--focus-ring` `--on-accent` (white words on a gradient fill)
Gradient ends (compose with `linear-gradient`): `--grad-primary-from/-to` (180deg),
`--grad-primary-diagonal-from/-to` (160deg), `--grad-danger-from/-to` (180deg),
`--grad-danger-diagonal-from/-to` (160deg), `--grad-mint-from/-to` (160deg),
`--grad-violet-from/-to` (160deg), `--grad-nav-active-from/-to` (165deg), `--grad-knob-from/-to`
(145deg), `--grad-avatar-from/-to` (160deg), `--grad-image-well-from/-to` (160deg),
`--grad-progress-from/-to` (90deg), `--grad-multistop-from/-to` (90deg)
Elevations (complete `box-shadow` values): `--e-raised-sm` `--e-raised` `--e-raised-lg`
`--e-pressed-sm` `--e-pressed` `--e-pressed-deep` `--e-modal` `--e-primary` `--e-primary-pressed`
`--e-primary-sm` `--e-nav-active` `--e-image-well` `--e-menu`
Radii: `--r-sm` 8px, `--r-md` 12px, `--r-lg` 18px, `--r-xl` 24px, `--r-2xl` 32px, `--r-pill` 999px
Fonts: `--font-sans` (Inter), `--font-mono` (JetBrains Mono)

An accent at an opacity, which the Rust writes as `theme.x.at(0.35)`, is
`color-mix(in srgb, var(--x) 35%, transparent)`. `Theme::glowing(glow, lift)` is
`box-shadow: calc(-1*lift) calc(-1*lift) calc(2*lift) var(--shadow-light), lift lift calc(3*lift) <glow>,
inset 1px 1px 1px rgb(255 255 255 / 0.25)` written out with numbers.

Type styles from `text.rs`, as utility classes already in the base stylesheet and also usable as
plain declarations: `.rux-t-display` 600 48/1.05 -0.03em, `.rux-t-h1` 600 32/1.2 -0.02em,
`.rux-t-h2` 600 24/1.3 -0.015em, `.rux-t-h3` 600 18/1.35 -0.01em, `.rux-t-body` 400 15/1.5,
`.rux-t-base` 400 14/1.5 -0.005em, `.rux-t-label` 500 13/1.4, `.rux-t-caption` mono 500 11/1.4
uppercase 0.14em, `.rux-t-field-label` mono 400 9.5 uppercase 0.14em, `.rux-t-ctrl-label` mono 400 9
uppercase 0.14em, `.rux-t-panel-title` mono 500 10.5 uppercase 0.16em, `.rux-t-control` 500 12
-0.005em, `.rux-t-control-lg` 500 13 -0.01em, `.rux-t-button` 600 14 -0.005em, `.rux-t-prose` 400
13/1.6, `.rux-t-number` mono 600 12.

## Icons

`<Icon name="search" size={12} />` renders the rux `SbIcon` mark (24 viewBox, stroke 1.8 by default,
`currentColor`). Names: search plus x check chevDown chevLeft chevRight arrowRight home docs spark dots
film storyboard layers folder settings bell user trash download heart star chat cube image play pause
music imagePlus edit video upload grip expand copy wand. Pass `stroke` to change the width.

## Props

Mirror the Rust builder: a builder method becomes a prop with the same name in camelCase
(`.primary()` -> `variant="primary"`, `.size(ButtonSize::Large)` -> `size="lg"`, `.icon(Icon::Spark)`
-> `icon="spark"`, `.enabled(false)` -> `disabled`, `.stretch()` -> `stretch`). An outcome the Rust
returns becomes a callback (`onClick`, `onChange(value)`, `onToggle(open)`). Components are
controlled where the Rust keeps state in the caller (a select's open flag, a text value): accept
`value`/`open` and fall back to internal state when they are not passed, so a preview works with
no wiring.

Every component forwards `className` and `style` to its root and spreads unknown props onto the root
so `data-*` and `aria-*` work. Give every control an accessible name (`aria-label` when it has no
words).

## Depth in CSS

A rux surface is `background` plus `box-shadow: var(--e-…)`. A well is a sunken surface with an inset
elevation. A hover wash is a `::after` overlay or a background change using `--hover`, never a new
shadow, because rux never changes elevation on hover (only on press). The modal and menus float with
`--e-modal` / `--e-menu`.

## What not to do

No Tailwind, no CSS frameworks, no web fonts loaded by URL (the fonts are installed by the system),
no `!important`, no colour literals except white and black inside an inset highlight that the Rust
itself writes as `Color32::WHITE.at(n)` / `Color32::BLACK.at(n)`.
