//! The verbs of `unluminous-cli plugins run skills ...`, each through the function its button calls.
//!
//! A skill is named by its id, such as `caveman`. A harness is named by its id or its name (`claude`, `codex`,
//! `Claude Code`), and `all` names every one. `--project` and `--global` (or the words `project` and `global`)
//! say where an install goes; without either it goes where the pane's switch says.

use serde_json::json;

use super::harness::{self, Scope, HARNESSES};
use super::model::{self, Skill};
use super::{frontmatter, ops, Filter, Modal, Skills};
use crate::services::plugin_ui::{rest, Answer};

/// The words of a command once its flags are taken out.
struct Parsed {
    words: Vec<String>,
    scope: Option<Scope>,
    force: bool,
    discard: bool,
}

/// Take `--project`, `--global`, `--force` and `--discard` out of a command's arguments, wherever they are.
fn parse(arguments: &[String]) -> Parsed {
    let mut parsed = Parsed { words: Vec::new(), scope: None, force: false, discard: false };
    for word in arguments {
        match word.as_str() {
            "--force" | "force" => parsed.force = true,
            "--discard" | "discard" => parsed.discard = true,
            other if other.starts_with("--") && Scope::parse(other).is_some() => {
                parsed.scope = Scope::parse(other)
            }
            "project" | "global" => parsed.scope = Scope::parse(word),
            _ => parsed.words.push(word.clone()),
        }
    }
    parsed
}

/// Answer one verb. A verb asked before the first read of the skills folders has landed waits for it, because
/// every answer names skills.
///
/// @param skills - the provider
/// @param command - the verb
/// @param arguments - the words after it
pub fn run(skills: &mut Skills, command: &str, arguments: &[String]) -> Result<Answer, String> {
    skills.wait_for_the_first_read();
    let parsed = parse(arguments);
    match command {
        "open-pane" => Ok(Answer::said("showing the Skills Manager")),
        "list" => list(skills, &parsed),
        "show" => show(skills, &parsed),
        "text" => text(skills, &parsed),
        "harnesses" => Ok(harnesses(skills)),
        "install" | "remove" => install_or_remove(skills, command, &parsed),
        "new" | "edit" | "write" | "delete" | "reset" => {
            library(skills, command, &parsed, arguments)
        }
        "check" | "update" => site(skills, command, &parsed),
        "scope" | "search" | "view" | "filter" => pane(skills, command, &parsed),
        other => {
            let known: Vec<&str> =
                unluminous_cli::plugins::SKILLS.iter().map(|(name, _)| *name).collect();
            Err(format!("there is no `{other}` command on `skills`: {}", known.join(", ")))
        }
    }
}

/// The skill a command names first.
fn named<'a>(skills: &'a Skills, parsed: &Parsed) -> Result<&'a Skill, String> {
    let id = parsed.words.first().ok_or("Name the skill, such as `caveman`.")?;
    skills.skill(id)
}

/// One skill as data: where it came from, whether it changed, and where it is installed.
pub fn skill_value(skills: &Skills, skill: &Skill) -> serde_json::Value {
    let project = skills.project.as_deref();
    let installed: Vec<serde_json::Value> = [Scope::Project, Scope::Global]
        .into_iter()
        .flat_map(|scope| {
            model::seen_by_ids(skill, &skills.places, scope, project)
                .into_iter()
                .map(move |harness| json!({ "harness": harness, "scope": scope.name() }))
        })
        .collect();
    let copies: Vec<serde_json::Value> = skill
        .copies
        .iter()
        .map(|copy| {
            json!({
                "folder": harness::shown(&skills.places, &copy.folder, project),
                "scope": copy.scope.name(),
                "ours": copy.ours,
                "current": copy.current,
            })
        })
        .collect();
    json!({
        "id": skill.id,
        "name": skill.entry.name,
        "author": skill.entry.author,
        "summary": skill.entry.summary,
        "category": skill.entry.category,
        "source": skill.source.name(),
        "edited": skill.edited,
        "update": skill.update,
        "partial": skill.partial,
        "library": skill.library.as_ref().map(|folder| folder.display().to_string()),
        "installed": installed,
        "copies": copies,
    })
}

/// `list [filter]`.
fn list(skills: &mut Skills, parsed: &Parsed) -> Result<Answer, String> {
    let filter = match parsed.words.first() {
        Some(word) if word == "unluminous" || word == "found" || word == "catalogue" => None,
        Some(word) => Some(Filter::parse(word).ok_or_else(|| {
            format!("`{word}` is not all, installed, mine, updates, unluminous or found.")
        })?),
        None => Some(Filter::All),
    };
    let wanted_source = parsed.words.first().filter(|_| filter.is_none()).cloned();
    let rows: Vec<serde_json::Value> = skills
        .shelf
        .skills
        .iter()
        .filter(|skill| filter.is_none_or(|filter| filter.keeps(skill)))
        .filter(|skill| wanted_source.as_deref().is_none_or(|source| skill.source.name() == source))
        .map(|skill| skill_value(skills, skill))
        .collect();
    Ok(Answer::said(counted(rows.len(), "skill"))
        .with(json!({ "scope": skills.scope_now().name(), "skills": rows })))
}

