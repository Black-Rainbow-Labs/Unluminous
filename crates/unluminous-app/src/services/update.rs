//! Whether a newer Unluminous has been released.
//!
//! `task-1804` §6: *"Nothing in the binary asks whether a newer version exists. A person who installs
//! 0.34.2 stays on 0.34.2 until they happen to visit the releases page. For a product that releases
//! on every finished task this is the largest single gap between 'we shipped a fix' and 'someone has
//! the fix'."*
//!
//! The machinery was already most of the way there: `tools/release.ps1` and `tools/release.sh`
//! publish a GitHub release with the installer attached on every finished task, and `releases/` keeps
//! the file. What was missing was a check and a prompt, and this is the check.
//!
//! ## Nothing is fetched that was not asked for, and that is not softened here
//!
//! `task-1692` drew this line and the chat pane kept it: *"There is no discovery, no model list, no
//! telemetry and nothing at startup."* An editor that phones home the moment it opens is exactly
//! what that rule exists to prevent, and "but it is only checking for updates" is what every program
//! that does it says.
//!
//! So **`update.check` is `off` until somebody says otherwise**, and with it off nothing is ever
//! sent. What is always there is `Unluminous -> Check for Updates`, which is a person asking, and
//! `unluminous-cli update check`, which is an agent asking. Turning the setting to `start` is a
//! person saying *ask every time I open it*, once, in a Settings page.
//!
//! ## What it asks, and what it does not
//!
//! One `GET`, unauthenticated, with no query, no header identifying this machine beyond the
//! `user-agent` GitHub requires, and no body. It sends **nothing about the person or the project**:
//! not which files are open, not which version is installed — the comparison is made here, on what
//! came back, so the server is never told what to compare against.
//!
//! ## Where it asks, and why that took a ticket
//!
//! `task-1993`: every address in this file named `jasonmcaffee/unluminous`, which is **private**.
//! Measured with no credential, both the API endpoint and the releases page answered **404**, so for
//! everybody who installed Unluminous from unluminous.com — which is everybody but Jason — the check
//! could not once have succeeded, and `update.check = start` invited a person to switch on a request
//! that was going to fail on every launch. The comment above about this being *"the largest single
//! gap between 'we shipped a fix' and 'someone has the fix'"* was true and the code left the gap
//! total.
//!
//! Jason's answer on that ticket was **unluminous.com first, GitHub as the fallback**, and
//! unluminous.com is where the bytes actually are: the site hosts the installer itself and its
//! Install section already prints the version, the size and the SHA-256. So it publishes a manifest
//! at `/releases/latest.json`, written from the same values the page prints by the site's own
//! `scripts/write-release-manifest.mjs`, and that is the first thing asked.
//!
//! The fallback is `Black-Rainbow-Labs/Unluminous`, which is the **public** repository the source
//! was opened under on `task-1989`, and never the private one again. `tools/release.ps1` creates the
//! release on both, so the fallback is a real answer rather than a URL that happens to resolve.
//!
//! **The two shapes are read by one reader.** The manifest calls its fields `version`, `url` and
//! `notes`; GitHub calls the same three `tag_name`, `html_url` and `body`. [`read`] takes either
//! spelling, which is six lines and leaves nothing to drift — where two readers would be two places
//! to get a release wrong, and only one of them would be exercised on any given day.
//!
//! **A source that fails moves to the next; a source that answers is the answer.** Only
//! [`Answer::Failed`] falls through: *"this is the newest there is"* is an answer, and asking the
//! fallback after it would be asking a second opinion about a question already settled. When every
//! source fails, the refusal names each host and what it said, because *which* of them was down is
//! the whole of what somebody can act on.
//!
//! ## Two channels, and `latest.json` is the stable one (`task-2328`)
//!
//! Every finished ticket used to be offered to every window the next day, so a release with a bad bug
//! in it reached everybody before anybody had used it. Releases are now on one of two channels:
//! `tools/release.ps1` publishes every ticket to **nightly**, and `tools/promote.ps1` makes a nightly
//! that has held up **stable**, without building it again. unluminous.com publishes
//! `/releases/stable.json` and `/releases/nightly.json`, each the newest version on that channel in the
//! shape [`read`] already reads, plus a `versions` list of the newest ten. `/releases/latest.json`
//! means the newest stable version, so every Unluminous released before this only offers promoted
//! builds. `update.channel` says which channel a window follows, stable by default.
//!
//! A stable check asks `stable.json`, then `latest.json`, then GitHub, whose `releases/latest` never
//! answers with a prerelease and nightlies are published as prereleases. A nightly check asks
//! `nightly.json` only: the newest GitHub release is the stable one, and calling it the newest nightly
//! would be wrong. `tasks/task-2328-nightly-and-stable-releases-tdd.md` is the design.
//!
//! ## The transport is the chat pane's
//!
//! `unluminous_chat::client::tls_config`, verbatim, rather than a second one. That function is where
//! two measured facts live — `ureq`'s default TLS provider is Rustls whether or not the feature is
//! on, so an `https` request that does not name the provider **panics inside the transport** on the
//! worker thread; and `RootCerts::WebPki` switches the machine's own certificate store off, which
//! fails behind an employer's private chain. A second copy of that reasoning would be a second place
//! to get it wrong.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The manifest unluminous.com publishes, which is where the installer a person downloads is. It names
/// the newest stable version, which is what every Unluminous before `task-2328` reads.
const SITE: &str = "https://unluminous.com/releases/latest.json";
/// The stable channel's manifest, with the newest ten stable versions in it.
const STABLE: &str = "https://unluminous.com/releases/stable.json";
/// The nightly channel's manifest: every release, the newest ten of them listed.
const NIGHTLY: &str = "https://unluminous.com/releases/nightly.json";
/// The public repository, asked when the site does not answer. Never the private one: it is 404.
const GITHUB: &str = "https://api.github.com/repos/Black-Rainbow-Labs/Unluminous/releases/latest";
/// Where a stable check asks, in order. The first that answers with a version is the answer.
/// `latest.json` is second so a site not yet rebuilt with the channel manifests still answers.
pub const SOURCES: [&str; 3] = [STABLE, SITE, GITHUB];
/// Where a nightly check asks. Only the site: GitHub's latest release is the stable one.
pub const NIGHTLY_SOURCES: [&str; 1] = [NIGHTLY];
/// How many versions of each channel the menu offers under `Latest`.
pub const SHOWN: usize = 5;

