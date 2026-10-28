//! `task-2285`: the first question after a restart resumes the agent's own session, and a conversation no
//! session of that agent holds is written out once.
//!
//! The scripted tests run a small program that answers as Claude Code does, so they cost nothing and run with
//! the rest. The live tests at the end run the real agents and are ignored; they were run by hand once, for
//! `task-2285`:
//!
//! ```text
//! cargo test -p unluminous-app --lib live_restore -- --ignored --nocapture --test-threads=1
//! ```

use super::*;

fn a_folder(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("unluminous-chat-restore-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a folder");
    folder
}

/// A pane on `folder`, speaking only to `provider`, in `project`.
fn a_pane(folder: &Path, project: &Path, provider: Provider) -> AgentChat {
    let mut chat = AgentChat::new();
    chat.open(&Context { folder: Some(folder.to_path_buf()), ..Context::default() })
        .expect("opened");
    chat.configuration.chosen = provider.name.clone();
    chat.configuration.providers = vec![provider];
    // The scripted agent reads its standard input to the end before it answers, as Claude Code without
    // `--input-format stream-json` does, so these are turns where nobody is asked (`task-2392`).
    chat.configuration.ask = false;
    chat.project = Some(project.to_path_buf());
    chat
}

/// Read replies until the turn ends, or fail after `seconds`.
fn until_the_turn_ends(chat: &mut AgentChat, seconds: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while chat.take_the_replies() {
        assert!(std::time::Instant::now() < deadline, "the turn did not end: {chat:?}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// A program that answers as Claude Code does. It writes its arguments and its standard input to the log it
/// returns, refuses to resume `s-gone` the way Claude Code 2.1.302 does, and otherwise answers `teal` in
/// session `s-new`.
fn a_scripted_claude(folder: &Path) -> (PathBuf, PathBuf) {
    let log = folder.join("asked.log");
    let refusal = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":0,"session_id":"s-gone","usage":{"input_tokens":0,"output_tokens":0},"errors":["No conversation found with session ID: s-gone"]}"#;
    let answer = [
        r#"{"type":"system","subtype":"init","session_id":"s-new","model":"scripted"}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},"session_id":"s-new"}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"teal"}},"session_id":"s-new"}"#,
        r#"{"type":"result","subtype":"success","is_error":false,"session_id":"s-new","usage":{"input_tokens":1,"output_tokens":1}}"#,
    ];
    #[cfg(windows)]
    let (program, text) = {
        let mut text = format!(
            "@echo off\r\necho ARGS %* >> \"{log}\"\r\nmore >> \"{log}\"\r\necho ===== >> \"{log}\"\r\n\
             for %%a in (%*) do if \"%%~a\"==\"s-gone\" goto refuse\r\n",
            log = log.display()
        );
        for line in answer {
            text.push_str(&format!("echo {line}\r\n"));
        }
        text.push_str(&format!("exit /b 0\r\n:refuse\r\necho {refusal}\r\nexit /b 0\r\n"));
        (folder.join("scripted-claude.cmd"), text)
    };
    #[cfg(not(windows))]
    let (program, text) = {
        let mut text = format!(
            "#!/bin/sh\necho \"ARGS $*\" >> '{log}'\ncat >> '{log}'\necho ===== >> '{log}'\n\
             case \"$*\" in *s-gone*) echo '{refusal}'; exit 0;; esac\n",
            log = log.display()
        );
        for line in answer {
            text.push_str(&format!("echo '{line}'\n"));
        }
        (folder.join("scripted-claude"), text)
    };
    std::fs::write(&program, text).expect("the script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
            .expect("runnable");
    }
    (program, log)
}

fn scripted(program: &Path) -> Provider {
    Provider {
        name: "scripted".to_owned(),
        wire: unluminous_chat::Wire::ClaudeCli,
        command: program.display().to_string(),
        ..Provider::defaults().remove(0)
    }
}

/// A conversation as a window that has just restarted finds it on disk.
fn written_down(folder: &Path, session: &str, session_provider: &str) -> String {
    let store = Store::at(Some(folder.to_path_buf()));
    let mut chat = Conversation::new(store.new_id(), "scripted");
    chat.push(Message::said(1, Role::User, "Remember the word teal."));
    chat.push(Message::said(2, Role::Assistant, "Remembered."));
    chat.session = session.to_owned();
    chat.session_provider = session_provider.to_owned();
    store.write(&chat).expect("written");
    chat.id
}

/// The turns the scripted agent was asked, each its arguments and what it read.
fn turns(log: &Path) -> Vec<String> {
    let asked = std::fs::read_to_string(log).expect("the log");
    asked.split("=====").filter(|one| one.contains("ARGS")).map(str::to_owned).collect()
}

#[test]
fn the_first_question_after_a_restart_resumes_the_agents_session_and_sends_only_itself() {
    let folder = a_folder("resume");
    let (program, log) = a_scripted_claude(&folder);
    let id = written_down(&folder, "s-kept", "scripted");
    let mut chat = a_pane(&folder, &folder, scripted(&program));
    chat.open_conversation(&id).expect("reopened");
    chat.draft = "What was the word?".to_owned();
    chat.send().expect("sent");
    until_the_turn_ends(&mut chat, 30);
    let asked = turns(&log);
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(asked[0].contains("--resume s-kept"), "{asked:?}");
    assert!(asked[0].contains("What was the word?"), "{asked:?}");
    assert!(!asked[0].contains("Remember the word teal"), "the agent already has it: {asked:?}");
    assert_eq!(chat.restored, Some("resume"));
    assert_eq!(chat.session.chat.session, "s-new", "the agent's newest id is kept");
    assert_eq!(chat.session.chat.last().expect("an answer").text(), "teal");
    let reread = Store::at(Some(folder.clone())).read(&id).expect("written again");
    assert_eq!(reread.session, "s-new", "and written down for the next restart");
    assert_eq!(reread.session_provider, "scripted");
}

#[test]
fn a_resume_the_agent_refuses_is_asked_again_once_with_the_conversation_written_out() {
    let folder = a_folder("refused");
    let (program, log) = a_scripted_claude(&folder);
    let id = written_down(&folder, "s-gone", "scripted");
    let mut chat = a_pane(&folder, &folder, scripted(&program));
    chat.open_conversation(&id).expect("reopened");
    chat.draft = "What was the word?".to_owned();
    chat.send().expect("sent");
    until_the_turn_ends(&mut chat, 30);
    let asked = turns(&log);
    assert_eq!(asked.len(), 2, "one refusal, one replay: {asked:?}");
    assert!(asked[0].contains("--resume s-gone"), "{asked:?}");
    assert!(!asked[1].contains("--resume"), "{asked:?}");
    assert!(asked[1].contains("<earlier-conversation>"), "{asked:?}");
    assert!(asked[1].contains("Remember the word teal."), "{asked:?}");
    assert_eq!(chat.restored, Some("retried"));
    assert!(chat.problem.is_none(), "the refusal was not shown: {:?}", chat.problem);
    assert_eq!(chat.session.chat.last().expect("an answer").text(), "teal");
    assert_eq!(chat.session.chat.session, "s-new");
}

#[test]
fn a_conversation_from_before_sessions_were_kept_is_replayed_once_and_then_resumed() {
    let folder = a_folder("older");
    let (program, log) = a_scripted_claude(&folder);
    let id = written_down(&folder, "", "");
    let mut chat = a_pane(&folder, &folder, scripted(&program));
    chat.open_conversation(&id).expect("reopened");
    chat.draft = "What was the word?".to_owned();
    chat.send().expect("sent");
    until_the_turn_ends(&mut chat, 30);
    assert_eq!(chat.restored, Some("replay"));
    chat.draft = "Thanks.".to_owned();
    chat.send().expect("sent");
    until_the_turn_ends(&mut chat, 30);
    assert_eq!(chat.restored, Some("resume"));
    let asked = turns(&log);
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert!(asked[0].contains("<earlier-conversation>"), "{asked:?}");
    assert!(!asked[1].contains("<earlier-conversation>"), "replayed once: {asked:?}");
    assert!(asked[1].contains("--resume s-new"), "{asked:?}");
}

#[test]
fn a_session_from_another_agent_is_not_handed_to_this_one() {
    let folder = a_folder("other-agent");
    let (program, log) = a_scripted_claude(&folder);
    let id = written_down(&folder, "codex-thread", "codex");
    let mut chat = a_pane(&folder, &folder, scripted(&program));
    chat.open_conversation(&id).expect("reopened");
    chat.draft = "What was the word?".to_owned();
    chat.send().expect("sent");
    until_the_turn_ends(&mut chat, 30);
    let asked = turns(&log);
    assert!(!asked[0].contains("codex-thread"), "{asked:?}");
    assert!(asked[0].contains("<earlier-conversation>"), "{asked:?}");
    assert_eq!(chat.restored, Some("replay"));
}

/// Tell a real agent a word, restart the pane as a window would, and ask for the word.
fn the_word_survives_a_restart(provider: &str, seconds: u64) {
    let folder = a_folder(&format!("live-{provider}"));
    let project = a_folder(&format!("live-{provider}-project"));
    let word = format!("zephyr{}", std::process::id() % 1000);
    let shipped =
        Provider::defaults().into_iter().find(|one| one.name == provider).expect("shipped");
    let mut chat = a_pane(&folder, &project, shipped.clone());
    chat.new_conversation();
    chat.draft = format!("Remember the word {word}. Reply with OK and nothing else.");
    chat.send().expect("sent");
    until_the_turn_ends(&mut chat, seconds);
    assert!(chat.problem.is_none(), "{:?}", chat.problem);
    assert_eq!(chat.restored, Some("fresh"));
    let id = chat.session.chat.id.clone();
    assert!(!chat.session.chat.session.is_empty(), "the agent gave a session");
    chat.write_the_conversation();
    drop(chat);

    let mut restarted = a_pane(&folder, &project, shipped);
    restarted.open_conversation(&id).expect("reopened");
    restarted.draft =
        "What was the word I asked you to remember? Reply with the word only.".to_owned();
    restarted.send().expect("sent");
    until_the_turn_ends(&mut restarted, seconds);
    let answer = restarted.session.chat.last().expect("an answer").text();
    println!("{provider} answered after the restart: {answer:?}");
    assert_eq!(restarted.restored, Some("resume"), "{:?}", restarted.problem);
    assert!(answer.to_lowercase().contains(&word), "{answer}");
}

#[test]
#[ignore = "runs Claude Code; run by hand"]
fn live_restore_claude() {
    the_word_survives_a_restart("claude", 240);
}

#[test]
#[ignore = "runs Codex; run by hand"]
fn live_restore_codex() {
    the_word_survives_a_restart("codex", 300);
}

#[test]
#[ignore = "runs OpenCode; run by hand"]
fn live_restore_opencode() {
    the_word_survives_a_restart("opencode", 600);
}

#[test]
#[ignore = "runs Claude Code; run by hand"]
fn live_restore_claude_refused_then_replayed() {
    let folder = a_folder("live-refused");
    let project = a_folder("live-refused-project");
    let word = format!("quartz{}", std::process::id() % 1000);
    let store = Store::at(Some(folder.clone()));
    let mut old = Conversation::new(store.new_id(), "claude");
    old.push(Message::said(1, Role::User, format!("Remember the word {word}.")));
    old.push(Message::said(2, Role::Assistant, "OK"));
    old.session = "11111111-2222-3333-4444-555555555555".to_owned();
    old.session_provider = "claude".to_owned();
    store.write(&old).expect("written");
    let mut chat = a_pane(&folder, &project, Provider::defaults().remove(0));
    chat.open_conversation(&old.id).expect("reopened");
    chat.draft = "What was the word I asked you to remember? Reply with the word only.".to_owned();
    chat.send().expect("sent");
    until_the_turn_ends(&mut chat, 240);
    let answer = chat.session.chat.last().expect("an answer").text();
    println!("claude answered after a refused resume: {answer:?}");
    assert_eq!(chat.restored, Some("retried"));
    assert!(answer.to_lowercase().contains(&word), "{answer}");
}
