//! The behaviour of the parser, case by case. Where it agrees with `vte`,
//! the reference tests cover every transition too.

use super::{Action, Parser, Recorder, StringEnd};

fn parse(input: &[u8]) -> Vec<Action> {
    parse_in_reads(&[input])
}

fn parse_in_reads(reads: &[&[u8]]) -> Vec<Action> {
    let mut parser = Parser::new();
    let mut recorder = Recorder::new();
    for read in reads {
        parser.advance(&mut recorder, read);
    }
    recorder.into_actions()
}

fn print(text: &str) -> Action {
    Action::Print(text.to_owned())
}

fn esc(intermediates: &[u8], final_byte: u8) -> Action {
    Action::Esc {
        intermediates: intermediates.to_vec(),
        final_byte,
    }
}

fn csi(params: &[&[u16]], intermediates: &[u8], final_byte: u8) -> Action {
    Action::Csi {
        params: params.iter().map(|group| group.to_vec()).collect(),
        intermediates: intermediates.to_vec(),
        final_byte,
    }
}

fn osc(payload: &[u8], end: StringEnd) -> Action {
    Action::Osc {
        payload: payload.to_vec(),
        end,
    }
}

fn dcs(params: &[&[u16]], intermediates: &[u8], final_byte: u8, payload: &[u8]) -> Action {
    Action::Dcs {
        params: params.iter().map(|group| group.to_vec()).collect(),
        intermediates: intermediates.to_vec(),
        final_byte,
        payload: payload.to_vec(),
        end: StringEnd::St,
    }
}

fn apc(payload: &[u8], end: StringEnd) -> Action {
    Action::Apc {
        payload: payload.to_vec(),
        end,
    }
}

#[test]
fn text_prints_and_c0_controls_execute() {
    assert_eq!(
        parse(b"ab\r\n\x00c"),
        [
            print("ab"),
            Action::Execute(b'\r'),
            Action::Execute(b'\n'),
            Action::Execute(0),
            print("c"),
        ]
    );
}

#[test]
fn del_is_ignored_in_text() {
    assert_eq!(parse(b"a\x7fb"), [print("ab")]);
}

#[test]
fn escape_sequences_dispatch_with_their_intermediates() {
    assert_eq!(
        parse(b"\x1b7\x1b(B\x1b#8\x1b(%5"),
        [
            esc(b"", b'7'),
            esc(b"(", b'B'),
            esc(b"#", b'8'),
            esc(b"(%", b'5'),
        ]
    );
}

#[test]
fn control_sequences_dispatch_with_their_parameters() {
    assert_eq!(
        parse(b"\x1b[m\x1b[1;31m\x1b[;5H\x1b[?25l\x1b[2 q\x1b[38:2::255:0:0m"),
        [
            csi(&[&[0]], b"", b'm'),
            csi(&[&[1], &[31]], b"", b'm'),
            csi(&[&[0], &[5]], b"", b'H'),
            csi(&[&[25]], b"?", b'l'),
            csi(&[&[2]], b" ", b'q'),
            csi(&[&[38, 2, 0, 255, 0, 0]], b"", b'm'),
        ]
    );
}

#[test]
fn c0_controls_inside_a_sequence_execute_without_ending_it() {
    assert_eq!(
        parse(b"\x1b[1\n2A\x1b\r7\x1b(\x08B"),
        [
            Action::Execute(b'\n'),
            csi(&[&[12]], b"", b'A'),
            Action::Execute(b'\r'),
            esc(b"", b'7'),
            Action::Execute(0x08),
            esc(b"(", b'B'),
        ]
    );
}

#[test]
fn can_and_sub_abort_a_sequence_and_execute() {
    assert_eq!(
        parse(b"\x1b[1\x18x\x1b(\x1ay"),
        [
            Action::Execute(0x18),
            print("x"),
            Action::Execute(0x1A),
            print("y"),
        ]
    );
}

#[test]
fn esc_restarts_a_sequence() {
    assert_eq!(
        parse(b"\x1b[1;2\x1b[3A\x1b\x1b7"),
        [csi(&[&[3]], b"", b'A'), esc(b"", b'7')]
    );
}

#[test]
fn malformed_control_sequences_are_consumed() {
    // A private marker after a parameter, then a parameter after an
    // intermediate.
    assert_eq!(parse(b"\x1b[1?hx\x1b[ 1qy"), [print("xy")]);
}

#[test]
fn sequences_with_too_many_intermediates_are_ignored() {
    assert_eq!(
        parse(b"\x1b(%!Ba\x1b[?1$!pb\x1bP=1$!qdata\x1b\\c"),
        [print("abc")]
    );
}

#[test]
fn osc_strings_end_with_st_or_bel() {
    assert_eq!(
        parse(b"\x1b]0;title\x07\x1b]2;caf\xc3\xa9\x1b\\"),
        [
            osc(b"0;title", StringEnd::Bel),
            osc("2;café".as_bytes(), StringEnd::St),
        ]
    );
}

#[test]
fn osc_strings_drop_c0_controls_but_keep_del() {
    assert_eq!(
        parse(b"\x1b]0;a\x01\x0db\x7f\x07"),
        [osc(b"0;ab\x7f", StringEnd::Bel)]
    );
}

#[test]
fn dcs_strings_keep_their_introducer() {
    assert_eq!(
        parse(b"\x1bP1$qm\x1b\\\x1bPq#0\x1b\\"),
        [
            dcs(&[&[1]], b"$", b'q', b"m"),
            dcs(&[&[0]], b"", b'q', b"#0"),
        ]
    );
}

