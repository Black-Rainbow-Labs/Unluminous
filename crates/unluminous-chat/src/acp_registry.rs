//! The ACP registry: the published list of agents that speak the Agent Client Protocol, and how each one is run
//! on this machine (`task-2308`).
//!
//! `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json` is `{ version, agents, extensions }`.
//! Each agent names one or more ways it is distributed: `npx` and `uvx` name a package that a package runner
//! starts, and `binary` names an archive for each platform with the program inside it. `tasks/task-2308-acp-tdd.md`
//! §2 has the shape and §4.6 what Unluminous does with it.
//!
//! **Nothing is fetched until somebody asks**, which is the rule the whole of this crate keeps: [`fetch`] is
//! called when `Load registry` is pressed or `settings acp-registry` is run, and an archive is downloaded when
//! `Add` is pressed for a binary agent.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::provider::{Provider, Wire, DEFAULT_MAX_TOKENS};

/// Where the registry is published.
pub const ADDRESS: &str = "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

/// The largest registry read, so a server that answers with something else entirely cannot fill the memory.
const LARGEST_REGISTRY: u64 = 8 * 1024 * 1024;

/// The largest archive downloaded for one agent.
const LARGEST_ARCHIVE: u64 = 1024 * 1024 * 1024;

/// One agent in the registry.
#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub repository: String,
    pub distributions: Vec<Distribution>,
}

/// One way an agent is distributed.
#[derive(Debug, Clone, PartialEq)]
pub enum Distribution {
    /// A package `npx` starts: `npx -y <package> <args>`.
    Npx { package: String, args: Vec<String>, env: Vec<(String, String)> },
    /// A package `uvx` starts: `uvx <package> <args>`.
    Uvx { package: String, args: Vec<String>, env: Vec<(String, String)> },
    /// An archive for one platform, with the program inside it.
    Binary {
        platform: String,
        archive: String,
        cmd: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        sha256: String,
    },
}

impl Distribution {
    /// What the registry calls this kind of distribution.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Npx { .. } => "npx",
            Self::Uvx { .. } => "uvx",
            Self::Binary { .. } => "binary",
        }
    }
}

/// The registry's name for this machine's platform, such as `windows-x86_64` or `darwin-aarch64`.
pub fn platform() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

/// The agents in a registry's text. An agent this version cannot read is left out rather than failing the list.
pub fn parse(text: &str) -> Result<Vec<Agent>, String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|problem| format!("The registry is not JSON: {problem}"))?;
    let Some(agents) = value["agents"].as_array() else {
        return Err("The registry has no list of agents.".to_owned());
    };
    Ok(agents.iter().filter_map(agent_of).collect())
}

fn agent_of(value: &Value) -> Option<Agent> {
    let text = |field: &str| value[field].as_str().unwrap_or_default().trim().to_owned();
    let id = text("id");
    if id.is_empty() {
        return None;
    }
    let mut distributions = Vec::new();
    let given = &value["distribution"];
    for (kind, runner) in [("npx", true), ("uvx", false)] {
        let entry = &given[kind];
        if let Some(package) = entry["package"].as_str().filter(|one| !one.trim().is_empty()) {
            let (package, args, env) =
                (package.trim().to_owned(), words(&entry["args"]), variables(&entry["env"]));
            distributions.push(match runner {
                true => Distribution::Npx { package, args, env },
                false => Distribution::Uvx { package, args, env },
            });
        }
    }
    for (platform, entry) in given["binary"].as_object().into_iter().flatten() {
        let (Some(archive), Some(cmd)) = (entry["archive"].as_str(), entry["cmd"].as_str()) else {
            continue;
        };
        distributions.push(Distribution::Binary {
            platform: platform.clone(),
            archive: archive.to_owned(),
            cmd: cmd.to_owned(),
            args: words(&entry["args"]),
            env: variables(&entry["env"]),
            sha256: entry["sha256"].as_str().unwrap_or_default().to_ascii_lowercase(),
        });
    }
    Some(Agent {
        name: Some(text("name")).filter(|name| !name.is_empty()).unwrap_or_else(|| id.clone()),
        id,
        version: text("version"),
        description: text("description"),
        repository: text("repository"),
        distributions,
    })
}