/// `show <id>`.
fn show(skills: &mut Skills, parsed: &Parsed) -> Result<Answer, String> {
    let skill = named(skills, parsed)?;
    let mut value = skill_value(skills, skill);
    value["description"] = json!(skill.entry.description);
    value["license"] = json!(skill.entry.license);
    value["repository"] = json!(skill.entry.repository);
    value["popularity"] = json!(skill.entry.popularity);
    value["tags"] = json!(skill.entry.tags);
    // The skills it calls with the Skill tool, which an install puts beside it (`task-2402`).
    value["calls"] = json!(skill.entry.requires);
    value["files"] = json!(ops::files_of(skill)
        .map(|files| files.keys().cloned().collect::<Vec<String>>())
        .unwrap_or_default());
    value["fetching"] = json!(skills.busy);
    Ok(Answer::said(format!("{}: {}", skill.entry.name, skill.entry.summary)).with(value))
}

/// `text <id>`: the `SKILL.md`.
fn text(skills: &mut Skills, parsed: &Parsed) -> Result<Answer, String> {
    let skill = named(skills, parsed)?;
    let words = skill_md(skill);
    Ok(Answer::said(words.clone()).with(json!({ "id": skill.id, "text": words })))
}

/// A skill's `SKILL.md` as it is now: the found file, the library copy, or what shipped.
pub fn skill_md(skill: &Skill) -> String {
    match skill.source {
        model::Source::Found => skill
            .copies
            .first()
            .and_then(|copy| std::fs::read_to_string(copy.folder.join("SKILL.md")).ok())
            .unwrap_or_default(),
        _ => ops::files_of(skill)
            .ok()
            .and_then(|files| {
                files.get("SKILL.md").map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            })
            .unwrap_or_else(|| catalogue_text(skill)),
    }
}

/// What shipped for a skill whose other files are on the site.
fn catalogue_text(skill: &Skill) -> String {
    let files = super::catalogue::bundled_files(&skill.id);
    files
        .get("SKILL.md")
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
}

/// `harnesses`.
fn harnesses(skills: &Skills) -> Answer {
    let chosen: Vec<&str> =
        skills.config.chosen_harnesses(&skills.places).iter().map(|harness| harness.id).collect();
    let project = skills.project.as_deref();
    let rows: Vec<serde_json::Value> = HARNESSES
        .iter()
        .map(|harness| {
            json!({
                "id": harness.id,
                "name": harness.name,
                "onThisMachine": skills.places.is_here(harness),
                "chosen": chosen.contains(&harness.id),
                "project": skills.places.write_folder(harness, Scope::Project, project).map(|folder| harness::shown(&skills.places, &folder, project)),
                "global": skills.places.write_folder(harness, Scope::Global, project).map(|folder| harness::shown(&skills.places, &folder, project)),
                "reload": harness.reload,
            })
        })
        .collect();
    Answer::said(format!("{} agents; installs go to {}", HARNESSES.len(), chosen.join(", ")))
        .with(json!({ "home": skills.places.home.display().to_string(), "harnesses": rows }))
}

/// `install <id> [harness...]` and `remove <id> [harness...]`.
fn install_or_remove(
    skills: &mut Skills,
    command: &str,
    parsed: &Parsed,
) -> Result<Answer, String> {
    let id = parsed.words.first().cloned().ok_or("Name the skill, such as `caveman`.")?;
    let targets = skills.harnesses_for(&parsed.words[1..])?;
    let scope = parsed.scope.unwrap_or_else(|| skills.scope_now());
    if scope == Scope::Project && skills.project.is_none() {
        return Err("No project is open, so there is no project folder; use --global.".into());
    }
    let said = match command {
        "install" => skills.install(&id, &targets, scope, parsed.force)?,
        _ => skills.remove(&id, &targets, scope, parsed.force)?,
    };
    skills.say(said.clone());
    let value = skills.skill(&id).map(|skill| skill_value(skills, skill)).unwrap_or(json!(null));
    Ok(Answer::said(said).with(value))
}

