//! An AI agent that runs in a pane, as Claude Code, Codex or Gemini CLI:
//! its state, at work, waiting for the user, or done with what it was
//! asked, as what its program does tells it, of no hook of the agent's own.
//!
//! - **At work** while it prints of its own, output that came apart from
//!   the echo of the user's keys and the redraw of a new size, while it
//!   tells a progress of OSC 9;4 that runs, and while its title spins, the
//!   glyph before its text turning as a spinner's frames, as agents show
//!   their work in a tab's title.
//! - **Waiting** once it asks for the user, by a notification of OSC 9 or
//!   OSC 777, or its bell, as an agent asks for a permission or tells that
//!   it waits: until the user types into its pane, whatever it prints
//!   meanwhile, as its prompt's redraw.
//! - **Done** once it showed no work for a while, the config's, after the
//!   user asked it something: its first output, as it starts, asked for
//!   nothing. It stays done until it works again, the user types into its
//!   pane, or its tab shows anew, as seen.
//!
//! An agent whose hooks tell its state, as notifications of OSC 777 of a
//! title of [`HOOKED`], is told exactly instead: at work from the user's
//! request to its end, done or failed at its end, waiting as it asks; its
//! output no longer tells its work, nor its quiet its end, nor its bell and
//! plain notifications its asking. But a key typed into it while it works
//! or waits, as an Escape that interrupts it, which its hooks do not tell,
//! lets its output and its quiet tell its state again until its next hook.

use std::time::{Duration, Instant};

use lokyr_plugin_sdk::host::AgentState;
use lokyr_plugin_sdk::text::is_hidden;

/// How long after its last sign of work an agent that asked for nothing
/// is done, unless the config gives another.
pub const DONE_AFTER: Duration = Duration::from_secs(3);

/// The start of the title of a notification of OSC 777 by which an agent's
/// hooks tell its state, as `lokyr-agent:done`, of which the app shows no
/// notification.
pub const HOOKED: &str = "lokyr-agent:";

/// A state that an agent's hooks tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hooked {
    /// The user asked it something, at its request's submission.
    Working,
    /// It asks for the user, as for a permission.
    Waiting,
    /// It ended its answer.
    Done,
    /// Its answer ended on an error, as its service's.
    Error,
}

impl Hooked {
    /// The state of a notification's `title`, `lokyr-agent:<state>`, if it
    /// is one of an agent's hooks.
    pub fn of(title: &str) -> Option<Self> {
        match title.strip_prefix(HOOKED)? {
            "working" => Some(Self::Working),
            "waiting" => Some(Self::Waiting),
            "done" => Some(Self::Done),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// The characters of an agent's message that it keeps, its start.
pub const MESSAGE: usize = 200;

/// The agents' settings, as the config gives them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentSettings {
    /// How long after its last sign of work an agent is done.
    pub done_after: Duration,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            done_after: DONE_AFTER,
        }
    }
}

/// An agent in a pane, by its program's name, and its state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agent {
    name: String,
    state: Option<AgentState>,
    /// Its latest sign of work, which a sign of before it does not move.
    latest: Option<Instant>,
    /// Whether the user typed into its pane since it started, so that it
    /// had something to do.
    asked: bool,
    /// The message of the notification by which it asked for the user,
    /// while it waits.
    message: Option<String>,
    /// When its state last changed, or it started.
    since: Instant,
    /// Whether its hooks tell its state.
    hooked: bool,
    /// Whether its output and its quiet tell its state meanwhile, after a
    /// key typed into it while its hooks told it at work or waiting.
    guessing: bool,
}

