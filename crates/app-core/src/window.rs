//! What the app keeps of each window that it opened: what it draws into,
//! its ids in the workspace, its focus, the input under way, and what its
//! last frame showed.

use std::path::PathBuf;
use std::time::Instant;

use lokyr_kernel::keymap::Resolver;
use lokyr_kernel::workspace::{self, Grid, PaneId};
use lokyr_platform_core::{
    Appearance, Backdrop, ContentRect, KeyCode, KeyEvent, LayerPlace, PointerArea, PointerShape,
    Sweep, Titlebar,
};
use lokyr_render_core::{CellBatch, Color, Item, Rect, Surface as _};
use lokyr_ui_core::cursor::Blink;
use lokyr_ui_core::focus::{Focus, Target};
use lokyr_ui_core::pane::Preedit;
use lokyr_vt_core::terminal::{SelectionShape, SelectionUnit};

use crate::app::chrome::SlotView;
use crate::drawing::Backend;
use crate::mouse::{Mouse, Spot};
use crate::notice;

/// A layer of a window, over its grid, which the window server blinks.
#[derive(Debug)]
pub(crate) struct WindowLayer<B: Backend> {
    /// What the app draws into, where the platform has the layer.
    pub(crate) surface: Option<B::Surface>,
    /// Where the app placed it last, and how it blinks: `None` while it is
    /// hidden.
    pub(crate) place: Option<LayerPlace>,
    /// The cells that it shows, at what scale, and of which of the
    /// drawing's fonts, which a frame draws again only as they change.
    pub(crate) drawn: Option<(CellBatch, f64, u64)>,
}

impl<B: Backend> WindowLayer<B> {
    pub(crate) fn new(surface: Option<B::Surface>) -> Self {
        Self {
            surface,
            place: None,
            drawn: None,
        }
    }

    /// Lets the layer's drawables go, all of them, as a frame of a pixel
    /// takes the place of the one that it shows; the next cells that it
    /// shows draw again.
    pub(crate) fn release(&mut self) {
        let Some(surface) = &self.surface else {
            return;
        };
        surface.release();
        if let Err(error) = surface.clear(Color::default().with_alpha(0.0)) {
            eprintln!("lokyr: {error}");
        }
        self.drawn = None;
    }
}