fn words(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|word| word.as_str().map(str::to_owned))
        .collect()
}

fn variables(value: &Value) -> Vec<(String, String)> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, value)| {
            (name.clone(), value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string()))
        })
        .collect()
}

/// How an agent runs on `platform`, given which package runners `has` says this machine has.
///
/// **A package runner that is installed first**, because it needs no download, then the archive for this
/// platform, then a package runner that is not installed, so the row says what to install rather than nothing.
/// `None` is an agent with no way to run here.
pub fn launch_for<'a>(
    agent: &'a Agent,
    platform: &str,
    has: &dyn Fn(&str) -> bool,
) -> Option<&'a Distribution> {
    let runner = |installed: bool| {
        agent.distributions.iter().find(|one| match one {
            Distribution::Npx { .. } => has("npx") == installed,
            Distribution::Uvx { .. } => has("uvx") == installed,
            Distribution::Binary { .. } => false,
        })
    };
    let binary = agent
        .distributions
        .iter()
        .find(|one| matches!(one, Distribution::Binary { platform: here, .. } if here == platform));
    runner(true).or(binary).or_else(|| runner(false))
}

/// The endpoint for an agent run through `distribution`. `program` is the installed program for a binary, and
/// unused for a package runner.
pub fn provider_for(name: &str, distribution: &Distribution, program: Option<&Path>) -> Provider {
    let (command, args, env) = match distribution {
        Distribution::Npx { package, args, env } => {
            let mut all = vec!["-y".to_owned(), package.clone()];
            all.extend(args.iter().cloned());
            ("npx".to_owned(), all, env.clone())
        }
        Distribution::Uvx { package, args, env } => {
            let mut all = vec![package.clone()];
            all.extend(args.iter().cloned());
            ("uvx".to_owned(), all, env.clone())
        }
        Distribution::Binary { cmd, args, env, .. } => (
            program.map(|path| path.display().to_string()).unwrap_or_else(|| cmd.clone()),
            args.clone(),
            env.clone(),
        ),
    };
    Provider {
        name: name.to_owned(),
        wire: Wire::Acp,
        command,
        url: String::new(),
        model: String::new(),
        key_env: String::new(),
        key_entry: String::new(),
        max_tokens: DEFAULT_MAX_TOKENS,
        args,
        env,
        mode: String::new(),
    }
}

/// A `ureq` agent for the registry and its archives, with the machine's own certificates.
fn web() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .tls_config(crate::client::tls_config())
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .timeout_recv_body(Some(std::time::Duration::from_secs(60)))
        .build();
    ureq::Agent::new_with_config(config)
}

/// The registry's text, read from [`ADDRESS`] now.
///
/// `UNLUMINOUS_ACP_REGISTRY` names another address, or a file, which is how a test reads the registry it keeps
/// in `tests/streams/acp-registry.json` with nothing fetched.
pub fn fetch() -> Result<String, String> {
    let address = std::env::var("UNLUMINOUS_ACP_REGISTRY").unwrap_or_else(|_| ADDRESS.to_owned());
    if !address.starts_with("http://") && !address.starts_with("https://") {
        return std::fs::read_to_string(&address).map_err(|problem| {
            format!("The ACP registry could not be read from {address}: {problem}")
        });
    }
    let answer = web().get(&address).call().map_err(|problem| {
        format!("The ACP registry could not be read from {address}: {problem}")
    })?;
    let mut text = String::new();
    answer
        .into_body()
        .into_reader()
        .take(LARGEST_REGISTRY)
        .read_to_string(&mut text)
        .map_err(|problem| format!("The ACP registry stopped arriving: {problem}"))?;
    Ok(text)
}