impl Agent {
    /// The agent of the program `name`, which shows no state until it
    /// works or asks.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            state: None,
            latest: None,
            asked: false,
            message: None,
            since: Instant::now(),
            hooked: false,
            guessing: false,
        }
    }

    /// Its program's name, as the config lists it.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its state, none before it works or asks, and once what it did or
    /// asked was seen.
    pub fn state(&self) -> Option<AgentState> {
        self.state
    }

    /// When its state last changed, or, of none since, it started.
    pub fn since(&self) -> Instant {
        self.since
    }

    /// Its state changed at `at`.
    pub(super) fn changed_at(&mut self, at: Instant) {
        self.since = at;
    }

    /// The message of the notification by which it asked for the user, as
    /// "Claude needs your permission to use Bash", while it waits: none of
    /// its bell alone.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// A sign of its work at `at`: output of its own, a progress that
    /// runs, its title's spinner; one no later than the latest tells
    /// nothing new. Waiting holds until the user answers.
    pub fn worked(&mut self, at: Instant) {
        if (self.hooked && !self.guessing)
            || self.state == Some(AgentState::Waiting)
            || self.latest.is_some_and(|latest| at <= latest)
        {
            return;
        }
        self.latest = Some(at);
        self.state = Some(AgentState::Working);
    }

    /// It asked for the user, by a notification of `message`, which takes
    /// the place of an earlier one's, or by its bell, which keeps it.
    pub fn asked_for_user(&mut self, message: Option<&str>) {
        self.state = Some(AgentState::Waiting);
        if let Some(message) = message.filter(|message| !message.is_empty()) {
            let kept: String = message
                .chars()
                .filter(|&character| !character.is_control() && !is_hidden(character))
                .take(MESSAGE)
                .collect();
            self.message = Some(kept);
        }
    }

    /// Its hooks told `state`, of `message`, its request's or its error's:
    /// from now on they alone tell its work and its end.
    pub fn hooked(&mut self, state: Hooked, message: Option<&str>) {
        self.hooked = true;
        self.guessing = false;
        self.latest = None;
        match state {
            Hooked::Working => {
                self.asked = true;
                self.message = None;
                self.state = Some(AgentState::Working);
            }
            Hooked::Waiting => self.asked_for_user(message),
            Hooked::Done => {
                self.message = None;
                self.state = Some(AgentState::Done);
            }
            Hooked::Error => {
                self.message = None;
                self.asked_for_user(message);
                self.state = Some(AgentState::Error);
            }
        }
    }

    /// Whether its hooks tell its state.
    pub fn is_hooked(&self) -> bool {
        self.hooked
    }

    /// The user typed into its pane at `at`: what it waited for is
    /// answered, and what it did is seen; it works on.
    pub fn typed(&mut self, at: Instant) {
        self.asked = true;
        self.message = None;
        self.state = match self.state {
            // Answered, or interrupted, as an Escape ends its turn of no
            // hook: it works on while its output tells so, done once quiet,
            // until its next hook.
            Some(AgentState::Waiting | AgentState::Working) if self.hooked => {
                self.guessing = true;
                self.latest = Some(at);
                Some(AgentState::Working)
            }
            Some(AgentState::Waiting | AgentState::Done | AgentState::Error) => None,
            state => state,
        };
    }

    /// Its tab showed anew: what it did is seen.
    pub fn seen(&mut self) {
        if self.state == Some(AgentState::Done) {
            self.state = None;
        }
    }

    /// When it is done unless it works again, as `settings` time it: none
    /// while it does not work.
    pub fn due(&self, settings: AgentSettings) -> Option<Instant> {
        let latest = self.latest.filter(|_| {
            (!self.hooked || self.guessing) && self.state == Some(AgentState::Working)
        })?;
        Some(latest + settings.done_after)
    }

    /// Looks at it at `now`, as `settings` time it, while its progress
    /// runs or not, `busy`: done once its time is up after a request of the
    /// user's, which it answered, so that its later output of its own, which
    /// nobody asked for, is done of nothing; of no state after its own
    /// start's output; at work still while its progress runs.
    pub fn look(&mut self, now: Instant, settings: AgentSettings, busy: bool) {
        let Some(due) = self.due(settings) else {
            return;
        };
        if busy {
            self.latest = Some(now);
        } else if due <= now {
            self.state = self.asked.then_some(AgentState::Done);
            self.asked = false;
            self.guessing = false;
        }
    }
}

