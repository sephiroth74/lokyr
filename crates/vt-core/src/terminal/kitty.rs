//! kitty's graphics protocol, over APC `G`
//! (<https://sw.kovidgoyal.net/kitty/graphics-protocol/>), a subset: the
//! images transmitted in the stream alone (`t=d`), of RGB, RGBA or PNG
//! (`f=24`, `32`, `100`), compressed by zlib or not (`o=z`), in chunks or
//! whole (`m=1`); transmitted, displayed, deleted and queried (`a=t`, `T`,
//! `p`, `d`, `q`); placed by their ids and numbers, of a size in cells, a
//! rectangle of their pixels, a z, the cursor moving past them or not
//! (`C=1`). The images take the terminal's store of inline images and its
//! memory. Files, temporary files and shared memory (`t=f`, `t`, `s`) are
//! refused, as a program's read of files of the user's through the
//! terminal; animation, relative placements and Unicode placeholders show
//! nothing.

use super::Terminal;
use super::images::{ImageFormat, Placing, Side, Source, base64, dimensions};
use crate::parser::StringEnd;

/// The bytes that the APC strings of a transmission's chunks keep at most,
/// those of a string past them going, as the parser's limit of APC strings
/// keeps them.
const MOST_APC: usize = 64 * 1024;

/// A command's keys, as its control data gives them, of the protocol's
/// defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Control {
    action: u8,
    quiet: u8,
    format: u32,
    medium: u8,
    width: u32,
    height: u32,
    id: u32,
    number: u32,
    placement: u32,
    compressed: bool,
    more: bool,
    columns: u32,
    rows: u32,
    source: (u32, u32, u32, u32),
    cursor_stays: bool,
    z: i32,
    delete: u8,
    cell: (u32, u32),
}

impl Default for Control {
    fn default() -> Self {
        Self {
            action: b't',
            quiet: 0,
            format: 32,
            medium: b'd',
            width: 0,
            height: 0,
            id: 0,
            number: 0,
            placement: 0,
            compressed: false,
            more: false,
            columns: 0,
            rows: 0,
            source: (0, 0, 0, 0),
            cursor_stays: false,
            z: 0,
            delete: b'a',
            cell: (0, 0),
        }
    }
}

/// The control data `text`, `key=value` between commas; a key of a value
/// that is not its kind's leaves the default.
fn control(text: &[u8]) -> Control {
    let mut control = Control::default();
    for pair in text.split(|&byte| byte == b',') {
        let Some(at) = pair.iter().position(|&byte| byte == b'=') else {
            continue;
        };
        let (key, value) = (&pair[..at], &pair[at + 1..]);
        let number = || std::str::from_utf8(value).ok()?.parse::<u32>().ok();
        let letter = || (value.len() == 1).then(|| value[0]);
        let unsigned = |field: &mut u32| {
            if let Some(number) = number() {
                *field = number;
            }
        };
        match key {
            b"a" => control.action = letter().unwrap_or(0),
            b"q" => control.quiet = number().map_or(0, |quiet| quiet.min(2) as u8),
            b"f" => unsigned(&mut control.format),
            b"t" => control.medium = letter().unwrap_or(0),
            b"s" => unsigned(&mut control.width),
            b"v" => unsigned(&mut control.height),
            b"i" => unsigned(&mut control.id),
            b"I" => unsigned(&mut control.number),
            b"p" => unsigned(&mut control.placement),
            b"o" => control.compressed = value == b"z",
            b"m" => control.more = value == b"1",
            b"c" => unsigned(&mut control.columns),
            b"r" => unsigned(&mut control.rows),
            b"x" => unsigned(&mut control.source.0),
            b"y" => unsigned(&mut control.source.1),
            b"w" => unsigned(&mut control.source.2),
            b"h" => unsigned(&mut control.source.3),
            b"C" => control.cursor_stays = value == b"1",
            b"z" => {
                if let Some(z) = std::str::from_utf8(value).ok().and_then(|z| z.parse().ok()) {
                    control.z = z;
                }
            }
            b"d" => control.delete = letter().unwrap_or(0),
            _ => {}
        }
    }
    // A deletion's cell, by `x` and `y` from 1, whatever the keys' order.
    if control.action == b'd' {
        control.cell = (control.source.0, control.source.1);
    }
    control
}

/// A transmission under way, of chunks: its first chunk's keys, and the
/// bytes of its chunks so far.
#[derive(Debug)]
struct Transmission {
    control: Control,
    data: Vec<u8>,
}

/// The protocol's state: the APC string being read, and a transmission
/// under way.
#[derive(Debug, Default)]
pub(super) struct Graphics {
    apc: Vec<u8>,
    transmission: Option<Transmission>,
}

