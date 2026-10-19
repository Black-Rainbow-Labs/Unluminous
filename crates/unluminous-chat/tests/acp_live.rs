//! A real ACP agent, driven through `acp::run` the way the chat pane drives it (`task-2308`).
//!
//! Ignored, because it needs `npx`, the network the first time `npx` fetches the package, and a signed in
//! Claude account. Run it when the ACP transport or the adapter changes:
//!
//! `cargo test -p unluminous-chat --test acp_live -- --ignored --nocapture`

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use unluminous_chat::acp_registry::{provider_for, Distribution};
use unluminous_chat::agent::{Ask, Permission, Running};
use unluminous_chat::{Environment, Reply};

/// Claude's adapter at the version this was measured against.
fn claude_adapter() -> unluminous_chat::Provider {
    let npx = Distribution::Npx {
        package: "@agentclientprotocol/claude-agent-acp@0.94.0".to_owned(),
        args: Vec::new(),
        env: Vec::new(),
    };
    provider_for("claude-acp", &npx, None)
}

/// One turn, and every reply it made.
fn turn(ask: &Ask) -> Vec<Reply> {
    let replies = Mutex::new(Vec::new());
    let started = std::time::Instant::now();
    unluminous_chat::acp::run(
        &claude_adapter(),
        ask,
        &AtomicBool::new(false),
        &Running::default(),
        1,
        &|reply| {
            if replies.lock().map(|all| all.is_empty()).unwrap_or(false) {
                eprintln!("first reply after {} ms", started.elapsed().as_millis());
            }
            eprintln!("{reply:?}");
            replies.lock().expect("lock").push(reply);
            true
        },
    );
    eprintln!("turn took {} ms", started.elapsed().as_millis());
    replies.into_inner().expect("lock")
}

#[test]
#[ignore]
fn claudes_adapter_answers_a_turn_and_carries_the_session_on_in_a_second() {
    let folder = std::env::temp_dir().join(format!("unluminous-acp-live-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("made");
    std::fs::write(folder.join("colour.txt"), "teal\n").expect("written");
    let mut ask = Ask {
        prompt: "Read colour.txt and answer with only the word in it.".to_owned(),
        folder: Some(folder.clone()),
        permission: Permission::Full,
        environment: Environment::of_this_process(),
        ..Ask::default()
    };
    let first = turn(&ask);
    let session = first
        .iter()
        .find_map(|reply| match reply {
            Reply::Session(id) => Some(id.clone()),
            _ => None,
        })
        .expect("a session");
    assert!(first.iter().any(|reply| matches!(reply, Reply::ToolCall { .. })), "{first:?}");
    let said: String = first
        .iter()
        .filter_map(|reply| if let Reply::Text(said) = reply { Some(said.as_str()) } else { None })
        .collect();
    assert!(said.to_lowercase().contains("teal"), "{said}");
    assert!(matches!(first.last(), Some(Reply::Context { .. })), "{first:?}");

    // The second turn starts a new process and carries the session on with `session/resume`.
    ask.prompt = "What was the word you just read? Answer with only the word.".to_owned();
    ask.session = session.clone();
    let second = turn(&ask);
    assert!(second.contains(&Reply::Session(session)));
    let said: String = second
        .iter()
        .filter_map(|reply| if let Reply::Text(said) = reply { Some(said.as_str()) } else { None })
        .collect();
    assert!(said.to_lowercase().contains("teal"), "{said}");
    let _ = std::fs::remove_dir_all(&folder);
}
