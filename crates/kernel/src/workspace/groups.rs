//! The groups of a window's tabs: each of a name, a color and whether it is
//! folded, its tabs next to one another in their window, none pinned, one
//! at least. A tab that comes between two tabs of a group joins it; one
//! that leaves its group's run leaves the group, which goes of no tab left,
//! and a group whose every tab went to another window goes with them.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_vt_core::page::Rgb;

use super::{GroupId, TabId, WindowId, Workspace};

/// A group of a window's tabs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    id: GroupId,
    window: WindowId,
    name: String,
    color: Option<Rgb>,
    folded: bool,
}

impl Group {
    /// The group's id.
    pub fn id(&self) -> GroupId {
        self.id
    }

    /// The window of its tabs.
    pub fn window(&self) -> WindowId {
        self.window
    }

    /// Its name, empty for none.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its color, if it has one.
    pub fn color(&self) -> Option<Rgb> {
        self.color
    }

    /// Whether it is folded, its tabs hidden behind its name in the bar.
    pub fn folded(&self) -> bool {
        self.folded
    }
}

impl Workspace {
    /// The group `id`, if it is there.
    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.get(&id)
    }

    /// The tabs of the group `id`, in their window's order; none of a group
    /// not there.
    pub fn group_tabs(&self, id: GroupId) -> Vec<TabId> {
        let Some(window) = self
            .groups
            .get(&id)
            .and_then(|group| self.windows.get(&group.window))
        else {
            return Vec::new();
        };
        window
            .tabs
            .iter()
            .copied()
            .filter(|tab| {
                self.tabs
                    .get(tab)
                    .is_some_and(|shown| shown.group == Some(id))
            })
            .collect()
    }

    /// The groups of `window`'s tabs, in the order of their tabs.
    pub fn groups_of(&self, window: WindowId) -> Vec<&Group> {
        let mut found: Vec<&Group> = Vec::new();
        let Some(shown) = self.windows.get(&window) else {
            return found;
        };
        for tab in &shown.tabs {
            if let Some(group) = self
                .tabs
                .get(tab)
                .and_then(|shown| shown.group)
                .and_then(|id| self.groups.get(&id))
                && !found.iter().any(|known| known.id == group.id)
            {
                found.push(group);
            }
        }
        found
    }

    /// Makes a group of `tabs`, of one window and none pinned, of `name` and
    /// `color`, as [`Workspace::new_group_as`] does, of a new id.
    pub fn new_group(&mut self, tabs: &[TabId], name: &str, color: Option<Rgb>) -> Option<GroupId> {
        self.new_group_as(GroupId::new(), tabs, name, color)
    }

    /// Makes the group `id` of `tabs`, of one window and none pinned, of
    /// `name` and `color`: its tabs next to one another at the first's place
    /// in their window's order, each out of the group that it was in. Its
    /// id; none, and nothing done, of no tab, of a tab not there or pinned,
    /// of tabs of several windows, or of an id that a group has.
    pub fn new_group_as(
        &mut self,
        id: GroupId,
        tabs: &[TabId],
        name: &str,
        color: Option<Rgb>,
    ) -> Option<GroupId> {
        let window = self.tabs.get(tabs.first()?)?.window;
        let mut members: Vec<TabId> = Vec::new();
        for &tab in tabs {
            let shown = self.tabs.get(&tab)?;
            if shown.window != window || shown.marks.pinned {
                return None;
            }
            if !members.contains(&tab) {
                members.push(tab);
            }
        }
        if self.groups.contains_key(&id) || !self.windows.contains_key(&window) {
            return None;
        }
        // Each leaves its group for the place right after the group's
        // tabs: the last first, so that they keep their order there.
        let before = self.windows.get(&window)?.tabs.clone();
        members.sort_by_key(|tab| before.iter().position(|id| id == tab));
        for &tab in members.iter().rev() {
            self.leave_group(tab);
        }
        let order = self.windows.get(&window)?.tabs.clone();
        members.sort_by_key(|tab| order.iter().position(|id| id == tab));
        let at = order.iter().position(|&tab| tab == members[0])?;
        for (k, &tab) in members.iter().enumerate() {
            self.move_tab(tab, window, at + k);
        }
        for tab in &members {
            if let Some(shown) = self.tabs.get_mut(tab) {
                shown.group = Some(id);
            }
        }
        self.groups.insert(
            id,
            Group {
                id,
                window,
                name: name.to_owned(),
                color,
                folded: false,
            },
        );
        self.group_changed(id, window);
        Some(id)
    }

    /// Puts `tab`, not pinned, at the end of `group`'s tabs, in its window,
    /// out of the group that it was in; whether both are there and the tab
    /// is not pinned.
    pub fn add_to_group(&mut self, tab: TabId, group: GroupId) -> bool {
        let Some(window) = self.groups.get(&group).map(|group| group.window) else {
            return false;
        };
        let Some(shown) = self.tabs.get(&tab) else {
            return false;
        };
        if shown.marks.pinned {
            return false;
        }
        if shown.group == Some(group) {
            return true;
        }
        self.leave_group(tab);
        let Some(&last) = self.group_tabs(group).last() else {
            return false;
        };
        let Some(order) = self.windows.get(&window).map(|shown| shown.tabs.clone()) else {
            return false;
        };
        let Some(last_at) = order.iter().position(|&id| id == last) else {
            return false;
        };
        // Right after the group's last tab, as it stands once the tab left
        // its place.
        let index = match order.iter().position(|&id| id == tab) {
            Some(at) if at < last_at => last_at,
            _ => last_at + 1,
        };
        self.move_tab(tab, window, index);
        if let Some(shown) = self.tabs.get_mut(&tab) {
            shown.group = Some(group);
        }
        self.group_changed(group, window);
        self.unfold_if_shown(tab);
        true
    }

    /// Takes `tab` out of its group, right after the group's tabs; whether
    /// the tab is there.
    pub fn remove_from_group(&mut self, tab: TabId) -> bool {
        if !self.tabs.contains_key(&tab) {
            return false;
        }
        self.leave_group(tab);
        true
    }

    /// Gives `group` the name `name`, empty for none; whether it is there.
    pub fn rename_group(&mut self, group: GroupId, name: &str) -> bool {
        let Some(shown) = self.groups.get_mut(&group) else {
            return false;
        };
        if shown.name != name {
            name.clone_into(&mut shown.name);
            let window = shown.window;
            self.group_changed(group, window);
        }
        true
    }

    /// Gives `group` the color `color`, or none; whether it is there.
    pub fn set_group_color(&mut self, group: GroupId, color: Option<Rgb>) -> bool {
        let Some(shown) = self.groups.get_mut(&group) else {
            return false;
        };
        if shown.color != color {
            shown.color = color;
            let window = shown.window;
            self.group_changed(group, window);
        }
        true
    }

    /// Folds `group`, its tabs hidden behind its name in the bar, or
    /// unfolds it: a window that shows one of its tabs shows the tab after
    /// the group, or the one before it. Whether it is there, and, folding,
    /// whether its window has a tab out of it, as a group of the window's
    /// every tab folds not.
    pub fn fold_group(&mut self, group: GroupId, folded: bool) -> bool {
        let Some((window, was)) = self
            .groups
            .get(&group)
            .map(|shown| (shown.window, shown.folded))
        else {
            return false;
        };
        if was == folded {
            return true;
        }
        if folded {
            let tabs = self.group_tabs(group);
            let Some((order, active)) = self
                .windows
                .get(&window)
                .map(|shown| (shown.tabs.clone(), shown.active))
            else {
                return false;
            };
            if tabs.len() >= order.len() {
                return false;
            }
            if let Some(shown) = self.groups.get_mut(&group) {
                shown.folded = true;
            }
            if tabs.contains(&active) {
                let position = |tab: &TabId| order.iter().position(|id| id == tab);
                let after = tabs.last().and_then(position).map(|at| at + 1);
                let before = tabs
                    .first()
                    .and_then(position)
                    .and_then(|at| at.checked_sub(1));
                let next = after
                    .and_then(|at| order.get(at))
                    .or_else(|| before.and_then(|at| order.get(at)));
                if let Some(&next) = next {
                    self.activate_tab(next);
                }
            }
        } else if let Some(shown) = self.groups.get_mut(&group) {
            shown.folded = false;
        }
        self.group_changed(group, window);
        true
    }

    /// Takes `group` apart, its tabs staying where they are; whether it was
    /// there.
    pub fn ungroup(&mut self, group: GroupId) -> bool {
        let Some(window) = self.groups.remove(&group).map(|shown| shown.window) else {
            return false;
        };
        for shown in self.tabs.values_mut() {
            if shown.group == Some(group) {
                shown.group = None;
            }
        }
        self.group_changed(group, window);
        true
    }

    /// Unfolds the group of `tab` where its window shows it, as a tab that
    /// joins a folded group may be the one that its window shows, which a
    /// folded group would hide.
    fn unfold_if_shown(&mut self, tab: TabId) {
        let shown = self
            .tabs
            .get(&tab)
            .and_then(|shown| self.windows.get(&shown.window))
            .is_some_and(|window| window.active == tab);
        if shown {
            self.unfold_group_of(tab);
        }
    }

    /// Unfolds the group of `tab`, which its window shows, if it is folded.
    pub(super) fn unfold_group_of(&mut self, tab: TabId) {
        let Some(group) = self.tabs.get(&tab).and_then(|shown| shown.group) else {
            return;
        };
        if let Some(shown) = self.groups.get_mut(&group)
            && shown.folded
        {
            shown.folded = false;
            let window = shown.window;
            self.group_changed(group, window);
        }
    }

    /// Takes `tab` out of its group, moved right after the group's tabs
    /// where others follow it there; a group of no tab left goes.
    pub(super) fn leave_group(&mut self, tab: TabId) {
        let Some((group, window)) = self
            .tabs
            .get(&tab)
            .and_then(|shown| Some((shown.group?, shown.window)))
        else {
            return;
        };
        let run = self.group_tabs(group);
        let Some(shown) = self.tabs.get_mut(&tab) else {
            return;
        };
        shown.group = None;
        if let Some(&last) = run.last()
            && last != tab
            && run.contains(&tab)
            && let Some(last_at) = self
                .windows
                .get(&window)
                .and_then(|shown| shown.tabs.iter().position(|&id| id == last))
        {
            // Before the group's last tab, it goes to the last's place once
            // it left its own.
            self.move_tab(tab, window, last_at);
        }
        let window = self.groups.get(&group).map_or(window, |shown| shown.window);
        if self.group_tabs(group).is_empty() {
            self.groups.remove(&group);
        }
        self.group_changed(group, window);
    }

    /// `tab`, which came to its place, joins the group of the tabs on both
    /// sides of it, if they share one and it is not pinned.
    pub(super) fn join_between(&mut self, tab: TabId) {
        let Some(shown) = self.tabs.get(&tab) else {
            return;
        };
        if shown.marks.pinned {
            return;
        }
        let before = shown.group;
        let Some(order) = self.windows.get(&shown.window).map(|shown| &shown.tabs) else {
            return;
        };
        let Some(at) = order.iter().position(|&id| id == tab) else {
            return;
        };
        let group_at = |index: usize| {
            order
                .get(index)
                .and_then(|id| self.tabs.get(id))
                .and_then(|shown| shown.group)
        };
        let (Some(left), Some(right)) = (at.checked_sub(1).and_then(group_at), group_at(at + 1))
        else {
            return;
        };
        if left != right || before == Some(left) {
            return;
        }
        if let Some(shown) = self.tabs.get_mut(&tab) {
            shown.group = Some(left);
        }
        if let Some(window) = self.groups.get(&left).map(|group| group.window) {
            self.group_changed(left, window);
        }
        self.unfold_if_shown(tab);
        if let Some(before) = before
            && !self.group_tabs(before).is_empty()
            && let Some(window) = self.groups.get(&before).map(|group| group.window)
        {
            self.group_changed(before, window);
        }
    }

    /// Settles the groups as their windows' tabs changed: a pinned tab, or
    /// one of a group not there, leaves its group; of a group's tabs split
    /// into runs, the longest run of its window keeps it, or of another
    /// window, which the group goes to, of none in its own, and the others
    /// leave it; a group of no tab left goes. Each group that changed is
    /// told.
    pub(super) fn settle_groups(&mut self) {
        let mut changed: Vec<(GroupId, WindowId)> = Vec::new();
        for shown in self.tabs.values_mut() {
            if let Some(group) = shown.group
                && (shown.marks.pinned || !self.groups.contains_key(&group))
            {
                shown.group = None;
                if let Some(known) = self.groups.get(&group) {
                    changed.push((group, known.window));
                }
            }
        }
        let mut runs: BTreeMap<GroupId, Vec<(WindowId, Vec<TabId>)>> = BTreeMap::new();
        for (&window, shown) in &self.windows {
            let mut last = None;
            for &tab in &shown.tabs {
                // The settings tab, of no pane, splits no run.
                let Some(held) = self.tabs.get(&tab) else {
                    continue;
                };
                let group = held.group;
                if let Some(group) = group {
                    let found = runs.entry(group).or_default();
                    match found.last_mut() {
                        Some((_, run)) if last == Some(group) => run.push(tab),
                        _ => found.push((window, vec![tab])),
                    }
                }
                last = group;
            }
        }
        let ids: Vec<GroupId> = self.groups.keys().copied().collect();
        for id in ids {
            let Some(window) = self.groups.get(&id).map(|group| group.window) else {
                continue;
            };
            let Some(found) = runs.remove(&id) else {
                self.groups.remove(&id);
                changed.push((id, window));
                continue;
            };
            let longest = |own: bool| {
                found
                    .iter()
                    .enumerate()
                    .filter(|(_, (place, _))| !own || *place == window)
                    .max_by_key(|(index, (_, run))| (run.len(), Reverse(*index)))
                    .map(|(index, _)| index)
            };
            let Some(keeper) = longest(true).or_else(|| longest(false)) else {
                continue;
            };
            let mut moved = false;
            for (index, (place, run)) in found.iter().enumerate() {
                if index == keeper {
                    if *place != window
                        && let Some(group) = self.groups.get_mut(&id)
                    {
                        group.window = *place;
                        moved = true;
                    }
                    continue;
                }
                for tab in run {
                    if let Some(shown) = self.tabs.get_mut(tab) {
                        shown.group = None;
                    }
                }
                moved = true;
            }
            if moved && let Some(group) = self.groups.get(&id) {
                changed.push((id, group.window));
            }
        }
        let mut told: Vec<GroupId> = Vec::new();
        for (group, window) in changed {
            if !told.contains(&group) {
                told.push(group);
                self.group_changed(group, window);
            }
        }
    }

    /// Tells that `group`, of `window`, changed, or went.
    pub(super) fn group_changed(&mut self, group: GroupId, window: WindowId) {
        self.changed(WorkspaceEvent::GroupChanged {
            group: group.into(),
            window: window.into(),
        });
    }
}
