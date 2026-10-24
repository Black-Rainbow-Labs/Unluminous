//! File templates: what a new file starts with, the way the reference editor's `File and Code Templates` work.
//!
//! `task-2336`: *"When I right click folder explorer, I want to see new file options for common types, such
//! as rust file, text, js, html, css. We also want to support custom templates, similar to the reference editor."*
//!
//! A template is a file called `<Name>.<extension>.ft`, the shape the reference editor keeps in `.idea/fileTemplates`,
//! in one of two folders:
//!
//! | Scope | Folder | Wins over |
//! |---|---|---|
//! | [`Scope::Project`] | `<project>/.unluminous/templates/` | both others |
//! | [`Scope::User`] | `<settings folder>/templates/` | the built in ones |
//! | [`Scope::BuiltIn`] | [`built_in`], in the binary | nothing |
//!
//! The five built in ones are the common types the ticket names, and they are templates like any other, so a
//! person can replace `Rust File` with their own by saving one under that name. Includes are templates in an
//! `includes` folder beside the others, and `#parse("Header.rs")` puts one in.
//!
//! [`expand`] is the reference editor's variables, without the rest of Velocity: `${NAME}`, `$NAME`, the date and time,
//! `${USER}`, `${PROJECT_NAME}`, `${DIR_PATH}`, `${FILE_NAME}`, `${DS}` for a dollar sign, `#set` and `#parse`.
//! A variable in braces that none of those fill is asked for when the file is made ([`variables_asked`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a template file's name ends with.
pub const EXTENSION: &str = "ft";

/// How deep `#parse` may go, which stops a template that includes itself.
const DEEPEST_INCLUDE: usize = 8;

/// Where a template came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    BuiltIn,
    User,
    Project,
}

impl Scope {
    /// The word `explorer templates` prints.
    pub fn name(self) -> &'static str {
        match self {
            Scope::BuiltIn => "built-in",
            Scope::User => "user",
            Scope::Project => "project",
        }
    }
}

/// One template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// What the menu calls it: `Rust File`, `React Component`.
    pub name: String,
    /// The extension a file made from it gets, without the dot: `rs`, `tsx`.
    pub extension: String,
    pub body: String,
    pub scope: Scope,
    /// What a file made from it is called, written with variables: `${NAME}.test` or `../${NAME}`. Empty asks
    /// for a name and adds the extension, which is what a template with no pattern has always done. Kept in a
    /// `<file>.ft.conf` beside the template as `file_name = …`, because a `.ft` file is its body and nothing
    /// else. The reference editor's `File name` field. `task-2336`.
    pub file_name: String,
}

impl Template {
    /// The name as a command line word: lower case, a hyphen between words. `react-component`.
    pub fn slug(&self) -> String {
        slug(&self.name)
    }

    /// Whether this is one of the built in names, wherever this copy of it came from.
    pub fn is_a_common_type(&self) -> bool {
        built_in().iter().any(|common| common.name.eq_ignore_ascii_case(&self.name))
    }
}

/// `name` as a command line word.
///
/// @param name - a template's name
pub fn slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

/// The five common types, in the order the menu lists them.
pub fn built_in() -> Vec<Template> {
    let template = |name: &str, extension: &str, body: &str| Template {
        name: name.to_owned(),
        extension: extension.to_owned(),
        body: body.to_owned(),
        scope: Scope::BuiltIn,
        file_name: String::new(),
    };
    vec![
        template("Rust File", "rs", ""),
        template("Text File", "txt", ""),
        template("JavaScript File", "js", ""),
        template(
            "HTML File",
            "html",
            "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>${NAME}</title>\n</head>\n<body>\n\n</body>\n</html>\n",
        ),
        template("CSS File", "css", ""),
    ]
}

/// The folder a person's own templates are in.
///
/// @param settings - Unluminous's settings folder
pub fn user_folder(settings: &Path) -> PathBuf {
    settings.join("templates")
}

/// The folder a project's templates are in.
///
/// @param project - the project folder
pub fn project_folder(project: &Path) -> PathBuf {
    project.join(".unluminous").join("templates")
}

/// Every template there is, one a name: the built in ones in their order, then the rest by name.
///
/// A template of the same name in a later scope replaces an earlier one where it stands, so `Rust File` stays
/// first in the list whichever folder its body came from.
///
/// @param project - the project folder
/// @param user - Unluminous's settings folder, when there is one
pub fn all(project: &Path, user: Option<&Path>) -> Vec<Template> {
    let mut templates = built_in();
    let mut custom: Vec<Template> = Vec::new();
    let scopes =
        [(user.map(user_folder), Scope::User), (Some(project_folder(project)), Scope::Project)];
    for (folder, scope) in scopes {
        let Some(folder) = folder else { continue };
        for template in read_folder(&folder, scope) {
            let same = |other: &Template| other.name.eq_ignore_ascii_case(&template.name);
            if let Some(held) = templates.iter_mut().find(|other| same(other)) {
                *held = template;
            } else if let Some(held) = custom.iter_mut().find(|other| same(other)) {
                *held = template;
            } else {
                custom.push(template);
            }
        }
    }
    custom.sort_by_key(|template| template.name.to_lowercase());
    templates.extend(custom);
    templates
}

