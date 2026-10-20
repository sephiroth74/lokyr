//! The interface between the parser and what it drives.

use super::Params;

/// How a control string (OSC, DCS or APC) ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringEnd {
    /// The string terminator, ST (`ESC \`).
    St,
    /// BEL, which xterm also accepts at the end of an OSC string. A reply
    /// to a query ends the way the query did.
    Bel,
    /// CAN or SUB aborted the string.
    Cancelled,
    /// The payload would pass its size limit, set by
    /// [`Limits`](super::Limits). The parser drops the rest of the string,
    /// so the handler must discard what it received.
    TooLong,
}

/// Receives what the parser finds in the byte stream.
///
/// Every method does nothing by default, so a handler implements only what
/// it needs.
///
/// A control string arrives as a start, any number of chunks and an end.
/// The parser buffers nothing, so a chunk is a run of payload bytes from a
/// single read, and chunks split the payload at arbitrary points. A string
/// that would pass its size limit ends as [`StringEnd::TooLong`], and the
/// rest of it never reaches the handler.
#[allow(unused_variables)]
pub trait Handler {
    /// Prints a character.
    fn print(&mut self, c: char) {}

    /// Prints a run of printable ASCII (`0x20..=0x7E`), which the parser
    /// found in a single read. By default, prints its characters one by one.
    fn print_ascii(&mut self, text: &[u8]) {
        for &byte in text {
            self.print(char::from(byte));
        }
    }

    /// Prints a run of characters decoded from UTF-8, none of them ASCII or
    /// a C1 control, which the parser found in a single read. By default,
    /// prints them one by one.
    fn print_chars(&mut self, chars: &[char]) {
        for &c in chars {
            self.print(c);
        }
    }

    /// Executes a C0 control character, such as LF or BS.
    fn execute(&mut self, byte: u8) {}

    /// Dispatches an escape sequence: `ESC`, the `intermediates`, then the
    /// final byte.
    fn esc_dispatch(&mut self, intermediates: &[u8], final_byte: u8) {}

    /// Dispatches a control sequence: `CSI`, the parameters, the
    /// `intermediates`, then the final byte. A private marker (`?`, `>`, `<`
    /// or `=`) comes first among the intermediates.
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {}

    /// Starts an OSC string.
    fn osc_start(&mut self) {}

    /// Receives the next chunk of the OSC string.
    fn osc_put(&mut self, data: &[u8]) {}

    /// Ends the OSC string.
    fn osc_end(&mut self, end: StringEnd) {}

    /// Starts a DCS string, with the parameters, intermediates and final
    /// byte that introduce it, as for [`Handler::csi_dispatch`].
    fn dcs_start(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {}

    /// Receives the next chunk of the DCS string.
    fn dcs_put(&mut self, data: &[u8]) {}

    /// Ends the DCS string.
    fn dcs_end(&mut self, end: StringEnd) {}

    /// Starts an APC string. SOS and PM strings are consumed without
    /// reaching the handler.
    fn apc_start(&mut self) {}

    /// Receives the next chunk of the APC string.
    fn apc_put(&mut self, data: &[u8]) {}

    /// Ends the APC string.
    fn apc_end(&mut self, end: StringEnd) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handler that implements nothing but `print`.
    struct Text(String);

    impl Handler for Text {
        fn print(&mut self, c: char) {
            self.0.push(c);
        }
    }

    #[test]
    fn ascii_runs_print_one_byte_at_a_time_by_default() {
        let mut text = Text(String::new());
        text.print_ascii(b"hello");
        text.execute(b'\n');
        text.print('!');
        assert_eq!(text.0, "hello!");
    }
}
