//! Word documents: the `.docx` file, its styles and lists, editing, pagination and the exports.
//!
//! These modules, and none of them knows there is a window:
//!
//! - [`zip`], [`xml`] and [`package`] read and write the container: the zip, the XML of each part, the
//!   content types and the relationships.
//! - [`model`] is the document as Unluminous holds it, and [`read`] and [`write`] turn a file into one and
//!   back, keeping every part and element nobody modelled. [`order`] is the schema's order for each
//!   properties element, which is what keeps Word from calling a written file damaged.
//!
//! **No user interface dependency**, for the reason `unluminous-core` has none: the tests run with no
//! window, no graphics card and no fonts. Text is measured through `unluminous_core::FontMetrics`.

pub mod builtin;
pub mod edit;
pub mod export;
pub mod fields;
pub mod kern;
pub mod layout;
pub mod math;
pub mod media;
pub mod model;
pub mod new;
pub mod numbering;
pub mod ops;
pub mod order;
pub mod package;
pub mod pdf;
pub mod position;
pub mod props;
pub mod read;
pub mod references;
pub mod session;
pub mod spell;
pub mod styles;
pub mod write;
pub mod xml;
pub mod zip;

pub use model::Document;