#[test]
fn dcs_payloads_keep_c0_controls_and_drop_del() {
    assert_eq!(
        parse(b"\x1bPq\x01a\x07\x7fb\x1b\\"),
        [dcs(&[&[0]], b"", b'q', b"\x01a\x07b")]
    );
}

#[test]
fn dcs_payloads_keep_bytes_from_0x80_up() {
    for byte in 0x80..=u8::MAX {
        let input = [b"\x1bPq".as_slice(), &[byte], b"\x1b\\"].concat();
        assert_eq!(
            parse(&input),
            [dcs(&[&[0]], b"", b'q', &[byte])],
            "{byte:#04x}"
        );
    }
}

#[test]
fn apc_strings_reach_the_handler() {
    assert_eq!(
        parse(b"\x1b_Gf=100;\x01AAAA\xff\x1b\\"),
        [apc(b"Gf=100;AAAA\xff", StringEnd::St)]
    );
}

#[test]
fn sos_and_pm_strings_are_dropped() {
    assert_eq!(
        parse(b"\x1bXsos\x1b\\\x1b^pm\x07still pm\x1b\\x"),
        [print("x")]
    );
}

#[test]
fn malformed_dcs_strings_are_consumed() {
    assert_eq!(parse(b"\x1bP1<qdata\x1b\\x"), [print("x")]);
}

#[test]
fn can_and_sub_cancel_a_string() {
    assert_eq!(
        parse(b"\x1b]0;a\x18b\x1bPqc\x1ad\x1b_e\x18f"),
        [
            osc(b"0;a", StringEnd::Cancelled),
            Action::Execute(0x18),
            print("b"),
            Action::Dcs {
                params: vec![vec![0]],
                intermediates: Vec::new(),
                final_byte: b'q',
                payload: b"c".to_vec(),
                end: StringEnd::Cancelled,
            },
            Action::Execute(0x1A),
            print("d"),
            apc(b"e", StringEnd::Cancelled),
            Action::Execute(0x18),
            print("f"),
        ]
    );
}

#[test]
fn the_backslash_of_st_is_consumed_after_a_string_only() {
    assert_eq!(parse(b"\x1b]0;a\x1b\\"), [osc(b"0;a", StringEnd::St)]);
    assert_eq!(parse(b"\x1bXa\x1b\\"), Vec::new());
    // On its own, ST is an escape sequence like any other.
    assert_eq!(parse(b"\x1b\\"), [esc(b"", b'\\')]);
    // So it is after a second ESC, which starts a new sequence.
    assert_eq!(
        parse(b"\x1b]0;a\x1b\x1b\\"),
        [osc(b"0;a", StringEnd::St), esc(b"", b'\\')]
    );
    // C0 controls between ESC and `\` execute, as in any escape sequence.
    assert_eq!(
        parse(b"\x1b]0;a\x1b\r\\"),
        [osc(b"0;a", StringEnd::St), Action::Execute(b'\r')]
    );
}

#[test]
fn an_escape_after_a_string_can_start_anything() {
    assert_eq!(
        parse(b"\x1b]0;a\x1b[1m\x1bP1$qb\x1b(B\x1b_c\x1b]1;d\x07"),
        [
            osc(b"0;a", StringEnd::St),
            csi(&[&[1]], b"", b'm'),
            // The intermediates of the string do not carry over.
            dcs(&[&[1]], b"$", b'q', b"b"),
            esc(b"(", b'B'),
            apc(b"c", StringEnd::St),
            osc(b"1;d", StringEnd::Bel),
        ]
    );
}

#[test]
fn raw_c1_bytes_are_not_controls() {
    // CSI and ST as single bytes: invalid UTF-8 in text, payload in strings.
    assert_eq!(parse(b"\x9b1m"), [print("\u{FFFD}1m")]);
    assert_eq!(
        parse(b"\x1b]0;a\x9cb\x07"),
        [osc(b"0;a\x9cb", StringEnd::Bel)]
    );
}

#[test]
fn utf8_text_prints() {
    let text = "héllo ✓ 🎉 日本語";
    assert_eq!(parse(text.as_bytes()), [print(text)]);
}

#[test]
fn utf8_characters_can_be_split_across_reads() {
    assert_eq!(
        parse_in_reads(&[b"a\xe2", b"\x82", b"\xacb\xf0\x9f", b"\x8e\x89"]),
        [print("a€b🎉")]
    );
}

#[test]
fn every_lone_byte_from_0x80_up_is_invalid() {
    for byte in 0x80..=u8::MAX {
        assert_eq!(parse(&[byte, b'x']), [print("\u{FFFD}x")], "{byte:#04x}");
    }
}

#[test]
fn an_interrupted_utf8_character_is_invalid() {
    assert_eq!(
        parse(b"\xe2\x82x\xe2\x82\x1b[m\xf0\x9f\xe2\x82\xac"),
        [
            print("\u{FFFD}x\u{FFFD}"),
            csi(&[&[0]], b"", b'm'),
            print("\u{FFFD}€"),
        ]
    );
}

#[test]
fn c1_code_points_are_dropped() {
    assert_eq!(parse("a\u{9b}b\u{85}c".as_bytes()), [print("abc")]);
}

#[test]
fn sequences_can_be_split_across_reads() {
    assert_eq!(
        parse_in_reads(&[b"\x1b", b"[1", b";2H\x1b]0;t", b"itle\x1b", b"\\x"]),
        [
            csi(&[&[1], &[2]], b"", b'H'),
            osc(b"0;title", StringEnd::St),
            print("x"),
        ]
    );
}
