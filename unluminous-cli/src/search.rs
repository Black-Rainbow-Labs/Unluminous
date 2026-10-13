//! The `search` area: the code index, which is Atrius (`atrius-index` on crates.io).
//!
//! The index, its host process, the lock and host file in the index folder, and the wire protocol all
//! live in the `atrius-index` crate. This module is the seam between it and Unluminous: it tells Atrius
//! that a host Unluminous starts is `unluminous-cli search serve`, names Unluminous in the host file,
//! carries the older `UNLUMINOUS_*` settings over to their `ATRIUS_*` names, and turns Atrius's reply
//! into this crate's `Reply`.
//!
//! Because the lock, the host file and the commands are Atrius's, a checkout has one index whether the
//! first caller was this window, `unluminous-cli`, an agent through `unluminous_search`, or the
//! `atrius` command line and `atrius-mcp`: every later caller asks whichever process hosts it.
//!
//! **Who hosts depends on how long the caller lives.** A window and an MCP server host in their own
//! process ([`Hosting::InProcess`]). A one off `unluminous-cli search ...` starts a detached
//! `unluminous-cli search serve` instead ([`Hosting::Spawn`]) and asks it. When no host can be had,
//! `search find` is answered by scanning the files directly and says `"index":"none"`.

use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use serde_json::{Map, Value};

pub use atrius_index::host::Hosting;

use crate::protocol::Reply;

/// How long a headless host waits with no request before it stops.
pub const IDLE_EXIT: Duration = atrius_index::host::IDLE_EXIT;

/// Configures Atrius once per process: which program a spawned host is, what this process is called in
/// the host file, and the settings Unluminous used to read under its own names.
pub fn configure() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // `UNLUMINOUS_INDEX_CACHE` and `UNLUMINOUS_EMBED` are what the evaluation harness, the tests
        // and people's own shells set. Atrius reads the `ATRIUS_` names, so each older name is copied
        // across when the newer one is not set. This runs before any index thread exists.
        for (old, new) in [
            ("UNLUMINOUS_INDEX_CACHE", "ATRIUS_INDEX_CACHE"),
            ("UNLUMINOUS_EMBED", "ATRIUS_EMBED"),
            ("UNLUMINOUS_EMBED_BATCH", "ATRIUS_EMBED_BATCH"),
            ("UNLUMINOUS_SEARCH_ABSTAIN", "ATRIUS_SEARCH_ABSTAIN"),
            ("UNLUMINOUS_SEARCH_CONFIDENCE", "ATRIUS_SEARCH_CONFIDENCE"),
            ("UNLUMINOUS_PASSAGE_FUSION", "ATRIUS_PASSAGE_FUSION"),
        ] {
            if std::env::var_os(new).is_none() {
                if let Some(value) = std::env::var_os(old) {
                    std::env::set_var(new, value);
                }
            }
        }
        keep_the_model_briefly();
        embed_on_few_threads();
        if let Some(cli) = cli_program() {
            atrius_index::host::set_server_command(cli, vec!["search".into(), "serve".into()]);
        }
        atrius_index::host::set_program_name(&format!(
            "unluminous-cli {}",
            env!("CARGO_PKG_VERSION")
        ));
    });
}

/// How long an index host in this process keeps the embedding model once nothing is asking for it.
pub const MODEL_IDLE: &str = "idle:60s";

/// Releases the embedding model a minute after its last use, unless somebody has chosen otherwise.
///
/// The model is ONNX Runtime and `nomic-embed-text`, and while it is loaded it is about 1 GB of the
/// process: measured on 0.76.0, a window on a folder with one file in it was 1,208 MB of working set
/// with the model and 175 MB without it (`task-2247`). Inillucent's own default keeps it for five
/// minutes after the last embedding or question, which is right for a database a person queries and
/// wrong for an editor that embeds a few changed passages and then sits open all day. A minute still
/// covers a burst of questions. `INILLUCENT_EMBED_RESIDENCY`, or a residency `inillucent
/// setup-embeddings` recorded, wins over this.
fn keep_the_model_briefly() {
    const VARIABLE: &str = "INILLUCENT_EMBED_RESIDENCY";
    if std::env::var_os(VARIABLE).is_some() || was_recorded("residency") {
        return;
    }
    std::env::set_var(VARIABLE, MODEL_IDLE);
}

