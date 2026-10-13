//! The size limits of control strings.

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;

/// The most bytes that the payload of a control string can have.
///
/// Past its limit, a string ends as [`StringEnd::TooLong`], and the parser
/// consumes the rest of it without buffering it. The default is 64 KiB for
/// every string, except OSC 52, which carries clipboard contents and gets
/// 1 MiB.
///
/// The limit of an OSC string can depend on its number, as `52` opens
/// `52;c;...`. Until the parser has read that number, the string has the
/// limit of OSC strings in general.
///
/// [`StringEnd::TooLong`]: super::StringEnd::TooLong
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// OSC strings whose number has no limit of its own.
    pub osc: usize,
    /// OSC strings with a limit of their own, by number. The first entry
    /// for a number wins.
    pub osc_commands: Vec<(u16, usize)>,
    /// DCS strings.
    pub dcs: usize,
    /// APC strings.
    pub apc: usize,
}

impl Limits {
    /// The limit of an OSC string with `number`, or without a number.
    pub(crate) fn osc_limit(&self, number: Option<u16>) -> usize {
        number
            .and_then(|number| self.osc_commands.iter().find(|&&(n, _)| n == number))
            .map_or(self.osc, |&(_, limit)| limit)
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            osc: 64 * KIB,
            osc_commands: vec![(52, MIB)],
            dcs: 64 * KIB,
            apc: 64 * KIB,
        }
    }
}

/// Reads the number that opens an OSC payload, one chunk at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OscNumber {
    /// The digits read so far, if any, and their value.
    Reading { digits: bool, value: u16 },
    /// The number has been read, or the payload has none.
    #[default]
    Done,
}

impl OscNumber {
    /// Ready for the payload of a new OSC string.
    pub(crate) fn start() -> Self {
        Self::Reading {
            digits: false,
            value: 0,
        }
    }

    /// Reads from the start of the next chunk of the payload. Once the number
    /// is known, which happens once, returns how many bytes of the chunk it
    /// took to know it, and the limit of the string.
    ///
    /// The number is made of the digits before the first `;`. Without them,
    /// or with a value too large for a `u16`, the payload has no number.
    pub(crate) fn read(&mut self, chunk: &[u8], limits: &Limits) -> Option<(usize, usize)> {
        let Self::Reading {
            mut digits,
            mut value,
        } = *self
        else {
            return None;
        };
        for (index, &byte) in chunk.iter().enumerate() {
            let number = match byte {
                b'0'..=b'9' => {
                    let next = value
                        .checked_mul(10)
                        .and_then(|value| value.checked_add(u16::from(byte - b'0')));
                    if let Some(next) = next {
                        value = next;
                        digits = true;
                        continue;
                    }
                    None
                }
                b';' if digits => Some(value),
                _ => None,
            };
            *self = Self::Done;
            return Some((index + 1, limits.osc_limit(number)));
        }
        *self = Self::Reading { digits, value };
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc_52_gets_more_room_by_default() {
        let limits = Limits::default();
        assert_eq!(limits.osc_limit(Some(52)), MIB);
        assert_eq!(limits.osc_limit(Some(0)), 64 * KIB);
        assert_eq!(limits.osc_limit(None), 64 * KIB);
    }

    /// What `chunks` tell about an OSC string, with OSC 52 at 52 bytes and
    /// every other OSC string at 1 byte: which chunk makes the number known,
    /// how many of its bytes it takes, and the limit.
    fn read_from(chunks: &[&[u8]]) -> Option<(usize, usize, usize)> {
        let limits = Limits {
            osc: 1,
            osc_commands: vec![(52, 52)],
            ..Limits::default()
        };
        let mut number = OscNumber::start();
        let found: Vec<(usize, usize, usize)> = chunks
            .iter()
            .enumerate()
            .filter_map(|(index, chunk)| {
                let (read, limit) = number.read(chunk, &limits)?;
                Some((index, read, limit))
            })
            .collect();
        assert!(found.len() <= 1, "the limit is known once");
        found.first().copied()
    }

    #[test]
    fn the_number_comes_before_the_first_semicolon() {
        assert_eq!(read_from(&[b"52;c;?"]), Some((0, 3, 52)));
        assert_eq!(read_from(&[b"5", b"2", b";c", b";?"]), Some((2, 1, 52)));
        assert_eq!(read_from(&[b"052;"]), Some((0, 4, 52)));
        assert_eq!(read_from(&[b"2;title"]), Some((0, 2, 1)));
    }

    #[test]
    fn a_payload_without_a_number_gets_the_general_limit() {
        assert_eq!(read_from(&[b";52"]), Some((0, 1, 1)));
        assert_eq!(read_from(&[b"5x;"]), Some((0, 2, 1)));
        assert_eq!(read_from(&[b"65552;"]), Some((0, 5, 1)));
    }

    #[test]
    fn a_number_that_ends_the_payload_is_never_needed() {
        // As in OSC 104, which resets the palette.
        assert_eq!(read_from(&[b"104"]), None);
    }
}
