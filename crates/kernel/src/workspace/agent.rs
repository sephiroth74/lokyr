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

use std::time::{Duration, Instant};

use lokyr_plugin_sdk::host::AgentState;

/// How long after its last sign of work an agent that asked for nothing
/// is done, unless the config gives another.
pub const DONE_AFTER: Duration = Duration::from_secs(3);

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

    /// A sign of its work at `at`: output of its own, a progress that
    /// runs, its title's spinner; one no later than the latest tells
    /// nothing new. Waiting holds until the user answers.
    pub fn worked(&mut self, at: Instant) {
        if self.state == Some(AgentState::Waiting) || self.latest.is_some_and(|latest| at <= latest)
        {
            return;
        }
        self.latest = Some(at);
        self.state = Some(AgentState::Working);
    }

    /// It asked for the user, by a notification or its bell.
    pub fn asked_for_user(&mut self) {
        self.state = Some(AgentState::Waiting);
    }

    /// The user typed into its pane: what it waited for is answered, and
    /// what it did is seen; it works on.
    pub fn typed(&mut self) {
        self.asked = true;
        if matches!(self.state, Some(AgentState::Waiting | AgentState::Done)) {
            self.state = None;
        }
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
        let latest = self
            .latest
            .filter(|_| self.state == Some(AgentState::Working))?;
        Some(latest + settings.done_after)
    }

    /// Looks at it at `now`, as `settings` time it, while its progress
    /// runs or not, `busy`: done once its time is up after a request of the
    /// user's, or of no state after its own start's output; at work still
    /// while its progress runs.
    pub fn look(&mut self, now: Instant, settings: AgentSettings, busy: bool) {
        let Some(due) = self.due(settings) else {
            return;
        };
        if busy {
            self.latest = Some(now);
        } else if due <= now {
            self.state = self.asked.then_some(AgentState::Done);
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
/// before work, and work before done.
pub fn most_urgent(states: impl IntoIterator<Item = Option<AgentState>>) -> Option<AgentState> {
    let rank = |state: AgentState| match state {
        AgentState::Waiting => 3,
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
        agent.typed();
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
        agent.worked(asked + Duration::from_secs(20));
        agent.look(asked + Duration::from_secs(30), SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Done));
        agent.typed();
        assert_eq!(agent.state(), None);
    }

    #[test]
    fn an_agent_that_asks_waits_until_the_user_types_whatever_it_prints() {
        let start = Instant::now();
        let mut agent = Agent::new("codex");
        agent.typed();
        agent.worked(start);
        agent.asked_for_user();
        assert_eq!(agent.state(), Some(AgentState::Waiting));
        assert_eq!(agent.due(SETTINGS), None);
        // Its prompt's redraw changes nothing, nor its time.
        agent.worked(start + Duration::from_secs(1));
        agent.look(start + Duration::from_secs(60), SETTINGS, false);
        assert_eq!(agent.state(), Some(AgentState::Waiting));
        // Its tab shown: it still waits.
        agent.seen();
        assert_eq!(agent.state(), Some(AgentState::Waiting));
        // Answered: it works on.
        agent.typed();
        assert_eq!(agent.state(), None);
        agent.worked(start + Duration::from_secs(61));
        assert_eq!(agent.state(), Some(AgentState::Working));
    }

    #[test]
    fn a_progress_that_runs_keeps_an_agent_at_work() {
        let start = Instant::now();
        let mut agent = Agent::new("gemini");
        agent.typed();
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
        assert_eq!(most_urgent([None, None]), None);
    }
}