/// A window that the app opened.
#[derive(Debug)]
pub(crate) struct Window<B: Backend> {
    /// What it draws into, once the app draws and the platform has a layer.
    pub(crate) surface: Option<B::Surface>,
    /// The layers of its text that blinks, over the grid, and of its
    /// cursor's cells, over everything.
    pub(crate) text_layer: WindowLayer<B>,
    pub(crate) cursor_layer: WindowLayer<B>,
    /// The workspace's window that it shows.
    pub(crate) window: workspace::WindowId,
    /// Where its active tab's panes lie, as its last frame laid them out:
    /// the zoomed one alone while one is.
    pub(crate) views: Vec<PaneView>,
    /// The text that the input method composes, shown at the cursor.
    pub(crate) preedit: Option<Preedit>,
    /// Whether it has the keyboard's focus.
    pub(crate) focused: bool,
    /// The clock of its cursor's blinking.
    pub(crate) blink: Blink,
    /// Whether its cursor blinked at its last frame.
    pub(crate) blinking: bool,
    /// Until when it flashes for its bell.
    pub(crate) flash_until: Option<Instant>,
    /// Whether its panes changed since its accessibility tree went last,
    /// and the keyboard's pane of that tree.
    pub(crate) access_changed: bool,
    pub(crate) access_focus: Option<PaneId>,
    /// When its last bell acted.
    pub(crate) rang_at: Option<Instant>,
    /// When its program's last notification showed.
    pub(crate) notified_at: Option<Instant>,
    /// Whether its title shows that its bell rang while it had no focus.
    pub(crate) bell_marked: bool,
    /// The backdrop that the platform has for it, while it is translucent.
    pub(crate) backdrop: Option<Backdrop>,
    /// The mouse over it.
    pub(crate) mouse: Mouse,
    /// Whether the platform tells the pointer's moves without a button, as
    /// a program asked for them.
    pub(crate) mouse_moves: bool,
    /// How the pointer looks over it, as the platform has it.
    pub(crate) pointer: PointerShape,
    /// The sequence of keys under way, as the keymap resolves it.
    pub(crate) resolver: Resolver,
    /// The keys of the sequence under way, which go to the program if the
    /// next key does not go on with it.
    pub(crate) held_back: Vec<KeyEvent>,
    /// The keys whose presses the keymap took, whose releases go nowhere,
    /// each once.
    pub(crate) taken: Vec<KeyCode>,
    /// The selection that the mouse makes, or made last, which Shift's click
    /// extends.
    pub(crate) selecting: Option<Selecting>,
    /// The title of the man page that it shows, as `man ls`, when it shows
    /// one: it takes the man page's colors, and keeps the page once `man`
    /// has ended.
    pub(crate) page: Option<String>,
    /// The last part of the directory of the program in its foreground, as
    /// `lokyr`, and that program's name, as `zsh`, which its title shows.
    pub(crate) place: (String, String),
    /// That directory, whole, as the title last looked, which stays once
    /// the program ended.
    pub(crate) directory: Option<PathBuf>,
    /// Whether the user resizes it, dragging its edge; when it drew last;
    /// the grid's size that it drew last, its columns and rows; and until
    /// when the label of its grid's size shows, after the grid changed.
    pub(crate) live_resize: bool,
    pub(crate) drawn_at: Option<Instant>,
    pub(crate) grid_drawn: Option<(u16, u16)>,
    pub(crate) size_label: Option<Instant>,
    /// Where its last frame drew the notice's bar and its close, in pixels,
    /// and the pointer's areas that the platform has of them, in points.
    pub(crate) notice: Option<notice::Rects>,
    pub(crate) pointer_areas: Vec<PointerArea>,
    /// The title that the platform shows, once the app gave one.
    pub(crate) title_shown: Option<String>,
    /// When its last frame of output came, since its title last looked, for
    /// the titles' timer to look once more `TITLE_LOOK` after it.
    pub(crate) output_at: Option<Instant>,
    /// The title that the user gave it, which takes the place of its
    /// program's and of its own.
    pub(crate) user_title: Option<String>,
    /// The appearance of its own that the platform has for it, once the
    /// app gave one: `None` for the app's.
    pub(crate) own_appearance: Option<Option<Appearance>>,
    /// Whether it is hidden or covered, which draws nothing.
    pub(crate) occluded: bool,
    /// What its surface shows, as its last frame drew it: its size in
    /// pixels and its scale, the drawing's fonts, the background and the
    /// items, which a frame of the same presents again for nothing.
    pub(crate) shown: Option<Shown>,
    /// The views of its slots, each a surface of its own, in the order that
    /// they draw.
    pub(crate) chrome: Vec<SlotView>,
    /// Its titlebar, as the app gave it: one of a tab bar while a tab
    /// bar's view shows.
    pub(crate) titlebar: Titlebar,
    /// The top of the room that its panes share, in pixels, below its
    /// titlebar and its tab bar, as its last layout placed it.
    pub(crate) panes_top: u32,
    /// The indeterminate progresses of its slots' views that the platform
    /// moves, and the moment from which their sweeps run.
    pub(crate) sweeps: (Vec<Sweep>, Instant),
    /// The keyboard's focus, on its pane or on a node of a slot's view, by
    /// the view's id.
    pub(crate) focus: Focus<u32>,
    /// The pane that had its keyboard, as the app looked last.
    pub(crate) keyboard: Option<PaneId>,
    /// The id that its next slot's view takes.
    pub(crate) next_view: u32,
    /// The slot's view that the pointer's button went down over, which
    /// takes its moves and its release until it goes up, and the one that
    /// the pointer is over.
    pub(crate) captured: Option<u32>,
    pub(crate) over: Option<u32>,
    /// The parts of its content where the pointer's moves come, its slots'
    /// views, and of its titlebar that are the tab bar's own, in points, as
    /// the app gave them last.
    pub(crate) move_areas: Vec<ContentRect>,
    pub(crate) titlebar_areas: Vec<ContentRect>,
    /// Whether a render took away a field's composition, or set its text,
    /// which the platform's input method is to forget.
    pub(crate) composition_dropped: bool,
}

