//! Telling Claude Code and Codex that a ticket's folder is trusted, before the agent is started in it.
//!
//! `task-2263`: *"when I run a task for the first time in a folder, i get message in terminal: Claude Code'll be
//! able to read, edit, and execute files here ... No, exit / Yes, I trust this folder. I can't select yes."* Both
//! agents ask that question the first time they meet a folder, and `--dangerously-skip-permissions` does not
//! answer it, because it is a question about the folder rather than about a tool. Every ticket worktree is a
//! folder neither agent has met, so every first run stopped there, and the handoff line the board types a few
//! seconds later went into the dialog instead of the prompt.
//!
//! The folder reaching this code is one the person chose: the board's project, or a worktree the board made
//! inside it. So the trust decision has already been made by a person, and what is missing is the agent being
//! told. It is recorded where each agent records its own answer: `hasTrustDialogAccepted` on the folder's entry
//! in Claude Code's `.claude.json`, and `trust_level = "trusted"` on the folder's table in Codex's
//! `config.toml`. ai-service does the same in `backend/src/utils/agentFolderTrust.ts` (`task-1801`).
//!
//! **Trust is a convenience and never a gate.** A file that cannot be read or parsed is left alone and the
//! agent is started anyway; the worst outcome is the question, which the ticket's Chat tab now shows as
//! buttons (`services::agent_tasks::screen_choice`).

use std::path::{Path, PathBuf};

/// The variable a test sets to name the Claude Code configuration file, so it never edits the person's own.
pub const CLAUDE_CONFIG_VARIABLE: &str = "UNLUMINOUS_CLAUDE_CONFIG_FILE";

/// The variable a test sets to name the Codex configuration file, so it never edits the person's own.
pub const CODEX_CONFIG_VARIABLE: &str = "UNLUMINOUS_CODEX_CONFIG_FILE";

/// Record that `folder` is trusted by the agent `program` names, and answer whether a file was changed.
///
/// `program` is the stem of the program being started (`claude`, `codex`). Anything else, such as OpenCode,
/// asks no trust question and is left alone.
pub fn trust(program: &str, folder: &Path) -> Result<bool, String> {
    match program {
        "claude" => match claude_config_file() {
            Some(file) => trust_for_claude(&file, folder),
            None => Ok(false),
        },
        "codex" => match codex_config_file() {
            Some(file) => trust_for_codex(&file, folder),
            None => Ok(false),
        },
        _ => Ok(false),
    }
}

/// Where Claude Code keeps its per folder answers.
///
/// The test variable first; then `CLAUDE_CONFIG_DIR/.claude.json`, which is where Claude Code reads it when that
/// variable is set; then `.claude.json` in the home folder. A unit test with no variable set gets `None`, so a
/// test that starts an agent never edits the configuration of the machine that ran it.
fn claude_config_file() -> Option<PathBuf> {
    if let Some(named) = named_by(CLAUDE_CONFIG_VARIABLE) {
        return Some(named);
    }
    if cfg!(test) {
        return None;
    }
    if let Some(folder) = named_by("CLAUDE_CONFIG_DIR") {
        return Some(folder.join(".claude.json"));
    }
    home().map(|home| home.join(".claude.json"))
}

/// Where Codex keeps its per folder answers: the test variable, `CODEX_HOME/config.toml`, then
/// `~/.codex/config.toml`.
fn codex_config_file() -> Option<PathBuf> {
    if let Some(named) = named_by(CODEX_CONFIG_VARIABLE) {
        return Some(named);
    }
    if cfg!(test) {
        return None;
    }
    if let Some(folder) = named_by("CODEX_HOME") {
        return Some(folder.join("config.toml"));
    }
    home().map(|home| home.join(".codex").join("config.toml"))
}

/// The path an environment variable holds, when it holds one.
fn named_by(variable: &str) -> Option<PathBuf> {
    std::env::var_os(variable).filter(|value| !value.is_empty()).map(PathBuf::from)
}

/// The person's home folder.
fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// A folder as a comparable string: forward slashes, no trailing slash, and lower case on Windows.
///
/// Claude Code keys its entries by the folder exactly as it was started in, so `C:\x`, `C:/x` and `c:/X` can all
/// be entries for one folder, and every one of them has to be found.
fn comparable(folder: &str) -> String {
    let forward = folder.trim().replace('\\', "/");
    let trimmed = forward.trim_end_matches('/');
    match cfg!(windows) {
        true => trimmed.to_lowercase(),
        false => trimmed.to_owned(),
    }
}