/// Whether a title of `before` that became `after` spins: the glyph before
/// its text, not a letter nor a digit, turned and its text stayed, as a
/// spinner's frames turn; or it begins with one of Braille's patterns, as
/// the frames of the spinners of agents' titles do.
pub fn spins(before: &str, after: &str) -> bool {
    let braille = |character: char| ('\u{2800}'..='\u{28ff}').contains(&character);
    let mut new = after.chars();
    let Some(first) = new.next() else {
        return false;
    };
    if braille(first) && before != after {
        return true;
    }
    let mut old = before.chars();
    let Some(was) = old.next() else {
        return false;
    };
    was != first
        && !first.is_alphanumeric()
        && !was.is_alphanumeric()
        && !first.is_whitespace()
        && old.as_str() == new.as_str()
        && !new.as_str().trim().is_empty()
}

/// The state that a tab shows of its panes' agents' `states`: waiting
/// before an error, an error before work, and work before done.
pub fn most_urgent(states: impl IntoIterator<Item = Option<AgentState>>) -> Option<AgentState> {
    let rank = |state: AgentState| match state {
        AgentState::Waiting => 4,
        AgentState::Error => 3,
        AgentState::Working => 2,
        AgentState::Done => 1,
    };
    states
        .into_iter()
        .flatten()
        .max_by_key(|&state| rank(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: AgentSettings = AgentSettings {
        done_after: DONE_AFTER,
    };

    #[test]
    fn an_agent_works_then_is_done_once_quiet_after_a_request() {
        let start = Instant::now();
        let mut agent = Agent::new("claude");
        assert_eq!((agent.name(), agent.state()), ("claude", None));
        // Its start's output asked for nothing: no state once quiet.
        agent.worked(start);
        assert_eq!(agent.state(), Some(AgentState::Working));
        assert_eq!(agent.due(SETTINGS), Some(start + DONE_AFTER));
        agent.look(start + DONE_AFTER, SETTINGS, false);
        assert_eq!(agent.state(), None);
        // Asked: at work, still within its time, done past it.
        agent.typed(start);
        let asked = start + Duration::from_secs(10);
        agent.worked(asked);
        agent.worked(asked + Duration::from_secs(1));
        agent.look(asked + DONE_AFTER, SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Working));
        agent.look(asked + Duration::from_secs(4), SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Done));
        assert_eq!(agent.due(SETTINGS), None);
        // A sign of before its latest tells nothing new.
        agent.worked(asked);
        assert_eq!(agent.state(), Some(AgentState::Done));
        // Seen as its tab shows, or as the user types.
        agent.seen();
        assert_eq!(agent.state(), None);
        agent.typed(asked + Duration::from_secs(19));
        agent.worked(asked + Duration::from_secs(20));
        agent.look(asked + Duration::from_secs(30), SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Done));
        agent.typed(start);
        assert_eq!(agent.state(), None);
    }

    #[test]
    fn an_agent_that_asks_waits_until_the_user_types_whatever_it_prints() {
        let start = Instant::now();
        let mut agent = Agent::new("codex");
        agent.typed(start);
        agent.worked(start);
        agent.asked_for_user(None);
        assert_eq!(agent.state(), Some(AgentState::Waiting));
        assert_eq!(agent.due(SETTINGS), None);
        // Its prompt's redraw changes nothing, nor its time.
        agent.worked(start + Duration::from_secs(1));
        agent.look(start + Duration::from_secs(60), SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Waiting));
        // Its tab shown: it still waits.
        agent.seen();
        assert_eq!(agent.state(), Some(AgentState::Waiting));
        // Answered: it works on, its message gone.
        agent.typed(start);
        assert_eq!((agent.state(), agent.message()), (None, None));
        agent.worked(start + Duration::from_secs(61));
        assert_eq!(agent.state(), Some(AgentState::Working));
    }

    #[test]
    fn an_agent_that_waits_keeps_its_last_message() {
        let mut agent = Agent::new("claude");
        assert_eq!(agent.message(), None);
        agent.asked_for_user(Some("Claude needs your permission to use Bash"));
        assert_eq!(
            agent.message(),
            Some("Claude needs your permission to use Bash")
        );
        // Its bell keeps it; another notification takes its place.
        agent.asked_for_user(None);
        assert_eq!(
            agent.message(),
            Some("Claude needs your permission to use Bash")
        );
        agent.asked_for_user(Some("Claude is waiting\u{7} for your input"));
        assert_eq!(agent.message(), Some("Claude is waiting for your input"));
        // Its start alone, of no control character.
        agent.asked_for_user(Some(&"x".repeat(MESSAGE + 10)));
        assert_eq!(agent.message().map(str::len), Some(MESSAGE));
        // Seen as its tab shows, it still waits, of its message.
        agent.seen();
        assert!(agent.message().is_some());
        agent.typed(Instant::now());
        assert_eq!(agent.message(), None);
        // A message of a change of direction and of a character of no
        // width, which would show otherwise than it is.
        agent.asked_for_user(Some("run \u{202E}hsab\u{202C} now\u{200B}?"));
        assert_eq!(agent.message(), Some("run hsab now?"));
    }

    #[test]
    fn a_hooked_agent_interrupted_by_a_key_is_done_once_quiet() {
        let start = Instant::now();
        let mut agent = Agent::new("claude");
        agent.hooked(Hooked::Working, None);
        // An Escape while it works, which its hooks do not tell: its output
        // keeps it at work, its quiet makes it done.
        agent.typed(start);
        assert_eq!(agent.state(), Some(AgentState::Working));
        agent.worked(start + Duration::from_secs(2));
        agent.look(start + Duration::from_secs(4), SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Working));
        agent.look(start + Duration::from_secs(6), SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Done));
        // Done, its hooks tell its state alone again.
        agent.worked(start + Duration::from_secs(7));
        assert_eq!(agent.state(), Some(AgentState::Done));
        // A permission refused by an Escape, alike.
        agent.hooked(Hooked::Working, None);
        agent.hooked(Hooked::Waiting, Some("Claude needs your permission"));
        let later = start + Duration::from_secs(10);
        agent.typed(later);
        assert_eq!(agent.state(), Some(AgentState::Working));
        agent.look(later + DONE_AFTER, SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Done));
        // A hook meanwhile tells its state again.
        agent.hooked(Hooked::Working, None);
        agent.typed(later);
        agent.hooked(Hooked::Done, None);
        assert_eq!(agent.due(SETTINGS), None);
    }

    #[test]
    fn an_agent_done_is_not_done_again_of_output_that_nobody_asked_for() {
        let start = Instant::now();
        let mut agent = Agent::new("claude");
        agent.typed(start);
        agent.worked(start + Duration::from_secs(1));
        agent.look(start + Duration::from_secs(5), SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Done));
        agent.seen();
        // Output of its own, later, of no key: no state once quiet.
        agent.worked(start + Duration::from_secs(10));
        agent.look(start + Duration::from_secs(14), SETTINGS, false);
        assert_eq!(agent.state(), None);
        // A key asks again.
        agent.typed(start + Duration::from_secs(15));
        agent.worked(start + Duration::from_secs(16));
        agent.look(start + Duration::from_secs(20), SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Done));
    }

    #[test]
    fn an_agent_s_hooks_tell_its_state_alone() {
        assert_eq!(Hooked::of("lokyr-agent:done"), Some(Hooked::Done));
        assert_eq!(Hooked::of("lokyr-agent:waiting"), Some(Hooked::Waiting));
        assert_eq!(Hooked::of("lokyr-agent:other"), None);
        assert_eq!(Hooked::of("Claude Code"), None);
        let start = Instant::now();
        let mut agent = Agent::new("claude");
        agent.hooked(Hooked::Working, None);
        assert!(agent.is_hooked());
        assert_eq!(agent.state(), Some(AgentState::Working));
        // No quiet ends it, nor output tells its work.
        assert_eq!(agent.due(SETTINGS), None);
        agent.look(start + Duration::from_secs(60), SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Working));
        // A permission: waiting, of its message; answered, it works on.
        agent.hooked(
            Hooked::Waiting,
            Some("Claude needs your permission to use Bash"),
        );
        assert_eq!(agent.state(), Some(AgentState::Waiting));
        assert!(agent.message().is_some());
        agent.typed(start);
        assert_eq!(
            (agent.state(), agent.message()),
            (Some(AgentState::Working), None)
        );
        // Its end, after which its prompt's redraw tells nothing.
        agent.hooked(Hooked::Done, None);
        agent.worked(start + Duration::from_secs(70));
        assert_eq!(agent.state(), Some(AgentState::Done));
        // A new request, then an error, which a key clears.
        agent.typed(start);
        assert_eq!(agent.state(), None);
        agent.hooked(Hooked::Working, None);
        agent.hooked(Hooked::Error, Some("rate_limit"));
        assert_eq!(
            (agent.state(), agent.message()),
            (Some(AgentState::Error), Some("rate_limit"))
        );
        agent.typed(start);
        assert_eq!(agent.state(), None);
    }

    #[test]
    fn a_progress_that_runs_keeps_an_agent_at_work() {
        let start = Instant::now();
        let mut agent = Agent::new("gemini");
        agent.typed(start);
        agent.worked(start);
        agent.look(start + DONE_AFTER, SETTINGS, true);
        assert_eq!(agent.state(), Some(AgentState::Working));
        assert_eq!(agent.due(SETTINGS), Some(start + DONE_AFTER * 2));
        agent.look(start + DONE_AFTER * 2, SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Done));
        // Of the config's time.
        let slow = AgentSettings {
            done_after: Duration::from_secs(10),
        };
        agent.typed(start + Duration::from_secs(19));
        agent.worked(start + Duration::from_secs(20));
        agent.look(start + Duration::from_secs(25), slow, false);
        assert_eq!(agent.state(), Some(AgentState::Working));
        agent.look(start + Duration::from_secs(30), slow, false);
        assert_eq!(agent.state(), Some(AgentState::Done));
    }

    #[test]
    fn a_title_spins_as_its_first_glyph_turns_and_its_text_stays() {
        // Braille's frames, and another spinner's.
        assert!(spins("⠋ Fixing the parser", "⠙ Fixing the parser"));
        assert!(spins("", "⠐ Thinking"));
        assert!(spins("✶ Fixing the parser", "✻ Fixing the parser"));
        assert!(spins("⠐ Fixing the parser", "✳ Fixing the parser"));
        // Its text that changed, a letter, a word, or nothing new.
        assert!(!spins("✳ Fixing the parser", "✳ Writing tests"));
        assert!(!spins("a title", "b title"));
        assert!(!spins("vim", "zsh"));
        assert!(!spins("⠋ Fixing", "⠋ Fixing"));
        assert!(!spins("* ", "+ "));
        assert!(!spins("", ""));
    }

    #[test]
    fn a_tab_shows_its_most_urgent_agent() {
        use AgentState::{Done, Waiting, Working};
        assert_eq!(
            most_urgent([None, Some(Done), Some(Working)]),
            Some(Working)
        );
        assert_eq!(most_urgent([Some(Waiting), Some(Working)]), Some(Waiting));
        assert_eq!(most_urgent([Some(Done)]), Some(Done));
        assert_eq!(
            most_urgent([Some(AgentState::Error), Some(Working)]),
            Some(AgentState::Error)
        );
        assert_eq!(most_urgent([None, None]), None);
    }
}