/// The includes there are, a person's and the project's, by name. An include is a fragment a template puts in
/// with `#parse("<Name>.<extension>")`, kept in an `includes` folder beside the templates.
///
/// @param project - the project folder
/// @param user - Unluminous's settings folder, when there is one
pub fn includes(project: &Path, user: Option<&Path>) -> Vec<Template> {
    let mut found = Vec::new();
    if let Some(user) = user {
        found.extend(read_folder(&user_folder(user).join("includes"), Scope::User));
    }
    found.extend(read_folder(&project_folder(project).join("includes"), Scope::Project));
    found.sort_by_key(|template| (template.scope != Scope::User, template.name.to_lowercase()));
    found
}

/// The built in template called `name`, as Unluminous ships it, which is what `Reset` puts back.
///
/// @param name - a template's name
pub fn original(name: &str) -> Option<Template> {
    built_in().into_iter().find(|template| template.name.eq_ignore_ascii_case(name.trim()))
}

/// The template a name or a slug names.
///
/// @param templates - what [`all`] answered
/// @param wanted - a template's name, in any case, or its slug
pub fn find<'a>(templates: &'a [Template], wanted: &str) -> Option<&'a Template> {
    let wanted_slug = slug(wanted);
    templates.iter().find(|template| {
        template.name.eq_ignore_ascii_case(wanted.trim()) || template.slug() == wanted_slug
    })
}

/// The templates in one folder, read by their file names.
fn read_folder(folder: &Path, scope: Scope) -> Vec<Template> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().to_string();
            let (name, extension) = parse_file_name(&file)?;
            let body = std::fs::read_to_string(entry.path()).ok()?;
            let file_name = read_file_name_pattern(&entry.path());
            Some(Template { name, extension, body: body.replace("\r\n", "\n"), scope, file_name })
        })
        .collect()
}

/// The `.ft.conf` beside a template, which holds what is not its body.
fn settings_beside(template_file: &Path) -> PathBuf {
    let mut name = template_file.as_os_str().to_owned();
    name.push(".conf");
    PathBuf::from(name)
}

/// The file name pattern kept beside a template, or empty when there is none.
fn read_file_name_pattern(template_file: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(settings_beside(template_file)) else {
        return String::new();
    };
    text.lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "file_name")
        .map(|(_, value)| value.trim().to_owned())
        .unwrap_or_default()
}

/// A template file's name read as the template's name and extension: `React Component.tsx.ft` is
/// `React Component` and `tsx`. A file with no extension of its own (`Makefile.ft`) makes files with none.
///
/// @param file - the file's name
pub fn parse_file_name(file: &str) -> Option<(String, String)> {
    let stem = file.strip_suffix(&format!(".{EXTENSION}"))?;
    if stem.trim().is_empty() {
        return None;
    }
    match stem.rsplit_once('.') {
        Some((name, extension)) if !name.trim().is_empty() && !extension.contains(' ') => {
            Some((name.trim().to_owned(), extension.to_owned()))
        }
        _ => Some((stem.trim().to_owned(), String::new())),
    }
}

/// The file name a template is kept under.
///
/// @param name - the template's name
/// @param extension - the extension a file made from it gets
pub fn file_name_of(name: &str, extension: &str) -> String {
    match extension.is_empty() {
        true => format!("{}.{EXTENSION}", name.trim()),
        false => format!("{}.{extension}.{EXTENSION}", name.trim()),
    }
}

/// Whether making a file from `template` needs a name typed: always, unless its file name pattern names the
/// file without `${NAME}`, which is the reference editor's rule for its `File name` field.
///
/// @param template - the template
pub fn asks_for_a_name(template: &Template) -> bool {
    let pattern = template.file_name.trim();
    pattern.is_empty() || pattern.contains("NAME")
}

/// What a file is called when it is made from `template` with `name` typed. With no file name pattern, the
/// name typed with the template's extension added when it is not already there. With one, the pattern with
/// `${NAME}` and the other variables filled in, and the extension added when the pattern did not end with it;
/// the pattern may name folders, `../${NAME}` or `tests/${NAME}`.
///
/// @param template - the template
/// @param name - what was typed, or empty for a pattern that does not use it
pub fn file_name_for(template: &Template, name: &str) -> String {
    let pattern = template.file_name.trim();
    if !pattern.is_empty() {
        let stem = name.trim();
        let stem = match template.extension.is_empty() {
            true => stem,
            false => stem.strip_suffix(&format!(".{}", template.extension)).unwrap_or(stem),
        };
        let mut values = standard_values(Path::new(&format!("{stem}.x")), Path::new(""));
        values.insert("NAME".to_owned(), stem.to_owned());
        let made = expand(pattern, &values, None).unwrap_or_else(|_| pattern.to_owned());
        let plain = Template { file_name: String::new(), ..template.clone() };
        return file_name_for(&plain, &made);
    }
    let name = name.trim();
    if template.extension.is_empty()
        || name.to_lowercase().ends_with(&format!(".{}", template.extension.to_lowercase()))
    {
        return name.to_owned();
    }
    format!("{name}.{}", template.extension)
}