/// The key Claude Code itself writes for a folder: forward slashes and a capital drive letter on Windows.
fn claude_key(folder: &Path) -> String {
    let forward = folder.display().to_string().replace('\\', "/");
    let trimmed = forward.trim_end_matches('/');
    let mut characters = trimmed.chars();
    match (characters.next(), characters.next()) {
        (Some(drive), Some(':')) if drive.is_ascii_alphabetic() => {
            format!("{}{}", drive.to_ascii_uppercase(), &trimmed[1..])
        }
        _ => trimmed.to_owned(),
    }
}

/// Set `hasTrustDialogAccepted` on every entry in `file` that is `folder`, adding an entry when there is none.
///
/// A file that does not exist is left alone: it means Claude Code has never been run by this person, and
/// creating its configuration would be Unluminous deciding what the first run looks like.
pub fn trust_for_claude(file: &Path, folder: &Path) -> Result<bool, String> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Ok(false);
    };
    let mut config: serde_json::Value = serde_json::from_str(&text)
        .map_err(|problem| format!("{} is not JSON: {problem}", file.display()))?;
    let Some(root) = config.as_object_mut() else {
        return Err(format!("{} is not a JSON object", file.display()));
    };
    let projects = root.entry("projects").or_insert_with(|| serde_json::json!({}));
    let Some(projects) = projects.as_object_mut() else {
        return Err(format!("`projects` in {} is not an object", file.display()));
    };
    let wanted = comparable(&folder.display().to_string());
    let mut changed = false;
    let mut found = false;
    for (key, entry) in projects.iter_mut() {
        if comparable(key) != wanted {
            continue;
        }
        found = true;
        if entry.get("hasTrustDialogAccepted").and_then(|value| value.as_bool()) == Some(true) {
            continue;
        }
        if let Some(entry) = entry.as_object_mut() {
            entry.insert("hasTrustDialogAccepted".to_owned(), serde_json::Value::Bool(true));
            changed = true;
        }
    }
    if !found {
        projects.insert(claude_key(folder), new_claude_entry());
        changed = true;
    }
    if changed {
        let written = serde_json::to_string_pretty(&config)
            .map_err(|problem| format!("could not write {}: {problem}", file.display()))?;
        write_whole(file, &written)?;
    }
    Ok(changed)
}

/// The entry Claude Code writes for a folder it has just been told to trust, without its per session counts.
fn new_claude_entry() -> serde_json::Value {
    serde_json::json!({
        "allowedTools": [],
        "history": [],
        "mcpContextUris": [],
        "mcpServers": {},
        "enabledMcpjsonServers": [],
        "disabledMcpjsonServers": [],
        "hasTrustDialogAccepted": true,
        "hasClaudeMdExternalIncludesApproved": false,
        "hasClaudeMdExternalIncludesWarningShown": false,
    })
}