/// Why a command failed, as the protocol's error tells it.
type Failure = &'static str;

/// An image's format, its width and its height in pixels, and its bytes.
type Decoded = ((ImageFormat, u32, u32), Vec<u8>);

impl Terminal {
    /// An APC string starts.
    pub(super) fn start_apc(&mut self) {
        self.graphics.apc.clear();
    }

    /// The next chunk of the APC string.
    pub(super) fn read_apc(&mut self, data: &[u8]) {
        let room = MOST_APC.saturating_sub(self.graphics.apc.len());
        self.graphics
            .apc
            .extend_from_slice(&data[..data.len().min(room)]);
    }

    /// The APC string ended: kitty's graphics command of a string from `G`,
    /// if it ended with ST, as a cancelled one does nothing.
    pub(super) fn end_apc(&mut self, end: StringEnd) {
        let apc = std::mem::take(&mut self.graphics.apc);
        if end == StringEnd::St
            && let Some(command) = apc.strip_prefix(b"G")
        {
            self.graphics_command(command);
        }
        if apc.capacity() <= MOST_APC {
            self.graphics.apc = apc;
        }
    }

    /// A graphics command: its control data, `;`, and its payload in
    /// base64. A chunk after a first of `m=1` takes that first's keys; the
    /// last runs the command.
    fn graphics_command(&mut self, command: &[u8]) {
        let (keys, payload) = match command.iter().position(|&byte| byte == b';') {
            Some(at) => (&command[..at], &command[at + 1..]),
            None => (command, &[][..]),
        };
        let keys = control(keys);
        let limit = self.images.limit();
        let Some(chunk) = base64(payload) else {
            let mut control = self
                .graphics
                .transmission
                .take()
                .map_or(keys, |kept| kept.control);
            control.more = false;
            self.reply_graphics(&control, Err("EINVAL:the payload is not base64"));
            return;
        };
        let (control, data) = match self.graphics.transmission.take() {
            Some(mut transmission) => {
                transmission.data.extend_from_slice(&chunk);
                if keys.more {
                    if transmission.data.len() > limit {
                        self.reply_graphics(
                            &transmission.control,
                            Err("EFBIG:the image is too large"),
                        );
                        return;
                    }
                    self.graphics.transmission = Some(transmission);
                    return;
                }
                (transmission.control, transmission.data)
            }
            None if keys.more => {
                self.graphics.transmission = Some(Transmission {
                    control: keys,
                    data: chunk,
                });
                return;
            }
            None => (keys, chunk),
        };
        let done = self.run_graphics(&control, data);
        self.reply_graphics(&control, done);
    }

    /// Runs `control`'s command, of `data`, the payload of its chunks: what
    /// its reply tells.
    fn run_graphics(&mut self, control: &Control, data: Vec<u8>) -> Result<(), Failure> {
        match control.action {
            b'q' => decoded(control, data, self.images.limit()).map(drop),
            b't' | b'T' => {
                let id = self.transmit(control, data)?;
                if control.action == b'T' {
                    self.display(control, id)?;
                }
                Ok(())
            }
            b'p' => {
                let id = self.addressed(control)?;
                self.display(control, id)
            }
            b'd' => {
                self.delete(control);
                Ok(())
            }
            _ => Err("EINVAL:the action is not supported"),
        }
    }

    /// Keeps the image that `control` transmits, of `data`: its id, the
    /// program's, or, of a number or of neither, one of the terminal's.
    fn transmit(&mut self, control: &Control, data: Vec<u8>) -> Result<u32, Failure> {
        if self.images.limit() == 0 {
            return Err("ENOSPC:the terminal keeps no images");
        }
        let (found, data) = decoded(control, data, self.images.limit())?;
        let asked = (control.id > 0).then_some(control.id);
        let id = self
            .keep_image(asked, found, data, true)
            .ok_or("ENOSPC:the image does not fit the terminal's memory")?;
        if control.number > 0 {
            self.number_image(id, control.number);
        }
        Ok(id)
    }

    /// The kept image that `control` names by its id or its number.
    fn addressed(&self, control: &Control) -> Result<u32, Failure> {
        let id = if control.id > 0 {
            Some(control.id).filter(|&id| self.keeps_image(id))
        } else if control.number > 0 {
            self.numbered_image(control.number)
        } else {
            return Err("EINVAL:no image is named");
        };
        id.ok_or("ENOENT:no such image")
    }

