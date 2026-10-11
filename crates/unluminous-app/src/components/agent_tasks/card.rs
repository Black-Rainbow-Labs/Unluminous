//! What the board says about a count of things.
//!
//! This file drew a card until `task-2241`, when the board took the Agent-Tasks design and a card became
//! `rux::components::TaskCard`, drawn from `components::agent_tasks::lanes`. What is left is the one piece of
//! wording three places on the board share.

/// `1 comment`, `2 comments`. One function, because three places on the board count things and a board
/// that says `1 comments` reads as a board nobody looked at.
pub(crate) fn plural(count: i64, what: &str) -> String {
    match count {
        1 => format!("1 {what}"),
        other => format!("{other} {what}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zero is plural, one is not, and everything else is — which is what stops the board reading
    /// `1 comments` and is the whole reason the function exists rather than a bare `format!` at each
    /// of the three call sites.
    #[test]
    fn only_one_of_a_thing_is_singular() {
        assert_eq!(plural(0, "comment"), "0 comments");
        assert_eq!(plural(1, "comment"), "1 comment");
        assert_eq!(plural(2, "comment"), "2 comments");
        assert_eq!(plural(1, "task"), "1 task");
        assert_eq!(plural(7, "task"), "7 tasks");
    }
}
