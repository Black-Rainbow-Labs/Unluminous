//! The structural tier of completion: what the code index and the open tabs' own structure know about
//! the place the caret is in. `task-2231` §5.2, §6.4 and §6.5.
//!
//! `app::completion` asks three questions of this module and ranks what comes back with everything else
//! through `unluminous_core::completion::order`:
//!
//! - **What is local here.** The parameters of the function the caret is in and the variables it binds
//!   above the caret, read from the open tab's structure, are offered nearest of all.
//! - **What the value before a `.` or `::` has.** The receiver's type is read from the enclosing
//!   `impl` or class for `self` and `this`, from a parameter's or variable's written type or constructor,
//!   from a field's type for `self.field`, or from the name itself for `Type::`. Its members are every
//!   definition whose container is that type, in the open tabs and in the project. When no type can be
//!   read, every member of every container whose name the stem matches is offered, each detailed with
//!   its container, and with nothing typed nothing is offered rather than the whole project.
//! - **What the project defines.** Names that start with the stem, have a later word it starts, or have
//!   initials it starts, each a range of the index rather than a walk of every name. A name defined in a
//!   file this one does not import is offered as needing its import (`Source::Import`), and accepting it
//!   adds the import (`app::auto_import`).
//!
//! **The ownership rule stands**: an open file's structure comes from its live text, read here and kept
//! until that text changes, and the index's copy of an open file is never offered beside it.

use std::path::{Path, PathBuf};

use atrius_index::outline::Definition as Shaped;
use atrius_index::structure::{Import, Kind as ShapeKind, Visibility};
use unluminous_core::completion::{self, Candidate, Insert, Kind, Locality, Source};
use unluminous_core::place::{self, Place, Receiver};

use crate::app::completion::MOST_FROM_THE_INDEX;
use crate::app::UnluminousApp;

/// One open tab's structure, read from its live text and kept until that text changes.
#[derive(Debug, Clone, Default)]
pub struct TabStructure {
    /// The `text_revision` it was read at.
    pub revision: u64,
    /// How many lines the text had then. While the count is the same the structure is kept across
    /// revisions, because typing within a line moves no definition to another line and every question
    /// the tiers ask of it is answered by lines and names.
    pub lines: usize,
    /// Every definition, locals included, with containers, parameters and types.
    pub definitions: Vec<Shaped>,
    pub imports: Vec<Import>,
}

/// The place a word is being typed in, and the value in front of it when it is a member.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Asked {
    pub place: Place,
    pub receiver: Option<Receiver>,
}

/// What a receiver's type turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReceiverType {
    /// A type whose members are the answer.
    Known(String),
    /// A module, whose top level definitions are the answer.
    Module(String),
    /// Nothing could be read.
    Unknown,
}

/// The completion kind of a structural kind. The two use the same words.
///
/// @param kind - the structural kind
pub fn kind_of(kind: ShapeKind) -> Kind {
    Kind::parse(kind.name()).unwrap_or(Kind::Variable)
}

/// The type a written type names, for looking its members up: references, pointers and the wrappers a
/// value is reached through (`Box`, `Rc`, `Arc`, `&mut`) are looked through, and generic arguments are
/// dropped, so `&mut Vec<Layout>` is `Vec` and `Box<Painter>` is `Painter`.
///
/// @param written - the type as written
pub fn type_name(written: &str) -> Option<String> {
    let mut text = written.trim();
    loop {
        let before = text;
        text = text.trim_start_matches(['&', '*']).trim_start();
        text = text.strip_prefix("mut ").unwrap_or(text).trim_start();
        text = text.strip_prefix("dyn ").unwrap_or(text).trim_start();
        text = text.strip_prefix("impl ").unwrap_or(text).trim_start();
        if text.starts_with('\'') {
            text = text.split_once(char::is_whitespace).map_or("", |(_, rest)| rest).trim_start();
        }
        for wrapper in ["Box<", "Rc<", "Arc<", "RefCell<", "Readonly<"] {
            if let Some(inner) = text.strip_prefix(wrapper) {
                text = inner.strip_suffix('>').unwrap_or(inner);
            }
        }
        if text == before {
            break;
        }
    }
    let base = text.split(['<', '[', '(', ' ', '|', ',', ';', '{']).next().unwrap_or(text);
    let last = base.rsplit("::").next().unwrap_or(base).rsplit('.').next().unwrap_or(base);
    let last = last.trim();
    let usable = last.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_');
    usable.then(|| last.to_owned())
}