/// Which builds a window follows. `task-2328`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Channel {
    /// Versions somebody promoted after using them as a nightly. What a window follows by default.
    #[default]
    Stable,
    /// Every release, which is every finished ticket.
    Nightly,
}

impl Channel {
    pub const ALL: [Channel; 2] = [Channel::Stable, Channel::Nightly];

    /// The word the settings file, the command line and the manifest's file name use.
    pub fn name(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Nightly => "nightly",
        }
    }

    /// The word a menu shows.
    pub fn label(self) -> &'static str {
        match self {
            Channel::Stable => "Stable",
            Channel::Nightly => "Nightly",
        }
    }

    pub fn from_name(name: &str) -> Option<Channel> {
        Channel::ALL.into_iter().find(|channel| channel.name() == name.trim().to_ascii_lowercase())
    }

    /// [`Channel::from_name`] under the name the settings table reads a coded value with.
    pub fn parse(name: &str) -> Option<Channel> {
        Channel::from_name(name)
    }

    /// Where this channel is asked, in order: [`SOURCES`] or [`NIGHTLY_SOURCES`], unless the
    /// environment names others. See [`releases_endpoints`].
    pub fn sources(self) -> Vec<String> {
        releases_endpoints(self)
    }
}
/// Where a person goes to get one. The Install section of the site, which holds the installer.
pub const RELEASES_PAGE: &str = "https://unluminous.com/#install";
/// GitHub refuses a request with no `user-agent` outright, so it names the program and nothing else.
const AGENT: &str = "Unluminous";
/// How long the whole thing is given. It is a background question and a slow answer is no answer.
///
/// The catalogue's `update check --timeout` says 15000 by default and this was ten seconds whatever
/// the caller asked (`task-1984` L1), so the documented default was not the real one and the flag
/// did nothing. `Check::start_for` takes a wait; this is what a check nobody gave one is given.
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// What the releases page says the newest one is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// `0.35.0` — the tag with its `v` taken off.
    pub version: String,
    /// The page a person downloads it from.
    pub url: String,
    /// What the release said about itself, cut to something a status bar can hold.
    pub notes: String,
    /// The file that installs it on this platform, when the source said which one. `task-2063`.
    ///
    /// unluminous.com's manifest names the Windows setup program and the macOS zip, each with its size
    /// and its SHA-256; GitHub's answer names neither, and Linux has no installer at all. `None` is the
    /// answer for all three, and the offer then opens the download page instead.
    pub download: Option<Download>,
}

/// One installer: where it is, how long it is and what its SHA-256 is.
///
/// The size and the hash are what make it safe to run: the file is checked against both before
/// anything is started, and one that does not match is deleted. See `services::update_install`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub url: String,
    pub bytes: u64,
    /// Lower case hexadecimal, sixty four characters.
    pub sha256: String,
}

/// What a check came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// There is a newer one.
    Newer(Release),
    /// This is the newest one, and this is what it is.
    Current(String),
    /// It could not be asked. The message is what went wrong, in the server's own words where there
    /// are any -- `unluminous-git`'s rule about never inventing one.
    Failed(String),
}

impl Answer {
    /// One sentence for the status bar and for the command line's `message`.
    pub fn sentence(&self) -> String {
        match self {
            Answer::Newer(release) => format!(
                "Unluminous {} is out. This is {}. {}",
                release.version,
                crate::build_info::VERSION,
                RELEASES_PAGE
            ),
            // A source can be behind the window asking it: the site is published a step after the
            // release, so for the minutes in between somebody can be running something later than
            // anything published. "0.52.0 is the newest there is" to a person on 0.53.0 is a wrong
            // sentence with a true-sounding shape, so that case says what is actually the case.
            Answer::Current(version) => match version == crate::build_info::VERSION {
                true => format!("Unluminous {version} is the newest there is."),
                false => format!(
                    "This is Unluminous {}, which is later than the newest published, {version}.",
                    crate::build_info::VERSION
                ),
            },
            Answer::Failed(problem) => format!("Could not check for a newer Unluminous: {problem}"),
        }
    }
}

/// A check running on a thread, and its answer when it arrives.
///
/// A thread and a waker, arranged as `unluminous_git::Worker`, the text search and the chat client
/// already are — because a request over the network on the drawing thread would stop the window
/// drawing for as long as it took, which on a slow connection looks exactly like a crash.
pub struct Check {
    answers: Receiver<Answer>,
    /// True until the answer has been taken, so the About box can say it is asking.
    asking: bool,
}

impl Check {
    /// Start asking `channel`. `wake` asks the window to draw again when the answer lands.
    pub fn start(channel: Channel, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::start_for(channel, TIMEOUT, wake)
    }

    /// The same, with a wait the caller chose.
    ///
    /// `unluminous-cli update check --timeout` (`task-1984` L1). The catalogue has declared that flag
    /// since the command was written and nothing read it: the wait was a constant, and the number the
    /// documentation gave was not the number the code used.
    pub fn start_for(
        channel: Channel,
        timeout: Duration,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (sender, answers) = std::sync::mpsc::channel();
        let urls = channel.sources();
        std::thread::Builder::new()
            .name("unluminous-update-check".to_owned())
            .spawn(move || {
                let answer = ask_each(&urls, timeout);
                // The window may have gone; a send to a closed channel is the ordinary end of this
                // thread rather than something to report.
                let _ = sender.send(answer);
                wake();
            })
            .ok();
        Self { answers, asking: true }
    }

