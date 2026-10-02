//! `search` -- the code index (`tasks/task-2138-unluminous-code-index-tdd.md`).
//!
//! The window answers through the same function the command line and the MCP server use,
//! `unluminous_cli::search::ask`, so a search typed here and one an agent makes are one function. A
//! window lives as long as somebody is working in it, so when no other process hosts this project's
//! index the window becomes its host, in its own process, and every later search here costs no socket.

use super::*;

impl UnluminousApp {
    /// Answers a `search` command for the project this window shows, or for `--root` when it is given.
    ///
    /// @param request - the command and its arguments
    /// @param verb - the verb after `search.`
    pub(crate) fn cli_search(&mut self, request: &Request, _verb: &str) -> Outcome {
        let given =
            request.text("root").unwrap_or_else(|| self.tree.root().to_string_lossy().into_owned());
        let root = unluminous_cli::search::project_root(Some(&given));
        Outcome::Reply(unluminous_cli::search::ask(
            &root,
            &request.command,
            &request.arguments,
            std::time::Duration::from_secs(15),
            unluminous_cli::search::Hosting::InProcess,
        ))
    }
}