/// The values every template is given: the file's name and folder, the project's name, who is making it
/// and when.
///
/// @param file - the file that is being made
/// @param project - the project folder
pub fn standard_values(file: &Path, project: &Path) -> BTreeMap<String, String> {
    let now = crate::services::clock::date_time();
    let months = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let name_of =
        |path: &Path| path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let stem = file.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    let month_name = months.get(usize::from(now.month).saturating_sub(1)).copied().unwrap_or("");
    let pairs = [
        ("NAME", stem),
        ("FILE_NAME", name_of(file)),
        ("DIR_PATH", file.parent().map(|dir| relative(dir, project)).unwrap_or_default()),
        ("PROJECT_NAME", name_of(project)),
        ("USER", user),
        ("DATE", format!("{:04}-{:02}-{:02}", now.year, now.month, now.day)),
        ("TIME", format!("{:02}:{:02}", now.hour, now.minute)),
        ("YEAR", format!("{:04}", now.year)),
        ("MONTH", format!("{:02}", now.month)),
        ("MONTH_NAME_FULL", month_name.to_owned()),
        ("MONTH_NAME_SHORT", month_name.chars().take(3).collect()),
        ("DAY", format!("{:02}", now.day)),
        ("HOUR", format!("{:02}", now.hour)),
        ("MINUTE", format!("{:02}", now.minute)),
        ("SECOND", format!("{:02}", now.second)),
        ("PRODUCT_NAME", "Unluminous".to_owned()),
        ("DS", "$".to_owned()),
    ];
    pairs.into_iter().map(|(key, value)| (key.to_owned(), value)).collect()
}

/// `path` from `project`, with forward slashes, or the whole path when it is not inside the project.
fn relative(path: &Path, project: &Path) -> String {
    let shown = path.strip_prefix(project).unwrap_or(path);
    shown.to_string_lossy().replace('\\', "/")
}

/// The variables a template uses in braces that [`standard_values`] and its own `#set` lines do not fill, in
/// the order they first appear, through its includes too. These are what is asked for when a file is made.
///
/// @param body - the template's text
/// @param includes - the folder its includes are in, when there is one
pub fn variables_asked(body: &str, includes: Option<&Path>) -> Vec<String> {
    let known = standard_values(Path::new("x"), Path::new("x"));
    let mut set = Vec::new();
    let mut asked = Vec::new();
    collect_variables(body, includes, 0, &mut set, &mut asked);
    asked.retain(|name| !known.contains_key(name) && !set.contains(name));
    asked
}

/// Walk `body` and its includes for the variables set and used.
fn collect_variables(
    body: &str,
    includes: Option<&Path>,
    depth: usize,
    set: &mut Vec<String>,
    used: &mut Vec<String>,
) {
    for line in body.lines() {
        if let Some((name, _)) = set_line(line) {
            set.push(name);
            continue;
        }
        if let Some(included) = parse_line(line) {
            if depth < DEEPEST_INCLUDE {
                if let Some(text) = includes.and_then(|folder| read_include(folder, &included)) {
                    collect_variables(&text, includes, depth + 1, set, used);
                }
            }
            continue;
        }
        for name in references(line) {
            if !used.contains(&name) {
                used.push(name);
            }
        }
    }
}

/// Fill a template's variables and directives in.
///
/// `values` holds every variable's value, the standard ones and those that were asked for. A variable in
/// braces with no value is refused, naming it, so a file is never written with `${Author}` left in it by
/// mistake. `$word` with no braces and no value is left as it is written, as Velocity leaves an unknown
/// reference, so `$scope` in a JavaScript template or `$HOME` in a script stays code.
///
/// @param body - the template's text
/// @param values - each variable's value
/// @param includes - the folder `#parse` reads from, when there is one
pub fn expand(
    body: &str,
    values: &BTreeMap<String, String>,
    includes: Option<&Path>,
) -> Result<String, String> {
    let mut values = values.clone();
    let mut out = String::new();
    expand_into(body, &mut values, includes, 0, &mut out)?;
    Ok(out)
}

/// [`expand`], into `out`, at an include depth.
fn expand_into(
    body: &str,
    values: &mut BTreeMap<String, String>,
    includes: Option<&Path>,
    depth: usize,
    out: &mut String,
) -> Result<(), String> {
    for piece in body.split_inclusive('\n') {
        let line = piece.trim_end_matches('\n').trim_end_matches('\r');
        if let Some((name, value)) = set_line(line) {
            let value = substitute(&value, values)?;
            values.insert(name, value);
            continue;
        }
        if let Some(included) = parse_line(line) {
            if depth >= DEEPEST_INCLUDE {
                return Err(format!(
                    "#parse(\"{included}\") goes more than {DEEPEST_INCLUDE} includes deep, which is a template including itself."
                ));
            }
            let Some(text) = includes.and_then(|folder| read_include(folder, &included)) else {
                return Err(format!("There is no include called {included}."));
            };
            expand_into(&text, values, includes, depth + 1, out)?;
            if !out.ends_with('\n') && piece.ends_with('\n') {
                out.push('\n');
            }
            continue;
        }
        out.push_str(&substitute(piece, values)?);
    }
    Ok(())
}

