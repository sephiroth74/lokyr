//! The sessions: groups of windows, each of a name, of which one is active
//! and shows its windows, the others' hidden, their programs running. Each
//! window is of one session, but the quick windows and the settings', which
//! show whichever session is active. Lokyr starts in a session of no name;
//! a window opens in the active session, or in the session of the window
//! that its tab comes from. A session goes as its last window closes,
//! unless it is the only one; the active one that goes leaves the one used
//! before it active. A session of no window that a switch leaves goes too,
//! but one whose windows wait to open, as those kept of a session that has
//! not shown since the app started.

use std::collections::BTreeMap;

use lokyr_plugin_sdk::event::WorkspaceEvent;

use super::{SessionId, WindowId, Workspace};

/// A session of windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    id: SessionId,
    name: String,
    /// The folder that it was opened in, as the sessions' switcher opens
    /// one of a folder, which finds it there again.
    folder: Option<String>,
    /// The count of the sessions' uses as it last became active.
    used: u64,
    /// Whether windows wait to open in it, as the app holds them, which
    /// keeps it of none.
    waiting: bool,
}

impl Session {
    /// The session's id.
    pub fn id(&self) -> SessionId {
        self.id
    }

    /// Its name, empty for none.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The folder that it was opened in, if it was opened in one.
    pub fn folder(&self) -> Option<&str> {
        self.folder.as_deref()
    }
}

/// The sessions, and the active one.
#[derive(Debug)]
pub(super) struct Sessions {
    list: BTreeMap<SessionId, Session>,
    active: SessionId,
    /// The count of the sessions' uses, the active one's.
    uses: u64,
}

impl Default for Sessions {
    /// One session, of no name, active.
    fn default() -> Self {
        let id = SessionId::new();
        let session = Session {
            id,
            name: String::new(),
            folder: None,
            used: 0,
            waiting: false,
        };
        Self {
            list: BTreeMap::from([(id, session)]),
            active: id,
            uses: 0,
        }
    }
}

