//! The coding agents that read Agent Skills, and the folders each one reads them from.
//!
//! Read out of each agent's own documentation in October 2026; `_agent_output/task-2396-skills/research/
//! harnesses.md` has the page and the sentence for every row. A harness has one folder Unluminous writes into
//! for a project and one for the person (`write`), and the folders it reads (`reads`), which for most of them
//! include the shared `.agents/skills`. A skill is **seen** by a harness when any folder it reads holds it.

use std::path::{Path, PathBuf};

/// Where an install goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scope {
    /// The project's own folder, which the agent reads when it is started in the project.
    Project,
    /// The person's folder, which the agent reads in every project.
    Global,
}

impl Scope {
    pub fn name(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::Global => "global",
        }
    }

    pub fn parse(word: &str) -> Option<Scope> {
        match word.trim().trim_start_matches('-') {
            "project" | "here" | "local" => Some(Scope::Project),
            "global" | "user" | "everyone" | "all-projects" => Some(Scope::Global),
            _ => None,
        }
    }

    /// What a person reads for it.
    pub fn label(self) -> &'static str {
        match self {
            Scope::Project => "This project",
            Scope::Global => "Everyone",
        }
    }
}

/// A folder inside the home folder, or inside the folder an environment variable names when it is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeFolder {
    /// Under the home folder, with `/` between the parts.
    pub path: &'static str,
    /// When this variable is set, the folder is `<variable>/<inside>` instead.
    pub variable: Option<(&'static str, &'static str)>,
}

const fn home(path: &'static str) -> HomeFolder {
    HomeFolder { path, variable: None }
}

const fn home_or(path: &'static str, variable: &'static str, inside: &'static str) -> HomeFolder {
    HomeFolder { path, variable: Some((variable, inside)) }
}

/// One coding agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Harness {
    /// What `install` and `remove` name it by.
    pub id: &'static str,
    pub name: &'static str,
    /// Two letters for its disc on a card.
    pub initials: &'static str,
    /// The project folder Unluminous writes into, relative to the project.
    pub project: &'static str,
    /// The person's folder Unluminous writes into.
    pub global: HomeFolder,
    /// Other project folders it reads skills from.
    pub reads_project: &'static [&'static str],
    /// Other folders of the person's it reads skills from.
    pub reads_global: &'static [HomeFolder],
    /// A folder whose being there says the agent is used on this machine.
    pub detect: HomeFolder,
    /// What its documentation says about seeing a new skill.
    pub reload: &'static str,
}

const AGENTS: &str = ".agents/skills";
const CLAUDE: &str = ".claude/skills";
const SHARED_HOME: HomeFolder = home(".agents/skills");
const CLAUDE_HOME: HomeFolder = home_or(".claude/skills", "CLAUDE_CONFIG_DIR", "skills");