/// Set `trust_level = "trusted"` on `folder`'s table in Codex's `config.toml`, adding the table when there is none.
///
/// Edited line by line rather than parsed and written back, so every comment and the order of a file the person
/// edits by hand stay exactly as they were. A table that is there with another trust level has the line replaced,
/// because a second table of the same name is a TOML error that would stop Codex starting at all.
pub fn trust_for_codex(file: &Path, folder: &Path) -> Result<bool, String> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Ok(false);
    };
    match codex_with_trust(&text, folder) {
        Some(updated) => {
            write_whole(file, &updated)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// The folder a `[projects.'<folder>']` header names, or `None` for any other line.
fn codex_table_folder(line: &str) -> Option<String> {
    let header = line.trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    let named = header.strip_prefix("projects.")?;
    let unquoted = named
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .or_else(|| named.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')))
        .unwrap_or(named);
    Some(unquoted.replace("\\\\", "\\"))
}

/// `text` with `folder` trusted, or `None` when it already is.
fn codex_with_trust(text: &str, folder: &Path) -> Option<String> {
    let wanted = comparable(&folder.display().to_string());
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut inside = false;
    let mut header_at = None;
    let mut trust_at = None;
    for (index, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with('[') {
            inside = codex_table_folder(line).is_some_and(|named| comparable(&named) == wanted);
            if inside && header_at.is_none() {
                header_at = Some(index);
            }
            continue;
        }
        if inside && line.trim_start().starts_with("trust_level") && trust_at.is_none() {
            trust_at = Some(index);
        }
    }
    match (header_at, trust_at) {
        (Some(_), Some(at)) => {
            let value = lines[at].split_once('=').map(|(_, value)| value.trim()).unwrap_or("");
            if value.trim_matches(|c| c == '"' || c == '\'') == "trusted" {
                return None;
            }
            lines[at] = "trust_level = \"trusted\"".to_owned();
        }
        (Some(at), None) => lines.insert(at + 1, "trust_level = \"trusted\"".to_owned()),
        (None, _) => {
            // Codex writes the table itself with the folder as it was started in, and a literal string keeps a
            // Windows path's backslashes as they are.
            if lines.last().is_some_and(|last| !last.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[projects.'{}']", folder.display()));
            lines.push("trust_level = \"trusted\"".to_owned());
        }
    }
    let mut updated = lines.join("\n");
    updated.push('\n');
    Some(updated)
}

/// Write a file through a temporary file beside it and a rename, so the agent never reads half of one.
fn write_whole(file: &Path, text: &str) -> Result<(), String> {
    crate::services::store::write_atomically(file, text.as_bytes())
        .map_err(|problem| format!("could not write {}: {problem}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-folder-trust-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a scratch folder");
        folder
    }

    #[test]
    fn a_folder_claude_has_never_met_is_added_trusted_and_other_entries_are_kept() {
        let folder = scratch("claude-new");
        let file = folder.join(".claude.json");
        std::fs::write(
            &file,
            r#"{"numStartups": 3, "projects": {"C:/elsewhere": {"hasTrustDialogAccepted": false}}}"#,
        )
        .unwrap();
        let worktree = folder.join("worktree");
        assert!(trust_for_claude(&file, &worktree).unwrap());
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(config["numStartups"], 3);
        assert_eq!(config["projects"]["C:/elsewhere"]["hasTrustDialogAccepted"], false);
        assert_eq!(config["projects"][claude_key(&worktree)]["hasTrustDialogAccepted"], true);
        // A second run changes nothing, so the common case never rewrites the file.
        assert!(!trust_for_claude(&file, &worktree).unwrap());
    }

    #[test]
    fn every_spelling_of_one_folder_is_trusted_and_no_third_is_added() {
        let folder = scratch("claude-spellings");
        let file = folder.join(".claude.json");
        let worktree = folder.join("Ticket");
        let forward = worktree.display().to_string().replace('\\', "/");
        let back = worktree.display().to_string().replace('/', "\\");
        let config = serde_json::json!({"projects": {
            forward.clone(): {"hasTrustDialogAccepted": false},
            format!("{back}\\"): {"hasTrustDialogAccepted": false, "allowedTools": ["Bash"]},
        }});
        std::fs::write(&file, config.to_string()).unwrap();
        assert!(trust_for_claude(&file, &worktree).unwrap());
        let after: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let projects = after["projects"].as_object().unwrap();
        assert_eq!(projects.len(), 2, "no third entry: {projects:?}");
        assert!(projects.values().all(|entry| entry["hasTrustDialogAccepted"] == true));
        assert_eq!(projects[&format!("{back}\\")]["allowedTools"][0], "Bash");
    }

    #[test]
    fn a_missing_or_broken_claude_file_is_left_alone() {
        let folder = scratch("claude-missing");
        let file = folder.join(".claude.json");
        assert!(!trust_for_claude(&file, &folder).unwrap());
        assert!(!file.exists(), "Unluminous does not create Claude Code's configuration");
        std::fs::write(&file, "{ not json").unwrap();
        assert!(trust_for_claude(&file, &folder).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
    }

    #[test]
    fn codex_gets_a_table_and_keeps_every_comment() {
        let folder = scratch("codex-new");
        let file = folder.join("config.toml");
        std::fs::write(
            &file,
            "# mine\nmodel = \"gpt\"\n\n[projects.'c:\\other']\ntrust_level = \"trusted\"\n",
        )
        .unwrap();
        let worktree = folder.join("worktree");
        assert!(trust_for_codex(&file, &worktree).unwrap());
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# mine\nmodel = \"gpt\"\n"));
        assert!(text
            .contains(&format!("[projects.'{}']\ntrust_level = \"trusted\"", worktree.display())));
        assert!(!trust_for_codex(&file, &worktree).unwrap(), "already trusted");
    }

    #[test]
    fn a_codex_table_that_is_untrusted_has_its_line_replaced_rather_than_a_second_table() {
        let folder = scratch("codex-untrusted");
        let file = folder.join("config.toml");
        let worktree = folder.join("worktree");
        std::fs::write(
            &file,
            format!("[projects.'{}']\ntrust_level = \"untrusted\"\n", worktree.display()),
        )
        .unwrap();
        assert!(trust_for_codex(&file, &worktree).unwrap());
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(text.matches("[projects.").count(), 1);
        assert!(text.contains("trust_level = \"trusted\""));
        assert!(!text.contains("\"untrusted\""));
    }

    #[test]
    fn a_unit_test_with_no_variable_never_names_the_persons_own_files() {
        if std::env::var_os(CLAUDE_CONFIG_VARIABLE).is_none() {
            assert_eq!(claude_config_file(), None);
        }
        if std::env::var_os(CODEX_CONFIG_VARIABLE).is_none() {
            assert_eq!(codex_config_file(), None);
        }
    }
}
