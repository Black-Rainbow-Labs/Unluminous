A dialog over a dimmed page: a scrim, a frame, a header with a close button, a body and an optional footer.

Source: Rust `Modal`, `Present`, `sheet_frame` and `split_body` (`components/modal.rs`), reference rules `.ic-modal-backdrop`, `.ic-modal`, `.ic-modal__head`, `.ic-modal__title-icon`, `.ic-modal__close`, `.ic-modal__body`.

## When to use
A task that needs the whole attention: choosing a prompt, editing a ticket, a confirmation that needs an answer. Not for a transient choice (use `Select`).

## Props
| Prop | Default | Meaning |
|---|---|---|
| `title` | | Header title and the dialog's accessible name. |
| `icon`, `iconTint` | | 28px raised chip left of the title with a 14px mark, `--accent-amber` unless `iconTint` (a CSS colour or var()). |
| `width`, `height` | 920, 640 | Wanted size in px, capped to `100vw - 48px` and `100vh - 64px`. |
| `footer` | | Node. When given, a 62px footer on `--surface-0` with a hairline over it (padding 14px 22px, 34px buttons, right aligned). |
| `left`, `leftWidth` | 260 | Node for the left half of the body, with a 1px `--surface-sunken` rule (Rust `split_body`). |
| `children` | | The body (the right half when `left` is given). |
| `open`, `onClose` | true | Close button, scrim press (unless `lightDismiss` is false) and `Escape` all call `onClose`. |
| `lightDismiss` | true | A dialog that must have an answer sets false. |
| `present` | `dialog` | `sheet`: 12px margin either side, up from the bottom edge, only top corners `--r-xl`, a 36 x 4 grabber in `--surface-sunken`, the device bottom inset added. Rust `Automatic` chooses sheet on a compact screen; pick it at the use site. |
| `raised` | false | `.ic-modal`'s own `--e-raised-lg, 0 30px 80px` shadow instead of `--e-modal`. The default is `--e-modal` because the pale half of `--e-raised-lg` reads as a halo over a dimmed page. |
| `inline`, `boxHeight` | false, 520 | Draw scrim and dialog inside a box of `boxHeight` px, in normal flow (for previews and docs). |

## Look
Frame radius `--r-xl`, `--surface-1`, `--e-modal`. Header padding 18px 22px, 1px `--surface-sunken` underneath; title 15px 600 `--ink-900`; close is a 32px round raised button, hover `--accent-coral`, press `--e-pressed-sm`. Scrim is `--scrim` with `backdrop-filter: blur(8px)` (the Rust does not draw the blur; the CSS does, as the reference has it).

## Consumer provides
What is in the body and the footer buttons. Focus handling beyond `Escape` is the caller's.

## Do / do not
Do give a dialog a title. Do put the primary action last in the footer. Do not stack dialogs. Do not close on scrim press when losing input would hurt.
