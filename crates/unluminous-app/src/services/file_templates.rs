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
    custom.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    templates.extend(custom);
    templates
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
            Some(Template { name, extension, body: body.replace("\r\n", "\n"), scope })
        })
        .collect()
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

/// What a file is called when it is made from `template` with `name` typed: the extension is added when
/// what was typed does not already end with it.
///
/// @param template - the template
/// @param name - what was typed
pub fn file_name_for(template: &Template, name: &str) -> String {
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

/// Put each `${NAME}` and `$NAME` in `text` in, from `values`.
fn substitute(text: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
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

/// Make the user's templates folder when it is missing, with a short read me so it is not an empty folder
/// with no explanation. Answers the folder.
///
/// @param settings - Unluminous's settings folder
pub fn ensure_user_folder(settings: &Path) -> Result<PathBuf, String> {
    let folder = user_folder(settings);
    std::fs::create_dir_all(folder.join("includes"))
        .map_err(|problem| format!("Unluminous could not make {}: {problem}", folder.display()))?;
    let readme = folder.join("README.txt");
    if !readme.exists() {
        let _ = crate::services::store::write_atomically(&readme, README.as_bytes());
    }
    Ok(folder)
}

/// What the templates folder's read me says.
const README: &str = "Unluminous file templates\n\
\n\
Each file here called <Name>.<extension>.ft is a template, listed under New -> Templates on the explorer's\n\
right click menu and under New Tab on the editor's. A project's own templates go in .unluminous/templates in\n\
the project, and win over these. A template named Rust File, Text File, JavaScript File, HTML File or CSS File\n\
replaces the one Unluminous ships.\n\
\n\
Variables: ${NAME} (the name typed, without its extension), ${FILE_NAME}, ${DIR_PATH}, ${PROJECT_NAME},\n\
${USER}, ${DATE}, ${TIME}, ${YEAR}, ${MONTH}, ${MONTH_NAME_FULL}, ${MONTH_NAME_SHORT}, ${DAY}, ${HOUR},\n\
${MINUTE}, ${SECOND}, ${PRODUCT_NAME} and ${DS} for a dollar sign. Any other variable in braces is asked for\n\
when the file is made.\n\
\n\
#set( $Author = \"Jason\" ) gives a variable a value. #parse(\"Header.rs\") puts in the include\n\
includes/Header.rs.ft.\n";

#[cfg(test)]
mod tests {
    use super::*;

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