    /// Places the image `id` at the cursor as `control` asks.
    fn display(&mut self, control: &Control, id: u32) -> Result<(), Failure> {
        let cells = |count: u32| {
            if count > 0 {
                Side::Cells(count)
            } else {
                Side::Auto
            }
        };
        let (x, y, width, height) = control.source;
        let placing = Placing {
            width: cells(control.columns),
            height: cells(control.rows),
            preserve: control.columns == 0 || control.rows == 0,
            move_cursor: !control.cursor_stays,
            z: control.z,
            source: ((x, y, width, height) != (0, 0, 0, 0)).then_some(Source {
                x,
                y,
                width,
                height,
            }),
            number: (control.placement > 0).then_some(control.placement),
        };
        if self.place_image(id, placing) {
            Ok(())
        } else {
            Err("EINVAL:the image does not place there")
        }
    }

    /// Deletes what `control`'s `d` names, of the active screen: its
    /// placements, and, of an upper case letter, the images that they
    /// leave of none, or the image named.
    fn delete(&mut self, control: &Control) {
        let free = control.delete.is_ascii_uppercase();
        let top = self.screen_top();
        let rows = i64::from(self.rows);
        let cursor = self.cursor_cell();
        let cell_row = |y: u32| top + i64::from(y.saturating_sub(1));
        let cell_column = |x: u32| u16::try_from(x.saturating_sub(1)).unwrap_or(u16::MAX);
        let (x, y) = control.cell;
        match control.delete.to_ascii_lowercase() {
            b'a' => self.drop_placements(
                |seen| seen.row < top + rows && seen.row + i64::from(seen.rows) > top,
                free,
            ),
            b'i' => {
                let id = control.id;
                let number = (control.placement > 0).then_some(control.placement);
                self.drop_placements(
                    |seen| {
                        seen.image == id && number.is_none_or(|number| seen.number == Some(number))
                    },
                    free,
                );
                if free && number.is_none() {
                    self.drop_image(id);
                }
            }
            b'n' => {
                if let Some(id) = self.numbered_image(control.number) {
                    self.drop_placements(|seen| seen.image == id, free);
                    if free {
                        self.drop_image(id);
                    }
                }
            }
            b'c' => self.drop_placements(|seen| seen.covers(Some(cursor.0), Some(cursor.1)), free),
            b'p' => self.drop_placements(
                |seen| seen.covers(Some(cell_column(x)), Some(cell_row(y))),
                free,
            ),
            b'x' => self.drop_placements(|seen| seen.covers(Some(cell_column(x)), None), free),
            b'y' => self.drop_placements(|seen| seen.covers(None, Some(cell_row(y))), free),
            b'z' => {
                let z = control.z;
                self.drop_placements(|seen| seen.z == z, free);
            }
            _ => {}
        }
    }

    /// Replies to `control`'s command, of how it ended, as the protocol
    /// asks: to a command that names its image, by its id or its number,
    /// but where its `q` keeps quiet; `OK` or the error.
    fn reply_graphics(&mut self, control: &Control, done: Result<(), Failure>) {
        if control.id == 0 && control.number == 0 {
            return;
        }
        let quiet = match done {
            Ok(()) => control.quiet >= 1,
            Err(_) => control.quiet >= 2,
        };
        if quiet {
            return;
        }
        let mut reply = String::from("\x1b_G");
        if control.id > 0 {
            reply.push_str(&format!("i={}", control.id));
        } else if let Some(id) = self.numbered_image(control.number) {
            reply.push_str(&format!("i={id}"));
        }
        if control.number > 0 {
            if !reply.ends_with('G') {
                reply.push(',');
            }
            reply.push_str(&format!("I={}", control.number));
        }
        if control.placement > 0 {
            reply.push_str(&format!(",p={}", control.placement));
        }
        reply.push(';');
        reply.push_str(done.err().unwrap_or("OK"));
        reply.push_str("\x1b\\");
        self.reply(reply.as_bytes());
    }
}

