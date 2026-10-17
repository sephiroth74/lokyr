//! The clipboard: what programs write to it, and read from it when the
//! configuration allows, with OSC 52.

use super::{Event, Terminal};
use crate::parser::StringEnd;

/// The clipboards that OSC 52 names: `c` the clipboard, and `p` and `s`
/// the primary selection. The cut buffers, `0` to `7`, are left out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipboardTarget {
    /// The clipboard.
    pub clipboard: bool,
    /// The primary selection.
    pub primary: bool,
}

/// A program's request to read a clipboard, which the embedder answers
/// with [`Terminal::reply_clipboard`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardRequest {
    /// The clipboards it asks for.
    pub target: ClipboardTarget,
    /// Whether the request ended with BEL, as the reply must.
    bel: bool,
}

impl Terminal {
    /// OSC 52: `targets;data`, where the data is base64 to write to the
    /// clipboards, or `?` to read them. A read becomes an event only when
    /// the configuration allows it; otherwise it gets no reply.
    pub(super) fn clipboard(&mut self, fields: &[u8], end: StringEnd) {
        let Some(at) = fields.iter().position(|&byte| byte == b';') else {
            return;
        };
        let Some(target) = target(&fields[..at]) else {
            return;
        };
        let data = &fields[at + 1..];
        if data == b"?" {
            if self.clipboard_reads {
                let request = ClipboardRequest {
                    target,
                    bel: end == StringEnd::Bel,
                };
                self.events.push(Event::ClipboardRead(request));
            }
            return;
        }
        if let Some(data) = decode(data) {
            self.events.push(Event::ClipboardWrite { target, data });
        }
    }

    /// Answers `request` with `data`, what the clipboard holds: the answer
    /// waits with the replies, whole, past their limit, as the user allowed
    /// it. An embedder that writes it itself, at once, takes it from
    /// [`Terminal::clipboard_answer`].
    pub fn reply_clipboard(&mut self, request: ClipboardRequest, data: &[u8]) {
        self.answer(&Self::clipboard_answer(request, data));
    }

    /// The answer to `request`, with `data`, what the clipboard holds, for
    /// the program.
    pub fn clipboard_answer(request: ClipboardRequest, data: &[u8]) -> Vec<u8> {
        let letter = if request.target.clipboard { 'c' } else { 'p' };
        let end = if request.bel {
            StringEnd::Bel
        } else {
            StringEnd::St
        };
        super::colors::osc(&format!("52;{letter};{}", encode(data)), end).into_bytes()
    }
}

/// The clipboards that the letters of OSC 52 name; none names the
/// clipboard.
fn target(letters: &[u8]) -> Option<ClipboardTarget> {
    if letters.is_empty() {
        return Some(ClipboardTarget {
            clipboard: true,
            primary: false,
        });
    }
    let mut target = ClipboardTarget::default();
    for letter in letters {
        match letter {
            b'c' => target.clipboard = true,
            b'p' | b's' => target.primary = true,
            _ => {}
        }
    }
    (target.clipboard || target.primary).then_some(target)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `data` in base64, padded.
fn encode(data: &[u8]) -> String {
    let mut text = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let bits = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        for index in 0..4 {
            if index <= chunk.len() {
                let digit = (bits >> (18 - 6 * index)) & 0x3F;
                text.push(char::from(ALPHABET[digit as usize]));
            } else {
                text.push('=');
            }
        }
    }
    text
}

/// The bytes that `text`, base64 with or without its padding, stands for.
fn decode(text: &[u8]) -> Option<Vec<u8>> {
    let padding = text.iter().rev().take_while(|&&byte| byte == b'=').count();
    if padding > 2 {
        return None;
    }
    let digits = &text[..text.len() - padding];
    if digits.len() % 4 == 1 {
        return None;
    }
    let mut data = Vec::with_capacity(digits.len() * 3 / 4);
    let (mut bits, mut count) = (0_u32, 0);
    for &byte in digits {
        let value = ALPHABET.iter().position(|&digit| digit == byte)? as u32;
        bits = (bits << 6) | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            data.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    Some(data)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn base64_follows_the_standard_alphabet() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(&[0xFB, 0xFF]), "+/8=");
        assert_eq!(decode(b"Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(decode(b"Zm8").unwrap(), b"fo", "without the padding");
        assert_eq!(decode(b"Zm8=").unwrap(), b"fo");
        assert_eq!(decode(b"").unwrap(), b"");
        assert_eq!(decode(b"Zm9v!"), None);
        assert_eq!(decode(b"Z"), None);
        assert_eq!(decode(b"Zg==="), None);
    }

    proptest! {
        #[test]
        fn base64_gives_back_what_it_encoded(data in prop::collection::vec(any::<u8>(), 0..64)) {
            prop_assert_eq!(decode(encode(&data).as_bytes()), Some(data));
        }
    }
}