    /// The same, against a scripted server, which is what the tests drive.
    pub fn start_from(url: String, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, answers) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("unluminous-update-check".to_owned())
            .spawn(move || {
                let _ = sender.send(ask_at(&url));
                wake();
            })
            .ok();
        Self { answers, asking: true }
    }

    /// The answer, once. `None` while it is still being asked.
    pub fn poll(&mut self) -> Option<Answer> {
        match self.answers.try_recv() {
            Ok(answer) => {
                self.asking = false;
                Some(answer)
            }
            Err(_) => None,
        }
    }

    /// True while nothing has come back yet.
    pub fn is_asking(&self) -> bool {
        self.asking
    }
}

/// Where a check on `channel` asks, which is [`SOURCES`] or [`NIGHTLY_SOURCES`] unless something in
/// the environment says otherwise.
///
/// `UNLUMINOUS_RELEASES` is a test seam of the shape `UNLUMINOUS_HOME`, `UNLUMINOUS_INSTANCES` and
/// `UNLUMINOUS_CLI_BIN` already are: a scripted server on loopback stands in for the real ones, so
/// `update check` can be driven to a real success in the suite rather than sitting on
/// `CANNOT_BE_MADE_TO_SUCCEED` for ever (`task-1984` L1). Nothing in a released Unluminous sets it,
/// so the addresses a person's window asks are unchanged.
///
/// It is a **comma-separated list** rather than one address (`task-1993`), so a test can drive the
/// ordering the real thing has — a first source that fails and a second that answers — rather than
/// only the single-source case. One address is still one address, which is what every test written
/// before that ticket passes.
///
/// `UNLUMINOUS_RELEASES_STABLE` and `UNLUMINOUS_RELEASES_NIGHTLY` name one channel's sources and win
/// over `UNLUMINOUS_RELEASES`, which names both (`task-2328`).
fn releases_endpoints(channel: Channel) -> Vec<String> {
    let own = format!("UNLUMINOUS_RELEASES_{}", channel.name().to_ascii_uppercase());
    if let Ok(named) = std::env::var(own) {
        return sources_from(&named);
    }
    match std::env::var("UNLUMINOUS_RELEASES") {
        Ok(named) => sources_from(&named),
        Err(_) => defaults(channel),
    }
}

/// The addresses a released Unluminous asks for `channel`.
fn defaults(channel: Channel) -> Vec<String> {
    let sources: &[&str] = match channel {
        Channel::Stable => &SOURCES,
        Channel::Nightly => &NIGHTLY_SOURCES,
    };
    sources.iter().map(|&url| url.to_owned()).collect()
}

/// The addresses a comma-separated list names, with the blanks and the spacing taken out.
fn sources_from(named: &str) -> Vec<String> {
    named
        .split(',')
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The host an address names, which is what a refusal calls the source that would not answer.
///
/// Derived rather than written beside each address, so the test seam's loopback ports name
/// themselves too and there is no second list to keep in step with [`SOURCES`].
fn host_of(url: &str) -> &str {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    after_scheme.split(['/', '?']).next().unwrap_or(after_scheme)
}

/// Ask the real releases endpoints of `channel`, in order. Runs on the worker thread.
pub fn ask(channel: Channel) -> Answer {
    ask_each(&channel.sources(), TIMEOUT)
}

/// Ask each address in turn, and answer with the first that answers.
///
/// **Only a failure falls through.** `Current` and `Newer` are answers, and asking the next source
/// after one of them would be asking a second opinion about a settled question — and would get a
/// different one whenever the site had been deployed and the GitHub release had not, or the other
/// way round.
///
/// **The budget is shared out rather than handed to each in turn**, so the whole check still fits
/// inside the wait the caller chose: with two sources and fifteen seconds the first gets seven and a
/// half, and whatever it leaves is the second's. A first source that hangs for the whole timeout
/// would otherwise mean the fallback was never asked at all, which is the one case it exists for.
pub fn ask_each(urls: &[String], timeout: Duration) -> Answer {
    let deadline = Instant::now() + timeout;
    let mut refusals: Vec<String> = Vec::new();
    for (index, url) in urls.iter().enumerate() {
        let left = deadline.saturating_duration_since(Instant::now());
        let share = left / (urls.len() - index) as u32;
        if share.is_zero() {
            refusals.push(format!("{}: the wait ran out before it could be asked", host_of(url)));
            continue;
        }
        match ask_within(url, share) {
            Answer::Failed(problem) => refusals.push(format!("{}: {problem}", host_of(url))),
            answered => return answered,
        }
    }
    match refusals.is_empty() {
        true => Answer::Failed("there is nowhere to ask".to_owned()),
        false => Answer::Failed(refusals.join("; ")),
    }
}

/// The same against any address, so a test can point it at a server on loopback.
pub fn ask_at(url: &str) -> Answer {
    ask_within(url, TIMEOUT)
}

/// The same, with a wait the caller chose. See [`Check::start_for`].
pub fn ask_within(url: &str, timeout: Duration) -> Answer {
    let body = match fetch(url, timeout) {
        Ok(body) => body,
        Err(problem) => return Answer::Failed(problem),
    };
    match read(&body) {
        Some(release) => match is_newer(crate::build_info::VERSION, &release.version) {
            true => Answer::Newer(release),
            false => Answer::Current(release.version),
        },
        None => Answer::Failed("answered something this version cannot read".to_owned()),
    }
}

/// The body one address answers with, or what went wrong in the server's own words.
pub fn fetch(url: &str, timeout: Duration) -> Result<String, String> {
    let config = ureq::Agent::config_builder()
        .tls_config(unluminous_chat::client::tls_config())
        // The body of a 403 is where GitHub says *why* -- a rate limit, usually -- and that is the
        // whole of what there is to tell somebody. `unluminous-chat` makes the same argument.
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        // A redirect goes somewhere nobody named. There is nothing secret in this request, so this is
        // not the security decision it is in the chat client; it is the same decision about honesty.
        .max_redirects(0)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let sent = agent
        .get(url)
        .header("user-agent", AGENT)
        // Both sources answer JSON and both accept this: GitHub prefers `application/vnd.github+json`
        // and was measured answering 200 to this one, and unluminous.com serves a static file to
        // whatever asks. One header for both rather than a rule about which host is being asked.
        .header("accept", "application/json")
        .call();
    let mut reply = match sent {
        Ok(reply) => reply,
        Err(problem) => return Err(problem.to_string()),
    };
    let status = reply.status().as_u16();
    let body = reply.body_mut().read_to_string().unwrap_or_default();
    if status != 200 {
        // GitHub's own words, cut short: its `message` is a sentence and the rest of the object is
        // documentation links nobody reads out of a status bar. A site answering with its 404 page
        // has no message in it, so what is left to say is the number. `ask_each` puts the host in
        // front of whichever of the two this is.
        let said = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value.get("message")?.as_str().map(str::to_owned))
            .unwrap_or_else(|| format!("answered {status}"));
        return Err(said);
    }
    Ok(body)
}

