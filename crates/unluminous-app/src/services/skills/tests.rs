//! The Skills Manager with no window: installing, removing, keeping copies in step, the library, and the
//! verbs, each in a home folder and a project made for the test.

use std::path::{Path, PathBuf};

use super::harness::{find, Places, Scope};
use super::model::Source;
use super::{frontmatter, Skills};
use crate::services::plugin_ui::{Context, UiProvider};

/// A home, a project and a plugin folder of the test's own, and the provider opened on them.
struct Bench {
    root: PathBuf,
    skills: Skills,
}

impl Bench {
    fn new(name: &str) -> Bench {
        let root =
            std::env::temp_dir().join(format!("unluminous-skills-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for folder in ["home", "project", "plugin"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        let mut skills = Skills::new();
        skills.places = Places::at(root.join("home"));
        let context = Context {
            project: Some(root.join("project")),
            folder: Some(root.join("plugin")),
            ..Context::default()
        };
        skills.open(&context).unwrap();
        // Opening starts the read on a thread (`task-2401`); a test reads the shelf straight away.
        skills.wait_for_the_first_read();
        Bench { root, skills }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn project(&self) -> PathBuf {
        self.root.join("project")
    }

    fn run(&mut self, verb: &str, words: &[&str]) -> Result<String, String> {
        let words: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
        self.skills.command(verb, &words).map(|answer| answer.message)
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn every_bundled_skill_is_listed_with_unluminous_first() {
    let bench = Bench::new("listed");
    let first = &bench.skills.shelf.skills[0];
    assert_eq!(first.source, Source::Unluminous);
    assert_eq!(first.id, "unluminous-task");
    assert!(bench.skills.shelf.skills.len() >= 33, "{}", bench.skills.shelf.skills.len());
}

#[test]
fn install_writes_a_marked_copy_for_the_project_and_remove_takes_it_away() {
    let mut bench = Bench::new("install");
    let said = bench.run("install", &["caveman", "claude", "--project"]).unwrap();
    assert!(said.contains("Claude Code: installed"), "{said}");
    let copy = bench.project().join(".claude").join("skills").join("caveman").join("SKILL.md");
    let text = read(&copy);
    assert!(frontmatter::has_mark(&text), "the copy says Unluminous wrote it");
    assert_eq!(frontmatter::parse(&text).name(), Some("caveman"));
    let skill = bench.skills.skill("caveman").unwrap();
    assert!(skill.seen_by(
        &bench.skills.places,
        find("claude").unwrap(),
        Scope::Project,
        Some(&bench.project())
    ));
    assert!(!skill.seen_by(
        &bench.skills.places,
        find("claude").unwrap(),
        Scope::Global,
        Some(&bench.project())
    ));
    bench.run("remove", &["caveman", "claude", "project"]).unwrap();
    assert!(!copy.exists(), "removed");
}

#[test]
fn agents_that_share_a_folder_get_one_copy() {
    let mut bench = Bench::new("shared");
    let said = bench.run("install", &["tdd", "codex", "cursor", "--global"]).unwrap();
    let shared = bench.home().join(".agents").join("skills").join("tdd");
    assert!(shared.join("SKILL.md").exists(), "{said}");
    assert!(said.contains("Cursor: reads it from ~/.agents/skills"), "{said}");
    assert!(!bench.home().join(".cursor").join("skills").join("tdd").exists(), "no second copy");
    let skill = bench.skills.skill("tdd").unwrap();
    for agent in ["codex", "cursor", "pi", "goose"] {
        assert!(
            skill.seen_by(&bench.skills.places, find(agent).unwrap(), Scope::Global, None),
            "{agent} reads it"
        );
    }
}

#[test]
fn a_folder_somebody_else_wrote_is_left_alone_without_force() {
    let mut bench = Bench::new("theirs");
    let theirs = bench.home().join(".claude").join("skills").join("caveman");
    std::fs::create_dir_all(&theirs).unwrap();
    std::fs::write(theirs.join("SKILL.md"), "---\nname: caveman\ndescription: mine\n---\nmy own\n")
        .unwrap();
    bench.skills.refresh();
    let said = bench.run("install", &["caveman", "claude", "--global"]).unwrap();
    assert!(said.contains("did not write"), "{said}");
    assert!(read(&theirs.join("SKILL.md")).contains("my own"), "untouched");
    let said = bench.run("remove", &["caveman", "claude", "--global"]).unwrap();
    assert!(said.contains("not written by Unluminous"), "{said}");
    assert!(theirs.join("SKILL.md").exists());
}

#[test]
fn a_skill_on_the_machine_that_nothing_knows_is_found() {
    let mut bench = Bench::new("found");
    let folder = bench.home().join(".pi").join("agent").join("skills").join("my-notes");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("SKILL.md"),
        "---\nname: my-notes\ndescription: Keeps notes. Always.\n---\n",
    )
    .unwrap();
    bench.skills.refresh();
    let skill = bench.skills.skill("my-notes").unwrap();
    assert_eq!(skill.source, Source::Found);
    assert_eq!(skill.entry.summary, "Keeps notes.");
    assert!(bench.run("text", &["my-notes"]).unwrap().contains("Keeps notes"));
}

#[test]
fn a_new_skill_is_written_into_the_library_and_opened() {
    let mut bench = Bench::new("new");
    let said = bench.run("new", &["Release Notes", "Write", "release", "notes"]).unwrap();
    assert!(said.contains("release-notes"), "{said}");
    let file = bench.root.join("plugin").join("library").join("release-notes").join("SKILL.md");
    assert_eq!(frontmatter::parse(&read(&file)).description(), Some("Write release notes"));
    let asked = bench.skills.asking();
    assert!(asked.iter().any(|request| matches!(request, crate::services::plugin_ui::Request::OpenFile(path) if path == &file)));
    assert_eq!(bench.skills.skill("release-notes").unwrap().source, Source::Mine);
    assert!(bench.run("new", &["release-notes"]).is_err(), "a name is taken once");
}

#[test]
fn an_edit_reaches_every_installed_copy_and_reset_puts_back_what_shipped() {
    let mut bench = Bench::new("edit");
    bench.run("install", &["unluminous-task", "claude", "codex", "--project"]).unwrap();
    bench.run("edit", &["unluminous-task"]).unwrap();
    let library =
        bench.root.join("plugin").join("library").join("unluminous-task").join("SKILL.md");
    let edited = format!("{}\nOne more rule.\n", read(&library));
    std::fs::write(&library, &edited).unwrap();
    bench.skills.refresh();
    let skill = bench.skills.skill("unluminous-task").unwrap();
    assert!(skill.edited, "the library copy differs from what shipped");
    for folder in [".claude", ".agents"] {
        let copy =
            bench.project().join(folder).join("skills").join("unluminous-task").join("SKILL.md");
        assert!(read(&copy).contains("One more rule."), "{folder} has the edit");
    }
    bench.run("reset", &["unluminous-task"]).unwrap();
    let skill = bench.skills.skill("unluminous-task").unwrap();
    assert!(!skill.edited && skill.library.is_none());
    let copy =
        bench.project().join(".claude").join("skills").join("unluminous-task").join("SKILL.md");
    assert!(!read(&copy).contains("One more rule."), "the copies follow the reset");
}

#[test]
fn a_skill_of_my_own_is_deleted_with_its_copies() {
    let mut bench = Bench::new("delete");
    bench.run("new", &["scratch"]).unwrap();
    bench.run("install", &["scratch", "claude", "--global"]).unwrap();
    let copy = bench.home().join(".claude").join("skills").join("scratch");
    assert!(copy.exists());
    bench.run("delete", &["scratch"]).unwrap();
    assert!(!copy.exists());
    assert!(bench.skills.skill("scratch").is_err());
    assert!(bench.run("delete", &["caveman"]).is_err(), "a shipped skill is reset, not deleted");
}

#[test]
fn a_partial_skill_is_fetched_from_the_site_before_it_installs() {
    let mut bench = Bench::new("partial");
    let skill = bench.skills.skill("impeccable").unwrap();
    assert!(skill.partial, "impeccable is too large to ship whole");
    // With no catalogue address the window refuses rather than fetching.
    let said = bench.run("install", &["impeccable", "claude", "--global"]);
    assert!(said.is_err() || said.as_ref().unwrap().contains("Fetching"), "{said:?}");
}

#[test]
fn the_verbs_are_the_cli_tables_and_each_answers() {
    let mut bench = Bench::new("verbs");
    for (verb, _) in unluminous_cli::plugins::SKILLS {
        let words: &[&str] = match *verb {
            "show" | "text" | "edit" | "reset" | "view" => &["caveman"],
            "install" | "remove" => &["caveman", "claude", "--global"],
            "write" => &["unluminous", "---\nname: unluminous\ndescription: x\n---\n"],
            "delete" | "update" | "check" => continue,
            "new" => &["verb-check"],
            "scope" => &["global"],
            "filter" => &["installed"],
            "search" => &["design"],
            _ => &[],
        };
        assert!(bench.run(verb, words).is_ok(), "{verb} answered {:?}", bench.run(verb, words));
    }
    assert!(bench.run("nonsense", &[]).is_err());
}

#[test]
fn a_newer_skill_on_the_site_is_an_update_that_is_fetched_checked_and_taken() {
    use sha2::{Digest, Sha256};
    let mut bench = Bench::new("update");
    // A site with a newer caveman: the catalogue beside `files/caveman/SKILL.md`, as the site lays them out.
    let site = bench.root.join("site");
    // Windows line endings, which the site's checkout may have given the file, so the check has to see past them.
    let newer = "---\r\nname: caveman\r\ndescription: Talk less.\r\n---\r\nNewer words.\r\n";
    std::fs::create_dir_all(site.join("files").join("caveman")).unwrap();
    std::fs::write(site.join("files").join("caveman").join("SKILL.md"), newer).unwrap();
    let mut files = super::catalogue::Files::new();
    files.insert("SKILL.md".into(), newer.as_bytes().to_vec());
    let plain: String = Sha256::digest(newer.replace("\r\n", "\n").as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let catalogue = serde_json::json!({
        "schema": 1, "updated": "2099-01-01",
        "skills": [{ "id": "caveman", "name": "Caveman", "sha256": super::catalogue::hash(&files),
                     "files": [{ "path": "SKILL.md", "sha256": plain, "size": newer.len() }] }]
    });
    std::fs::write(site.join("catalogue.json"), catalogue.to_string()).unwrap();
    bench.skills.catalogue_here = Some(site.join("catalogue.json").display().to_string());
    bench.run("install", &["caveman", "claude", "--global"]).unwrap();

    bench.run("check", &[]).unwrap();
    let mut waited = 0;
    // The catalogue's arrival starts a read on a thread, and the update is known once that read has landed.
    while (bench.skills.remote.is_none() || bench.skills.reading.is_some()) && waited < 200 {
        bench.skills.catch_up();
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
    }
    assert!(
        bench.skills.skill("caveman").unwrap().update,
        "the site's hash differs from what shipped"
    );
    assert_eq!(bench.run("list", &["updates"]).unwrap(), "1 skill");

    bench.run("update", &["caveman"]).unwrap();
    let mut waited = 0;
    while bench.skills.skill("caveman").unwrap().library.is_none() && waited < 200 {
        bench.skills.catch_up();
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
    }
    let skill = bench.skills.skill("caveman").unwrap();
    assert!(!skill.update && !skill.edited, "taken, and the library is what the site has");
    let copy = bench.home().join(".claude").join("skills").join("caveman").join("SKILL.md");
    bench.skills.refresh();
    assert!(read(&copy).contains("Newer words."), "the installed copy follows the update");
}

/// `task-2401`: opening reads the skills folders on a thread, so the pane is drawn at once and says it is looking,
/// and a read that started before a change the person made is thrown away when it lands.
#[test]
fn opening_reads_on_a_thread_and_a_read_older_than_a_change_is_thrown_away() {
    let root =
        std::env::temp_dir().join(format!("unluminous-skills-thread-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for folder in ["home", "project", "plugin"] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
    }
    let mut skills = Skills::new();
    skills.places = Places::at(root.join("home"));
    let context = Context {
        project: Some(root.join("project")),
        folder: Some(root.join("plugin")),
        ..Context::default()
    };
    skills.open(&context).unwrap();
    assert!(!skills.read, "opening returns before the folders are read");
    assert!(skills.shelf.skills.is_empty());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !skills.read && std::time::Instant::now() < deadline {
        skills.catch_up();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(skills.read && !skills.shelf.skills.is_empty(), "the read landed");

    // A read on the thread, then a change: the read that started first must not undo what the change shows.
    skills.read_in_background();
    skills.command("install", &["caveman".into(), "claude".into(), "--global".into()]).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    skills.catch_up();
    assert!(skills.skill("caveman").unwrap().is_installed(), "the install is still shown");
    let _ = std::fs::remove_dir_all(&root);
}