/// Put each `${NAME}` and `$NAME` in `text` in, from `values`, with the string methods a template may call on
/// one: `${NAME.toUpperCase()}`, `toLowerCase()`, `trim()`, `length()`, and
/// `substring(a)` or `substring(a,b)`, which the reference editor's Velocity offers through Java.
fn substitute(text: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        if let Some((name, methods, used)) = call_at(after) {
            let Some(value) = values.get(&name) else {
                return Err(format!("The template needs a value for {name}."));
            };
            out.push_str(&call_methods(value, &methods)?);
            rest = &after[used..];
            continue;
        }
        match variable_at(after) {
            Some((name, used, _)) if values.contains_key(&name) => {
                out.push_str(&values[&name]);
                rest = &after[used..];
            }
            Some((name, _, true)) => {
                return Err(format!("The template needs a value for {name}."));
            }
            _ => {
                out.push('$');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// `{NAME.method()…}` straight after a `$`: the variable, the methods called on it in order, and how many bytes
/// it took. `None` for anything else, which is read as a plain variable.
fn call_at(after: &str) -> Option<(String, Vec<String>, usize)> {
    let inside = after.strip_prefix('{')?;
    let end = inside.find('}')?;
    let (name, calls) = inside[..end].split_once('.')?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let methods: Vec<String> =
        calls.split(").").map(|call| call.trim_end_matches(')').to_owned()).collect();
    methods.iter().all(|call| call.contains('(')).then(|| (name.to_owned(), methods, end + 2))
}

/// Apply each of `methods` (`toUpperCase(`, `substring(0,3`) to `value`, refusing one this does not know.
fn call_methods(value: &str, methods: &[String]) -> Result<String, String> {
    let mut value = value.to_owned();
    for call in methods {
        let (method, arguments) = call.split_once('(').unwrap_or((call, ""));
        let numbers: Vec<usize> =
            arguments.split(',').filter_map(|number| number.trim().parse().ok()).collect();
        let characters: Vec<char> = value.chars().collect();
        value = match (method.trim(), numbers.as_slice()) {
            ("toUpperCase", _) => value.to_uppercase(),
            ("toLowerCase", _) => value.to_lowercase(),
            ("trim", _) => value.trim().to_owned(),
            ("length", _) => characters.len().to_string(),
            ("substring", [from]) => characters.iter().skip(*from).collect(),
            ("substring", [from, to]) => characters.iter().take(*to).skip(*from).collect(),
            (other, _) => return Err(format!("A template cannot call {other}() on a value.")),
        };
    }
    Ok(value)
}

/// The variable named straight after a `$`, how many bytes it took, and whether it was in braces: `{NAME}`
/// or `NAME`.
fn variable_at(after: &str) -> Option<(String, usize, bool)> {
    let word = |text: &str| -> usize {
        text.char_indices()
            .find(|(at, character)| {
                !(character.is_ascii_alphanumeric() || *character == '_')
                    || (*at == 0 && character.is_ascii_digit())
            })
            .map(|(at, _)| at)
            .unwrap_or(text.len())
    };
    if let Some(inside) = after.strip_prefix('{') {
        let length = word(inside);
        if length > 0 && inside[length..].starts_with('}') {
            return Some((inside[..length].to_owned(), length + 2, true));
        }
        return None;
    }
    let length = word(after);
    (length > 0).then(|| (after[..length].to_owned(), length, false))
}

/// Every variable a line refers to in braces, which is the form that is asked for when nothing fills it.
fn references(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find('$') {
        let after = &rest[at + 1..];
        if let Some((name, _, used)) = call_at(after) {
            found.push(name);
            rest = &after[used..];
            continue;
        }
        match variable_at(after) {
            Some((name, used, braced)) => {
                if braced {
                    found.push(name);
                }
                rest = &after[used..];
            }
            None => rest = after,
        }
    }
    found
}

/// `#set( $Name = "value" )`, read as the name and the value.
fn set_line(line: &str) -> Option<(String, String)> {
    let inside = line.trim().strip_prefix("#set")?.trim_start().strip_prefix('(')?;
    let inside = inside.trim_end().strip_suffix(')')?;
    let (name, value) = inside.split_once('=')?;
    let name = name.trim().strip_prefix('$')?;
    let name = name.trim_start_matches('{').trim_end_matches('}').trim();
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|value| value.strip_suffix('\'')))
        .unwrap_or(value);
    (!name.is_empty()).then(|| (name.to_owned(), value.to_owned()))
}

/// `#parse("File Header.rs")`, read as the include's file name.
fn parse_line(line: &str) -> Option<String> {
    let inside = line.trim().strip_prefix("#parse")?.trim_start().strip_prefix('(')?;
    let inside = inside.trim_end().strip_suffix(')')?.trim();
    let name = inside
        .strip_prefix('"')
        .and_then(|name| name.strip_suffix('"'))
        .or_else(|| inside.strip_prefix('\'').and_then(|name| name.strip_suffix('\'')))?;
    (!name.trim().is_empty()).then(|| name.trim().to_owned())
}

/// An include's text: `<folder>/includes/<name>.ft`, or the name as it is when it already ends with `.ft`.
fn read_include(folder: &Path, name: &str) -> Option<String> {
    let file = match name.ends_with(&format!(".{EXTENSION}")) {
        true => name.to_owned(),
        false => format!("{name}.{EXTENSION}"),
    };
    // Only a name, never a path, so an include cannot read a file outside the folder.
    if file.contains(['/', '\\']) || file.contains("..") {
        return None;
    }
    let text = std::fs::read_to_string(folder.join("includes").join(file)).ok()?;
    Some(text.replace("\r\n", "\n"))
}

/// The folder the includes of a template in `scope` are read from: its own folder's `includes`. The built in
/// templates use the user's.
///
/// @param scope - where the template came from
/// @param project - the project folder
/// @param user - Unluminous's settings folder, when there is one
pub fn includes_for(scope: Scope, project: &Path, user: Option<&Path>) -> Option<PathBuf> {
    match scope {
        Scope::Project => Some(project_folder(project)),
        Scope::User | Scope::BuiltIn => user.map(user_folder),
    }
}

/// Make a template from a file: `Tools -> Save File as Template` in the reference editor. The file's text is the body as
/// it is, and its extension is the template's.
///
/// @param file - the file to copy
/// @param name - what the template is called
/// @param folder - the templates folder it goes in, made when it is missing
pub fn save_as_template(file: &Path, name: &str, folder: &Path) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':']) {
        return Err("A template's name is one plain name, with no folder in it.".to_owned());
    }
    let body = std::fs::read_to_string(file)
        .map_err(|problem| format!("Unluminous could not read {}: {problem}", file.display()))?;
    let extension = file
        .extension()
        .map(|extension| extension.to_string_lossy().to_string())
        .unwrap_or_default();
    std::fs::create_dir_all(folder)
        .map_err(|problem| format!("Unluminous could not make {}: {problem}", folder.display()))?;
    let target = folder.join(file_name_of(name, &extension));
    crate::services::store::write_a_source_file(&target, body.replace("\r\n", "\n").as_bytes())
        .map_err(|problem| format!("Unluminous could not write {}: {problem}", target.display()))?;
    Ok(target)
}