/// Every version a channel's manifest lists, newest first.
///
/// `stable.json` and `nightly.json` carry a `versions` list beside the newest version (`task-2328`).
/// `latest.json` and GitHub's answer carry only the newest, which is then a list of one, so a stable
/// list still comes back while the site has not been rebuilt with the channel manifests. A version that
/// is listed twice is kept once, where it first appears.
pub fn versions_in(body: &str) -> Option<Vec<Release>> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let mut found: Vec<Release> = match value.get("versions").and_then(|listed| listed.as_array()) {
        Some(listed) => listed.iter().filter_map(release_from).collect(),
        None => vec![release_from(&value)?],
    };
    let mut seen = std::collections::HashSet::new();
    found.retain(|release| seen.insert(release.version.clone()));
    (!found.is_empty()).then_some(found)
}

/// Ask each address in turn for a channel's list, and answer with the first that gives one.
///
/// The same order and the same shared budget as [`ask_each`], and the same rule that only a failure
/// falls through.
pub fn list_each(urls: &[String], timeout: Duration) -> Result<Vec<Release>, String> {
    let deadline = Instant::now() + timeout;
    let mut refusals: Vec<String> = Vec::new();
    for (index, url) in urls.iter().enumerate() {
        let left = deadline.saturating_duration_since(Instant::now());
        let share = left / (urls.len() - index) as u32;
        if share.is_zero() {
            refusals.push(format!("{}: the wait ran out before it could be asked", host_of(url)));
            continue;
        }
        match fetch(url, share) {
            Ok(body) => match versions_in(&body) {
                Some(found) => return Ok(found),
                None => refusals
                    .push(format!("{}: answered something this version cannot read", host_of(url))),
            },
            Err(problem) => refusals.push(format!("{}: {problem}", host_of(url))),
        }
    }
    match refusals.is_empty() {
        true => Err("there is nowhere to ask".to_owned()),
        false => Err(refusals.join("; ")),
    }
}

/// What both channels list, or why one could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versions {
    pub stable: Result<Vec<Release>, String>,
    pub nightly: Result<Vec<Release>, String>,
}

impl Versions {
    /// One channel's list.
    pub fn of(&self, channel: Channel) -> &Result<Vec<Release>, String> {
        match channel {
            Channel::Stable => &self.stable,
            Channel::Nightly => &self.nightly,
        }
    }

    /// The release a channel lists under `version`, with its installer.
    pub fn find(&self, channel: Channel, version: &str) -> Option<&Release> {
        self.of(channel).as_ref().ok()?.iter().find(|release| release.version == version)
    }

    /// The newest release a channel lists.
    pub fn newest(&self, channel: Channel) -> Option<&Release> {
        self.of(channel).as_ref().ok()?.first()
    }
}

/// Both channels' lists being fetched on a thread, and the answer when it arrives.
///
/// Arranged as [`Check`] is. The two channels are asked at once, so the wait is the slower of the two
/// rather than their sum.
pub struct Listing {
    answers: Receiver<Versions>,
    asking: bool,
}

impl Listing {
    /// Start fetching both channels. `wake` asks the window to draw again when the lists land.
    pub fn start(timeout: Duration, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, answers) = std::sync::mpsc::channel();
        let (stable, nightly) = (Channel::Stable.sources(), Channel::Nightly.sources());
        std::thread::Builder::new()
            .name("unluminous-update-versions".to_owned())
            .spawn(move || {
                let versions = std::thread::scope(|scope| {
                    let stable = scope.spawn(|| list_each(&stable, timeout));
                    let nightly = scope.spawn(|| list_each(&nightly, timeout));
                    let stopped = |_| Err("the request stopped".to_owned());
                    Versions {
                        stable: stable.join().unwrap_or_else(stopped),
                        nightly: nightly.join().unwrap_or_else(stopped),
                    }
                });
                let _ = sender.send(versions);
                wake();
            })
            .ok();
        Self { answers, asking: true }
    }

    /// The lists, once. `None` while they are still being fetched.
    pub fn poll(&mut self) -> Option<Versions> {
        match self.answers.try_recv() {
            Ok(versions) => {
                self.asking = false;
                Some(versions)
            }
            Err(_) => None,
        }
    }

    pub fn is_asking(&self) -> bool {
        self.asking
    }
}

/// One release out of what a source sent, in either of the two shapes there are.
///
/// unluminous.com's manifest says `version`, `url` and `notes`; GitHub's `releases/latest` says
/// `tag_name`, `html_url` and `body` for the same three things. Reading both here is what makes the
/// fallback free: `ask_each` does not know or care which source answered it, and there is no second
/// reader to be wrong in a way only one day's outage would ever show.
///
/// Pure, so every shape it has to survive is a test with no socket behind it.
pub fn read(body: &str) -> Option<Release> {
    release_from(&serde_json::from_str(body).ok()?)
}