/// Every harness, in the order the pane and `harnesses` list them.
pub const HARNESSES: &[Harness] = &[
    Harness {
        id: "claude",
        name: "Claude Code",
        initials: "Cc",
        project: CLAUDE,
        global: CLAUDE_HOME,
        reads_project: &[],
        reads_global: &[],
        detect: home_or(".claude", "CLAUDE_CONFIG_DIR", ""),
        reload: "Seen at once; a skills folder that did not exist before needs /reload-skills.",
    },
    Harness {
        id: "codex",
        name: "Codex",
        initials: "Cx",
        project: AGENTS,
        global: SHARED_HOME,
        reads_project: &[],
        reads_global: &[],
        detect: home_or(".codex", "CODEX_HOME", ""),
        reload: "Seen at once; restart Codex if an update does not show.",
    },
    Harness {
        id: "opencode",
        name: "OpenCode",
        initials: "Oc",
        project: ".opencode/skills",
        global: home_or(".config/opencode/skills", "XDG_CONFIG_HOME", "opencode/skills"),
        reads_project: &[CLAUDE, AGENTS],
        reads_global: &[CLAUDE_HOME, SHARED_HOME],
        detect: home_or(".config/opencode", "XDG_CONFIG_HOME", "opencode"),
        reload: "Read when a session starts.",
    },
    Harness {
        id: "pi",
        name: "Pi",
        initials: "Pi",
        project: ".pi/skills",
        global: home_or(".pi/agent/skills", "PI_CODING_AGENT_DIR", "skills"),
        reads_project: &[AGENTS],
        reads_global: &[SHARED_HOME],
        detect: home_or(".pi", "PI_CODING_AGENT_DIR", ""),
        reload: "Run /reload.",
    },
    Harness {
        id: "gemini",
        name: "Gemini CLI",
        initials: "Ge",
        project: ".gemini/skills",
        global: home(".gemini/skills"),
        reads_project: &[AGENTS],
        reads_global: &[SHARED_HOME],
        detect: home(".gemini"),
        reload: "Run /skills reload.",
    },
    Harness {
        id: "cursor",
        name: "Cursor",
        initials: "Cu",
        project: ".cursor/skills",
        global: home(".cursor/skills"),
        reads_project: &[AGENTS, CLAUDE],
        reads_global: &[SHARED_HOME, CLAUDE_HOME],
        detect: home(".cursor"),
        reload: "Read when a chat starts.",
    },
    Harness {
        id: "copilot",
        name: "GitHub Copilot",
        initials: "Gh",
        project: ".github/skills",
        global: home_or(".copilot/skills", "COPILOT_HOME", "skills"),
        reads_project: &[CLAUDE, AGENTS],
        reads_global: &[CLAUDE_HOME, SHARED_HOME],
        detect: home_or(".copilot", "COPILOT_HOME", ""),
        reload: "Run /skills reload in Copilot CLI.",
    },
    Harness {
        id: "windsurf",
        name: "Windsurf",
        initials: "Ws",
        project: ".windsurf/skills",
        global: home(".codeium/windsurf/skills"),
        reads_project: &[AGENTS],
        reads_global: &[],
        detect: home(".codeium/windsurf"),
        reload: "Read when a chat starts.",
    },
    Harness {
        id: "amp",
        name: "Amp",
        initials: "Am",
        project: AGENTS,
        global: home(".config/agents/skills"),
        reads_project: &[CLAUDE],
        reads_global: &[SHARED_HOME, home(".config/amp/skills"), CLAUDE_HOME],
        detect: home(".config/amp"),
        reload: "Seen at once.",
    },
    Harness {
        id: "kiro",
        name: "Kiro",
        initials: "Ki",
        project: ".kiro/skills",
        global: home(".kiro/skills"),
        reads_project: &[],
        reads_global: &[],
        detect: home(".kiro"),
        reload: "Read when a chat starts.",
    },
    Harness {
        id: "cline",
        name: "Cline",
        initials: "Cl",
        project: ".cline/skills",
        global: home(".cline/skills"),
        reads_project: &[".clinerules/skills", CLAUDE],
        reads_global: &[],
        detect: home(".cline"),
        reload: "Read when a task starts.",
    },
    Harness {
        id: "roo",
        name: "Roo Code",
        initials: "Ro",
        project: ".roo/skills",
        global: home(".roo/skills"),
        reads_project: &[AGENTS],
        reads_global: &[SHARED_HOME],
        detect: home(".roo"),
        reload: "Seen at once.",
    },
    Harness {
        id: "goose",
        name: "Goose",
        initials: "Go",
        project: AGENTS,
        global: SHARED_HOME,
        reads_project: &[".goose/skills", CLAUDE],
        reads_global: &[],
        detect: home(".config/goose"),
        reload: "Start a new session.",
    },
    Harness {
        id: "droid",
        name: "Factory Droid",
        initials: "Fd",
        project: ".factory/skills",
        global: home(".factory/skills"),
        reads_project: &[AGENTS],
        reads_global: &[SHARED_HOME],
        detect: home(".factory"),
        reload: "Start a new session if the skill does not show.",
    },
    Harness {
        id: "junie",
        name: "Junie",
        initials: "Ju",
        project: ".junie/skills",
        global: home(".junie/skills"),
        reads_project: &[AGENTS],
        reads_global: &[SHARED_HOME],
        detect: home(".junie"),
        reload: "Run /skills.",
    },
    Harness {
        id: "qwen",
        name: "Qwen Code",
        initials: "Qw",
        project: ".qwen/skills",
        global: home(".qwen/skills"),
        reads_project: &[AGENTS],
        reads_global: &[SHARED_HOME],
        detect: home(".qwen"),
        reload: "Seen at once.",
    },
    Harness {
        id: "zed",
        name: "Zed",
        initials: "Ze",
        project: AGENTS,
        global: SHARED_HOME,
        reads_project: &[],
        reads_global: &[],
        detect: home(".config/zed"),
        reload: "Seen at once, in a trusted worktree.",
    },
];

/// The harness named `word`, by its id or its name, without regard to case.
pub fn find(word: &str) -> Option<&'static Harness> {
    let word = word.trim().to_lowercase();
    HARNESSES.iter().find(|harness| {
        harness.id == word
            || harness.name.to_lowercase() == word
            || harness.name.to_lowercase().replace(' ', "-") == word
            || (word == "claude-code" && harness.id == "claude")
            || (word == "gemini-cli" && harness.id == "gemini")
            || (word == "github-copilot" && harness.id == "copilot")
            || (word == "factory" && harness.id == "droid")
    })
}

/// The folders on this machine, worked out once from the home folder and the environment.
#[derive(Debug, Clone)]
pub struct Places {
    pub home: PathBuf,
    /// Whether the environment may move a folder. Off in a test, whose home is a folder it made.
    pub follow_the_environment: bool,
}

impl Places {
    /// This machine's places: `UNLUMINOUS_HOME` when a test set it, the person's home folder otherwise.
    pub fn here() -> Places {
        Places {
            home: unluminous_cli::mcp::install::home(),
            follow_the_environment: std::env::var_os("UNLUMINOUS_HOME").is_none(),
        }
    }

    /// Places under `home`, ignoring the environment, which is what a test uses.
    pub fn at(home: impl Into<PathBuf>) -> Places {
        Places { home: home.into(), follow_the_environment: false }
    }

