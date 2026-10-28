//! Claude Code asking the person before it writes a file, against the real `claude` on this machine (`task-2392`).
//!
//! Ignored, because it runs a real agent and costs a real turn: `cargo test -p unluminous-chat --test
//! claude_asks_live -- --ignored --nocapture`. Run it when the approval protocol or Claude Code's output changes.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use unluminous_chat::agent::{run, Ask, Running};
use unluminous_chat::approval::Answers;
use unluminous_chat::{Permission, Provider, Reply, Wire};

/// One turn asking Claude Code to write a file in `--permission-mode manual`, answering its question with
/// `choice`, and every reply it made.
fn turn(folder: &std::path::Path, choice: &'static str) -> Vec<Reply> {
    let provider = Provider::defaults()
        .into_iter()
        .find(|one| one.wire == Wire::ClaudeCli)
        .expect("Claude Code ships");
    let answers = Arc::new(Answers::default());
    let ask = Ask {
        prompt: "Create a file named hello.txt containing the word hi, using the Write tool. Then say done."
            .to_owned(),
        folder: Some(folder.to_path_buf()),
        permission: Permission::Read,
        answers: Some(Arc::clone(&answers)),
        ..Ask::default()
    };
    let replies = Mutex::new(Vec::new());
    let say = |reply: Reply| {
        if let Reply::Asked(question) = &reply {
            answers.give(&question.id, choice);
        }
        replies.lock().unwrap().push(reply);
        true
    };
    run(&provider, &ask, &AtomicBool::new(false), &Running::default(), 0, &say);
    replies.into_inner().unwrap()
}

#[test]
#[ignore = "runs the real Claude Code"]
fn claude_code_asks_before_writing_and_does_what_the_person_chose() {
    let folder = std::env::temp_dir().join(format!("unluminous-asks-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();

    let replies = turn(&folder, "allow");
    assert!(replies
        .iter()
        .any(|reply| matches!(reply, Reply::Asked(question) if question.tool == "Write")));
    assert!(replies
        .iter()
        .any(|reply| matches!(reply, Reply::Answered { said, .. } if said.starts_with("Allowed"))));
    assert!(replies.iter().any(|reply| matches!(reply, Reply::Finished { .. })), "{replies:?}");
    assert_eq!(std::fs::read_to_string(folder.join("hello.txt")).unwrap().trim(), "hi");

    std::fs::remove_file(folder.join("hello.txt")).unwrap();
    let replies = turn(&folder, "deny");
    assert!(replies
        .iter()
        .any(|reply| matches!(reply, Reply::Answered { said, .. } if said.starts_with("Denied"))));
    assert!(replies.iter().any(|reply| matches!(reply, Reply::Finished { .. })), "{replies:?}");
    assert!(!folder.join("hello.txt").exists(), "a denied write is not made");
    let _ = std::fs::remove_dir(&folder);
}
