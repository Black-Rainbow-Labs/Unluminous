# task-2229: Rust notebooks, notebook completion, and the kernel picker (TDD)

The ticket asks for three things:

1. Full Rust support in notebooks, "with autocomplete, state between cells, etc."
2. Completion suggestions in notebooks.
3. A fix for the kernel picker, whose rows run past the edge of its list and whose button, when
   nothing has been chosen yet, draws over the Variables button beside it.

`tasks/task-2220-jupyter-notebooks-tdd.md` is the design this builds on. Everything below assumes it:
one document per notebook, cells behind `# %%` marker lines, and a kernel reached through
`kernel/bridge.py` running on the machine's own Python with `jupyter_client`.

## 1. What other tools do for Rust in a notebook

| Option | What it is | State between cells | Completion | Verdict |
|---|---|---|---|---|
| **evcxr_jupyter** | The Rust kernel for Jupyter, from the evcxr project (`cargo install evcxr_jupyter`, then `evcxr_jupyter --install`). Used by JupyterLab, VS Code and Google Colab's Rust support. | Yes. Each cell is compiled into a crate and run in a long lived child process; variables are kept in that process between cells. | Yes. It answers `complete_request` with rust-analyzer, which it links as a library (`ra_ap_ide`). | **Chosen.** |
| evcxr linked into Unluminous | Use the `evcxr` crate in process. | Yes | Yes | Rejected. It brings rust-analyzer and a compile pipeline into the editor's own binary (the kernel alone is 1m 43s of release build), and a cell that crashes would be inside the editor. |
| IRust's Jupyter kernel | A Python wrapper round the IRust REPL. | Yes | Partly | Rejected. Less maintained, needs IRust and a Python package, and it is a second way of doing what evcxr already does. |
| Talking ZeroMQ to evcxr from Rust | Skip the Python bridge for Rust notebooks. | Yes | Yes | Rejected for the reason task-2220 §2.3 gives: a ZeroMQ library is either a C dependency in every build or the pure Rust `zeromq` crate, which needs an async runtime this program does not have. |

So a Rust notebook runs on evcxr through the bridge that already exists. The bridge starts any kernelspec
`jupyter_client` can find, so the protocol work is already done; what is missing is everything around
it, and that is what this ticket builds.

**Measured on this machine** (evcxr 0.22.0, through `jupyter_client` from anaconda3):

- The kernel is ready 3.1 s after it is started.
- `let x: i32 = 5;` took 1.4 s the first time and later cells took 0.4 to 0.5 s each.
- `x + 1` in a later cell answered `6`: state is kept between cells.
- `:vars` answers with an `execute_result` whose `text/plain` is one `name: type` line per variable.
- `complete_request` on `x.ch` answered 18 matches with `cursor_start` 2, such as `checked_add(rhs)`,
  and `_jupyter_types_experimental` metadata giving each one's type (`function`, `module`,
  `instance`) and a display text (`checked_add(…)`).
- `complete_request` on `let y = s.` with the cursor after the dot answered 127 matches with an empty
  range (`cursor_start` = `cursor_end` = 10). A completion after a dot is the common case.
- `std::coll` answered `collections::` with type `module`, and `pri` answered `print!()` and
  `println!()`.
- `inspect_request` on `x` answered `x: i32`.
- `kernel_info_reply` gives `language_info.name` = `Rust` (capital R) and `file_extension` = `.rs`.

## 2. Rust notebooks

### 2.1 The notebook knows its language

Today `Grammars::for_path` reads every `.ipynb` as Python. Colouring already reads the language from
the notebook's metadata (`notebook_frame::notebook_extension`), but completion, the comment toggle and
the symbol reading do not, so a Rust cell would be offered Python's keywords and `Ctrl+/` would put `#`
in front of a Rust line.

`UnluminousApp::language_path(path)` answers with the path a notebook's code is read as: for an open
notebook tab, `cell.<extension>` from the notebook's own metadata, and for anything else the path
itself. `grammar_for`, `completion_applies_here`, the comment toggle and `tab_symbols` ask it. The
default stays Python, so every notebook that already works keeps working.

**When a kernel starts, its `language_info` is written into the notebook's metadata**, and the
kernelspec's `display_name` and `language` beside its `name`. That is what Jupyter does when it saves,
and it is what makes choosing the Rust kernel recolour the cells and change the completion: the metadata
is the one place every reader already looks.

### 2.2 Getting the kernel

The kernel picker lists kernelspecs under **Kernel**. When none of them is a Rust kernel, the picker
offers **Install the Rust kernel (evcxr)**, and so does the banner when a notebook asks for a kernel
named `rust` that is not installed.

Installing runs in the run tile, so it can be watched, as installing ipykernel does. The command is the
notebook's Python running a short script (`kernel::rust_install_command`), because the run tile runs
no shell and the job is two programs, one after the other:

1. Find `cargo` with `shutil.which`. If there is none, print where Rust comes from
   (`https://rustup.rs`) and stop.
2. `cargo install --locked evcxr_jupyter`.
3. `evcxr_jupyter --install`, found with `shutil.which` or in `~/.cargo/bin`, which writes the
   kernelspec into the user's Jupyter folder.

The kernelspec list is cached per Python. A listing older than ten seconds is asked for again when the
picker is opened, so a kernel installed while the window was open appears without a restart. The
bridge reports a notebook that asks for the `rust` kernelspec when none is installed as missing
`evcxr`, which is what puts the install button on the banner.

`unluminous-cli notebook kernel install-rust` is the same thing for an agent.

### 2.3 A new Rust notebook