/// One template or include as the Templates manager holds it while it is being edited (`task-2336`).
///
/// The manager edits a list of these and writes them all with [`apply`] when `Apply` or `OK` is pressed, so
/// `Cancel` throws every change away, which is how the reference editor's settings page behaves. `unluminous-cli
/// explorer template-*` builds one and applies it the same way, so a template made from the command line and
/// one made in the dialog are the same file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    /// What it says now. `scope` is where it goes: a built in one that was changed goes with the user's.
    pub template: Template,
    /// An include rather than a template, kept in the `includes` folder.
    pub include: bool,
    /// The file it was read from, when it was read from one. A built in template nobody changed has none.
    pub file: Option<PathBuf>,
    /// Take it out when the changes are applied.
    pub removed: bool,
}

impl Draft {
    /// A new template or include, in `scope`, named and filled in the way the manager's `New` makes one.
    ///
    /// @param name - what it is called
    /// @param scope - the person's or the project's
    /// @param include - an include rather than a template
    pub fn new(name: &str, scope: Scope, include: bool) -> Self {
        let template = Template {
            name: name.to_owned(),
            extension: "txt".to_owned(),
            body: String::new(),
            scope,
            file_name: String::new(),
        };
        Self { template, include, file: None, removed: false }
    }

    /// Whether this is a built in template, changed or not, which cannot be removed, only reset.
    pub fn is_built_in(&self) -> bool {
        !self.include
            && original(&self.template.name).is_some()
            && self.template.scope != Scope::Project
    }

    /// Whether a built in template says something other than what Unluminous ships, which is what `Reset`
    /// offers to undo.
    pub fn differs_from_the_original(&self) -> bool {
        match original(&self.template.name) {
            Some(shipped) if self.is_built_in() => {
                shipped.body != self.template.body
                    || shipped.extension != self.template.extension
                    || !self.template.file_name.trim().is_empty()
            }
            _ => false,
        }
    }
}

/// The folder a template or an include in `scope` is kept in, or `None` for the person's when there is no
/// settings folder. A built in template that is changed is kept with the person's.
///
/// @param scope - where it goes
/// @param include - an include rather than a template
/// @param project - the project folder
/// @param settings - Unluminous's settings folder, when there is one
pub fn folder_for(
    scope: Scope,
    include: bool,
    project: &Path,
    settings: Option<&Path>,
) -> Option<PathBuf> {
    let folder = match scope {
        Scope::Project => project_folder(project),
        Scope::User | Scope::BuiltIn => user_folder(settings?),
    };
    Some(match include {
        true => folder.join("includes"),
        false => folder,
    })
}

/// Everything the Templates manager lists, as drafts: the person's templates with the built in ones among them,
/// the project's, and the includes of both. Built in templates come first, in the menu's order.
///
/// @param project - the project folder
/// @param settings - Unluminous's settings folder, when there is one
pub fn drafts(project: &Path, settings: Option<&Path>) -> Vec<Draft> {
    let mut found = Vec::new();
    let user = settings.map(user_folder);
    let mine: Vec<Template> =
        user.as_deref().map(|folder| read_folder(folder, Scope::User)).unwrap_or_default();
    for shipped in built_in() {
        let changed =
            mine.iter().find(|template| template.name.eq_ignore_ascii_case(&shipped.name));
        found.push(match changed {
            Some(template) => Draft {
                file: user
                    .as_ref()
                    .map(|folder| folder.join(file_name_of(&template.name, &template.extension))),
                template: template.clone(),
                include: false,
                removed: false,
            },
            None => Draft { template: shipped, include: false, file: None, removed: false },
        });
    }
    let mut rest: Vec<Draft> = Vec::new();
    for (folder, scope) in
        [(user.clone(), Scope::User), (Some(project_folder(project)), Scope::Project)]
    {
        let Some(folder) = folder else { continue };
        for (place, include) in [(folder.clone(), false), (folder.join("includes"), true)] {
            for template in read_folder(&place, scope) {
                if !include && scope == Scope::User && template.is_a_common_type() {
                    continue;
                }
                let file = place.join(file_name_of(&template.name, &template.extension));
                rest.push(Draft { template, include, file: Some(file), removed: false });
            }
        }
    }
    rest.sort_by_key(|draft| (draft.include, draft.template.name.to_lowercase()));
    found.extend(rest);
    found
}

