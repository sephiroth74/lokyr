//! The AI agents in the panes: the app finds each pane's agent by the
//! names that its foreground's programs go by, as `[agents] programs`
//! lists them, once a look finds another group in its foreground; tells
//! the workspace of the keys typed into a pane, which answer an agent that
//! waits; and tells what an agent's state brings, a notification while its
//! window has no focus, unless the agent's own told it, and the Dock's
//! badge of the agents that wait.

use lokyr_kernel::config::AgentsConfig;
use lokyr_kernel::l10n::Arg;
use lokyr_kernel::plugins::host::AgentState;
use lokyr_kernel::workspace::{self, AgentSettings, PaneId};
use lokyr_platform_core::{Platform, WindowId};

use super::{App, runs_other_than_shell};
use crate::drawing::Backend;

impl<B: Backend> App<B> {
    /// Looks at the agent of `pane`, whose foreground's program is
    /// `program`, as its place's look finds it: once the foreground's group
    /// or its program is another than at the last look, as a program that
    /// a shell became, the names that its processes go by, of a program
    /// other than a shell, some microseconds a process; nothing otherwise,
    /// as the place's look comes at each output.
    pub(super) fn look_at_agent(&mut self, pane: PaneId, program: &str) {
        let Some(shown) = self.workspace.pane(pane) else {
            return;
        };
        let group = (shown.foreground_group(), program.to_owned());
        if self.agent_groups.get(&pane) == Some(&group) {
            return;
        }
        let agents = &self.config.agents;
        let agent = (runs_other_than_shell(program) && !agents.programs.is_empty())
            .then(|| {
                agents
                    .agent_of(&shown.foreground_names())
                    .map(str::to_owned)
            })
            .flatten();
        self.agent_groups.insert(pane, group);
        self.workspace.set_pane_agent(pane, agent.as_deref());
    }

    /// Looks at every pane's agent anew, as the agents' programs changed.
    fn look_at_agents_again(&mut self) {
        self.agent_groups.clear();
        let panes: Vec<(PaneId, String)> = self
            .workspace
            .panes()
            .map(|pane| (pane.id(), pane.foreground_program().unwrap_or_default()))
            .collect();
        for (pane, program) in panes {
            self.look_at_agent(pane, &program);
        }
    }

    /// Forgets the looks at the agents of `panes`, which closed.
    pub(super) fn forget_agents(&mut self, panes: &[PaneId]) {
        for pane in panes {
            self.agent_groups.remove(pane);
        }
    }

    /// The user typed into `window`'s pane that has the keyboard, keys, a
    /// text or a paste: what its agent waited for is answered.
    pub(super) fn typed_into(&mut self, window: WindowId) {
        let pane = self
            .windows
            .get(&window)
            .and_then(|state| self.workspace.focused(state.window))
            .map(workspace::Pane::id);
        if let Some(pane) = pane {
            self.workspace.typed(pane);
        }
    }

    /// The agent `name` of `pane` went from `before` to `now`: the Dock's
    /// badge counts the agents that wait, and one that starts waiting, or
    /// is done, tells it in a notification while its window has no focus,
    /// as `[agents]` says, of its tab's title, which a click brings
    /// forward; one that asked for the user by a notification of its own,
    /// `told`, which shows as a program's, is not told twice.
    pub(super) fn agent_changed(
        &mut self,
        platform: &mut dyn Platform,
        pane: PaneId,
        name: &str,
        (before, now): (Option<AgentState>, Option<AgentState>),
        told: bool,
    ) {
        self.show_badge(platform);
        if !self.config.agents.notify {
            return;
        }
        let message = match now {
            Some(AgentState::Waiting) if before != Some(AgentState::Waiting) && !told => {
                "agent-waiting"
            }
            Some(AgentState::Done) => "agent-done",
            _ => return,
        };
        let Some(window) = self.window_of(pane) else {
            return;
        };
        if self.windows.get(&window).is_none_or(|state| state.focused) {
            return;
        }
        let title = self
            .workspace
            .tab_of(pane)
            .and_then(|tab| self.workspace.tab(tab))
            .map(|tab| unspun(tab.title()).to_owned())
            .unwrap_or_default();
        let body = self.texts.text_with(message, &[("agent", Arg::Text(name))]);
        self.notify(platform, pane, &title, &body);
    }