/// `new`, `edit`, `write`, `delete` and `reset`.
fn library(
    skills: &mut Skills,
    command: &str,
    parsed: &Parsed,
    arguments: &[String],
) -> Result<Answer, String> {
    match command {
        "new" => {
            let Some(name) = parsed.words.first() else {
                skills.modal =
                    Some(Modal::NewSkill { name: String::new(), description: String::new() });
                return Ok(Answer::said("Name the new skill").with(json!({ "asked": true })));
            };
            let description = parsed.words[1..].join(" ");
            let (id, file) = skills.new_skill(name, &description)?;
            Ok(Answer::said(format!("Made {id}; its SKILL.md is open"))
                .with(json!({ "id": id, "file": file.display().to_string() })))
        }
        "edit" => {
            let id = parsed.words.first().cloned().ok_or("Name the skill.")?;
            let said = skills.edit(&id)?;
            Ok(Answer::said(said))
        }
        "write" => {
            let skill = named(skills, parsed)?.clone();
            let words = rest(arguments, 1);
            if frontmatter::parse(&words).name().is_none() {
                return Err("The text has no frontmatter naming the skill; start it with ---, name: and description:.".into());
            }
            let file = ops::write_text(&skills.here(), &skill, &words)?;
            skills.refresh();
            Ok(Answer::said(format!("Wrote {}; every installed copy follows it.", file.display())))
        }
        "delete" => {
            let skill = named(skills, parsed)?.clone();
            let said = ops::delete(&skills.here(), &skill)?;
            skills.refresh();
            skills.viewing = None;
            Ok(Answer::said(said))
        }
        _ => {
            let skill = named(skills, parsed)?.clone();
            let said = ops::reset(&skills.here(), &skill)?;
            skills.refresh();
            Ok(Answer::said(said))
        }
    }
}

/// `check` and `update <id|all> [--discard]`.
fn site(skills: &mut Skills, command: &str, parsed: &Parsed) -> Result<Answer, String> {
    if command == "check" {
        return skills.check().map(Answer::said);
    }
    let wanted = parsed.words.first().cloned().ok_or("Name the skill to update, or all.")?;
    let ids: Vec<String> = match wanted.as_str() {
        "all" => skills
            .shelf
            .skills
            .iter()
            .filter(|skill| skill.update)
            .map(|skill| skill.id.clone())
            .collect(),
        id => vec![id.to_owned()],
    };
    if ids.is_empty() {
        return Ok(Answer::said("Every skill is up to date."));
    }
    let mut lines = Vec::new();
    for id in ids {
        lines.push(skills.update(&id, parsed.discard).unwrap_or_else(|why| why));
    }
    Ok(Answer::said(lines.join(" ")))
}

/// `scope`, `search`, `filter` and `view`: the pane's own controls.
fn pane(skills: &mut Skills, command: &str, parsed: &Parsed) -> Result<Answer, String> {
    let first = parsed.words.first().map(String::as_str);
    match command {
        "scope" => {
            if let Some(scope) = parsed.scope.or_else(|| first.and_then(Scope::parse)) {
                skills.scope = scope;
            }
            Ok(Answer::said(format!("Installs go to {}", skills.scope_now().label()))
                .with(json!({ "scope": skills.scope_now().name() })))
        }
        "search" => {
            skills.search = parsed.words.join(" ");
            let ids: Vec<&str> = skills.listed().iter().map(|skill| skill.id.as_str()).collect();
            Ok(Answer::said(counted(ids.len(), "skill")).with(json!({ "skills": ids })))
        }
        "filter" => {
            if let Some(word) = first {
                skills.filter = Filter::parse(word)
                    .ok_or_else(|| format!("`{word}` is not all, installed, mine or updates."))?;
            }
            Ok(Answer::said(skills.filter.title()).with(json!({ "filter": skills.filter.name() })))
        }
        _ => match first {
            None | Some("list") | Some("grid") => {
                skills.viewing = None;
                Ok(Answer::said("Showing the skills"))
            }
            Some(id) => {
                skills.skill(id)?;
                skills.viewing = Some(id.to_owned());
                Ok(Answer::said(format!("Showing {id}")))
            }
        },
    }
}

/// The whole pane as data, for `plugins view skills`.
pub fn view_value(skills: &Skills) -> serde_json::Value {
    json!({
        "scope": skills.scope_now().name(),
        "filter": skills.filter.name(),
        "search": skills.search,
        "viewing": skills.viewing,
        "status": skills.status,
        "fetching": skills.busy,
        "catalogue": skills.remote.as_ref().map(|remote| remote.updated.clone()),
        "listed": skills.listed().iter().map(|skill| skill.id.as_str()).collect::<Vec<&str>>(),
        "modal": skills.modal.as_ref().map(|modal| match modal {
            Modal::NewSkill { .. } => "new-skill",
            Modal::Delete { .. } => "delete",
        }),
        "updates": skills.shelf.skills.iter().filter(|skill| skill.update).count(),
        "loading": !skills.read,
        "picturesWaiting": skills.pictures.waiting(),
    })
}

/// `3 skills`, `1 skill`.
fn counted(count: usize, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        many => format!("{many} {noun}s"),
    }
}
