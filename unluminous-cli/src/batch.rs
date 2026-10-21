//! `batch`: several commands in one call, run in order, each answered in the one reply.
//!
//! An agent that has to open three files and split the view makes four tool calls, and each one is
//! a whole round trip through the model: the model writes the call, waits, reads the answer and
//! writes the next. `task-2314` asked whether batching helps, and this is the cheapest thing that
//! could: the steps are command lines, written exactly as `unluminous-cli` takes them and exactly as
//! the usage lines in every tool description print them, so there is nothing new to learn.
//!
//! Three rules, and each is the conservative choice:
//!
//! - **The steps run one after another, in the order given.** Nothing is run at the same time, so a
//!   step can rely on what the step before it did (`tab open` then `editor caret`).
//! - **The first step that fails stops the batch**, unless `keep-going` says otherwise, and the reply
//!   says which step it was and what it said. A later step was usually written on the assumption that
//!   the earlier one worked.
//! - **Every step goes through the one road every other command takes.** A step is parsed by the
//!   client's own parser and handed to the same `run` a single tool call is, so a step and the same
//!   command sent alone are the same thing, refused or answered the same way.
//!
//! A batch cannot hold a batch, and it cannot hold a command an agent is not offered as a tool
//! (`mcp::tools::offered`), because a batch is a way of sending several tool calls, not a way round
//! the list of them.

use serde_json::{json, Map, Value};

use crate::catalogue::Command;
use crate::protocol::{code, Reply};

/// How many steps one batch may hold. Enough for any job a person describes in a sentence, and few
/// enough that a model which decides to batch every file in the project is stopped and told why.
pub const MOST_STEPS: usize = 50;

/// One step, ready to run.
#[derive(Debug, Clone)]
pub struct Step {
    /// The step as it was written, which is how the reply names it.
    pub line: String,
    pub command: &'static Command,
    pub arguments: Map<String, Value>,
}

/// Read the `steps` argument: a JSON array of command lines, or one string with a command line on
/// each line. Answers with the steps, or with the sentence a refusal should say.
pub fn steps_from(arguments: &Map<String, Value>) -> Result<Vec<Step>, String> {
    let lines: Vec<String> = match arguments.get("steps") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::String(line) => Ok(line.clone()),
                other => Err(format!(
                    "Each step is a command line as text, such as \"tab open src/main.rs\"; {other} is not."
                )),
            })
            .collect::<Result<_, _>>()?,
        Some(Value::String(text)) => {
            // A JSON array written as a string is the commonest way a model sends a list to a
            // parameter its schema calls text, so it is read as the list it is.
            match serde_json::from_str::<Vec<String>>(text.trim()) {
                Ok(list) if text.trim_start().starts_with('[') => list,
                _ => text.lines().map(str::to_owned).collect(),
            }
        }
        _ => return Err(
            "`batch` needs `steps`: the command lines to run, one a line, such as \"tab open src/main.rs\"."
                .to_owned(),
        ),
    };
    let lines: Vec<String> = lines
        .into_iter()
        .map(|line| line.trim().trim_start_matches("unluminous-cli ").trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return Err("`batch` was given no steps.".to_owned());
    }
    if lines.len() > MOST_STEPS {
        return Err(format!(
            "`batch` takes at most {MOST_STEPS} steps and was given {}. Send them in several batches.",
            lines.len()
        ));
    }
    lines.into_iter().enumerate().map(|(at, line)| step(at, line)).collect()
}

/// Parse one step, or say why it cannot be run.
fn step(at: usize, line: String) -> Result<Step, String> {
    let words = crate::parse::shell_words(&line);
    let typed = crate::parse::parse(&words)
        .map_err(|problem| format!("Step {} (`{line}`): {}", at + 1, problem.message))?;
    let Some(command) = typed.command else {
        return Err(format!("Step {} (`{line}`) names no command.", at + 1));
    };
    if command.wire() == "batch" {
        return Err(format!("Step {} is a batch, and a batch cannot hold another.", at + 1));
    }
    if !crate::mcp::tools::offered(command) {
        return Err(format!(
            "Step {} (`{}`) is not something an agent is offered, so a batch cannot run it either.",
            at + 1,
            command.typed()
        ));
    }
    Ok(Step { line, command, arguments: typed.arguments })
}

