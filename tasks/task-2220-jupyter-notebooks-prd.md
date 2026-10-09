# task-2220: Jupyter notebooks in Unluminous (PRD)

## The request

> Deep research online how IntelliJ works with Jupyter notebooks. Use our IntelliJ to further explore
> features, take screenshots, etc. We want full feature parity. Create a PRD for all the features and
> functionality, then a TDD, then fully implement.

The research behind this document is in `_agent_output/task-2220-notebooks/` (gitignored):
`research-intellij-jupyter.md` is the online research with every source, and
`intellij-exploration.md` with `intellij-screens/` is the exploration of the PyCharm 2025.1 installed on
this machine. JetBrains ships the same notebook editor in IntelliJ IDEA Ultimate and in PyCharm, so the
two are one reference.

## Who uses it

- **A person** opens an `.ipynb` file from the explorer, writes code and Markdown in cells, runs them
  against a Python kernel, and reads the output under each cell, the way they would in PyCharm.
- **An agent** does the same through `unluminous-cli` and the MCP tools: reads the cells, edits them,
  runs one or all of them, and reads back what each printed or drew. Unluminous's own rule is that every
  control a person has, an agent has too, through the same code.

## Goals

1. Opening an `.ipynb` file shows a notebook: cells one under another, each code cell with its output
   below it, Markdown cells rendered.
2. A cell runs in a real Jupyter kernel started from the machine's own Python, with no separate Jupyter
   server to install or start by hand.
3. Saving writes a valid `.ipynb` that Jupyter, VS Code and PyCharm open. A notebook opened and saved
   with no change is written back byte for byte.
4. Every editor feature Unluminous already has works inside cells: undo, find and replace, multiple
   panes, completion, go to definition, rename, the command line's `editor` commands.
5. Parity with the IntelliJ notebook editor for everything that runs locally, listed below.

## What IntelliJ does, and what Unluminous will do

Each row is one IntelliJ behaviour from the research. The last column is the decision.

### Files

| IntelliJ | Unluminous |
|---|---|
| `.ipynb` recognised, own icon | Yes. A notebook icon in the explorer and on the tab. |
| New notebook from the project view or Alt+Insert | Yes. `File -> New Jupyter Notebook` and the explorer's right click menu. A new notebook has one empty code cell and the Python 3 kernelspec in its metadata. |
| Convert `.py` to notebook, notebook to `.py` | Yes. A `.py` file with `# %%` markers becomes one cell per marker; a file with none becomes one cell. |
| Export as HTML, Markdown, Python | Yes, written by Unluminous itself so it needs no `nbconvert`. HTML includes the outputs, pictures embedded. |
| External change reloads the notebook | Yes, the rule every Unluminous tab already follows. |
| Clear outputs before committing | Yes, as `Notebook -> Clear All Outputs` followed by save; no separate commit option. |
| Open notebook in browser, Gists, Trust Notebook | No. Open in browser needs a Jupyter server, which this design does not run. Gists is a GitHub service. Unluminous never runs JavaScript from an output, so there is nothing for trust to unlock. |

### Editing and the two modes

| IntelliJ | Unluminous |
|---|---|
| Edit mode: caret in a cell, Escape leaves it | Yes. |
| Command mode: a selected cell, keys act on whole cells, Enter returns to edit mode | Yes. The selected cell has a highlighted frame. |
| Up and Down move between cells in command mode, Shift extends | Yes. |
| Up on the first line of a cell goes to the cell above | Yes. The text is one document, so this is what the caret already does. |
| Ctrl+Home and Ctrl+End go to the start and end of the cell (edit mode) or first and last cell (command mode) | Yes. |
| Ctrl+A selects the cell's text, again for everything | Yes. |
| Cell types: code, Markdown, raw | Yes. SQL cells and AI cells are JetBrains products and are not part of this. |
| Change cell type: toolbar selector, M, Y, R | Yes, all three. |
| Add above/below: A, B, Alt+Shift+A, Alt+Shift+B, toolbar, the add buttons between cells | Yes. |
| Copy, cut, paste below and above: C, X, V, Shift+V | Yes, including several selected cells at once. |
| Delete: D D and Delete | Yes. Z undoes the last cell deletion, as does Ctrl+Z. |
| Duplicate, move up, move down | Yes. Ctrl+Shift+Up and Ctrl+Shift+Down move a cell. |
| Merge with the cell above or below, merge selected cells, split at the caret | Yes. Shift+M merges, Ctrl+Shift+Minus splits. |
| Collapse a cell, collapse a Markdown section | Yes. A collapsed section shows how many cells it holds. |
| Cell tags | Yes, shown on the cell and edited from its menu. |
| Comment out cells | Yes, Ctrl+/ on the selected cells. |
| Drag a cell by its handle | Yes. |
| Line numbers per cell, toggled | Yes, numbered within each cell. L toggles them. |

### Running

| IntelliJ | Unluminous |
|---|---|
| Ctrl+Enter runs the cell and stays | Yes. |
| Shift+Enter runs and selects the cell below, adding one at the end | Yes. On a Markdown cell it renders it. |
| Alt+Enter runs and inserts a cell below (classic Jupyter) | Yes. |
| Run all (Ctrl+Alt+Shift+Enter), run above, run cell and below, run a Markdown section | Yes. |
| Run button in the gutter of each cell and on the toolbar | Yes. |
| Execution count beside the cell, `[*]` while running, queued cells marked | Yes. |
| Duration of the last run, and when it finished | Yes, at the bottom of the cell. |
| A notice when a cell runs longer than 60 seconds | Yes. |
| Interrupt, restart, restart and run all, shut down | Yes. I I interrupts and 0 0 restarts in command mode. |
| Status: running, queued, finished, failed | Yes, drawn on the cell. |
| Stdin (`input()`) | Yes, a field under the cell. |

