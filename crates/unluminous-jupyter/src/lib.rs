//! Jupyter notebooks: the `.ipynb` file, the text a notebook tab edits, and the kernel a cell runs in.
//!
//! These modules, and none of them knows there is a window:
//!
//! - [`nbformat`] reads and writes the `.ipynb` file itself, keeping everything it does not
//!   understand so that a notebook opened and saved unchanged is written back byte for byte.
//! - [`text`] turns a notebook into the one piece of text a notebook tab edits, and that text back
//!   into cells. Each cell starts with a marker line, so every editor command already works on a
//!   notebook and an agent can read one with `editor text`.
//! - [`kernel`] starts a Jupyter kernel through the machine's own Python and `jupyter_client`, and
//!   speaks to it on a thread, arranged the way `unluminous_dap::Client` is.
//! - [`outputs`] chooses what to draw for a cell output and reads colour codes, HTML tables and
//!   progress lines into data a window can lay out.
//! - [`export`] writes a notebook as a Python file, Markdown or a web page, and reads a Python file
//!   back as a notebook.
//!
//! **No user interface dependency**, for the reason `unluminous-core` and `unluminous-dap` have none:
//! the tests run with no window, no graphics card and no fonts.

pub mod export;
pub mod kernel;
pub mod nbformat;
pub mod outputs;
pub mod text;
