//! The code index: what agents query instead of ripgrep.
//!
//! One index per checkout, kept in one Inillucent file outside the repository, owned by one host
//! process that also watches the files. It answers four kinds of question: exact text and regex with
//! the same results as ripgrep (`exact`), where a name is defined and used, questions written in plain
//! English, and "show me just this function". `tasks/task-2138-unluminous-code-index-tdd.md` is the
//! design, and `tools/search-eval/` is the harness that measures it against ripgrep.
//!
//! This crate has no user interface dependency. The window and the headless `unluminous-cli search
//! serve` host both run it.

pub mod direct;
pub mod exact;
pub mod files;
pub mod freshness;
pub mod grammars;
pub mod index;
pub mod outline;
pub mod passages;
pub mod paths;
pub mod plan;
pub mod shape;
pub mod store;
pub mod symbols;
pub mod trigram;
pub mod verbs;
pub mod words;