/// One release out of a JSON object in either shape. [`read`] for an object already parsed, which is
/// what each entry of a channel's `versions` list is.
pub fn release_from(value: &serde_json::Value) -> Option<Release> {
    let said = |keys: [&str; 2]| -> Option<&str> {
        keys.into_iter().find_map(|key| value.get(key)?.as_str())
    };
    // The `v` comes off whichever name the version arrived under: the manifest writes it without one
    // and a git tag carries one, and a person comparing them should not have to know that.
    let version = said(["version", "tag_name"])?.trim().trim_start_matches('v').trim().to_owned();
    if version.is_empty() {
        return None;
    }
    let url = said(["url", "html_url"]).unwrap_or(RELEASES_PAGE).to_owned();
    // The first line of the notes, which is what `release.ps1` writes as the summary and what the
    // manifest carries whole. The rest of a GitHub body is the download instructions, which somebody
    // reading a status bar does not need.
    let notes = said(["notes", "body"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned();
    let download = download_in(value, PLATFORM_FIELD);
    Some(Release { version, url, notes, download })
}

/// Which of the manifest's installers is this platform's: `installer` on Windows, `macos` on macOS,
/// and nothing anywhere else.
pub const PLATFORM_FIELD: Option<&str> = if cfg!(windows) {
    Some("installer")
} else if cfg!(target_os = "macos") {
    Some("macos")
} else {
    None
};

/// The installer a manifest names under `field`, with its size and hash, or nothing when any of the
/// three is missing. An installer that cannot be checked is not one this window will run.
pub fn download_in(value: &serde_json::Value, field: Option<&str>) -> Option<Download> {
    let field = field?;
    let url = value.get(field)?.as_str()?.trim().to_owned();
    let bytes = value.get(format!("{field}Bytes"))?.as_u64()?;
    let sha256 = value.get(format!("{field}Sha256"))?.as_str()?.trim().to_ascii_lowercase();
    let hex = sha256.len() == 64 && sha256.chars().all(|c| c.is_ascii_hexdigit());
    let secure = url.starts_with("https://") || is_loopback(&url);
    (secure && bytes > 0 && hex).then_some(Download { url, bytes, sha256 })
}

/// Whether an address is plain `http` on this machine's own loopback, which is only ever the scripted
/// server `UNLUMINOUS_RELEASES` points a test at. Everything unluminous.com publishes is `https`.
fn is_loopback(url: &str) -> bool {
    ["http://127.0.0.1:", "http://127.0.0.1/", "http://localhost:", "http://localhost/"]
        .iter()
        .any(|start| url.starts_with(start))
}

/// The manifest's own address, which an install asks when the answer it has came from GitHub and so
/// names no installer.
pub const MANIFEST: &str = SITE;

/// How long an automatic check waits before asking again. `task-2063`: *"once per day"*.
pub const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// Whether an automatic check is due, given when the last one was asked, as seconds since the epoch.
///
/// Pure, so the rule is a test with no clock and no file. Nothing written down means never asked,
/// which is due. A time in the future, which a clock put back can leave behind, is treated as due
/// rather than as a reason to wait for a day that may be years away.
pub fn is_due(last: Option<u64>, now: u64) -> bool {
    match last {
        None => true,
        Some(last) if last > now => true,
        Some(last) => now - last >= DAY.as_secs(),
    }
}

/// When the last automatic check was asked, read from `update-checked.txt` in the settings folder.
///
/// One file for every window, because each window is a process of its own and a clock held by one
/// would check once a day per window.
pub fn last_checked(folder: &std::path::Path) -> Option<u64> {
    std::fs::read_to_string(folder.join(CHECKED)).ok()?.trim().parse().ok()
}

/// Write down that an automatic check is being asked now.
///
/// Written **before** the request goes, so two windows starting together do not both ask.
pub fn note_checked(folder: &std::path::Path, now: u64) {
    let _ =
        crate::services::store::write_atomically(&folder.join(CHECKED), now.to_string().as_bytes());
}

/// The file [`last_checked`] reads.
const CHECKED: &str = "update-checked.txt";

/// Seconds since the epoch, which is what [`is_due`] is asked in.
pub fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// Whether `found` is a later version than `current`.
///
/// Compared as numbers a part at a time, because `0.9.0` is **older** than `0.34.2` and every
/// comparison of the two as text says otherwise. A part that is not a number stops the comparison
/// and answers no: a tag like `v1.0.0-rc1` is not something this version knows how to be sure about,
/// and telling somebody there is an update when there may not be is worse than saying nothing.
pub fn is_newer(current: &str, found: &str) -> bool {
    let parts = |version: &str| -> Option<Vec<u64>> {
        version.split('.').map(|part| part.trim().parse::<u64>().ok()).collect()
    };
    let (Some(current), Some(found)) = (parts(current), parts(found)) else {
        return false;
    };
    for index in 0..current.len().max(found.len()) {
        let (here, there) =
            (current.get(index).copied().unwrap_or(0), found.get(index).copied().unwrap_or(0));
        if there != here {
            return there > here;
        }
    }
    false
}

/// A `Sender` that nothing reads, for a caller that wants the shape and not the answer.
pub type Answers = Sender<Answer>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_compared_as_numbers_rather_than_as_text() {
        assert!(is_newer("0.34.2", "0.35.0"));
        assert!(is_newer("0.34.2", "0.34.3"));
        assert!(is_newer("0.34.2", "1.0.0"));
        assert!(!is_newer("0.34.2", "0.34.2"));
        assert!(!is_newer("0.34.2", "0.34.1"));
        // The comparison this exists for: as text, "0.9.0" sorts after "0.34.2".
        assert!(!is_newer("0.34.2", "0.9.0"));
        assert!(is_newer("0.9.0", "0.34.2"));
        // Shorter is not smaller: 1.0 and 1.0.0 are the same version.
        assert!(!is_newer("1.0.0", "1.0"));
        assert!(!is_newer("1.0", "1.0.0"));
    }

    #[test]
    fn a_tag_this_version_cannot_be_sure_about_is_not_an_update() {
        assert!(!is_newer("0.34.2", "1.0.0-rc1"));
        assert!(!is_newer("0.34.2", "nightly"));
        assert!(!is_newer("0.34.2", ""));
    }

    #[test]
    fn a_release_is_read_out_of_what_github_sends() {
        let body = r#"{
            "tag_name": "v0.35.0",
            "html_url": "https://github.com/Black-Rainbow-Labs/Unluminous/releases/tag/v0.35.0",
            "body": "\n\nFind and Replace, and the keystroke at 2 MB\n\nWindows: download the setup below."
        }"#;
        let release = read(body).expect("it reads");
        assert_eq!(release.version, "0.35.0", "the v comes off");
        assert_eq!(
            release.url,
            "https://github.com/Black-Rainbow-Labs/Unluminous/releases/tag/v0.35.0"
        );
        assert_eq!(
            release.notes, "Find and Replace, and the keystroke at 2 MB",
            "the first line that says something, not the download instructions"
        );
    }

    /// The other shape: what unluminous.com's own manifest calls the same three things.
    ///
    /// `task-1993`. The site is asked first and GitHub only when it does not answer, so if these two
    /// needed two readers the one that mattered on any given day would be whichever had not been
    /// exercised.
    #[test]
    fn a_release_is_read_out_of_what_the_site_sends() {
        let body = r#"{
            "product": "Unluminous",
            "version": "0.52.0",
            "url": "https://unluminous.com/#install",
            "notes": "The backdrop records how it was made",
            "installer": "https://unluminous.com/downloads/UnluminousSetup-0.52.0-x64.exe",
            "installerBytes": 12846768,
            "installerSha256": "68150e72ed969ad3"
        }"#;
        let release = read(body).expect("it reads");
        assert_eq!(release.version, "0.52.0");
        assert_eq!(release.url, "https://unluminous.com/#install");
        assert_eq!(release.notes, "The backdrop records how it was made");
    }

    /// A manifest whose version carries a `v` reads the same as one that does not.
    #[test]
    fn the_v_comes_off_whichever_name_the_version_arrived_under() {
        assert_eq!(read(r#"{"version": "v1.2.3"}"#).expect("it reads").version, "1.2.3");
        assert_eq!(read(r#"{"tag_name": "1.2.3"}"#).expect("it reads").version, "1.2.3");
    }

    /// A source behind the window asking it does not get to call its version the newest there is.
    ///
    /// The site is published a step after the release, so for the minutes in between somebody is
    /// running something later than anything published — and a window that then said "0.52.0 is the
    /// newest there is" to a person on 0.53.0 would be stating a falsehood in the shape of an answer.
    #[test]
    fn a_source_behind_this_window_is_not_called_the_newest_there_is() {
        let behind = Answer::Current("0.1.0".to_owned()).sentence();
        assert!(behind.contains(crate::build_info::VERSION), "{behind}");
        assert!(!behind.contains("0.1.0 is the newest there is"), "{behind}");
        let level = Answer::Current(crate::build_info::VERSION.to_owned()).sentence();
        assert!(level.ends_with("is the newest there is."), "{level}");
    }

    #[test]
    fn a_host_is_read_out_of_an_address() {
        assert_eq!(host_of(SITE), "unluminous.com");
        assert_eq!(host_of(GITHUB), "api.github.com");
        assert_eq!(host_of("http://127.0.0.1:52341/releases/latest"), "127.0.0.1:52341");
    }

    /// The whole reason there are two: the first one is down and the answer still arrives.
    #[test]
    fn a_source_that_fails_falls_through_to_the_next_one() {
        let newer = r#"{"version": "999.0.0", "notes": "from the second source"}"#;
        let sources = vec!["http://127.0.0.1:1/releases".to_owned(), scripted(200, newer)];
        match ask_each(&sources, TIMEOUT) {
            Answer::Newer(release) => {
                assert_eq!(release.version, "999.0.0");
                assert_eq!(release.notes, "from the second source");
            }
            other => panic!("expected the second source to answer, got {other:?}"),
        }
    }

    /// And the reason only a *failure* falls through: "this is the newest" is an answer.
    ///
    /// A fallback asked after it would be asking a second opinion about a settled question, and
    /// would give a different one for as long as the site was deployed and the GitHub release was
    /// not.
    #[test]
    fn a_source_that_says_this_is_the_newest_is_not_asked_again_elsewhere() {
        let current = format!(r#"{{"version": "{}"}}"#, crate::build_info::VERSION);
        let later = r#"{"version": "999.0.0"}"#;
        let sources = vec![scripted(200, &current), scripted(200, later)];
        match ask_each(&sources, TIMEOUT) {
            Answer::Current(version) => assert_eq!(version, crate::build_info::VERSION),
            other => panic!("the first answer should have stood, got {other:?}"),
        }
    }

    /// Every source failing names each host and what it said, because which one was down is the
    /// whole of what somebody can act on.
    #[test]
    fn every_source_failing_names_each_one() {
        let sources = vec![
            "http://127.0.0.1:1/releases".to_owned(),
            scripted(403, r#"{"message":"rate limited"}"#),
        ];
        match ask_each(&sources, TIMEOUT) {
            Answer::Failed(said) => {
                assert!(said.contains("127.0.0.1:1"), "{said}");
                assert!(said.contains("rate limited"), "{said}");
                assert!(said.contains(';'), "both are reported, not just the last: {said}");
            }
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    /// The addresses a released Unluminous asks, and the order it asks them in.
    ///
    /// Pinned because the fault this file was rewritten for was an address: both constants named a
    /// private repository, which answers 404 to everybody who is not its owner, so the check could
    /// not once have succeeded for a person who installed Unluminous from the site.
    #[test]
    fn the_addresses_asked_are_public_ones_and_the_site_is_asked_first() {
        assert_eq!(
            SOURCES,
            [STABLE, SITE, GITHUB],
            "the stable manifest, then latest.json, then GitHub"
        );
        assert_eq!(NIGHTLY_SOURCES, [NIGHTLY], "a nightly is never answered by GitHub's latest");
        for address in [STABLE, NIGHTLY, SITE, GITHUB, RELEASES_PAGE] {
            assert!(
                !address.contains("jasonmcaffee"),
                "{address} names the private repository, which is 404 anonymously"
            );
        }
        assert!(RELEASES_PAGE.starts_with("https://unluminous.com/"));
    }

    /// A list in `UNLUMINOUS_RELEASES` is the ordering a test drives; one address is still one.
    ///
    /// The variable itself is not set here: it is process-wide and these tests run in parallel, so
    /// what is asserted is the reading rather than the environment.
    #[test]
    fn the_environment_may_name_one_source_or_several() {
        assert_eq!(sources_from("http://a/x"), vec!["http://a/x".to_owned()]);
        assert_eq!(
            sources_from(" http://a/x , http://b/y "),
            vec!["http://a/x".to_owned(), "http://b/y".to_owned()]
        );
        assert_eq!(sources_from(" , "), Vec::<String>::new(), "nothing named is nowhere to ask");
    }

    /// With nothing named, a check asks [`SOURCES`] — which is what a released Unluminous does.
    #[test]
    fn nothing_in_the_environment_means_the_real_sources() {
        if ["UNLUMINOUS_RELEASES", "UNLUMINOUS_RELEASES_STABLE", "UNLUMINOUS_RELEASES_NIGHTLY"]
            .iter()
            .any(|name| std::env::var(name).is_ok())
        {
            return;
        }
        assert_eq!(releases_endpoints(Channel::Stable), SOURCES.to_vec());
        assert_eq!(releases_endpoints(Channel::Nightly), NIGHTLY_SOURCES.to_vec());
    }

    #[test]
    fn a_channel_is_read_back_from_its_name() {
        for channel in Channel::ALL {
            assert_eq!(Channel::from_name(channel.name()), Some(channel));
        }
        assert_eq!(Channel::from_name(" Nightly "), Some(Channel::Nightly));
        assert_eq!(Channel::from_name("beta"), None);
        assert_eq!(Channel::default(), Channel::Stable, "a window follows stable unless told");
    }

    /// A channel manifest: the newest at the top level, as `latest.json` has it, and the list beside it.
    fn channel_manifest(versions: &[&str]) -> String {
        let entry = |version: &str| {
            serde_json::json!({
                "version": version,
                "notes": format!("notes for {version}"),
                "installer": format!("https://unluminous.com/downloads/UnluminousSetup-{version}-x64.exe"),
                "installerBytes": 100,
                "installerSha256": "a".repeat(64),
                "macos": format!("https://unluminous.com/downloads/Unluminous-{version}-macos.zip"),
                "macosBytes": 200,
                "macosSha256": "b".repeat(64),
            })
        };
        let mut top = entry(versions[0]);
        top["channel"] = serde_json::json!("nightly");
        top["versions"] = versions.iter().map(|version| entry(version)).collect();
        top.to_string()
    }

    #[test]
    fn a_channel_manifest_lists_its_versions_newest_first() {
        let body = channel_manifest(&["1.9.2", "1.9.1", "1.9.0"]);
        let found = versions_in(&body).expect("it reads");
        let names: Vec<&str> = found.iter().map(|release| release.version.as_str()).collect();
        assert_eq!(names, ["1.9.2", "1.9.1", "1.9.0"]);
        assert_eq!(found[1].notes, "notes for 1.9.1");
        if PLATFORM_FIELD.is_some() {
            assert!(
                found.iter().all(|release| release.download.is_some()),
                "each has its installer"
            );
        }
        // And the top level is still what an older Unluminous reads.
        assert_eq!(read(&body).expect("it reads").version, "1.9.2");
    }

    #[test]
    fn a_manifest_with_no_list_is_a_list_of_one() {
        let found = versions_in(r#"{"version": "1.8.3"}"#).expect("it reads");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].version, "1.8.3");
        assert_eq!(versions_in("{}"), None);
        assert_eq!(versions_in(r#"{"versions": []}"#), None);
    }

    #[test]
    fn a_version_listed_twice_is_kept_once() {
        let found = versions_in(&channel_manifest(&["1.9.1", "1.9.1", "1.9.0"])).expect("it reads");
        assert_eq!(found.len(), 2);
    }

    /// The stable list while the site has not been rebuilt: `stable.json` is 404 and `latest.json`
    /// answers with one version.
    #[test]
    fn a_stable_list_falls_through_to_latest_json() {
        let sources =
            vec![scripted(404, "<html>Not Found</html>"), scripted(200, r#"{"version": "1.8.3"}"#)];
        let found = list_each(&sources, TIMEOUT).expect("the second answers");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].version, "1.8.3");
    }

    #[test]
    fn a_list_nobody_answers_names_each_source() {
        let sources = vec!["http://127.0.0.1:1/x".to_owned(), scripted(404, "nope")];
        let refused = list_each(&sources, TIMEOUT).expect_err("nobody answers");
        assert!(refused.contains("127.0.0.1:1") && refused.contains("404"), "{refused}");
    }

    #[test]
    fn both_lists_are_fetched_and_a_version_is_found_by_name() {
        let versions = Versions {
            stable: versions_in(&channel_manifest(&["1.8.3", "1.8.2"])).ok_or_else(String::new),
            nightly: Err("down".to_owned()),
        };
        assert_eq!(versions.newest(Channel::Stable).map(|r| r.version.as_str()), Some("1.8.3"));
        assert!(versions.find(Channel::Stable, "1.8.2").is_some());
        assert!(versions.find(Channel::Stable, "1.0.0").is_none());
        assert!(versions.newest(Channel::Nightly).is_none());
    }

    #[test]
    fn the_installer_for_this_platform_is_read_with_its_size_and_hash() {
        let body = r#"{
            "version": "0.55.0",
            "installer": "https://unluminous.com/downloads/UnluminousSetup-0.55.0-x64.exe",
            "installerBytes": 12875680,
            "installerSha256": "66885379D68932BD94EAF6DA8D4B76CD35A7E49A0D4239C328F7B78A6C5173E6",
            "macos": "https://unluminous.com/downloads/Unluminous-0.55.0-macos.zip",
            "macosBytes": 33118584,
            "macosSha256": "d6507d0d07f747c8cd08b9739972cc132fe091978adec9e921e7a0a65450de74"
        }"#;
        let value: serde_json::Value = serde_json::from_str(body).expect("json");
        let windows = download_in(&value, Some("installer")).expect("the Windows one");
        assert_eq!(windows.bytes, 12875680);
        assert!(windows.sha256.starts_with("66885379d6"), "lower case: {}", windows.sha256);
        let macos = download_in(&value, Some("macos")).expect("the macOS one");
        assert!(macos.url.ends_with("macos.zip"));
        assert_eq!(download_in(&value, None), None, "a platform with no installer");
        // Whatever this platform is, `read` asks the same question.
        assert_eq!(read(body).expect("it reads").download, download_in(&value, PLATFORM_FIELD));
    }

    #[test]
    fn an_installer_that_cannot_be_checked_is_not_offered() {
        let missing_hash =
            serde_json::json!({ "installer": "https://x/a.exe", "installerBytes": 3 });
        assert_eq!(download_in(&missing_hash, Some("installer")), None);
        let short_hash = serde_json::json!({
            "installer": "https://x/a.exe", "installerBytes": 3, "installerSha256": "abc"
        });
        assert_eq!(download_in(&short_hash, Some("installer")), None);
        let plain_http = serde_json::json!({
            "installer": "http://x/a.exe", "installerBytes": 3, "installerSha256": "a".repeat(64)
        });
        assert_eq!(download_in(&plain_http, Some("installer")), None, "only https");
        let loopback = serde_json::json!({
            "installer": "http://127.0.0.1:5000/a.exe", "installerBytes": 3, "installerSha256": "a".repeat(64)
        });
        assert!(
            download_in(&loopback, Some("installer")).is_some(),
            "a test's own server on loopback"
        );
    }

    #[test]
    fn a_daily_check_is_due_once_a_day() {
        let day = DAY.as_secs();
        assert!(is_due(None, 1_000), "never asked");
        assert!(!is_due(Some(1_000), 1_000 + day - 1));
        assert!(is_due(Some(1_000), 1_000 + day));
        assert!(is_due(Some(5_000), 1_000), "a clock put back does not stop the checks for years");
    }

    #[test]
    fn when_a_check_was_asked_is_one_file_every_window_reads() {
        let folder = std::env::temp_dir().join(format!("unluminous-update-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");
        assert_eq!(last_checked(&folder), None);
        note_checked(&folder, 1234);
        assert_eq!(last_checked(&folder), Some(1234));
        let _ = std::fs::remove_file(folder.join(CHECKED));
        let _ = std::fs::remove_dir(&folder);
    }

    #[test]
    fn a_reply_with_no_tag_in_it_is_not_a_release() {
        assert_eq!(read("{}"), None);
        assert_eq!(read(r#"{"tag_name": ""}"#), None);
        assert_eq!(read("not json at all"), None);
        assert_eq!(read(r#"{"message": "Not Found"}"#), None);
    }

    #[test]
    fn a_release_with_no_notes_and_no_url_still_answers() {
        let release = read(r#"{"tag_name": "v1.2.3"}"#).expect("it reads");
        assert_eq!(release.version, "1.2.3");
        assert_eq!(release.url, RELEASES_PAGE, "the releases page, when the release names none");
        assert_eq!(release.notes, "");
    }

    /// The whole of it, end to end, against a scripted server on loopback.
    ///
    /// `unluminous-chat`'s arrangement: a `TcpListener` on `127.0.0.1:0` replaying fixed bytes, which
    /// is what makes "the client, end to end" a unit test with no network and nothing to be flaky
    /// about.
    #[test]
    fn a_newer_release_on_a_scripted_server_is_read_as_an_update() {
        let body = r#"{"tag_name": "v999.0.0", "html_url": "https://example.invalid/999", "body": "A much later one"}"#
            .to_string();
        let url = scripted(200, &body);
        match ask_at(&url) {
            Answer::Newer(release) => {
                assert_eq!(release.version, "999.0.0");
                assert_eq!(release.notes, "A much later one");
            }
            other => panic!("expected an update, got {other:?}"),
        }
    }

    #[test]
    fn the_version_this_is_reads_as_the_newest_there_is() {
        let body = format!(r#"{{"tag_name": "v{}"}}"#, crate::build_info::VERSION);
        match ask_at(&scripted(200, &body)) {
            Answer::Current(version) => assert_eq!(version, crate::build_info::VERSION),
            other => panic!("expected current, got {other:?}"),
        }
    }

    /// A refusal is quoted in the server's own words rather than replaced by one made up here.
    #[test]
    fn a_rate_limit_is_reported_in_githubs_own_words() {
        let body = r#"{"message": "API rate limit exceeded for 203.0.113.1."}"#;
        match ask_at(&scripted(403, body)) {
            Answer::Failed(said) => assert!(said.contains("rate limit"), "{said}"),
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn a_server_that_is_not_there_is_a_failure_rather_than_a_panic() {
        // Port 1 on loopback, which nothing listens on.
        match ask_at("http://127.0.0.1:1/releases") {
            Answer::Failed(_) => {}
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    /// One request, answered with `status` and `body`, and the address to ask it at.
    fn scripted(status: u16, body: &str) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let address = listener.local_addr().expect("the address");
        let body = body.to_owned();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            // Enough of the request to know it arrived. The client is not reading yet, so this must
            // not block on a body that is never sent.
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer);
            let reply = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
        });
        format!("http://{address}/releases/latest")
    }
}