/// Where a binary agent of this version is kept, under the settings folder.
pub fn install_folder(settings: &Path, id: &str, version: &str) -> PathBuf {
    let safe = |text: &str| -> String {
        text.chars()
            .map(|one| match one.is_ascii_alphanumeric() || matches!(one, '.' | '-' | '_') {
                true => one,
                false => '-',
            })
            .collect()
    };
    settings.join("acp-agents").join(safe(id)).join(safe(version))
}

/// The program inside an unpacked archive, from the registry's `cmd`, which is relative and may use either
/// separator: `./goose`, `./goose-package\goose.exe`.
pub fn program_in(folder: &Path, cmd: &str) -> PathBuf {
    let mut path = folder.to_path_buf();
    for part in cmd.split(['/', '\\']).filter(|part| !part.is_empty() && *part != ".") {
        path.push(part);
    }
    path
}

/// The npm package to pin an npx agent to: the one the registry names when this machine's npm can fetch it, and
/// otherwise the newest version npm here has (`task-2313`).
///
/// A Mac whose npm reads a company mirror answered `ETARGET` for the registry's
/// `@agentclientprotocol/claude-agent-acp@0.94.0`, and every turn then failed once, ran again unpinned and said so
/// in the conversation. Asking npm once when the agent is added is where that belongs. `None` when npm cannot be
/// asked at all (it is not installed, or it failed for another reason), which keeps the registry's pin: a turn
/// still copes with a missing version by itself, see [`crate::acp::run`].
pub fn npm_version_to_pin(package: &str, environment: &crate::Environment) -> Option<String> {
    let at = package.rfind('@').filter(|at| *at > 0)?;
    let (name, version) = (&package[..at], &package[at + 1..]);
    if version.is_empty() {
        return None;
    }
    let npm = crate::provider::program("npm", environment)?;
    let ask = |what: &str| -> Option<(bool, String)> {
        let mut command = std::process::Command::new(&npm);
        command.args(["view", what, "version"]).stdin(std::process::Stdio::null());
        if environment.is_whole() {
            command.env_clear();
        }
        command.envs(environment.variables().iter().map(|(name, value)| (name, value)));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let out = command.output().ok()?;
        let words = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        Some((
            out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().is_empty(),
            words,
        ))
    };
    let (found, said) = ask(package)?;
    if found {
        return Some(package.to_owned());
    }
    if !crate::acp::a_version_was_not_found(&said) && !said.contains("E404") {
        return None;
    }
    let (found, newest) = ask(name)?;
    let newest = newest.trim().lines().last().unwrap_or_default().trim().trim_matches(['\'', '"']);
    (found && !newest.is_empty()).then(|| format!("{name}@{newest}"))
}