/// Run the steps through `run`, which is whatever runs one command for the caller, and answer with
/// one reply that says what every step did.
///
/// `run` answers `Err` when it could not reach an Unluminous at all, which stops the batch whatever
/// `keep_going` says: there is no point sending the next step to a window that is not there.
pub fn run(
    steps: Vec<Step>,
    keep_going: bool,
    mut run: impl FnMut(&'static Command, Map<String, Value>) -> Result<Reply, (String, String)>,
) -> Reply {
    let total = steps.len();
    let mut answers: Vec<Value> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut failed = 0;
    let mut stopped: Option<(usize, String, String)> = None;
    for (at, step) in steps.into_iter().enumerate() {
        let reply = match run(step.command, step.arguments) {
            Ok(reply) => reply,
            Err((failure, message)) => {
                stopped = Some((at, failure, message));
                break;
            }
        };
        lines.push(format!(
            "{}. {} {}: {}",
            at + 1,
            if reply.ok { "ok" } else { "FAILED" },
            step.line,
            reply.message.lines().next().unwrap_or_default()
        ));
        let worked = reply.ok;
        answers.push(json!({ "step": step.line, "ok": reply.ok, "reply": reply.to_json() }));
        if !worked {
            failed += 1;
            if !keep_going {
                let code = reply
                    .error
                    .as_ref()
                    .map(|e| e.code.clone())
                    .unwrap_or_else(|| code::FAILED.to_owned());
                stopped = Some((at, code, reply.message.clone()));
                break;
            }
        }
    }
    let ran = answers.len();
    let result = json!({ "ran": ran, "of": total, "failed": failed, "steps": answers });
    match stopped {
        Some((at, failure, message)) => {
            let mut reply = Reply::failed(
                "batch",
                &failure,
                format!(
                    "Step {} of {total} failed{}: {message}\n\n{}",
                    at + 1,
                    // Nothing is said about the steps after it when there are none.
                    match total - at - 1 {
                        0 => String::new(),
                        1 => ", so the 1 after it did not run".to_owned(),
                        left => format!(", so the {left} after it did not run"),
                    },
                    lines.join("\n")
                ),
            );
            reply.result = result;
            reply
        }
        None => Reply::done(
            "batch",
            format!(
                "{ran} steps ran{}.\n\n{}",
                match failed {
                    0 => String::new(),
                    n => format!(", {n} failed"),
                },
                lines.join("\n")
            ),
            result,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(steps: Value) -> Map<String, Value> {
        let mut arguments = Map::new();
        arguments.insert("steps".to_owned(), steps);
        arguments
    }

    #[test]
    fn steps_are_read_from_an_array_a_json_string_or_lines() {
        for steps in [
            json!(["tab open src/main.rs", "tab list"]),
            json!("[\"tab open src/main.rs\", \"tab list\"]"),
            json!("tab open src/main.rs\nunluminous-cli tab list\n"),
        ] {
            let read = steps_from(&given(steps.clone())).expect("reads");
            let names: Vec<String> = read.iter().map(|step| step.command.typed()).collect();
            assert_eq!(names, ["tab open", "tab list"], "from {steps}");
            assert_eq!(read[0].arguments["path"], "src/main.rs");
        }
    }

    #[test]
    fn a_step_that_is_not_a_command_is_refused_by_number_before_anything_runs() {
        let refused = steps_from(&given(json!(["tab open a.rs", "tab nonsense"]))).unwrap_err();
        assert!(refused.starts_with("Step 2"), "{refused}");
        assert!(steps_from(&given(json!(["batch --steps x"]))).is_err());
        assert!(steps_from(&given(json!(["mcp serve"]))).is_err(), "not offered, so not batchable");
        assert!(steps_from(&Map::new()).is_err());
    }

    #[test]
    fn the_first_failure_stops_the_batch_and_the_reply_says_where() {
        let steps = steps_from(&given(json!(["tab list", "tab show 9", "tab list"]))).unwrap();
        let mut sent = Vec::new();
        let reply = run(steps, false, |command, _| {
            sent.push(command.typed());
            Ok(match sent.len() {
                2 => Reply::failed(&command.wire(), code::NOT_FOUND, "There is no tab 9."),
                _ => Reply::done(&command.wire(), "1 open", Value::Null),
            })
        });
        assert_eq!(sent.len(), 2, "the third step does not run");
        assert!(!reply.ok);
        assert!(reply.message.starts_with("Step 2 of 3 failed"), "{}", reply.message);
        assert!(reply.message.contains("the 1 after it did not run"), "{}", reply.message);
        assert_eq!(reply.result["ran"], 2);
    }

    /// The last step failing has no steps after it to speak of. It said "the 0 after it did not run".
    #[test]
    fn a_failing_last_step_says_nothing_about_steps_after_it() {
        let steps = steps_from(&given(json!(["tab list", "tab show 9"]))).unwrap();
        let mut n = 0;
        let reply = run(steps, false, |command, _| {
            n += 1;
            Ok(match n {
                2 => Reply::failed(&command.wire(), code::NOT_FOUND, "There is no tab 9."),
                _ => Reply::done(&command.wire(), "1 open", Value::Null),
            })
        });
        assert!(
            reply.message.starts_with("Step 2 of 2 failed: There is no tab 9."),
            "{}",
            reply.message
        );
    }

    #[test]
    fn keep_going_runs_every_step_and_counts_the_failures() {
        let steps = steps_from(&given(json!(["tab list", "tab show 9", "tab list"]))).unwrap();
        let mut n = 0;
        let reply = run(steps, true, |command, _| {
            n += 1;
            Ok(match n {
                2 => Reply::failed(&command.wire(), code::NOT_FOUND, "There is no tab 9."),
                _ => Reply::done(&command.wire(), "ok", Value::Null),
            })
        });
        assert!(reply.ok);
        assert_eq!(reply.result["ran"], 3);
        assert_eq!(reply.result["failed"], 1);
    }
}