/// How many processor threads the embedding model runs on in a process that did not say.
///
/// `task-2255`: *"Unluminous now eats up a lot of CPU."* ONNX Runtime gives a session one thread for each
/// logical processor unless it is told otherwise, so a window embedding its project's passages ran 23
/// `session-1-intra-op` threads and was measured at 15 to 18 processors for the first minutes after it
/// opened, with every window of a restored session doing the same at once. Embedding is background work
/// that nobody is waiting on, so it gets two threads; a search still finds lines and names at once, and a
/// plain English question is answered from its words until the vectors are there.
pub const EMBED_THREADS: &str = "2";

/// Runs the embedding model on [`EMBED_THREADS`] threads, unless `INILLUCENT_EMBED_THREADS` or a thread count
/// `inillucent setup-embeddings --threads` recorded says otherwise.
fn embed_on_few_threads() {
    const VARIABLE: &str = "INILLUCENT_EMBED_THREADS";
    if std::env::var_os(VARIABLE).is_none() && !was_recorded("threads") {
        std::env::set_var(VARIABLE, EMBED_THREADS);
    }
}

/// Whether `inillucent setup-embeddings` wrote `key` (`residency` or `threads`) into its install state, read
/// from where Inillucent's `install::home` keeps it.
///
/// @param key - the setting's name in `embeddings.json`
fn was_recorded(key: &str) -> bool {
    let named = std::env::var_os("INILLUCENT_HOME").filter(|home| !home.is_empty());
    let home = if let Some(named) = named {
        Some(PathBuf::from(named))
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("inillucent"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/inillucent"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .map(|data| data.join("inillucent"))
    };
    home.and_then(|home| std::fs::read(home.join("embeddings.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|state| state.get(key).is_some_and(|value| !value.is_null()))
}

/// The `unluminous-cli` program: this one when it is the CLI under any name that starts with its own
/// (a copy such as `unluminous-cli-acf5c46.exe` is how the evaluation runs it), and the CLI beside it
/// when this is the window or a test.
fn cli_program() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let is_cli = exe.file_stem().is_some_and(|s| s.to_string_lossy().starts_with("unluminous-cli"));
    Some(if is_cli {
        exe
    } else {
        exe.with_file_name(if cfg!(windows) { "unluminous-cli.exe" } else { "unluminous-cli" })
    })
}

/// The project a search is about: the given folder, or the nearest folder at or above the working folder
/// that holds `.git`, or the working folder itself.
///
/// @param given - a `--root` argument, if one was given
pub fn project_root(given: Option<&str>) -> PathBuf {
    atrius_index::host::project_root(given)
}

/// Turns Atrius's reply into this crate's.
///
/// @param reply - what the index host answered
fn converted(reply: atrius_index::protocol::Reply) -> Reply {
    if reply.ok {
        Reply::done(&reply.command, "", reply.result)
    } else {
        Reply::failed(&reply.command, &reply.code, reply.message)
    }
}

/// Answers a `search` command for a root: through this process's host, another process's host, a host
/// started for the purpose, or, when none of those can be had, a direct scan.
///
/// @param root - the root
/// @param command - the wire name, such as `search.find`
/// @param arguments - the arguments
/// @param timeout - how long to wait for another process
/// @param hosting - whether this process may host
pub fn ask(
    root: &Path,
    command: &str,
    arguments: &Map<String, Value>,
    timeout: Duration,
    hosting: Hosting,
) -> Reply {
    configure();
    converted(atrius_index::host::ask(root, command, arguments, timeout, hosting))
}

/// Runs the headless host for `search serve` and does not return until it has been idle for
/// `IDLE_EXIT`, or `ATRIUS_IDLE_MINUTES`. Exit code 0, or 1 when another process is already the host.
///
/// @param root - the root
pub fn serve(root: &Path) -> i32 {
    configure();
    match atrius_index::host::serve(root, atrius_index::host::idle_limit()) {
        Ok(()) => 0,
        Err(problem) => {
            eprintln!("unluminous-cli search serve: {problem}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inillucent reads `INILLUCENT_EMBED_RESIDENCY` as `resident`, `on-demand`, `idle` or
    /// `idle:<duration>`, and ignores anything else with one line on standard error. `60s` alone was
    /// ignored that way, so the model stayed for Inillucent's five minutes (`task-2247`).
    #[test]
    fn the_model_is_kept_for_a_minute_in_the_form_inillucent_reads() {
        let after = MODEL_IDLE.strip_prefix("idle:").expect("written as idle:<duration>");
        let seconds: u64 =
            after.strip_suffix('s').and_then(|n| n.parse().ok()).expect("a number of seconds");
        assert_eq!(seconds, 60);
    }
}