/// Why `drafts` cannot be written as they are, or `None` when they can: a name that is empty or names a folder,
/// an extension with a separator in it, or two templates of one name in one folder.
///
/// @param drafts - what the manager holds
pub fn refusal(drafts: &[Draft]) -> Option<String> {
    let mut seen: Vec<(Scope, bool, String)> = Vec::new();
    for draft in drafts.iter().filter(|draft| !draft.removed) {
        let template = &draft.template;
        let name = template.name.trim();
        let what = if draft.include { "An include" } else { "A template" };
        if name.is_empty() {
            return Some(format!("{what} needs a name."));
        }
        if name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || name.starts_with('.') {
            return Some(format!("{name} is not a name a file can have."));
        }
        if template.extension.contains(['/', '\\', ' ', '.']) {
            return Some(format!(
                "The extension of {name} is one word with no dot, such as rs or tsx."
            ));
        }
        let scope = match template.scope {
            Scope::BuiltIn => Scope::User,
            other => other,
        };
        let key = (scope, draft.include, name.to_lowercase());
        if seen.contains(&key) {
            return Some(format!("Two templates in one place are both called {name}."));
        }
        seen.push(key);
    }
    None
}

/// Write the manager's changes: each draft's body and file name pattern into the folder it belongs in, a draft
/// that was renamed out of its old file, a removed one sent to the Recycle Bin, and a built in template that says
/// what Unluminous ships again with its own copy removed. A draft whose file already says the same is left alone.
/// Answers how many files changed.
///
/// @param drafts - what the manager holds
/// @param project - the project folder
/// @param settings - Unluminous's settings folder, when there is one
pub fn apply(drafts: &[Draft], project: &Path, settings: Option<&Path>) -> Result<usize, String> {
    if let Some(refusal) = refusal(drafts) {
        return Err(refusal);
    }
    let mut changed = 0;
    for draft in drafts {
        let template = &draft.template;
        let take_away = |file: &Path| -> Result<(), String> {
            for path in [file.to_path_buf(), settings_beside(file)] {
                if path.exists() {
                    crate::services::recycle::delete(&path).map_err(|problem| {
                        format!("Unluminous could not remove {}: {problem}", path.display())
                    })?;
                }
            }
            Ok(())
        };
        if draft.removed || (draft.is_built_in() && !draft.differs_from_the_original()) {
            if let Some(file) = draft.file.as_ref().filter(|file| file.exists()) {
                take_away(file)?;
                changed += 1;
            }
            continue;
        }
        let Some(folder) = folder_for(template.scope, draft.include, project, settings) else {
            return Err("Unluminous has no settings folder to keep templates in.".to_owned());
        };
        let target = folder.join(file_name_of(&template.name, &template.extension));
        if let Some(old) = draft.file.as_ref().filter(|old| **old != target && old.exists()) {
            take_away(old)?;
        }
        let same = std::fs::read_to_string(&target).ok().map(|text| text.replace("\r\n", "\n"))
            == Some(template.body.clone())
            && read_file_name_pattern(&target) == template.file_name.trim();
        if same {
            continue;
        }
        std::fs::create_dir_all(&folder).map_err(|problem| {
            format!("Unluminous could not make {}: {problem}", folder.display())
        })?;
        crate::services::store::write_a_source_file(&target, template.body.as_bytes()).map_err(
            |problem| format!("Unluminous could not write {}: {problem}", target.display()),
        )?;
        let beside = settings_beside(&target);
        match template.file_name.trim() {
            "" if beside.exists() => {
                crate::services::recycle::delete(&beside).map_err(|problem| problem.to_string())?
            }
            "" => {}
            pattern => crate::services::store::write_atomically(
                &beside,
                format!("file_name = {pattern}\n").as_bytes(),
            )
            .map_err(|problem| problem.to_string())?,
        }
        changed += 1;
    }
    Ok(changed)
}