impl Workspace {
    /// The session `id`, if it is there.
    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.list.get(&id)
    }

    /// The sessions, in the order of their use: the active one first, then
    /// the one before it.
    pub fn sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.list.values().collect();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.used));
        sessions
    }

    /// The active session, whose windows show.
    pub fn active_session(&self) -> SessionId {
        self.sessions.active
    }

    /// The session used before the active one, which the last two's toggle
    /// switches to; none of one session alone.
    pub fn last_session(&self) -> Option<SessionId> {
        self.sessions().get(1).map(|session| session.id)
    }

    /// The windows of the session `id`, in the order they opened.
    pub fn session_windows(&self, id: SessionId) -> Vec<WindowId> {
        self.windows
            .values()
            .filter(|window| window.session == Some(id))
            .map(|window| window.id)
            .collect()
    }

    /// Whether `window` shows, of the active session or of none, rather
    /// than hidden with the session that it is of.
    pub fn in_active_session(&self, window: WindowId) -> bool {
        self.windows
            .get(&window)
            .is_some_and(|shown| shown.session.is_none_or(|of| of == self.sessions.active))
    }

    /// Makes a session of `name` and switches to it, as
    /// [`Workspace::new_session_as`] does, of a new id.
    pub fn new_session(&mut self, name: &str) -> Option<SessionId> {
        self.new_session_as(SessionId::new(), name)
    }

    /// Makes the session `id` of `name`, of no window until one opens in
    /// it, and switches to it, the active one's windows hidden: its id;
    /// none, and nothing done, of an id that a session has.
    pub fn new_session_as(&mut self, id: SessionId, name: &str) -> Option<SessionId> {
        if self.sessions.list.contains_key(&id) {
            return None;
        }
        self.sessions.list.insert(
            id,
            Session {
                id,
                name: name.to_owned(),
                folder: None,
                used: 0,
                waiting: false,
            },
        );
        self.switch_session(id);
        Some(id)
    }

    /// Adds a session of `name`, of no window until one opens in it, after
    /// the others in the order of their use, the active one staying, as the
    /// sessions kept open again: its id.
    pub fn add_session(&mut self, name: &str) -> SessionId {
        let id = SessionId::new();
        self.sessions.list.insert(
            id,
            Session {
                id,
                name: name.to_owned(),
                folder: None,
                used: 0,
                waiting: false,
            },
        );
        self.session_changed(id);
        id
    }

    /// Switches to the session `id`, whose windows show, the active one's
    /// hidden, their programs running; whether it is there.
    pub fn switch_session(&mut self, id: SessionId) -> bool {
        if !self.sessions.list.contains_key(&id) {
            return false;
        }
        if self.sessions.active == id {
            return true;
        }
        let before = std::mem::replace(&mut self.sessions.active, id);
        self.sessions.uses += 1;
        let uses = self.sessions.uses;
        if let Some(session) = self.sessions.list.get_mut(&id) {
            session.used = uses;
        }
        self.session_changed(before);
        self.session_changed(id);
        // The session left goes of no window, as a new one made from none,
        // or one that a plugin made before another in the same call.
        if self
            .sessions
            .list
            .get(&before)
            .is_some_and(|left| !left.waiting)
        {
            self.session_lost_window(Some(before));
        }
        true
    }

    /// Marks the session `id` of windows that wait to open, as the app
    /// holds them until it first shows, or of none; whether it is there.
    pub fn set_session_waiting(&mut self, id: SessionId, waiting: bool) -> bool {
        let Some(session) = self.sessions.list.get_mut(&id) else {
            return false;
        };
        session.waiting = waiting;
        true
    }

    /// Switches to the session used before the active one; whether there
    /// is one.
    pub fn switch_to_last_session(&mut self) -> bool {
        self.last_session()
            .is_some_and(|last| self.switch_session(last))
    }

    /// Gives the session `id` the name `name`, empty for none; whether it
    /// is there.
    pub fn rename_session(&mut self, id: SessionId, name: &str) -> bool {
        let Some(session) = self.sessions.list.get_mut(&id) else {
            return false;
        };
        if session.name != name {
            name.clone_into(&mut session.name);
            self.session_changed(id);
        }
        true
    }

    /// Gives the session `id` the folder that it was opened in; whether it
    /// is there.
    pub fn set_session_folder(&mut self, id: SessionId, folder: Option<String>) -> bool {
        let Some(session) = self.sessions.list.get_mut(&id) else {
            return false;
        };
        session.folder = folder;
        true
    }

    /// The session opened in `folder`, the one used last of several.
    pub fn session_in(&self, folder: &str) -> Option<SessionId> {
        self.sessions()
            .into_iter()
            .find(|session| session.folder.as_deref() == Some(folder))
            .map(Session::id)
    }

    /// Closes the session `id`, with its windows, their tabs and panes,
    /// whose programs are hung up: it goes as its last window does, unless
    /// it is the only one, which stays, of no window. Whether it was there.
    /// The app asks first, as closing its windows would.
    pub fn close_session(&mut self, id: SessionId) -> bool {
        if !self.sessions.list.contains_key(&id) {
            return false;
        }
        for window in self.session_windows(id) {
            self.close_window(window);
        }
        // A session of no window, which none of its windows' closing took.
        self.session_lost_window(Some(id));
        true
    }

    /// Takes `window` out of the sessions, as a man page's window, which
    /// shows whichever session is active; whether it is open.
    pub fn leave_sessions(&mut self, window: WindowId) -> bool {
        let Some(shown) = self.windows.get_mut(&window) else {
            return false;
        };
        let session = shown.session.take();
        self.session_lost_window(session);
        true
    }

    /// The session that a window of `tab`, which comes from the window it
    /// is in, if it is in one, is of: that window's, or the active one, as
    /// a tab out of the quick terminal's window or a man page's, of no
    /// session, joins the session that shows.
    pub(super) fn session_for(&self, from: Option<WindowId>) -> Option<SessionId> {
        from.and_then(|window| self.windows.get(&window))
            .and_then(|window| window.session)
            .or(Some(self.sessions.active))
    }

    /// Takes away `session`, which lost a window, if it has none left and is
    /// not the only one; the active one that goes leaves the one used before
    /// it active.
    pub(super) fn session_lost_window(&mut self, session: Option<SessionId>) {
        let Some(id) = session else {
            return;
        };
        if self.sessions.list.len() < 2
            || self
                .windows
                .values()
                .any(|window| window.session == Some(id))
        {
            return;
        }
        if self.sessions.active == id {
            self.switch_to_last_session();
        }
        if self.sessions.list.remove(&id).is_some() {
            self.session_changed(id);
        }
    }

    /// Records that the session `id` changed, for the embedder.
    fn session_changed(&mut self, id: SessionId) {
        self.changed(WorkspaceEvent::SessionChanged { session: id.into() });
    }
}
