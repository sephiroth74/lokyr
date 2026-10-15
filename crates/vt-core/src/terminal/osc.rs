//! The OSC strings: what programs tell the terminal, and through it the
//! embedder, besides text: titles, the working directory, notifications,
//! progress and the tab's color.

use super::{ClipboardRequest, ClipboardTarget, Terminal};
use crate::page::{Rgb, RowFlag, Semantic};
use crate::parser::StringEnd;

/// The most that the buffer of OSC strings keeps between them. A larger
/// buffer, which OSC 52 can need, is let go after its string.
const KEPT_BYTES: usize = 4 * 1024;

/// The longest URI and `id` of a link, as the specification of OSC 8
/// limits them.
const MAX_URI: usize = 2083;
const MAX_LINK_ID: usize = 250;

/// The link that OSC 8 opened, with no URI while none is open. Its buffers
/// stay from one link to the next.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Link {
    pub(super) id: Vec<u8>,
    pub(super) uri: Vec<u8>,
}

impl Link {
    pub(super) fn is_open(&self) -> bool {
        !self.uri.is_empty()
    }
}

/// What the embedder has to act on, which programs asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The window's or the icon's title changed (OSC 0, 1 and 2).
    TitleChanged,
    /// The working directory changed (OSC 7).
    WorkingDirectory(WorkingDirectory),
    /// A notification (OSC 9 and 777), whose title can be empty.
    Notification {
        /// Its title.
        title: String,
        /// Its text.
        body: String,
    },
    /// The progress of a task (OSC 9;4).
    Progress(Progress),
    /// The tab's color changed, or went back to the default (OSC 6;1).
    TabColor(Option<Rgb>),
    /// A program changed the colors (OSC 4, 10 to 12, and their resets).
    ColorsChanged,
    /// A program wrote to the clipboards (OSC 52).
    ClipboardWrite {
        /// The clipboards.
        target: ClipboardTarget,
        /// What it wrote, decoded.
        data: Vec<u8>,
    },
    /// A program asks to read the clipboards (OSC 52), which the
    /// configuration allows; the embedder answers with
    /// [`Terminal::reply_clipboard`].
    ClipboardRead(ClipboardRequest),
}

/// Where a shell works, as OSC 7 tells it: a host and a path on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkingDirectory {
    /// The host, empty for the local one.
    pub host: String,
    /// The path, decoded.
    pub path: String,
}

/// The progress of a task, as ConEmu and Windows Terminal report it with
/// OSC 9;4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    /// No task: the progress goes away (0).
    Remove,
    /// A task, this far along, from 0 to 100 (1).
    Set(u8),
    /// A task that failed, this far along if known (2).
    Error(Option<u8>),
    /// A task whose progress is not known (3).
    Indeterminate,
    /// A task paused, this far along if known (4).
    Paused(Option<u8>),
}

impl Terminal {
    /// Starts an OSC string.
    pub(super) fn start_osc(&mut self) {
        self.osc.clear();
    }

    /// Reads the next chunk of the OSC string.
    pub(super) fn read_osc(&mut self, data: &[u8]) {
        self.osc.extend_from_slice(data);
    }

    /// Acts on the OSC string, if it ended with ST or BEL.
    pub(super) fn end_osc(&mut self, end: StringEnd) {
        let osc = std::mem::take(&mut self.osc);
        if matches!(end, StringEnd::St | StringEnd::Bel) {
            self.dispatch_osc(&osc, end);
        }
        if osc.capacity() <= KEPT_BYTES {
            self.osc = osc;
        }
    }

    fn dispatch_osc(&mut self, osc: &[u8], end: StringEnd) {
        let (number, rest) = match osc.iter().position(|&byte| byte == b';') {
            Some(at) => (&osc[..at], &osc[at + 1..]),
            None => (osc, &[][..]),
        };
        match number {
            b"0" => self.set_titles(rest, true, true),
            b"1" => self.set_titles(rest, false, true),
            b"2" => self.set_titles(rest, true, false),
            b"6" => self.set_tab_color(rest),
            b"4" => self.set_palette(rest, end),
            b"7" => self.set_working_directory(rest),
            b"8" => self.set_link(rest),
            b"10" => self.set_dynamic_colors(10, rest, end),
            b"11" => self.set_dynamic_colors(11, rest, end),
            b"12" => self.set_dynamic_colors(12, rest, end),
            b"52" => self.clipboard(rest, end),
            b"104" => self.reset_palette(rest),
            b"110" => self.reset_dynamic_color(10),
            b"111" => self.reset_dynamic_color(11),
            b"112" => self.reset_dynamic_color(12),
            b"133" => self.mark_prompt(rest),
            b"9" => match rest.strip_prefix(b"4") {
                Some([] | [b';', ..]) => self.report_progress(rest),
                _ => self.notify(String::new(), text(rest)),
            },
            b"777" => {
                let mut fields = rest.splitn(3, |&byte| byte == b';');
                if fields.next() == Some(b"notify") {
                    let title = text(fields.next().unwrap_or_default());
                    let body = text(fields.next().unwrap_or_default());
                    self.notify(title, body);
                }
            }
            _ => {}
        }
    }

    /// OSC 8, `params;uri`: opens the link to `uri`, which the text printed
    /// next is part of, or closes it with no URI. The `id` parameter ties
    /// the cells of one link together. A link whose scheme the
    /// configuration does not allow, or longer than the limits of the
    /// specification (<https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda>),
    /// or with control characters, closes the link instead.
    fn set_link(&mut self, fields: &[u8]) {
        let Some(at) = fields.iter().position(|&byte| byte == b';') else {
            return;
        };
        let (params, uri) = (&fields[..at], &fields[at + 1..]);
        let id = params
            .split(|&byte| byte == b':')
            .find_map(|param| param.strip_prefix(b"id="))
            .unwrap_or_default();
        let opens = !uri.is_empty()
            && uri.len() <= MAX_URI
            && id.len() <= MAX_LINK_ID
            && without_controls(uri)
            && without_controls(id)
            && self.allows_scheme(uri);
        self.link.id.clear();
        self.link.uri.clear();
        if opens {
            self.link.id.extend_from_slice(id);
            self.link.uri.extend_from_slice(uri);
        }
    }

