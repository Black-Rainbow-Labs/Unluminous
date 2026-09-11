//! The environment a key is read from, a program is looked for on, and a child is started with.
//!
//! **Given rather than read**, and that is the whole of the design. An Unluminous started from the Dock
//! or the Finder is started by launchd, which gives it about a dozen variables and
//! `PATH=/usr/bin:/bin:/usr/sbin:/sbin`. Nothing a person's `~/.zshrc` sets is in that process. So a key
//! read out of *this* process is a key that is not there, `claude` in `~/.local/bin` cannot be found at
//! all, and a child started from it gets none of the gateway's own variables.
//!
//! `task-1905` is the report — *"I get `illiad` reads its key from $ANTHROPIC_API_KEY and this window
//! has no such variable"* — and the sentence was telling the truth about the process it was asked about.
//!
//! `unluminous_app::services::login_shell` is what reads the person's profile, once, on a thread at
//! startup, and it is where the doctrine lives: why the profile rather than a list of well known
//! folders, what is deliberately not carried over, and why nothing runs a profile unless the released
//! binary asked for it. **This crate cannot call it** — it lives in `unluminous-app`, which depends on
//! this one — and it should not: the dependency points one way, which is what keeps this crate a leaf
//! with two dependencies and tests that run with no window.
//!
//! `provider.rs` already said so twice, at the two places it was forced to write something out for this
//! exact reason: the `PATH` walk carries *"which is `login_shell::find`'s rule"*, and the keychain read
//! carries *"This crate cannot call that module … so the two lines are here."* This is the third, and
//! rather than a third copy it is the seam: the window hands over what a command typed in Unluminous's
//! own terminal would have had, a test hands over a map it wrote, and
//! [`Environment::of_this_process`] is the honest answer for a caller with nothing better.

use std::ffi::OsString;

/// The variables a request or a program is answered out of.
#[derive(Clone, Default)]
pub struct Environment {
    variables: Vec<(String, String)>,
}

impl std::fmt::Debug for Environment {
    /// **The names and never the values.**
    ///
    /// This holds the person's whole shell profile, so `ANTHROPIC_API_KEY` is in it — and a derived
    /// `Debug` would print it. Nothing prints one today, but it is a field on [`crate::Ask`], which does
    /// derive `Debug`, so the next `{:?}` on a turn that failed would put a secret in a message, a log or a
    /// bug report. That is `services::agent_tasks::keychain`'s rule and `client.rs`'s: a key is redacted
    /// out of a server's own words before they are quoted, and the settings page says `set` or `not set`
    /// and never the value.
    ///
    /// What is printed is what a failing assertion actually wants: how many variables there are and which,
    /// which is enough to tell "the profile was read" from "it was not".
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.variables.iter().map(|(name, _)| name.as_str()).collect();
        out.debug_struct("Environment").field("names", &names).finish()
    }
}

impl Environment {
    /// The environment of this process, which is what a caller with nothing better to offer means.
    ///
    /// Honest rather than empty: when Unluminous was started from a terminal it is the whole of the
    /// person's environment anyway, and every existing caller means exactly this.
    pub fn of_this_process() -> Environment {
        Environment { variables: std::env::vars().collect() }
    }

    /// Nothing at all, for a test that wants to be sure the process's own is not being read.
    pub fn empty() -> Environment {
        Environment::default()
    }

    /// One variable, or nothing.
    pub fn variable(&self, name: &str) -> Option<&str> {
        self.variables
            .iter()
            .find(|(named, _)| named == name)
            .map(|(_, value)| value.as_str())
    }

    /// The `PATH` a program is looked for on, which is a variable and is also asked for on its own.
    pub fn search_path(&self) -> Option<OsString> {
        self.variable("PATH").map(OsString::from)
    }

    /// Every pair, for a caller that is starting a child and has to hand the lot over.
    pub fn variables(&self) -> &[(String, String)] {
        &self.variables
    }

