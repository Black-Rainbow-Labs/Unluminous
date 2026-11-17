//! `task-2737` (GitHub issue 4): Agent Chat's `full` permission on a machine whose Claude Code settings refuse
//! `bypassPermissions`. Claude Code then runs as `default` with no error, which its `init` line says. The scripted
//! agent here writes that line, so the test costs nothing and runs with the rest.

use super::*;

fn a_folder(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("unluminous-chat-bypass-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a folder");
    folder
}

/// A program that answers as Claude Code 2.1.326 does when a settings file sets
/// `permissions.disableBypassPermissionsMode`: whatever mode it is asked for, its `init` line says `default`. It
/// writes its arguments to the log it returns.
fn a_claude_that_refuses_bypass(folder: &Path) -> (PathBuf, PathBuf) {
    let log = folder.join("asked.log");
    let answer = [
        r#"{"type":"system","subtype":"init","session_id":"s-1","model":"scripted","permissionMode":"default"}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},"session_id":"s-1"}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ok"}},"session_id":"s-1"}"#,
        r#"{"type":"result","subtype":"success","is_error":false,"session_id":"s-1","usage":{"input_tokens":1,"output_tokens":1}}"#,
    ];
    #[cfg(windows)]
    let (program, text) = {
        let mut text = format!(
            "@echo off\r\necho ARGS %* >> \"{log}\"\r\nmore > nul\r\necho ===== >> \"{log}\"\r\n",
            log = log.display()
        );
        for line in answer {
            text.push_str(&format!("echo {line}\r\n"));
        }
        text.push_str("exit /b 0\r\n");
        (folder.join("scripted-claude.cmd"), text)
    };
    #[cfg(not(windows))]
    let (program, text) = {
        let mut text = format!(
            "#!/bin/sh\necho \"ARGS $*\" >> '{log}'\ncat > /dev/null\necho ===== >> '{log}'\n",
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

fn until_the_turn_ends(chat: &mut AgentChat, seconds: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while chat.take_the_replies() {
        assert!(std::time::Instant::now() < deadline, "the turn did not end: {chat:?}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The mode each turn's command line asked for.
fn modes_asked(log: &Path) -> Vec<String> {
    let asked = std::fs::read_to_string(log).expect("the log");
    asked
        .split("=====")
        .filter(|one| one.contains("ARGS"))
        .map(|one| {
            let words: Vec<&str> = one.split_whitespace().collect();
            let at = words.iter().position(|word| *word == "--permission-mode").expect("a mode");
            words[at + 1].to_owned()
        })
        .collect()
}

#[test]
fn a_refused_bypass_is_said_once_and_later_turns_ask_for_accept_edits() {
    let folder = a_folder("refused");
    let (program, log) = a_claude_that_refuses_bypass(&folder);
    let mut chat = AgentChat::new();
    chat.open(&Context { folder: Some(folder.clone()), ..Context::default() }).expect("opened");
    let provider = Provider {
        name: "scripted".to_owned(),
        wire: unluminous_chat::Wire::ClaudeCli,
        command: program.display().to_string(),
        ..Provider::defaults().remove(0)
    };
    chat.configuration.chosen = provider.name.clone();
    chat.configuration.providers = vec![provider];
    chat.configuration.permission = unluminous_chat::Permission::Full;
    // The scripted agent reads its standard input to the end, as Claude Code without `--input-format stream-json`.
    chat.configuration.ask = false;
    chat.project = Some(folder.clone());

    chat.draft = "Edit the file.".to_owned();
    chat.send().expect("sent");
    until_the_turn_ends(&mut chat, 30);
    let notices: Vec<String> = chat
        .asking()
        .into_iter()
        .filter_map(|request| match request {
            Request::Notice { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        notices[0].contains("bypassPermissions") && notices[0].contains("default"),
        "{notices:?}"
    );
    assert!(notices[0].contains("disableBypassPermissionsMode"), "{notices:?}");
    assert!(notices[0].contains("acceptEdits"), "{notices:?}");
    assert!(notices[0].contains("is refused"), "asking is off, so it says so: {notices:?}");

    chat.draft = "And again.".to_owned();
    chat.send().expect("sent");
    until_the_turn_ends(&mut chat, 30);
    assert_eq!(modes_asked(&log), vec!["bypassPermissions", "acceptEdits"]);
    let notices = chat.asking().into_iter().filter(|r| matches!(r, Request::Notice { .. })).count();
    assert_eq!(notices, 1, "acceptEdits was given, so nothing more is said");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_row_that_names_its_own_mode_is_left_alone() {
    let mut chat = AgentChat::new();
    chat.bypass_refused.push(Some(PathBuf::from("/a/project")));
    let ask = unluminous_chat::Ask {
        prompt: String::new(),
        folder: Some(PathBuf::from("/a/project")),
        session: String::new(),
        pictures: Vec::new(),
        permission: unluminous_chat::Permission::Full,
        environment: AgentChat::the_environment(),
        answers: None,
    };
    let claude =
        Provider { wire: unluminous_chat::Wire::ClaudeCli, ..Provider::defaults().remove(0) };
    assert_eq!(chat.instead_of_a_refused_bypass(&claude, &ask).mode, "acceptEdits");
    let planned = Provider { mode: "plan".to_owned(), ..claude.clone() };
    assert_eq!(chat.instead_of_a_refused_bypass(&planned, &ask).mode, "plan");
    let elsewhere = unluminous_chat::Ask { folder: Some(PathBuf::from("/b")), ..ask };
    assert_eq!(chat.instead_of_a_refused_bypass(&claude, &elsewhere).mode, "");
}