`nbformat::empty_for(language)` writes the kernelspec and `language_info` for Python or Rust.
`notebook new --language rust` and **File -> New Rust Notebook** make one. The kernelspec name is
`rust`, which is the name `evcxr_jupyter --install` registers.

### 2.4 Variables

The bridge already asks a Python kernel for its variables through a `user_expressions` lambda. A Rust
kernel cannot run that. The bridge reads the kernel's language from `kernel_info_reply` when it starts
and restarts, and for Rust it sends `:vars` as a quiet execute (no history, no `input()`), takes the
`execute_result` for that request, and turns each `name: type` line into a row. evcxr does not give
values, so the value column is empty for a Rust variable and the panel draws only the name and type.
`notebook variables` answers for Rust as it does for Python.

### 2.5 What stays Python only

**Debug Cell** uses debugpy inside ipykernel. In a notebook whose language is not Python the cell's
Debug button is not drawn, and `notebook run --debug` and the menu entry are refused with a sentence
saying debugging needs a Python kernel. This is the absent control rule.

### 2.6 Export

`notebook export` gains `rs`, a Rust file with `// %%` and `// %% [markdown]` cells, and `md` fences a
code cell with the language in lower case (`rust`, not `Rust`). `py` still writes Python.

## 3. Completion in notebooks

### 3.1 What was already there and why it did not feel like completion

task-2220 added the kernel as a fifth source: when a word is typed in a code cell, the kernel is asked
once for that word, and its answer is filtered as more letters are typed. Four things kept it from
working the way a notebook user expects:

1. **Nothing is offered after a dot.** The popup needs two letters of a word, and `df.` has none. In a
   notebook the point of asking the kernel is the attributes of a live object, and those are exactly
   what comes after a dot.
2. **The kernel's range was ignored.** A kernel answers with the range its matches replace
   (`cursor_start` to `cursor_end`). ipykernel answers `%ti` with `%timeit` from the `%`, and a path in
   a string from the start of the path. The editor's stem stops at `%` and at `/`, so accepting
   inserted `%timeit` after the `%` that was already there.
3. **Rust matches carry their arguments.** evcxr answers `checked_add(rhs)` and `println!()`. Inserted
   as they are, the argument names become text in the cell.
4. **Tab indents.** In Jupyter, Tab after a word asks for completions. Here it inserted a tab.

### 3.2 What changes

- **A member access asks the kernel straight away.** After `.` in any notebook code cell, and after `::`
  in a Rust one, the kernel is asked with an empty stem and the popup opens on its answer.
  `completion_offer` treats that position like an import position: an empty stem with rows behind it.
- **Kernel matches are fitted to the editor's stem** by one pure function,
  `unluminous_jupyter::completion::fit`. A match whose range starts before the stem has the text
  between the two taken off its front (`%timeit` becomes `timeit` for the stem `ti`); a match that does
  not begin with that text is dropped, because inserting it would change text the person did not ask
  to change.
- **A Rust match is split into the name and its arguments.** `checked_add(rhs)` inserts `checked_add`
  and shows `function (rhs)`; `println!()` inserts `println!` and shows `macro`.
- **The row says what the kernel said it is.** `_jupyter_types_experimental` gives a type for each
  match. It becomes the row's kind (function, type, module, variable) and its detail, so a kernel row
  reads like a row from the file's own definitions.
- **Tab asks for completions** in a notebook code cell when there is no selection and the character
  before the caret is part of a word, a `.` or a `:`. Anywhere else Tab still indents. With the popup
  open Tab still accepts, because the popup takes the key first.

### 3.3 For an agent

`notebook complete` answers what the popup would offer at a cell and line, waiting for the kernel's
answer when the kernel is live, and `--choose` applies a row. It reads the same `completion_offer` the
popup reads. `editor complete` on a notebook waits for the kernel too, so the two cannot disagree.

## 4. The kernel picker

Two faults, and both are in the shared controls, so every dropdown gets the fix:

- **The button's words were never clipped.** `controls::dropdown_over` drew its value at full length.
  `No kernel yet · starts on the first run` is longer than the button, and drew over the Variables
  button. The value is now cut with an ellipsis to the room between the left edge and the chevron, and
  the whole value is the button's hover text.
- **A row's words were never clipped.** `controls::menu_row` drew its name at full length in a popup as
  wide as the button. A name that does not fit is now cut with an ellipsis, with the whole name as the
  row's hover text, and a row's quiet column (`shortcut`) is kept clear. The kernel picker also asks for
  a wider list (`controls::dropdown_wide`), and `no ipykernel` moves into the quiet column on the right
  of each Python row instead of the end of its name.

## 5. Tests

- `unluminous-jupyter`: `completion::fit` (prefix taken off, match dropped, empty range, Rust split,
  type mapping); `empty_for(Rust)`; the Rust export; the `:vars` parser.
- `unluminous-jupyter/tests/kernel.rs`: a real Rust kernel when `evcxr_jupyter` is installed, skipped
  with a printed note otherwise, like the Python ones: state between cells, `:vars`, completion after a
  dot.
- `unluminous-app`: the language path of a Rust notebook gives Rust's grammar and `//`; kernel matches
  offered after a dot; a fitted `%timeit`; the dropdown and row truncation functions; the Debug button
  absent in a Rust notebook.
- `crates/unluminous-app/tests/notebooks.rs`: a Rust notebook run in a real window, a completion popup
  after a dot, and the kernel picker open with every row inside it.
- The real window: a Rust notebook made with `notebook new --language rust`, run, completed and read
  back through `unluminous-cli`.
