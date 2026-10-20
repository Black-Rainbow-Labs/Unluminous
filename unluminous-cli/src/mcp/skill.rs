//! The Unluminous skill, written beside an agent's `unluminous` MCP entry (`task-2314`).
//!
//! A tool description is read on every turn and has to be short; a skill is read when the agent
//! decides it needs it, so it can say how a call is written, which tool does which job, and what to
//! do when a call is refused. `skills/unluminous/SKILL.md` is the text, compiled in, and the test
//! below fails if it names a tool or a verb the catalogue has not got.
//!
//! **Where it goes.** All four agents read the same format, a folder holding `SKILL.md`:
//!
//! | Agent | Folder read |
//! |---|---|
//! | Claude Code | `~/.claude/skills/` (or under `CLAUDE_CONFIG_DIR`) |
//! | OpenCode | `~/.config/opencode/skills/`, `~/.claude/skills/` and `~/.agents/skills/`, one copy a name |
//! | Codex | `~/.agents/skills/` |
//! | Pi | `~/.pi/agent/skills/` and `~/.agents/skills/` |
//!
//! So Claude Code's copy goes in its own folder and every other agent's goes in `~/.agents/skills`,
//! which they share. OpenCode reading both is harmless, because it keeps one skill of a name.
//!
//! **Whose file it is.** A file is only replaced or removed when it carries [`MARK`], the line this
//! program writes into the frontmatter. A skill called `unluminous` that somebody wrote themselves is
//! left alone, which is the rule `mcp install` keeps about an entry that points at another program.
//! The shared copy is removed only when no agent that reads it still has an `unluminous` entry.

use std::path::{Path, PathBuf};

use super::install::{self, Client, Scope, Wanted};

/// The skill, as the agents will read it.
pub const SKILL: &str = include_str!("../../skills/unluminous/SKILL.md");

/// The frontmatter line that says this program wrote the file. Agents ignore a key they do not know.
pub const MARK: &str = "installed-by: unluminous-cli";

/// The skill's folder name, which every agent also requires to be its `name`.
pub const NAME: &str = "unluminous";

/// The folder an agent's copy of the skill goes in. A project scoped install puts it in the
/// project's own `.claude/skills` or `.agents/skills`, which the agents read from the folder they
/// were started in, beside the project's `.mcp.json`.
pub fn folder(client: Client, wanted: &Wanted) -> PathBuf {
    let shared = |base: PathBuf| base.join(".agents").join("skills").join(NAME);
    match (client, wanted.scope) {
        (Client::Claude, Scope::User) => install::claude_folder().join("skills").join(NAME),
        (Client::Claude, Scope::Project) => wanted.folder.join(".claude").join("skills").join(NAME),
        (_, Scope::User) => shared(install::home()),
        (_, Scope::Project) => shared(wanted.folder.clone()),
    }
}

/// The text written: the skill with [`MARK`] added to its frontmatter.
///
/// **Whatever line endings the checkout gave the file** (`task-2317`). git on Windows checks `SKILL.md` out
/// with `\r\n`, so a binary built there began `---\r\n`, the mark was never added, and the skill it installed
/// was one `mcp install` and `setup` could no longer recognise as their own, update or take out.
pub fn text() -> String {
    let skill = SKILL.replace("\r\n", "\n");
    match skill.strip_prefix("---\n") {
        Some(rest) => format!("---\n{MARK}\n{rest}"),
        None => skill,
    }
}

/// Whether the file at `path` is one this program wrote.
fn ours(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| text.lines().any(|line| line.trim() == MARK))
}

/// Write the skill for `client`. Answers with where it went, or `None` when a skill of that name that
/// somebody else wrote is already there and was left alone.
pub fn install_for(client: Client, wanted: &Wanted) -> Result<Option<PathBuf>, String> {
    let file = folder(client, wanted).join("SKILL.md");
    if file.exists() && !ours(&file) {
        return Ok(None);
    }
    let parent = file.parent().expect("a skill file has a folder");
    std::fs::create_dir_all(parent)
        .map_err(|problem| format!("could not make {}: {problem}", parent.display()))?;
    std::fs::write(&file, text())
        .map_err(|problem| format!("could not write {}: {problem}", file.display()))?;
    Ok(Some(file))
}