    /// Whether this looks like a whole environment rather than a handful of names.
    ///
    /// **What it decides is whether a child is started with only this and nothing inherited.** Replacing a
    /// process's environment wholesale is right when what is being handed over is the person's own profile,
    /// and wrong when it is three variables a test wrote or the little `login_shell` falls back to when the
    /// profile could not be read at all — a child started with no `HOME` and no `USER` is a worse failure
    /// than one that kept a stale variable.
    ///
    /// The test is that the things every process needs are in it. Deliberately not a count: a number would
    /// be a threshold nobody could defend, and these are the names a program actually cannot do without.
    pub fn is_whole(&self) -> bool {
        let needed = match cfg!(windows) {
            true => ["PATH", "USERPROFILE"],
            false => ["PATH", "HOME"],
        };
        needed.iter().all(|name| self.variable(name).is_some_and(|value| !value.trim().is_empty()))
    }

    /// Add or replace one variable, which is what a caller that knows something this does not means.
    pub fn with(mut self, name: &str, value: &str) -> Environment {
        match self.variables.iter_mut().find(|(named, _)| named == name) {
            Some((_, held)) => *held = value.to_owned(),
            None => self.variables.push((name.to_owned(), value.to_owned())),
        }
        self
    }
}

impl From<Vec<(String, String)>> for Environment {
    /// What the window hands over: `login_shell::for_a_child()`, which is the person's whole profile
    /// with `PATH` already set to the one a shell would search.
    fn from(variables: Vec<(String, String)>) -> Environment {
        Environment { variables }
    }
}

impl<const N: usize> From<[(&str, &str); N]> for Environment {
    /// What a test hands over.
    fn from(pairs: [(&str, &str); N]) -> Environment {
        Environment {
            variables: pairs
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
        }
    }
}

/// Whether `name` is worth looking for at all, which is the one thing both readers agree on.
pub(crate) fn non_empty(name: &str) -> Option<&str> {
    let name = name.trim();
    match name.is_empty() {
        true => None,
        false => Some(name),
    }
}

/// The `PATH` to search, or this process's own when the environment names none at all.
///
/// **The fallback is kept here and taken away for a key**, and the difference is what each is for. A `PATH`
/// is not a secret: an environment with none is a caller that has nothing to say about where to look, and
/// refusing would mean a program that cannot be found on a machine where it is plainly installed. A **key**
/// the person removed from their profile must not be replaced by a stale one — see [`variable_of`].
///
/// An environment that names a `PATH` is believed, empty or not, so a caller can say "look nowhere".
pub(crate) fn path_of(environment: &Environment) -> Option<OsString> {
    environment.search_path().or_else(|| std::env::var_os("PATH"))
}