/// Where a pane of a window lies, as a frame laid it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaneView {
    /// The pane.
    pub(crate) pane: PaneId,
    /// Its room, in pixels from the window's content's top left corner,
    /// which its grid fills from its top left corner, of whole cells.
    pub(crate) rect: Rect,
    /// Its grid, of the cells that its room holds.
    pub(crate) grid: Grid,
}

/// What a window's surface shows.
#[derive(Debug, PartialEq)]
pub(crate) struct Shown {
    pub(crate) pixels: (u32, u32),
    pub(crate) scale: f64,
    pub(crate) fonts: u64,
    pub(crate) background: Color,
    pub(crate) items: Vec<Item>,
}

/// A selection that the mouse's left button makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Selecting {
    /// Where the press started it.
    pub(crate) anchor: Spot,
    /// Where the pointer is, or was as the button went up.
    pub(crate) last: Spot,
    pub(crate) shape: SelectionShape,
    pub(crate) unit: SelectionUnit,
    /// Whether the button is down.
    pub(crate) held: bool,
    /// How many rows past the grid's top, negative, or bottom, positive,
    /// the pointer is, which scrolls the view.
    pub(crate) beyond: i64,
}

/// The surfaces of a window that the platform gives, where it has them:
/// its own, and those of its layers.
pub(crate) struct Surfaces<B: Backend> {
    /// The window's, of its frames.
    pub(crate) window: Option<B::Surface>,
    /// The layer's of its text that blinks.
    pub(crate) text: Option<B::Surface>,
    /// The layer's of its cursor's cells.
    pub(crate) cursor: Option<B::Surface>,
}

impl<B: Backend> Window<B> {
    pub(crate) fn new(
        surfaces: Surfaces<B>,
        window: workspace::WindowId,
        focused: bool,
        now: Instant,
    ) -> Self {
        Self {
            surface: surfaces.window,
            text_layer: WindowLayer::new(surfaces.text),
            cursor_layer: WindowLayer::new(surfaces.cursor),
            window,
            views: Vec::new(),
            preedit: None,
            focused,
            blink: Blink::new(now),
            blinking: false,
            flash_until: None,
            access_changed: false,
            access_focus: None,
            rang_at: None,
            notified_at: None,
            bell_marked: false,
            backdrop: None,
            mouse: Mouse::default(),
            mouse_moves: false,
            pointer: PointerShape::Arrow,
            resolver: Resolver::new(),
            held_back: Vec::new(),
            taken: Vec::new(),
            selecting: None,
            page: None,
            place: (String::new(), String::new()),
            directory: None,
            notice: None,
            live_resize: false,
            drawn_at: None,
            grid_drawn: None,
            size_label: None,
            pointer_areas: Vec::new(),
            title_shown: None,
            output_at: None,
            user_title: None,
            own_appearance: None,
            occluded: false,
            shown: None,
            chrome: Vec::new(),
            titlebar: Titlebar::default(),
            panes_top: 0,
            sweeps: (Vec::new(), now),
            focus: Focus::new(),
            keyboard: None,
            next_view: 0,
            captured: None,
            over: None,
            move_areas: Vec::new(),
            titlebar_areas: Vec::new(),
            composition_dropped: false,
        }
    }

    /// Whether its pane has the keyboard's focus, rather than a node of a
    /// slot's view.
    pub(crate) fn pane_has_focus(&self) -> bool {
        self.focus.target() == Target::Pane
    }

    /// Takes the press of the key at `code` for the keymap, so that its
    /// release goes nowhere.
    pub(crate) fn take(&mut self, code: KeyCode) {
        if !self.taken.contains(&code) {
            self.taken.push(code);
        }
    }

    /// Ends the sequence under way, and gives back its keys, for the
    /// program, whose releases go to it too, as the keymap no longer takes
    /// their presses.
    pub(crate) fn end_sequence(&mut self) -> Vec<KeyEvent> {
        self.resolver = Resolver::new();
        let keys = std::mem::take(&mut self.held_back);
        for key in &keys {
            self.taken.retain(|&code| code != key.code);
        }
        keys
    }
}