    /// Whether the configuration allows the scheme of `uri`, what comes
    /// before its first `:`, in any case.
    fn allows_scheme(&self, uri: &[u8]) -> bool {
        let Some(colon) = uri.iter().position(|&byte| byte == b':') else {
            return false;
        };
        let scheme = &uri[..colon];
        self.link_schemes
            .iter()
            .any(|allowed| allowed.as_bytes().eq_ignore_ascii_case(scheme))
    }

    /// OSC 133, FinalTerm's semantic prompts: `A` starts a prompt, which
    /// also marks its row, `B` the command that the user types, and `C`
    /// its output, as `D` ends the command. The text printed next is in
    /// that zone.
    fn mark_prompt(&mut self, fields: &[u8]) {
        let kind = fields
            .split(|&byte| byte == b';')
            .next()
            .unwrap_or_default();
        self.zone = match kind {
            b"A" => {
                let y = self.cursor().y;
                let (page, row) = self.screen_mut().rows.screen_row_mut(y);
                page.row_mut(row).set(RowFlag::PromptStart, true);
                Semantic::Prompt
            }
            b"B" => Semantic::Input,
            b"C" | b"D" => Semantic::Output,
            _ => return,
        };
    }

    /// OSC 0, 1 and 2: the window's title, the icon's, or both.
    fn set_titles(&mut self, title: &[u8], window: bool, icon: bool) {
        let title = text(title);
        if icon {
            self.icon_title.clone_from(&title);
        }
        if window {
            self.title = title;
        }
        self.events.push(Event::TitleChanged);
    }

    /// OSC 7: the working directory, as a `file:` URL.
    fn set_working_directory(&mut self, url: &[u8]) {
        let Some(rest) = url.strip_prefix(b"file://") else {
            return;
        };
        let slash = rest
            .iter()
            .position(|&byte| byte == b'/')
            .unwrap_or(rest.len());
        let (host, path) = rest.split_at(slash);
        if path.is_empty() {
            return;
        }
        let directory = WorkingDirectory {
            host: text(&percent_decoded(host)),
            path: text(&percent_decoded(path)),
        };
        self.working_directory = Some(directory.clone());
        self.events.push(Event::WorkingDirectory(directory));
    }

    /// OSC 9;4: the progress of a task, `4;state;progress`.
    fn report_progress(&mut self, fields: &[u8]) {
        let mut fields = fields.split(|&byte| byte == b';').skip(1);
        let mut number = || fields.next().and_then(decimal);
        let state = number().unwrap_or(0);
        let value = number().map(|value| value.min(100) as u8);
        let progress = match state {
            0 => Progress::Remove,
            1 => Progress::Set(value.unwrap_or(0)),
            2 => Progress::Error(value),
            3 => Progress::Indeterminate,
            4 => Progress::Paused(value),
            _ => return,
        };
        self.events.push(Event::Progress(progress));
    }

    /// A notification, from OSC 9 or 777.
    fn notify(&mut self, title: String, body: String) {
        self.events.push(Event::Notification { title, body });
    }

    /// OSC 6;1, iTerm2's tab color: `1;bg;red;brightness;N` sets one of
    /// red, green and blue, and `1;bg;*;default` goes back to the default.
    fn set_tab_color(&mut self, fields: &[u8]) {
        let fields: Vec<&[u8]> = fields.split(|&byte| byte == b';').collect();
        let color = match fields[..] {
            [b"1", b"bg", b"*", b"default"] => None,
            [b"1", b"bg", component, b"brightness", value] => {
                let Some(value) = decimal(value).and_then(|value| u8::try_from(value).ok()) else {
                    return;
                };
                let mut color = self.tab_color.unwrap_or_default();
                match component {
                    b"red" => color.r = value,
                    b"green" => color.g = value,
                    b"blue" => color.b = value,
                    _ => return,
                }
                Some(color)
            }
            _ => return,
        };
        self.tab_color = color;
        self.events.push(Event::TabColor(color));
    }
}

/// The text of `bytes`, as UTF-8, without its control characters.
pub(super) fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

/// Whether `bytes` are UTF-8 without control characters.
fn without_controls(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|text| !text.chars().any(char::is_control))
}

/// The byte that `pair`, two hexadecimal digits, stands for.
pub(super) fn hex_byte(pair: &[u8]) -> Option<u8> {
    let &[high, low] = pair else {
        return None;
    };
    let digit = |byte: u8| char::from(byte).to_digit(16);
    // Two digits make at most 0xFF.
    Some((digit(high)? * 16 + digit(low)?) as u8)
}

/// The number that `digits` write in decimal, if they all are digits.
pub(super) fn decimal(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() || digits.len() > 9 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

/// `bytes` with each `%` and two hexadecimal digits replaced by the byte
/// they stand for, as URLs encode them.
fn percent_decoded(bytes: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while let Some((&byte, after)) = rest.split_first() {
        let escaped = after.get(..2).and_then(hex_byte);
        match (byte, escaped) {
            (b'%', Some(value)) => {
                decoded.push(value);
                rest = &after[2..];
            }
            _ => {
                decoded.push(byte);
                rest = after;
            }
        }
    }
    decoded
}