/// One variable's value, out of the environment this was given.
///
/// **It does not fall back to this process, and that is the whole point of taking one.** An environment that
/// was handed over is the answer: falling back would mean a key the person has *removed* from their profile,
/// or blanked, is silently replaced by the stale one this process was launched with — so a revoked key would
/// go on being sent, and the settings page would go on saying the row was ready. The Codex Sol review of
/// `task-1905` found that; `keychain`'s rule is that a secret is where the person put it and nowhere else.
///
/// A caller with nothing better says so with [`Environment::of_this_process`], which is honest and is what
/// every pre-existing caller means.
pub(crate) fn variable_of(environment: &Environment, name: &str) -> Option<String> {
    let name = non_empty(name)?;
    let value = environment.variable(name)?.trim().to_owned();
    match value.is_empty() {
        true => None,
        false => Some(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_variable_comes_from_the_environment_it_was_given_and_from_nowhere_else() {
        // `task-1905`: the whole point of the value. This name is one no process has.
        let given = Environment::from([("UNLUMINOUS_TEST_ONLY_KEY", "sk-from-the-profile")]);
        assert_eq!(variable_of(&given, "UNLUMINOUS_TEST_ONLY_KEY").as_deref(), Some("sk-from-the-profile"));
        assert_eq!(variable_of(&Environment::empty(), "UNLUMINOUS_TEST_ONLY_KEY"), None);
    }

    /// **A key the profile does not have is not resurrected from this process.**
    ///
    /// The Codex Sol review of `task-1905` found the fallback: an Unluminous started from a terminal that
    /// still had an old `ANTHROPIC_API_KEY`, whose owner has since removed it from their profile, went on
    /// sending the old one — and the settings page went on saying the row was ready. A revoked key that goes
    /// on being sent is the worst shape this could take.
    #[test]
    fn a_key_this_process_holds_does_not_stand_in_for_one_the_profile_lacks() {
        let name = "UNLUMINOUS_TEST_STALE_KEY";
        // SAFETY: single-threaded test, and the variable is one nothing else reads.
        unsafe { std::env::set_var(name, "sk-the-stale-one") };
        // A profile that does not mention it at all, and one that mentions it blank — a person who
        // exported it and then cleared it.
        assert_eq!(variable_of(&Environment::empty(), name), None, "not from this process");
        let blanked = Environment::from([(name, "  ")]);
        assert_eq!(variable_of(&blanked, name), None, "and a blank profile value is not a value");
        unsafe { std::env::remove_var(name) };
    }

    #[test]
    fn a_blank_value_is_not_a_value() {
        let given = Environment::from([("UNLUMINOUS_TEST_ONLY_KEY", "   ")]);
        assert_eq!(variable_of(&given, "UNLUMINOUS_TEST_ONLY_KEY"), None);
        assert_eq!(variable_of(&given, "   "), None);
    }

    #[test]
    fn an_environment_with_no_path_falls_back_to_this_process() {
        // A caller that handed over no `PATH` means "look wherever you would have looked", which is
        // better than a program that cannot be found on a machine where it is installed.
        assert!(path_of(&Environment::empty()).is_some(), "this process has a PATH");
        let given = Environment::from([("PATH", "/somewhere/of/its/own")]);
        assert_eq!(path_of(&given), Some(OsString::from("/somewhere/of/its/own")));
    }

    /// The names and never the values, so a `{:?}` on a failed turn cannot put a key in a message.
    #[test]
    fn printing_an_environment_shows_the_names_and_never_the_values() {
        let given = Environment::from([("ANTHROPIC_API_KEY", "sk-the-actual-secret")]);
        let printed = format!("{given:?}");
        assert!(printed.contains("ANTHROPIC_API_KEY"), "{printed}");
        assert!(!printed.contains("sk-the-actual-secret"), "{printed}");
    }

    /// A handful of names is not a whole environment, and a child is not started with only it.
    #[test]
    fn only_a_whole_environment_replaces_a_childs_own() {
        assert!(!Environment::empty().is_whole(), "nothing at all is not an environment");
        let few = Environment::from([("ANTHROPIC_API_KEY", "sk-x")]);
        assert!(!few.is_whole(), "a key on its own is not an environment");
        // What `login_shell::for_a_child` answers when the profile could not be read: a `PATH` and no more.
        let only_a_path = Environment::from([("PATH", "/usr/bin:/bin")]);
        assert!(!only_a_path.is_whole(), "a PATH alone would start a child with no HOME");
        assert!(Environment::of_this_process().is_whole(), "this process's own is whole");
    }

    #[test]
    fn one_variable_can_be_laid_over_the_rest() {
        let given = Environment::from([("A", "1"), ("B", "2")]).with("B", "3").with("C", "4");
        assert_eq!(given.variable("A"), Some("1"));
        assert_eq!(given.variable("B"), Some("3"));
        assert_eq!(given.variable("C"), Some("4"));
        assert_eq!(given.variables().len(), 3, "replacing is not adding");
    }
}