### Kernels

| IntelliJ | Unluminous |
|---|---|
| Uses the project's interpreter; a notebook with no project uses the global one | Yes. Unluminous looks for `.venv`, `venv`, `env` in the project, then `VIRTUAL_ENV`, `CONDA_PREFIX`, the Python on `PATH`, the `py` launcher, and conda's base. |
| Starts a kernel on the first run, no manual start | Yes. |
| Since 2025.3, IPyKernel directly instead of a full Jupyter server | Yes, the same choice: Unluminous starts the kernel itself through `jupyter_client`, which `ipykernel` installs. |
| Choose the interpreter and the kernel from a toolbar drop down | Yes. Every kernelspec the chosen Python knows is listed, so R or Julia kernels installed there work too. |
| Missing `ipykernel` | IntelliJ shows an error. Unluminous offers to install it, running `python -m pip install ipykernel` in a terminal tab so the person sees it happen. |
| External servers, JupyterHub, SageMaker, Kaggle, Google Colab | No. Each is a remote service; this ticket is the local notebook. |

### Outputs

| IntelliJ | Unluminous |
|---|---|
| stdout and stderr streams | Yes. Stderr on a red tinted background. |
| ANSI colours in tracebacks, the traceback collapsed | Yes. The error line shows; the full traceback opens with one click. |
| PNG and JPEG pictures (matplotlib) | Yes, drawn inline at their own size, shrunk to fit the cell. |
| SVG | Drawn by the page view when opened in a browser tab; inline it shows its text alternative. |
| HTML | Tables (every pandas DataFrame) are drawn as a table that can be sorted by a column. Other HTML is shown as its text, with a button that opens the HTML in a browser tab, which renders it exactly. |
| DataFrame table view, Data View with filters and statistics | The table, sorting, and a row count; Data View's charts and data quality checks are JetBrains products and are not part of this. |
| Markdown output | Yes, rendered the way a Markdown cell is. |
| LaTeX, JSON | LaTeX as its source in a formula style; JSON pretty printed. |
| `update_display_data`, `clear_output(wait=True)` | Yes. A progress bar that redraws itself redraws in place. |
| ipywidgets and other JavaScript output | No. They need a JavaScript runtime connected to the kernel. The text fallback the kernel sends is shown. |
| Scroll long outputs, a maximum height | Yes. Outputs taller than 30 lines scroll inside the cell; a setting turns it off. |
| Clear one output, clear all | Yes. |

### Around the notebook

| IntelliJ | Unluminous |
|---|---|
| Variables tool window: name, type, value, shape; refresh; sort | Yes, a side panel in the notebook tab, refreshed after each run. |
| `?name` introspection | Yes. The kernel's answer shows as the cell's output, as in Jupyter. |
| Completion from the kernel | Yes, Ctrl+Space in a code cell asks the kernel, beside the completion Unluminous already has. |
| Structure view of headings and cells | Yes. `Notebook -> Notebook Outline` opens the toolbar's outline of headings and cells, and selecting one goes to it. Unluminous has no Navigate menu, so the entry is in the Notebook menu. |
| Jump to next and previous section (Ctrl+Alt+Down and Up) | Yes. |
| Debug a cell | Yes, through ipykernel's own debugger and the debugger tile Unluminous already has: breakpoints in the cell's gutter, step, the variables tree. |
| Inline values | Shown while debugging, as Unluminous already does for files. |
| AI cell editing, AI charts, data quality checks | No. JetBrains AI products. Unluminous's agent chat can already read and edit a notebook through the commands below. |

## For an agent

A `notebook` area in the command catalogue, so the commands are MCP tools the day they exist:

| Command | What it does |
|---|---|
| `notebook status` | The kernel, its state, and every cell: index, id, kind, execution count, state, first line, a summary of its outputs. |
| `notebook cell <n>` | One cell's source and its outputs as text. A picture is written to a file and its path given. |
| `notebook run` | Run one cell, a range, everything above or below, or all; `--wait` waits and returns the outputs. |
| `notebook add`, `notebook delete`, `notebook move`, `notebook kind`, `notebook merge`, `notebook split` | The cell operations, by index or id. |
| `notebook source <n>` | Replace a cell's source. |
| `notebook clear` | Clear one cell's outputs or all of them. |
| `notebook kernel` | Start, interrupt, restart, shut down, list the Pythons and kernelspecs, choose one. |
| `notebook variables` | The variables panel's rows. |
| `notebook input` | Answer an `input()` the kernel is waiting on. |
| `notebook export` | Write HTML, Markdown or Python. |
| `notebook new` | Make a new notebook. |

Every menu entry under `Notebook` is also reachable through `unluminous-cli action`, which is automatic.
The notebook's text is the tab's document, so `editor text`, `editor replace` and the rest work on a
notebook with nothing added.

## Success measures

- A notebook written by Jupyter opens, saves unchanged, and the file is identical to the original.
- A notebook with every output type in it opens in Jupyter after Unluminous has run it and saved it.
- A person can do every row marked Yes above, by the key or the button IntelliJ uses.
- An agent can open a notebook, change a cell, run it, and read the result, using only the catalogue.
- Tests over both, in the four layers `CLAUDE.md` describes.

## Not part of this ticket

SQL cells, AI cells, remote and hosted Jupyter servers, Google Colab, ipywidgets and JavaScript
outputs, Gists, the Data View's charts and data quality checks, opening the notebook in a browser
through a Jupyter server. Each is a service or runtime this design does not run, and each is listed in
the table it belongs to with the reason.