/// The format, the size in pixels and the bytes of the image that
/// `control` transmits as `data`: of the stream alone, inflated by zlib of
/// `limit` bytes at most where it is compressed; PNG of its header's size,
/// or pixels of RGB or RGBA of `control`'s, as many bytes as they take.
fn decoded(control: &Control, data: Vec<u8>, limit: usize) -> Result<Decoded, Failure> {
    if control.medium != b'd' {
        return Err("EINVAL:only a transmission in the stream is supported");
    }
    let data = if control.compressed {
        miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&data, limit)
            .map_err(|_| "EINVAL:the zlib data does not inflate within the terminal's memory")?
    } else {
        data
    };
    match control.format {
        100 => match dimensions(&data) {
            Some((ImageFormat::Png, width, height)) => {
                Ok(((ImageFormat::Png, width, height), data))
            }
            _ => Err("EINVAL:the data is not a PNG"),
        },
        24 | 32 => {
            let (width, height) = (control.width, control.height);
            let depth = if control.format == 24 { 3 } else { 4 };
            let wanted = u64::from(width) * u64::from(height) * depth;
            if width == 0 || height == 0 || wanted != data.len() as u64 {
                return Err("EINVAL:the pixels do not match their width and height");
            }
            let format = if depth == 3 {
                ImageFormat::Rgb
            } else {
                ImageFormat::Rgba
            };
            Ok(((format, width, height), data))
        }
        _ => Err("EINVAL:the format is not supported"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;
    use crate::terminal::{Options, Snapshot};

    /// A terminal of 20 by 6 cells of 10 by 20 pixels, and its parser.
    fn made() -> (Terminal, Parser) {
        let mut terminal = Terminal::new(20, 6, Options::default());
        terminal.set_cell_size(10, 20);
        (terminal, Parser::new())
    }

    /// The graphics command of `keys` and `data`, in base64.
    fn command(keys: &str, data: &[u8]) -> Vec<u8> {
        let mut sequence = format!("\x1b_G{keys};").into_bytes();
        sequence.extend(crate::terminal::clipboard::tests_encode(data).bytes());
        sequence.extend(b"\x1b\\");
        sequence
    }

    /// The replies that `terminal` queued, as text.
    fn replies(terminal: &mut Terminal) -> String {
        String::from_utf8(terminal.take_replies()).unwrap()
    }

    /// The placements that the view shows: each image's id, its column, its
    /// row, its columns and its rows.
    fn shown(terminal: &mut Terminal) -> Vec<(u32, u16, i32, u16, u16)> {
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
            .images()
            .iter()
            .map(|placed| {
                (
                    placed.image.id,
                    placed.column,
                    placed.row,
                    placed.columns,
                    placed.rows,
                )
            })
            .collect()
    }

    /// The header of a PNG of `width` by `height` pixels.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        data.extend(width.to_be_bytes());
        data.extend(height.to_be_bytes());
        data.extend([8, 6, 0, 0, 0]);
        data
    }

    #[test]
    fn an_image_transmitted_then_displayed_shows_and_replies_ok() {
        let (mut terminal, mut parser) = made();
        // Pixels of RGBA, 30 by 20: kept, of a reply.
        parser.advance(
            &mut terminal,
            &command("a=t,f=32,s=30,v=20,i=7", &[0; 30 * 20 * 4]),
        );
        assert_eq!(replies(&mut terminal), "\x1b_Gi=7;OK\x1b\\");
        assert!(shown(&mut terminal).is_empty());
        // Displayed, of 3 columns and 1 row; the cursor past it.
        parser.advance(&mut terminal, b"\x1b_Ga=p,i=7\x1b\\");
        assert_eq!(shown(&mut terminal), [(7, 0, 0, 3, 1)]);
        assert_eq!(terminal.cursor().x, 3);
        assert_eq!(replies(&mut terminal), "\x1b_Gi=7;OK\x1b\\");
        // Of a size in cells, the cursor staying, a placement of a number
        // that a second of it replaces.
        parser.advance(&mut terminal, b"\x1b_Ga=p,i=7,p=2,c=5,r=2,C=1,q=1\x1b\\");
        parser.advance(&mut terminal, b"\x1b_Ga=p,i=7,p=2,c=4,r=2,C=1,q=1\x1b\\");
        assert_eq!(shown(&mut terminal), [(7, 0, 0, 3, 1), (7, 3, 0, 4, 2)]);
        assert_eq!(replies(&mut terminal), "");
        // An image that is not there.
        parser.advance(&mut terminal, b"\x1b_Ga=p,i=9\x1b\\");
        assert_eq!(
            replies(&mut terminal),
            "\x1b_Gi=9;ENOENT:no such image\x1b\\"
        );
    }

    #[test]
    fn an_image_comes_in_chunks_compressed_or_not() {
        let (mut terminal, mut parser) = made();
        let data = png(40, 20);
        let (first, second) = data.split_at(12);
        parser.advance(&mut terminal, &command("a=T,f=100,i=3,m=1", first));
        assert!(shown(&mut terminal).is_empty());
        parser.advance(&mut terminal, &command("m=0", second));
        assert_eq!(shown(&mut terminal), [(3, 0, 0, 4, 1)]);
        assert_eq!(replies(&mut terminal), "\x1b_Gi=3;OK\x1b\\");
        // RGB compressed by zlib.
        let pixels = vec![200; 10 * 10 * 3];
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&pixels, 6);
        parser.advance(
            &mut terminal,
            &command("a=t,f=24,s=10,v=10,o=z,i=4", &compressed),
        );
        assert_eq!(replies(&mut terminal), "\x1b_Gi=4;OK\x1b\\");
        assert_eq!(terminal.kept_images().0, [3, 4]);
    }

    #[test]
    fn what_the_subset_leaves_out_is_refused_with_the_protocol_s_error() {
        let (mut terminal, mut parser) = made();
        // A file of the user's, which the terminal would read.
        parser.advance(&mut terminal, &command("a=t,t=f,i=1", b"/etc/passwd"));
        assert_eq!(
            replies(&mut terminal),
            "\x1b_Gi=1;EINVAL:only a transmission in the stream is supported\x1b\\"
        );
        // Pixels that their size does not take, another format, and an
        // animation's frame.
        parser.advance(&mut terminal, &command("a=t,f=32,s=10,v=10,i=2", &[0; 10]));
        assert!(replies(&mut terminal).contains("EINVAL:the pixels do not match"));
        parser.advance(&mut terminal, &command("a=t,f=7,i=2", b"x"));
        assert!(replies(&mut terminal).contains("EINVAL:the format is not supported"));
        parser.advance(&mut terminal, b"\x1b_Ga=f,i=2\x1b\\");
        assert!(replies(&mut terminal).contains("EINVAL:the action is not supported"));
        // Quiet of errors too, and no reply of no id.
        parser.advance(&mut terminal, &command("a=t,t=s,i=3,q=2", b"x"));
        parser.advance(&mut terminal, &command("a=t,t=s", b"x"));
        assert_eq!(replies(&mut terminal), "");
        assert!(terminal.kept_images().0.is_empty());
    }

    #[test]
    fn a_query_checks_an_image_and_keeps_nothing() {
        let (mut terminal, mut parser) = made();
        parser.advance(&mut terminal, &command("a=q,f=100,i=31", &png(2, 2)));
        assert_eq!(replies(&mut terminal), "\x1b_Gi=31;OK\x1b\\");
        parser.advance(&mut terminal, &command("a=q,f=100,i=31", b"not a png"));
        assert_eq!(
            replies(&mut terminal),
            "\x1b_Gi=31;EINVAL:the data is not a PNG\x1b\\"
        );
        assert!(terminal.kept_images().0.is_empty());
    }

    #[test]
    fn deletions_take_placements_and_upper_case_ones_the_images_too() {
        let (mut terminal, mut parser) = made();
        let pixels = [0; 10 * 20 * 4];
        parser.advance(
            &mut terminal,
            &command("a=T,f=32,s=10,v=20,i=1,q=1", &pixels),
        );
        parser.advance(
            &mut terminal,
            &command("a=T,f=32,s=10,v=20,i=2,q=1,z=5", &pixels),
        );
        assert_eq!(shown(&mut terminal).len(), 2);
        // By z, the placement alone: the image stays.
        parser.advance(&mut terminal, b"\x1b_Ga=d,d=z,z=5\x1b\\");
        assert_eq!(shown(&mut terminal), [(1, 0, 0, 1, 1)]);
        assert_eq!(terminal.kept_images().0, [1, 2]);
        // At a cell, from 1, and by id of an upper case letter.
        parser.advance(&mut terminal, b"\x1b_Ga=d,d=p,x=1,y=1\x1b\\");
        assert!(shown(&mut terminal).is_empty());
        parser.advance(&mut terminal, b"\x1b_Ga=d,d=I,i=2\x1b\\");
        assert_eq!(terminal.kept_images().0, [1]);
        // Every one of the screen's, the images too.
        parser.advance(&mut terminal, b"\x1b_Ga=p,i=1,q=1\x1b\\");
        parser.advance(&mut terminal, b"\x1b_Ga=d,d=A\x1b\\");
        assert!(shown(&mut terminal).is_empty());
        assert!(terminal.kept_images().0.is_empty());
    }

    #[test]
    fn an_image_numbered_by_the_program_is_found_by_its_number() {
        let (mut terminal, mut parser) = made();
        parser.advance(&mut terminal, &command("a=t,f=100,I=13", &png(10, 20)));
        let reply = replies(&mut terminal);
        assert!(
            reply.starts_with("\x1b_Gi=") && reply.ends_with(",I=13;OK\x1b\\"),
            "{reply}"
        );
        parser.advance(&mut terminal, b"\x1b_Ga=p,I=13,q=1\x1b\\");
        assert_eq!(shown(&mut terminal).len(), 1);
    }
}