    /// Where a folder of the person's is.
    pub fn resolve(&self, folder: &HomeFolder) -> PathBuf {
        if self.follow_the_environment {
            if let Some((variable, inside)) = folder.variable {
                if let Some(value) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
                    return join(PathBuf::from(value), inside);
                }
            }
        }
        join(self.home.clone(), folder.path)
    }

    /// The folder Unluminous writes `harness`'s skills into, for `scope`. `None` for a project scope with no
    /// project.
    pub fn write_folder(
        &self,
        harness: &Harness,
        scope: Scope,
        project: Option<&Path>,
    ) -> Option<PathBuf> {
        match scope {
            Scope::Global => Some(self.resolve(&harness.global)),
            Scope::Project => project.map(|project| join(project.to_path_buf(), harness.project)),
        }
    }

    /// Every folder `harness` reads skills from in `scope`, its own first.
    pub fn read_folders(
        &self,
        harness: &Harness,
        scope: Scope,
        project: Option<&Path>,
    ) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> =
            self.write_folder(harness, scope, project).into_iter().collect();
        match scope {
            Scope::Global => {
                folders.extend(harness.reads_global.iter().map(|folder| self.resolve(folder)))
            }
            Scope::Project => {
                if let Some(project) = project {
                    folders.extend(
                        harness
                            .reads_project
                            .iter()
                            .map(|inside| join(project.to_path_buf(), inside)),
                    );
                }
            }
        }
        let mut seen = Vec::new();
        folders.retain(|folder| {
            let fresh = !seen.contains(folder);
            seen.push(folder.clone());
            fresh
        });
        folders
    }

    /// Whether the agent is used on this machine.
    pub fn is_here(&self, harness: &Harness) -> bool {
        self.resolve(&harness.detect).is_dir()
    }

    /// Every folder any harness writes to or reads from, for one scope, each once.
    pub fn every_folder(&self, scope: Scope, project: Option<&Path>) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        for harness in HARNESSES {
            for folder in self.read_folders(harness, scope, project) {
                if !out.contains(&folder) {
                    out.push(folder);
                }
            }
        }
        out
    }
}

/// `base` with a relative path of `/` separated parts added, part by part, so the result uses the platform's
/// own separator.
fn join(mut base: PathBuf, inside: &str) -> PathBuf {
    for part in inside.split('/').filter(|part| !part.is_empty()) {
        base.push(part);
    }
    base
}

/// A folder as a person reads it: `~` for the home folder, the project's own path left relative.
pub fn shown(places: &Places, folder: &Path, project: Option<&Path>) -> String {
    let written = |path: &Path| path.display().to_string().replace('\\', "/");
    if let Some(project) = project {
        if let Ok(inside) = folder.strip_prefix(project) {
            return written(inside);
        }
    }
    match folder.strip_prefix(&places.home) {
        Ok(inside) => format!("~/{}", written(inside)),
        Err(_) => written(folder),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_harness_has_a_unique_id_and_initials() {
        for (index, harness) in HARNESSES.iter().enumerate() {
            assert!(
                HARNESSES[index + 1..].iter().all(|other| other.id != harness.id),
                "{}",
                harness.id
            );
            assert_eq!(harness.initials.chars().count(), 2, "{}", harness.id);
            assert_eq!(find(harness.id), Some(harness));
            assert_eq!(find(harness.name), Some(harness));
        }
        assert_eq!(find("Claude-Code").map(|harness| harness.id), Some("claude"));
    }

    #[test]
    fn folders_are_under_the_home_or_the_project() {
        let places = Places::at("/home/test");
        let claude = find("claude").unwrap();
        let project = Path::new("/work/app");
        assert_eq!(
            places.write_folder(claude, Scope::Global, None),
            Some(PathBuf::from("/home/test").join(".claude").join("skills"))
        );
        assert_eq!(
            places.write_folder(claude, Scope::Project, Some(project)),
            Some(project.join(".claude").join("skills"))
        );
        assert_eq!(places.write_folder(claude, Scope::Project, None), None);
        let cursor = find("cursor").unwrap();
        let reads = places.read_folders(cursor, Scope::Project, Some(project));
        assert_eq!(reads[0], project.join(".cursor").join("skills"), "its own first");
        assert!(reads.contains(&project.join(".agents").join("skills")));
        assert!(reads.contains(&project.join(".claude").join("skills")));
    }

    #[test]
    fn codex_goose_and_zed_share_one_folder() {
        let places = Places::at("/h");
        let folders: Vec<PathBuf> = ["codex", "goose", "zed"]
            .iter()
            .map(|id| places.write_folder(find(id).unwrap(), Scope::Global, None).unwrap())
            .collect();
        assert!(folders.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn a_folder_is_shown_with_a_tilde_or_relative_to_the_project() {
        let places = Places::at("/home/test");
        let global = places.write_folder(find("claude").unwrap(), Scope::Global, None).unwrap();
        assert_eq!(shown(&places, &global, None), "~/.claude/skills");
        let project = Path::new("/work/app");
        let local =
            places.write_folder(find("codex").unwrap(), Scope::Project, Some(project)).unwrap();
        assert_eq!(shown(&places, &local, Some(project)), ".agents/skills");
    }
}