/// Download, check and unpack a binary agent into `folder`, and answer the program to run.
///
/// A folder that already holds an unpacked copy is used as it is. A download whose SHA-256 is not the one the
/// registry gives is deleted before anything is unpacked from it.
pub fn install(distribution: &Distribution, folder: &Path) -> Result<PathBuf, String> {
    let Distribution::Binary { archive, cmd, sha256, .. } = distribution else {
        return Err("Only a binary agent is installed by Unluminous; npx and uvx fetch their own."
            .to_owned());
    };
    let program = program_in(folder, cmd);
    let marker = folder.join(".unluminous-installed");
    if marker.is_file() && program.is_file() {
        return Ok(program);
    }
    std::fs::create_dir_all(folder)
        .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
    let name =
        archive.rsplit('/').next().filter(|name| !name.is_empty()).unwrap_or("agent-archive");
    let downloaded = folder.join(name);
    download(archive, &downloaded)?;
    if !sha256.is_empty() {
        let found = sha256_of(&downloaded)?;
        if found != *sha256 {
            let _ = std::fs::remove_file(&downloaded);
            return Err(format!(
                "{archive} did not match the registry's SHA-256 ({sha256}, and the download was {found}), so it was deleted."
            ));
        }
    }
    unpack(&downloaded, folder)?;
    let _ = std::fs::remove_file(&downloaded);
    if !program.is_file() {
        return Err(format!("{archive} was unpacked and holds no {cmd}."));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::fs::write(&marker, archive.as_bytes());
    Ok(program)
}

fn download(address: &str, to: &Path) -> Result<(), String> {
    let answer = web()
        .get(address)
        .call()
        .map_err(|problem| format!("{address} could not be downloaded: {problem}"))?;
    let mut file = std::fs::File::create(to)
        .map_err(|problem| format!("{} could not be written: {problem}", to.display()))?;
    std::io::copy(&mut answer.into_body().into_reader().take(LARGEST_ARCHIVE), &mut file)
        .map_err(|problem| format!("{address} stopped arriving: {problem}"))?;
    Ok(())
}

/// The SHA-256 of a file, in lower case hexadecimal.
pub fn sha256_of(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)
        .map_err(|problem| format!("{} could not be read: {problem}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|problem| format!("{} could not be read: {problem}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Unpack an archive with `tar`, which reads a `.zip`, a `.tar.gz` and a `.tar.bz2`.
///
/// **On Windows it is `%SystemRoot%\System32\tar.exe` by its full path**, because a machine with Git for Windows
/// ahead of it on `PATH` has a GNU `tar` that reads no zip. That is `JS_DEBUG_FETCH`'s finding in
/// `services::debuggers`, kept here.
fn unpack(archive: &Path, into: &Path) -> Result<(), String> {
    let tar = match cfg!(windows) {
        true => std::env::var_os("SystemRoot")
            .map(|root| PathBuf::from(root).join("System32").join("tar.exe"))
            .unwrap_or_else(|| PathBuf::from("tar.exe")),
        false => PathBuf::from("tar"),
    };
    let mut command = std::process::Command::new(&tar);
    command.arg("-xf").arg(archive).arg("-C").arg(into);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|problem| format!("{} could not be started: {problem}", tar.display()))?;
    match output.status.success() {
        true => Ok(()),
        false => Err(format!(
            "{} could not be unpacked: {}",
            archive.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn the_registry() -> Vec<Agent> {
        let text = include_str!("../tests/streams/acp-registry.json");
        parse(text).expect("the registry of 2026-10-19 reads")
    }

    /// Asks the machine's real npm, so it is ignored: `cargo test -p unluminous-chat --lib npm_ -- --ignored`.
    #[test]
    #[ignore]
    fn npm_is_asked_which_version_of_a_package_it_can_fetch() {
        let here = crate::Environment::of_this_process();
        let package = "@agentclientprotocol/claude-agent-acp@0.94.0";
        assert_eq!(
            npm_version_to_pin(package, &here).as_deref(),
            Some(package),
            "a version npm has"
        );
        let newest = npm_version_to_pin("@agentclientprotocol/claude-agent-acp@99.0.0", &here)
            .expect("the newest version npm has, for one it has not");
        assert!(newest.starts_with("@agentclientprotocol/claude-agent-acp@"), "{newest}");
        assert_ne!(newest, "@agentclientprotocol/claude-agent-acp@99.0.0");
        assert!(
            npm_version_to_pin("@agentclientprotocol/claude-agent-acp", &here).is_none(),
            "nothing pinned"
        );
    }

    #[test]
    fn the_published_registry_reads_every_agent() {
        let agents = the_registry();
        assert_eq!(agents.len(), 42);
        let claude =
            agents.iter().find(|agent| agent.id == "claude-acp").expect("Claude's adapter");
        assert_eq!(claude.name, "Claude Agent");
        assert_eq!(
            claude.distributions,
            vec![Distribution::Npx {
                package: "@agentclientprotocol/claude-agent-acp@0.94.0".to_owned(),
                args: Vec::new(),
                env: Vec::new(),
            }]
        );
        let goose = agents.iter().find(|agent| agent.id == "goose").expect("goose");
        assert_eq!(goose.distributions.len(), 5, "one archive a platform");
        assert!(agents.iter().all(|agent| !agent.distributions.is_empty()));
    }

    #[test]
    fn an_npx_agent_becomes_an_acp_row_that_runs_npx() {
        let agents = the_registry();
        let auggie = agents.iter().find(|agent| agent.id == "auggie").expect("auggie");
        let chosen = launch_for(auggie, "windows-x86_64", &|_| true).expect("runs here");
        let row = provider_for("auggie", chosen, None);
        assert_eq!(row.wire, Wire::Acp);
        assert_eq!(row.command, "npx");
        assert_eq!(row.args, ["-y", "@augmentcode/auggie@0.36.0", "--acp"]);
        assert_eq!(row.env, [("AUGMENT_DISABLE_AUTO_UPDATE".to_owned(), "1".to_owned())]);
        assert!(row.is_a_program() && !row.wants_a_key());

        let fast = agents.iter().find(|agent| agent.id == "fast-agent").expect("fast-agent");
        let row = provider_for(
            "fast-agent",
            launch_for(fast, "linux-x86_64", &|_| true).expect("runs"),
            None,
        );
        assert_eq!(row.command, "uvx");
        assert_eq!(row.args, ["fast-agent-acp==0.10.1", "-x"]);
    }

    #[test]
    fn a_binary_agent_is_the_archive_for_this_platform_and_none_without_one() {
        let agents = the_registry();
        let goose = agents.iter().find(|agent| agent.id == "goose").expect("goose");
        match launch_for(goose, "windows-x86_64", &|_| false) {
            Some(Distribution::Binary { cmd, args, sha256, .. }) => {
                assert_eq!(cmd, "./goose-package\\goose.exe");
                assert_eq!(args, &["acp"]);
                assert_eq!(sha256.len(), 64);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(launch_for(goose, "plan9-mips", &|_| true), None);
        let folder = Path::new("agents");
        assert_eq!(
            program_in(folder, "./goose-package\\goose.exe"),
            folder.join("goose-package").join("goose.exe")
        );
        assert_eq!(program_in(folder, "./goose"), folder.join("goose"));

        // An agent with both: the package runner when it is installed, the archive when it is not.
        let kilo = agents.iter().find(|agent| agent.id == "kilo").expect("kilo");
        assert_eq!(
            launch_for(kilo, "windows-x86_64", &|_| true).map(Distribution::kind),
            Some("npx")
        );
        assert_eq!(
            launch_for(kilo, "windows-x86_64", &|_| false).map(Distribution::kind),
            Some("binary")
        );
    }

    #[test]
    fn a_download_that_does_not_match_its_sha256_is_deleted_and_never_unpacked() {
        let folder =
            std::env::temp_dir().join(format!("unluminous-acp-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("made");
        let file = folder.join("probe.bin");
        std::fs::write(&file, b"hi").expect("written");
        assert_eq!(
            sha256_of(&file).expect("read"),
            "8f434346648f6b96df89dda901c5176b10a6d83961dd3c1ac88b59b2dc327aa4"
        );
        // An archive on loopback that nothing serves: the refusal names the address rather than panicking.
        let wrong = Distribution::Binary {
            platform: platform(),
            archive: "http://127.0.0.1:9/agent.zip".to_owned(),
            cmd: "./agent".to_owned(),
            args: Vec::new(),
            env: Vec::new(),
            sha256: "00".repeat(32),
        };
        let refused = install(&wrong, &folder.join("agent")).expect_err("nothing to download");
        assert!(refused.contains("127.0.0.1:9"), "{refused}");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn this_machine_has_a_platform_name_the_registry_uses() {
        let name = platform();
        assert!(
            [
                "windows-x86_64",
                "windows-aarch64",
                "darwin-aarch64",
                "darwin-x86_64",
                "linux-x86_64",
                "linux-aarch64"
            ]
            .contains(&name.as_str()),
            "{name}"
        );
    }
}