    /// Brings forward the next pane after the one that has the keyboard, in
    /// the windows' order, as their numbers go, their tabs' and their
    /// panes', whose agent waits for the user; or, of none, is done; or, of
    /// none, works: its window, its tab and its pane; nothing of no agent.
    pub(super) fn next_agent(&mut self, platform: &mut dyn Platform) {
        // Every pane, from the one after the focused one, round.
        let mut windows: Vec<(WindowId, workspace::WindowId)> = self
            .windows
            .iter()
            .filter(|(window, _)| !self.quick.contains_key(window))
            .map(|(&window, state)| (window, state.window))
            .collect();
        windows.sort_by_key(|&(_, shown)| {
            self.workspace
                .window(shown)
                .and_then(workspace::Window::number)
                .unwrap_or(u32::MAX)
        });
        let mut panes: Vec<(WindowId, workspace::TabId, PaneId)> = Vec::new();
        for &(window, shown) in &windows {
            let Some(open) = self.workspace.window(shown) else {
                continue;
            };
            for &tab in open.tabs() {
                if let Some(tab) = self.workspace.tab(tab) {
                    panes.extend(
                        tab.layout()
                            .panes()
                            .into_iter()
                            .map(|pane| (window, tab.id(), pane)),
                    );
                }
            }
        }
        let focused = self
            .windows
            .values()
            .find(|state| state.focused)
            .and_then(|state| self.workspace.focused(state.window))
            .map(workspace::Pane::id);
        let start = focused
            .and_then(|focused| panes.iter().position(|&(_, _, pane)| pane == focused))
            .map_or(0, |at| at + 1);
        let state_of = |pane: PaneId| {
            self.workspace
                .pane(pane)
                .and_then(workspace::Pane::agent)
                .and_then(workspace::Agent::state)
        };
        let found = [AgentState::Waiting, AgentState::Done, AgentState::Working]
            .into_iter()
            .find_map(|wanted| {
                (0..panes.len())
                    .map(|step| panes[(start + step) % panes.len()])
                    .find(|&(_, _, pane)| state_of(pane) == Some(wanted))
            });
        let Some((window, tab, pane)) = found else {
            return;
        };
        self.workspace.activate_tab(tab);
        self.workspace.focus_pane(pane);
        platform.focus_window(window);
    }

    /// Shows the count of the agents that wait on the Dock's icon, as
    /// `[agents] dock-badge` says, or none, once it changed.
    pub(super) fn show_badge(&mut self, platform: &mut dyn Platform) {
        let count = if self.config.agents.dock_badge {
            self.workspace.agents_waiting()
        } else {
            0
        };
        if count == self.badge {
            return;
        }
        self.badge = count;
        let label = (count > 0).then(|| count.to_string());
        platform.set_badge(label.as_deref());
    }

    /// Takes `[agents]` as the config gives it anew, after `before`: the
    /// workspace's settings, the panes' agents of other programs, and the
    /// badge.
    pub(super) fn apply_agents(&mut self, platform: &mut dyn Platform, before: &AgentsConfig) {
        let agents = &self.config.agents;
        self.workspace.set_agent_settings(AgentSettings {
            done_after: agents.done_after.duration(),
        });
        if agents.programs != before.programs {
            self.look_at_agents_again();
        }
        self.show_badge(platform);
    }
}

/// `title` without the spinner's glyph before its text, as an agent's tab's
/// title shows its work, for a notification's title.
fn unspun(title: &str) -> &str {
    let mut characters = title.chars();
    match characters.next() {
        Some(first) if !first.is_alphanumeric() && characters.as_str().starts_with(' ') => {
            characters.as_str().trim_start()
        }
        _ => title,
    }
}

#[cfg(test)]
mod tests {
    use super::unspun;

    #[test]
    fn a_notification_s_title_leaves_the_spinner_out() {
        assert_eq!(unspun("✳ Fix the parser"), "Fix the parser");
        assert_eq!(unspun("⠋ Fix the parser"), "Fix the parser");
        assert_eq!(unspun("Fix the parser"), "Fix the parser");
        assert_eq!(unspun("vim"), "vim");
        assert_eq!(unspun(""), "");
    }
}
