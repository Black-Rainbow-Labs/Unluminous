//! Read a Claude Code transcript the way the ticket modal's Chat tab does, and say what came of it.
//!
//! `cargo run -p unluminous-chat --example read_transcript -- <file.jsonl>`. `task-2261`: the quickest way to
//! check a real transcript against `unluminous_chat::transcript` after Claude Code changes what it writes.

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: read_transcript <file.jsonl>");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|problem| {
        eprintln!("{path}: {problem}");
        std::process::exit(1);
    });
    let started = std::time::Instant::now();
    let mut chat = unluminous_chat::Conversation::new("transcript", "claude");
    let mut transcript = unluminous_chat::transcript::Transcript::new();
    for line in text.lines() {
        transcript.line(&mut chat, line);
    }
    let took = started.elapsed();
    let calls: usize = chat.messages.iter().map(|message| message.tools.len()).sum();
    let unanswered: usize = chat
        .messages
        .iter()
        .flat_map(|message| &message.tools)
        .filter(|call| call.is_running())
        .count();
    let failed: usize =
        chat.messages.iter().flat_map(|message| &message.tools).filter(|call| call.failed).count();
    println!("{} bytes read in {:.1} ms", text.len(), took.as_secs_f64() * 1000.0);
    println!(
        "{} messages, {} prompts, {} tool calls ({} unanswered, {} failed), model {}, turn open {}",
        chat.messages.len(),
        transcript.prompts,
        calls,
        unanswered,
        failed,
        transcript.model,
        transcript.turn_open()
    );
    for message in
        chat.messages.iter().filter(|message| message.role == unluminous_chat::Role::User)
    {
        let text = message.text();
        let short: String = text.chars().take(90).collect();
        println!("  > {}", short.replace('\n', " "));
    }
}