/// A candidate for one structural definition.
///
/// @param d - the definition
/// @param source - where it came from
/// @param locality - how near its answer is
/// @param detail - the row's quiet suffix
pub fn candidate_of(d: &Shaped, source: Source, locality: Locality, detail: String) -> Candidate {
    let kind = kind_of(d.symbol_kind);
    let mut candidate =
        Candidate::described(d.name.clone(), source, Some(kind), detail).at(locality);
    candidate.info.signature = Some(signature_of(d));
    candidate.info.container = d.container.clone();
    candidate.info.doc = d.doc.clone();
    if kind.is_callable() && kind != Kind::Macro {
        let has_parameters = d.parameters.iter().any(|p| !is_self(&p.name));
        candidate.info.insert = Insert::Call { has_parameters };
    }
    candidate
}

/// True for the receiver parameter of a method, which is not passed between its brackets.
///
/// @param name - the parameter's name
fn is_self(name: &str) -> bool {
    matches!(name, "self" | "this" | "cls")
}

/// The row's signature: the parameters and return type for a callable, the type for a field or a
/// variable, and the head line otherwise.
///
/// @param d - the definition
pub fn signature_of(d: &Shaped) -> String {
    let kind = kind_of(d.symbol_kind);
    if kind.is_callable() {
        // The receiver is left out, as a call is written and as the reference editor shows a method.
        let parameters = d
            .parameters
            .iter()
            .filter(|p| !is_self(&p.name))
            .map(|p| match &p.type_text {
                Some(ty) => format!("{}: {ty}", p.name),
                None => p.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        return match &d.returns {
            Some(returns) => format!("{}({parameters}) -> {returns}", d.name),
            None => format!("{}({parameters})", d.name),
        };
    }
    match &d.type_text {
        Some(ty)
            if matches!(kind, Kind::Field | Kind::Variable | Kind::Constant | Kind::Parameter) =>
        {
            format!("{}: {ty}", d.name)
        }
        _ => d.signature.clone(),
    }
}

/// True for a definition that only makes sense reached through its container: a field, a method, a
/// variant, or anything else declared inside a type.
///
/// @param d - the definition
fn is_member(d: &Shaped) -> bool {
    d.member
        || matches!(d.symbol_kind, ShapeKind::Method | ShapeKind::Field | ShapeKind::Variant)
        || (d.container.is_some()
            && d.symbol_kind != ShapeKind::Function
            && !d.symbol_kind.is_type())
}

/// How many lines a keystroke's edit may add or take away before the tab's structure has to be read
/// again on that keystroke. Enter adds one; a paste of a block is not typing and pays for the read.
const LINES_A_KEYSTROKE_MAY_MOVE: usize = 2;

impl UnluminousApp {
    /// The structure of the tab at `index`, read from its live text. `None` for a tab no language
    /// reads: an untitled tab, a picture.
    ///
    /// A keystroke keeps the structure read before while the line count has moved by no more than
    /// [`LINES_A_KEYSTROKE_MAY_MOVE`], which is typing and Enter: the read is about 6 ms on a 200 KB
    /// file, more than the 5 ms a whole keystroke may take. An idle frame reads it again
    /// ([`Self::refresh_the_tab_structure_when_idle`]), so a stale structure lasts until the person
    /// stops typing. [`Self::exact_tab_structure`] is for a caller that needs this very revision.
    ///
    /// @param index - the tab
    pub(crate) fn tab_structure(&mut self, index: usize) -> Option<&TabStructure> {
        self.read_tab_structure(index, false)
    }

    /// The structure of the tab at `index` read at exactly its current revision, for a caller that
    /// writes at the byte ranges it gives: an import composed for an accepted completion.
    ///
    /// @param index - the tab
    pub(crate) fn exact_tab_structure(&mut self, index: usize) -> Option<&TabStructure> {
        self.read_tab_structure(index, true)
    }

    /// The structure of a tab, read again when it is stale by the rule the caller asked for.
    ///
    /// @param index - the tab
    /// @param exact - whether only this very revision will do
    fn read_tab_structure(&mut self, index: usize, exact: bool) -> Option<&TabStructure> {
        let file = self.files.at(index);
        let path = file.path()?.to_path_buf();
        let revision = file.document.text_revision();
        let lines = file.document.text().len_lines();
        let fresh = file.cached.structure.as_ref().is_some_and(|s| {
            s.revision == revision
                || (!exact && s.lines.abs_diff(lines) <= LINES_A_KEYSTROKE_MAY_MOVE)
        });
        if !fresh {
            let language = self.language_path(&path);
            let rel = language.file_name()?.to_string_lossy().into_owned();
            let text = self.files.at(index).document.text().to_string();
            let (definitions, imports) = atrius_index::outline::read_structure(&rel, &text);
            self.files.at_mut(index).cached.structure =
                Some(TabStructure { revision, lines, definitions, imports });
        }
        self.files.at(index).cached.structure.as_ref()
    }

    /// Reads the showing tab's structure again when it is behind the text, on a frame with no input in
    /// it, so the read never lands on a keystroke. Called once a frame; a frame in which nothing moved
    /// costs two integer comparisons.
    pub(crate) fn refresh_the_tab_structure_when_idle(&mut self) {
        let index = self.files.active_index();
        let file = self.files.at(index);
        let behind = file
            .cached
            .structure
            .as_ref()
            .is_some_and(|s| s.revision != file.document.text_revision());
        if behind {
            self.exact_tab_structure(index);
        }
    }

    /// The place a word starting at `word_start` in the tab that is showing is typed in, and the value
    /// in front of it when it is a member.
    ///
    /// @param word_start - where the word starts: the caret, less what has been typed of it
    pub(crate) fn asked_at(&self, word_start: usize) -> Asked {
        if let Some(asked) = &self.asked_override {
            return asked.clone();
        }
        let text = self.document().text().to_string();
        self.asked_in(&text, word_start)
    }

    /// The place a word starting at `word_start` in `text` is typed in, read with the grammar of the
    /// tab that is showing. `editor complete --after` asks this of the text as it would be.
    ///
    /// @param text - the text to read
    /// @param word_start - where the word starts in it
    pub(crate) fn asked_in(&self, text: &str, word_start: usize) -> Asked {
        let Some(grammar) = self.grammar_for(self.files.active().path()) else {
            return Asked::default();
        };
        let place = place::at(text, word_start, grammar);
        let receiver = match place {
            Place::Member => place::receiver_at(text, word_start, grammar),
            _ => None,
        };
        Asked { place, receiver }
    }

    /// The innermost function or method of the tab that is showing whose body holds a byte.
    ///
    /// @param offset - the byte
    fn enclosing_callable(&mut self, offset: usize) -> Option<Shaped> {
        let line = self.document().text().byte_slice(0..offset).to_string().matches('\n').count()
            as u32
            + 1;
        let index = self.files.active_index();
        let structure = self.tab_structure(index)?;
        structure
            .definitions
            .iter()
            .filter(|d| {
                d.symbol_kind.is_callable() && d.line <= line && d.end >= line && d.end > d.line
            })
            .max_by_key(|d| d.line)
            .cloned()
    }

    /// The parameters of the function the caret is in and the variables it binds above the caret, as
    /// candidates offered nearest of all.
    ///
    /// @param stem - what has been typed
    /// @param offset - the caret
    pub(crate) fn local_candidates(&mut self, stem: &str, offset: usize) -> Vec<Candidate> {
        let Some(enclosing) = self.enclosing_callable(offset) else { return Vec::new() };
        let caret_line =
            self.document().text().byte_slice(0..offset).to_string().matches('\n').count() as u32
                + 1;
        let mut out = Vec::new();
        for parameter in &enclosing.parameters {
            if is_self(&parameter.name)
                || !offers(stem, &parameter.name)
                || parameter.name.starts_with(['{', '['])
            {
                continue;
            }
            let mut c = Candidate::described(
                parameter.name.clone(),
                Source::ThisFile,
                Some(Kind::Parameter),
                "parameter",
            )
            .at(Locality::Local);
            c.info.signature = Some(match &parameter.type_text {
                Some(ty) => format!("{}: {ty}", parameter.name),
                None => parameter.name.clone(),
            });
            out.push(c);
        }
        let index = self.files.active_index();
        let Some(structure) = self.tab_structure(index) else { return out };
        for d in &structure.definitions {
            // By lines and not by bytes, because the structure may have been read a few keystrokes ago
            // on this line: a variable bound on an earlier line of the function is in scope, and one on
            // the caret's own line only when it was written before the caret.
            let bound_here = d.symbol_kind == ShapeKind::Variable
                && d.line >= enclosing.line
                && (d.line < caret_line || (d.line == caret_line && d.range.end <= offset));
            if bound_here && offers(stem, &d.name) {
                let detail = d.type_text.clone().unwrap_or_else(|| "local".to_owned());
                out.push(candidate_of(d, Source::ThisFile, Locality::Local, detail));
            }
        }
        out
    }

    /// The members of the value before a member separator, from the open tabs' structure and the
    /// project's index. `task-2231` §6.4 steps 1 and 2.
    ///
    /// @param receiver - the value before the separator
    /// @param stem - what has been typed after it
    /// @param offset - the caret
    pub(crate) fn member_candidates(
        &mut self,
        receiver: &Receiver,
        stem: &str,
        offset: usize,
    ) -> Vec<Candidate> {
        match self.type_of_receiver(receiver, offset) {
            ReceiverType::Known(ty) => self
                .members_of(&ty)
                .into_iter()
                .filter(|(d, _)| offers(stem, &d.name))
                .map(|(d, _)| {
                    let detail =
                        d.type_text.clone().or_else(|| d.returns.clone()).unwrap_or_default();
                    candidate_of(&d, Source::Member, Locality::Receiver, detail)
                })
                .collect(),
            ReceiverType::Module(module) => self
                .module_members(&module)
                .into_iter()
                .filter(|d| offers(stem, &d.name))
                .map(|d| candidate_of(&d, Source::Member, Locality::Receiver, module.clone()))
                .collect(),
            ReceiverType::Unknown if stem.is_empty() => Vec::new(),
            ReceiverType::Unknown => self.any_members_matching(stem),
        }
    }

    /// The type of a receiver, read by the rules of §6.4.
    ///
    /// @param receiver - the value before the separator
    /// @param offset - the caret
    fn type_of_receiver(&mut self, receiver: &Receiver, offset: usize) -> ReceiverType {
        let Some(first) = receiver.segments.first() else { return ReceiverType::Unknown };
        let enclosing = self.enclosing_callable(offset);
        let mut current = if matches!(first.as_str(), "self" | "this" | "Self") {
            enclosing.as_ref().and_then(|e| e.container.clone())
        } else if let Some(ty) = self.local_type(first, enclosing.as_ref(), offset) {
            Some(ty)
        } else if first.ends_with(')') {
            self.call_returns(None, first)
        } else if first.chars().next().is_some_and(char::is_uppercase) {
            Some(first.clone())
        } else if receiver.separator == "::" {
            return match receiver.segments.len() {
                1 => ReceiverType::Module(first.clone()),
                _ => ReceiverType::Module(receiver.segments.last().cloned().unwrap_or_default()),
            };
        } else {
            None
        };
        for segment in &receiver.segments[1..] {
            let Some(ty) = current.clone() else { break };
            current = match segment.ends_with(')') {
                true => self.call_returns(Some(&ty), segment),
                false => self
                    .members_of(&ty)
                    .into_iter()
                    .find(|(d, _)| d.name == *segment)
                    .and_then(|(d, _)| d.type_text.as_deref().and_then(type_name))
                    .or_else(|| {
                        segment
                            .chars()
                            .next()
                            .is_some_and(char::is_uppercase)
                            .then(|| segment.clone())
                    }),
            };
        }
        match current {
            Some(ty) => ReceiverType::Known(ty),
            None => ReceiverType::Unknown,
        }
    }

    /// The type a call's result has: the return type of the method of that name on `on`, or of the
    /// function of that name anywhere, with `Self` read as the type it is on. `Type::new()` is a `Type`
    /// when nothing says otherwise.
    ///
    /// @param on - the type the call is made on, when there is one
    /// @param call - the segment, `name(...)`
    fn call_returns(&mut self, on: Option<&str>, call: &str) -> Option<String> {
        let name = call.split('(').next()?.trim();
        let found = match on {
            Some(ty) => {
                self.members_of(ty).into_iter().find(|(d, _)| d.name == name).map(|(d, _)| d)
            }
            None => None,
        };
        let returns = found.and_then(|d| d.returns.clone())?;
        let ty = type_name(&returns)?;
        match ty.as_str() {
            "Self" => on.map(str::to_owned),
            _ => Some(ty),
        }
    }

    /// The type of a local or parameter of the enclosing function, from its written type or its
    /// constructor.
    ///
    /// @param name - the local's name
    /// @param enclosing - the function the caret is in
    /// @param offset - the caret
    fn local_type(
        &mut self,
        name: &str,
        enclosing: Option<&Shaped>,
        offset: usize,
    ) -> Option<String> {
        let enclosing = enclosing?;
        if let Some(p) = enclosing.parameters.iter().find(|p| p.name == name) {
            return p.type_text.as_deref().and_then(type_name).map(|ty| match ty.as_str() {
                "Self" => enclosing.container.clone().unwrap_or(ty),
                _ => ty,
            });
        }
        let index = self.files.active_index();
        let structure = self.tab_structure(index)?;
        structure
            .definitions
            .iter()
            .filter(|d| {
                d.symbol_kind == ShapeKind::Variable
                    && d.name == name
                    && d.line >= enclosing.line
                    && d.range.end <= offset
            })
            .max_by_key(|d| d.line)
            .and_then(|d| d.type_text.as_deref().and_then(type_name))
    }

    /// Every member of a type: from each open tab's structure, and from the project's index for every
    /// file that is not open.
    ///
    /// @param ty - the type's name
    fn members_of(&mut self, ty: &str) -> Vec<(Shaped, Option<PathBuf>)> {
        let mut out = Vec::new();
        let mut open: Vec<PathBuf> = Vec::new();
        for index in 0..self.files.len() {
            let Some(path) = self.files.at(index).path().map(Path::to_path_buf) else { continue };
            open.push(atrius_index::host::canonical(&path));
            let Some(structure) = self.tab_structure(index) else { continue };
            for d in &structure.definitions {
                if d.container.as_deref() == Some(ty) {
                    out.push((d.clone(), Some(path.clone())));
                }
            }
        }
        if let Some(symbols) = self.project_symbols.as_ref() {
            let found: Vec<(String, Shaped)> = symbols
                .read(|table| {
                    table.members_of(ty).map(|(p, d)| (p.to_owned(), d.clone())).collect()
                })
                .unwrap_or_default();
            for (rel, d) in found {
                let path = symbols.absolute(&rel);
                if !open.contains(&path) && symbols.offers(&rel) {
                    out.push((d, Some(path)));
                }
            }
        }
        out
    }

    /// The top level definitions of a module: those in files named after it (`layout.rs`,
    /// `layout/mod.rs`, `layout/index.ts`) that belong to no container.
    ///
    /// @param module - the module's name
    fn module_members(&mut self, module: &str) -> Vec<Shaped> {
        let Some(symbols) = self.project_symbols.as_ref() else { return Vec::new() };
        let module = module.to_owned();
        symbols
            .read(|table| {
                table
                    .files()
                    .filter(|(rel, _)| names_module(rel, &module) && symbols.offers(rel))
                    .flat_map(|(_, defs)| {
                        defs.iter().filter(|d| d.container.is_none() && d.depth == 0).cloned()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every member of every container whose name the stem matches, each detailed with its container:
    /// what is offered after a `.` whose value's type could not be read.
    ///
    /// @param stem - what has been typed, never empty
    fn any_members_matching(&mut self, stem: &str) -> Vec<Candidate> {
        let mut out = Vec::new();
        for index in 0..self.files.len() {
            let Some(structure) = self.tab_structure(index) else { continue };
            for d in structure
                .definitions
                .iter()
                .filter(|d| is_member(d) && completion::could_match(stem, &d.name))
            {
                let detail = d.container.clone().unwrap_or_default();
                out.push(candidate_of(d, Source::Member, Locality::Project, detail));
            }
        }
        let Some(symbols) = self.project_symbols.as_ref() else { return out };
        let lower = stem.to_lowercase();
        let found: Vec<Shaped> = symbols
            .read(|table| {
                names_reached(table, &lower, MOST_FROM_THE_INDEX)
                    .into_iter()
                    .flat_map(|name| {
                        table
                            .named(name)
                            .iter()
                            .filter(|d| symbols.offers(&d.path))
                            .map(|d| d.definition.clone())
                            .collect::<Vec<_>>()
                    })
                    .filter(is_member)
                    .collect()
            })
            .unwrap_or_default();
        for d in found {
            let detail = d.container.clone().unwrap_or_default();
            out.push(candidate_of(&d, Source::Member, Locality::Project, detail));
        }
        out
    }

    /// The project's names a stem reaches, as candidates: names that start with it, have a later word
    /// it starts, or have initials it starts, at most [`MOST_FROM_THE_INDEX`] of them. Members, open
    /// files and locals of other files are left out. A name a file this one does not import is offered
    /// as needing its import, in a language that writes imports.
    ///
    /// @param stem - what has been typed, never empty
    pub(crate) fn project_candidates(&mut self, stem: &str) -> Vec<Candidate> {
        let here = self.files.active().path().map(Path::to_path_buf);
        let open: Vec<PathBuf> =
            self.files.iter().filter_map(|f| f.path().map(atrius_index::host::canonical)).collect();
        let writes_imports = self.grammar_for(here.as_deref()).is_some_and(|g| g.imports.is_some());
        let in_scope = self.names_in_scope();
        let Some(symbols) = self.project_symbols.as_ref() else { return Vec::new() };
        let here_rel = here.as_deref().and_then(|p| symbols.relative(p));
        // The open files as the index names them, so each name is checked with a string comparison
        // rather than by building a path.
        let open: std::collections::HashSet<String> =
            open.iter().filter_map(|p| symbols.relative(p)).collect();
        let lower = stem.to_lowercase();
        let found: Vec<(String, Shaped)> = symbols
            .read(|table| {
                let mut out = Vec::new();
                for name in names_reached(table, &lower, index_cap(stem)) {
                    let chosen = table.named(name).iter().find(|d| {
                        !is_member(&d.definition)
                            && symbols.offers(&d.path)
                            && !open.contains(&d.path)
                    });
                    if let Some(d) = chosen {
                        out.push((d.path.clone(), d.definition.clone()));
                    }
                }
                out
            })
            .unwrap_or_default();
        found
            .into_iter()
            .map(|(rel, d)| {
                let locality = locality_of(here_rel.as_deref(), &rel);
                let needs_import = writes_imports
                    && d.visibility != Visibility::Private
                    && !in_scope.contains(&d.name)
                    && !in_scope.contains(&module_of_path(&rel));
                let file = rel.rsplit('/').next().unwrap_or(&rel).to_owned();
                match needs_import {
                    true => {
                        let mut c = candidate_of(&d, Source::Import, Locality::NeedsImport, file);
                        c.info.needs_import = Some(rel);
                        c
                    }
                    false => candidate_of(&d, Source::Index, locality, file),
                }
            })
            .collect()
    }

    /// The names the file that is showing has in scope through its imports: each import's local name,
    /// and the module a glob import brings everything of.
    fn names_in_scope(&mut self) -> std::collections::HashSet<String> {
        let index = self.files.active_index();
        let mut out = std::collections::HashSet::new();
        if let Some(structure) = self.tab_structure(index) {
            for import in &structure.imports {
                if import.glob {
                    if let Some(last) = import.path.last() {
                        out.insert(module_name(last));
                    }
                } else if let Some(name) = import.local_name() {
                    out.insert(name.to_owned());
                }
            }
        }
        out
    }
}

/// How many of the index's names one stem may draw: [`MOST_FROM_THE_INDEX`], or
/// [`MOST_FOR_ONE_LETTER`] for a stem of one letter, which only `Ctrl+Space` asks about (the automatic
/// popup waits for two) and which reaches more names than any other.
///
/// @param stem - what has been typed
fn index_cap(stem: &str) -> usize {
    match stem.chars().count() {
        1 => crate::app::completion::MOST_FOR_ONE_LETTER,
        _ => MOST_FROM_THE_INDEX,
    }
}

/// Whether a stem offers a name: everything when nothing has been typed, the cheap subsequence check
/// otherwise.
///
/// @param stem - what has been typed
/// @param name - the name
fn offers(stem: &str, name: &str) -> bool {
    stem.is_empty() || completion::could_match(stem, name)
}

/// The lower case names of the index a lower case stem reaches, in that order: by prefix, by a later
/// word, by initials, each name once, at most `limit`.
///
/// @param table - the index's definitions
/// @param lower - the stem, lower case
/// @param limit - how many names at most
fn names_reached<'a>(
    table: &'a atrius_index::symbols::SymbolTable,
    lower: &'a str,
    limit: usize,
) -> Vec<&'a str> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let prefixed = table.prefixed(lower).map(|(name, _)| name);
    let later = table.word_started(lower);
    let humped = table.humped(lower);
    for name in prefixed.chain(later).chain(humped) {
        if out.len() >= limit {
            break;
        }
        if seen.insert(name) {
            out.push(name);
        }
    }
    out
}

/// How near a project file is to the one being typed in: the same folder, the same package (the same
/// first two folders, `crates/unluminous-app`), or the project.
///
/// @param here - the file being typed in, relative to the project
/// @param there - the defining file, relative to the project
fn locality_of(here: Option<&str>, there: &str) -> Locality {
    let Some(here) = here else { return Locality::Project };
    let folder = |p: &str| p.rsplit_once('/').map_or(String::new(), |(f, _)| f.to_owned());
    if folder(here) == folder(there) {
        return Locality::SameFolder;
    }
    let package = |p: &str| p.split('/').take(2).collect::<Vec<_>>().join("/");
    match package(here) == package(there) {
        true => Locality::Package,
        false => Locality::Project,
    }
}

/// The module a file is, by its name: `src/layout.rs` and `src/layout/mod.rs` are `layout`.
///
/// @param rel - the file, relative to the project
fn module_of_path(rel: &str) -> String {
    let mut parts: Vec<&str> = rel.split('/').collect();
    let file = parts.pop().unwrap_or_default();
    let stem = file.split('.').next().unwrap_or(file);
    match stem {
        "mod" | "index" | "__init__" | "lib" | "main" => parts.pop().unwrap_or(stem).to_owned(),
        _ => stem.to_owned(),
    }
}

/// A module's name as an import's last segment writes it: a path segment as it is, a specifier by its
/// file stem (`./layout` is `layout`).
///
/// @param segment - the import's last segment
fn module_name(segment: &str) -> String {
    let base = segment.rsplit('/').next().unwrap_or(segment);
    base.split('.').next().unwrap_or(base).to_owned()
}

/// Whether a file is a module of a name.
///
/// @param rel - the file, relative to the project
/// @param module - the module's name
fn names_module(rel: &str, module: &str) -> bool {
    module_of_path(rel) == module
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_type_is_read_down_to_the_type_whose_members_are_wanted() {
        assert_eq!(type_name("&mut Vec<Layout>").as_deref(), Some("Vec"));
        assert_eq!(type_name("Box<Painter>").as_deref(), Some("Painter"));
        assert_eq!(type_name("Option<T>").as_deref(), Some("Option"));
        assert_eq!(type_name("&'a crate::layout::Layout").as_deref(), Some("Layout"));
        assert_eq!(type_name("Arc<RwLock<Index>>").as_deref(), Some("RwLock"));
        assert_eq!(type_name("CardProps[]").as_deref(), Some("CardProps"));
        assert_eq!(type_name("(a: string) => void"), None);
    }

    #[test]
    fn a_files_nearness_is_its_folder_then_its_package_then_the_project() {
        let here = Some("crates/unluminous-app/src/app/completion.rs");
        assert_eq!(
            locality_of(here, "crates/unluminous-app/src/app/gather.rs"),
            Locality::SameFolder
        );
        assert_eq!(
            locality_of(here, "crates/unluminous-app/src/services/stats.rs"),
            Locality::Package
        );
        assert_eq!(locality_of(here, "crates/unluminous-core/src/layout.rs"), Locality::Project);
    }

    #[test]
    fn a_files_module_is_its_stem_or_its_folder() {
        assert_eq!(module_of_path("src/layout.rs"), "layout");
        assert_eq!(module_of_path("src/layout/mod.rs"), "layout");
        assert_eq!(module_of_path("ui/components/index.ts"), "components");
        assert_eq!(module_name("./layout"), "layout");
        assert_eq!(module_name("caret"), "caret");
    }
}