/// The variables a template can use and what each holds, which the manager lists beside the body and inserts at
/// the caret. The reference editor's own list, less `PACKAGE_NAME`, which belongs to Java.
pub const VARIABLES: [(&str, &str); 17] = [
    ("NAME", "The name typed for the new file, without its extension"),
    ("FILE_NAME", "The new file's name with its extension"),
    ("DIR_PATH", "The folder the file goes in, from the project"),
    ("PROJECT_NAME", "The project folder's name"),
    ("USER", "Who is signed in on this computer"),
    ("DATE", "Today, as 2026-10-23"),
    ("TIME", "Now, as 14:05"),
    ("YEAR", "This year"),
    ("MONTH", "This month as a number"),
    ("MONTH_NAME_FULL", "This month's name"),
    ("MONTH_NAME_SHORT", "This month's name in three letters"),
    ("DAY", "Today's day of the month"),
    ("HOUR", "This hour"),
    ("MINUTE", "This minute"),
    ("SECOND", "This second"),
    ("PRODUCT_NAME", "Unluminous"),
    ("DS", "A dollar sign, where one would otherwise start a variable"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_can_call_string_methods_on_a_value() {
        let given = values(&[("NAME", "widget panel")]);
        let made =
            expand("${NAME.toUpperCase()} ${NAME.substring(0,3)} ${NAME.length()}", &given, None);
        assert_eq!(made.unwrap(), "WIDGET PANEL wid 12");
        assert!(expand("${NAME.explode()}", &given, None).unwrap_err().contains("explode"));
        assert_eq!(variables_asked("${Thing.toLowerCase()}", None), vec!["Thing".to_owned()]);
    }

    #[test]
    fn a_file_name_pattern_names_the_file_and_may_not_need_a_name_typed() {
        let mut test = built_in()[0].clone();
        test.file_name = "${NAME}_test".to_owned();
        assert!(asks_for_a_name(&test));
        assert_eq!(file_name_for(&test, "parser"), "parser_test.rs");
        assert_eq!(file_name_for(&test, "parser.rs"), "parser_test.rs");
        test.file_name = "lib".to_owned();
        assert!(!asks_for_a_name(&test));
        assert_eq!(file_name_for(&test, ""), "lib.rs");
    }

    #[test]
    fn the_manager_writes_renames_removes_and_resets_through_apply() {
        let project = scratch("manager");
        let settings = scratch("manager-settings");
        let mut list = drafts(&project, Some(&settings));
        assert_eq!(list.len(), 5, "only the built in ones to start with");
        // A new template in the project, with a file name pattern.
        let mut card = Draft::new("Card", Scope::Project, false);
        card.template.extension = "tsx".to_owned();
        card.template.body = "export const ${NAME} = () => null;\n".to_owned();
        card.template.file_name = "${NAME}.view".to_owned();
        list.push(card);
        // A changed built in template, and an include of the person's.
        list[0].template.body = "// ${NAME}\n".to_owned();
        let mut header = Draft::new("Header", Scope::User, true);
        header.template.extension = "rs".to_owned();
        header.template.body = "// by ${USER}\n".to_owned();
        list.push(header);
        assert_eq!(apply(&list, &project, Some(&settings)).unwrap(), 3);
        let card = project_folder(&project).join("Card.tsx.ft");
        assert!(card.is_file());
        assert_eq!(read_file_name_pattern(&card), "${NAME}.view");
        assert!(user_folder(&settings).join("includes").join("Header.rs.ft").is_file());
        let read = drafts(&project, Some(&settings));
        assert_eq!(read[0].template.body, "// ${NAME}\n");
        assert!(read[0].differs_from_the_original());
        assert_eq!(apply(&read, &project, Some(&settings)).unwrap(), 0, "nothing changed");
        // Rename the card, remove the include, and put Rust File back as it ships.
        let mut next = read.clone();
        let at = next.iter().position(|draft| draft.template.name == "Card").unwrap();
        next[at].template.name = "Panel".to_owned();
        next.iter_mut().find(|draft| draft.include).unwrap().removed = true;
        next[0].template = original("Rust File").unwrap();
        apply(&next, &project, Some(&settings)).unwrap();
        assert!(!card.exists() && project_folder(&project).join("Panel.tsx.ft").is_file());
        assert!(
            !user_folder(&settings).join("Rust File.rs.ft").exists(),
            "a reset removes the copy"
        );
        assert!(!user_folder(&settings).join("includes").join("Header.rs.ft").exists());
        // Two of one name in one place are refused before anything is written.
        let mut twice = drafts(&project, Some(&settings));
        twice.push(Draft::new("panel", Scope::Project, false));
        assert!(refusal(&twice).unwrap().contains("both called"));
        assert!(refusal(&[Draft::new("a/b", Scope::User, false)]).is_some());
    }

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect()
    }

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("unluminous-templates-{name}"));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_variable_is_filled_in_with_and_without_braces_and_a_dollar_alone_is_kept() {
        let given = values(&[("NAME", "page"), ("DS", "$")]);
        let filled = expand("<title>${NAME}</title> $NAME costs $5 and ${DS}x", &given, None);
        assert_eq!(filled.unwrap(), "<title>page</title> page costs $5 and $x");
    }

    #[test]
    fn a_variable_in_braces_with_no_value_is_refused_by_name_and_one_without_is_code() {
        assert_eq!(
            expand("by ${Author}", &values(&[]), None),
            Err("The template needs a value for Author.".to_owned())
        );
        let code = expand("$scope.x = $NAME;", &values(&[("NAME", "n")]), None);
        assert_eq!(code.unwrap(), "$scope.x = n;", "an unknown $word is code");
    }

    #[test]
    fn set_gives_a_variable_a_value_and_is_not_written_out() {
        let body = "#set( $Author = \"Jason ${NAME}\" )\n// ${Author}\n";
        assert_eq!(expand(body, &values(&[("NAME", "x")]), None).unwrap(), "// Jason x\n");
        assert!(variables_asked(body, None).is_empty(), "a set variable is not asked for");
    }

    #[test]
    fn the_variables_asked_for_are_the_ones_in_braces_nothing_fills() {
        let body = "${NAME} ${Author} ${DATE} ${Licence} ${Author} $scope";
        assert_eq!(variables_asked(body, None), vec!["Author".to_owned(), "Licence".to_owned()]);
    }

    #[test]
    fn parse_puts_an_include_in_and_a_loop_is_refused() {
        let folder = scratch("includes");
        std::fs::create_dir_all(folder.join("includes")).unwrap();
        std::fs::write(folder.join("includes").join("Header.rs.ft"), "// ${NAME} by ${Author}\n")
            .unwrap();
        std::fs::write(folder.join("includes").join("Loop.ft"), "#parse(\"Loop\")\n").unwrap();
        let body = "#parse(\"Header.rs\")\nfn main() {}\n";
        let given = values(&[("NAME", "main"), ("Author", "J")]);
        assert_eq!(expand(body, &given, Some(&folder)).unwrap(), "// main by J\nfn main() {}\n");
        assert_eq!(variables_asked(body, Some(&folder)), vec!["Author".to_owned()]);
        let looped = expand("#parse(\"Loop\")\n", &values(&[]), Some(&folder));
        assert!(looped.unwrap_err().contains("including itself"));
        let outside = expand("#parse(\"../x\")\n", &values(&[]), Some(&folder));
        assert!(outside.is_err(), "a path is not an include");
    }

    #[test]
    fn a_template_file_name_is_its_name_and_its_extension() {
        assert_eq!(
            parse_file_name("React Component.tsx.ft"),
            Some(("React Component".into(), "tsx".into()))
        );
        assert_eq!(parse_file_name("Makefile.ft"), Some(("Makefile".into(), String::new())));
        assert_eq!(parse_file_name("notes.txt"), None);
        assert_eq!(parse_file_name(".ft"), None);
        assert_eq!(file_name_of("React Component", "tsx"), "React Component.tsx.ft");
    }

    #[test]
    fn a_project_template_wins_over_a_user_one_and_both_over_the_built_in_one() {
        let project = scratch("project");
        let settings = scratch("settings");
        let user = user_folder(&settings);
        let ours = project_folder(&project);
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&ours).unwrap();
        std::fs::write(user.join("Rust File.rs.ft"), "// user\n").unwrap();
        std::fs::write(user.join("Widget.tsx.ft"), "user widget").unwrap();
        std::fs::write(ours.join("Widget.tsx.ft"), "project widget").unwrap();
        std::fs::write(ours.join("Alpha.md.ft"), "# ${NAME}").unwrap();
        let templates = all(&project, Some(&settings));
        let names: Vec<&str> = templates.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Rust File",
                "Text File",
                "JavaScript File",
                "HTML File",
                "CSS File",
                "Alpha",
                "Widget"
            ]
        );
        assert_eq!(templates[0].body, "// user\n");
        assert_eq!(templates[0].scope, Scope::User);
        let widget = find(&templates, "widget").unwrap();
        assert_eq!((widget.body.as_str(), widget.scope), ("project widget", Scope::Project));
        assert_eq!(find(&templates, "html-file").unwrap().extension, "html");
        assert!(find(&templates, "nothing").is_none());
    }

    #[test]
    fn a_file_name_gets_the_templates_extension_once() {
        let rust = &built_in()[0];
        assert_eq!(file_name_for(rust, "main"), "main.rs");
        assert_eq!(file_name_for(rust, "main.rs"), "main.rs");
        assert_eq!(file_name_for(rust, "Main.RS"), "Main.RS");
    }

    #[test]
    fn the_html_template_titles_the_page_with_its_name() {
        let html = find(&built_in(), "HTML File").unwrap().clone();
        let values = standard_values(Path::new("/p/site/index.html"), Path::new("/p"));
        assert!(expand(&html.body, &values, None).unwrap().contains("<title>index</title>"));
    }

    #[test]
    fn the_standard_values_name_the_file_its_folder_and_the_project() {
        let found = standard_values(Path::new("/work/app/src/ui/page.tsx"), Path::new("/work/app"));
        assert_eq!(found["NAME"], "page");
        assert_eq!(found["FILE_NAME"], "page.tsx");
        assert_eq!(found["DIR_PATH"], "src/ui");
        assert_eq!(found["PROJECT_NAME"], "app");
        assert_eq!(found["DATE"].len(), 10);
    }

    #[test]
    fn a_file_saved_as_a_template_is_listed_under_its_name() {
        let project = scratch("save-as");
        let file = project.join("card.tsx");
        std::fs::write(&file, "export const Card = () => null;\r\n").unwrap();
        let settings = scratch("save-as-settings");
        let saved = save_as_template(&file, "Card", &user_folder(&settings)).unwrap();
        assert!(saved.ends_with("Card.tsx.ft"));
        let card = find(&all(&project, Some(&settings)), "Card").unwrap().clone();
        assert_eq!(
            (card.extension.as_str(), card.body.as_str()),
            ("tsx", "export const Card = () => null;\n")
        );
        assert!(save_as_template(&file, "a/b", &user_folder(&settings)).is_err());
    }

    #[test]
    fn a_slug_is_lower_case_and_hyphenated() {
        assert_eq!(slug("React Component"), "react-component");
        assert_eq!(slug("  HTML File "), "html-file");
        assert_eq!(slug("C++ Header"), "c-header");
    }
}