/// Take `client`'s copy out, when this program wrote it and no other agent still reads it. Answers
/// with the file removed, if one was.
pub fn remove_for(client: Client, wanted: &Wanted) -> Result<Option<PathBuf>, String> {
    let file = folder(client, wanted).join("SKILL.md");
    if !file.exists() || !ours(&file) {
        return Ok(None);
    }
    let shared_with: Vec<Client> = [Client::Codex, Client::OpenCode, Client::Pi]
        .into_iter()
        .filter(|other| *other != client && folder(*other, wanted) == folder(client, wanted))
        .filter(|other| install::installed(*other, wanted).is_some())
        .collect();
    if !shared_with.is_empty() {
        return Ok(None);
    }
    std::fs::remove_file(&file)
        .map_err(|problem| format!("could not remove {}: {problem}", file.display()))?;
    // The folder is this program's too once its one file is gone; anything else left in it is not.
    let parent = file.parent().expect("a skill file has a folder");
    let _ = std::fs::remove_dir(parent);
    Ok(Some(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue;

    #[test]
    fn the_frontmatter_has_what_every_agent_requires() {
        let text = text();
        let front = text.split("---").nth(1).expect("frontmatter");
        assert!(
            front.lines().any(|line| line.trim() == format!("name: {NAME}")),
            "name matches the folder"
        );
        let description = front
            .lines()
            .find_map(|line| line.trim().strip_prefix("description: "))
            .expect("a description");
        // OpenCode refuses more than 1,024 characters, and Codex shortens long ones first.
        assert!(description.len() <= 1024, "description is {} characters", description.len());
        assert!(front.contains(MARK));
    }

    /// **Every tool and verb the skill names is one the catalogue has.** A skill is read instead of
    /// the tool list, so a verb in it that does not exist is a call an agent makes with confidence
    /// and has refused.
    #[test]
    fn every_tool_and_verb_the_skill_names_exists() {
        let tools: Vec<String> = crate::mcp::tools::tools(crate::mcp::Shape::Grouped)
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        // A row names each tool by its area in bold (`**tab**`), because a client puts its own prefix
        // on the real name; the tool is the one whose name is `unluminous_` and the area.
        let mut rows = 0;
        for line in SKILL.lines().filter(|line| line.starts_with('|') && line.contains("**")) {
            rows += 1;
            let cell = line.split('|').nth(2).expect("a second cell");
            for (at, named) in cell.split("**").enumerate().filter(|(at, _)| at % 2 == 1) {
                let tool = format!("unluminous_{named}");
                assert!(tools.contains(&tool), "the skill names {named}, and there is no {tool}");
                // The commands after a tool belong to it, up to the next tool in the cell.
                let after = cell.split("**").nth(at + 1).unwrap_or_default();
                let verbs = after.split(" or ").next().unwrap_or_default();
                let area = named.replace('_', "-");
                if !catalogue::areas().contains(&area.as_str()) {
                    continue;
                }
                for verb in verbs.split('`').skip(1).step_by(2) {
                    let found = catalogue::find(&format!("{area} {verb}")).or_else(|| {
                        (area == "editor")
                            .then(|| catalogue::find(&format!("tab {verb}")))
                            .flatten()
                    });
                    assert!(
                        found.is_some(),
                        "the skill names `{area} {verb}`, which is not a command"
                    );
                }
            }
        }
        assert!(rows > 20, "the table of tools was not read: {rows} rows");
        for verb in ["batch", "commands"] {
            assert!(catalogue::find(verb).is_some(), "the skill names `{verb}`");
        }
    }

    #[test]
    fn a_skill_somebody_else_wrote_is_left_alone() {
        let _lock =
            install::tests::ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let home = std::env::temp_dir().join(format!("unluminous-skill-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("UNLUMINOUS_HOME", &home);

        let written = install_for(Client::OpenCode, &Wanted::default()).unwrap().expect("written");
        assert_eq!(
            written,
            home.join(".agents").join("skills").join("unluminous").join("SKILL.md")
        );
        assert!(ours(&written));
        let removed = remove_for(Client::OpenCode, &Wanted::default()).unwrap();
        assert_eq!(removed.as_deref(), Some(written.as_path()));
        assert!(!written.exists());

        std::fs::create_dir_all(written.parent().unwrap()).unwrap();
        std::fs::write(&written, "---\nname: unluminous\ndescription: mine\n---\n").unwrap();
        assert_eq!(
            install_for(Client::OpenCode, &Wanted::default()).unwrap(),
            None,
            "not replaced"
        );
        assert_eq!(remove_for(Client::OpenCode, &Wanted::default()).unwrap(), None, "not removed");
        assert!(std::fs::read_to_string(&written).unwrap().contains("mine"));

        std::env::remove_var("UNLUMINOUS_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }
}
