//! The behaviour of the parser, case by case. Where it agrees with `vte`,
//! the reference tests cover every transition too.

use super::{Action, Handler, Limits, MAX_PARAMS, Params, Parser, Recorder, StringEnd};

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

/// What a handler receives as text: each run of ASCII, as one piece, and
/// each other character on its own.
#[derive(Default)]
struct Runs(Vec<String>);

impl Handler for Runs {
    fn print(&mut self, c: char) {
        self.0.push(c.to_string());
    }

    fn print_ascii(&mut self, text: &[u8]) {
        self.0
            .push(String::from_utf8(text.to_vec()).expect("ASCII"));
    }
}

fn runs_in_reads(reads: &[&[u8]]) -> Vec<String> {
    let mut parser = Parser::new();
    let mut runs = Runs::default();
    for read in reads {
        parser.advance(&mut runs, read);
    }
    runs.0
}

#[test]
fn printable_ascii_arrives_in_runs() {
    let text = "a run of more than sixteen bytes";
    assert_eq!(runs_in_reads(&[text.as_bytes()]), [text]);
    assert_eq!(
        runs_in_reads(&["ab\r\ncd\x1b[1mef\x7fgh\u{E9}ij~".as_bytes()]),
        ["ab", "cd", "ef", "gh", "\u{E9}", "ij~"]
    );
    // A run ends with the read, and a sequence split across reads is still
    // a sequence.
    assert_eq!(
        runs_in_reads(&[b"abc", b"def\x1b", b"[mxyz"]),
        ["abc", "def", "xyz"]
    );
}

#[test]
fn only_printable_ascii_prints_and_only_in_the_ground_state() {
    // The runs of ASCII rest on it: the scan stands for the state machine.
    use super::State;
    use super::table::{Action, STATES};

    for state in STATES {
        for byte in 0..=u8::MAX {
            let transition = super::TABLE[state as usize][usize::from(byte)];
            let printable = state == State::Ground && (0x20..=0x7E).contains(&byte);
            assert_eq!(
                transition.action == Action::Print,
                printable,
                "{state:?}, {byte:#x}"
            );
            if printable {
                assert_eq!(transition.next, State::Ground, "{byte:#x}");
            }
        }
    }
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
    // The first and last character of each length, and of each lead byte
    // whose second byte has a range of its own. U+0080 to U+009F are left
    // out, since the parser drops them.
    let chars = [
        'A',
        '\u{7F}',
        '\u{A0}',
        '\u{7FF}',
        '\u{800}',
        '\u{FFF}',
        '\u{D000}',
        '\u{D7FF}',
        '\u{E000}',
        '\u{FFFF}',
        '\u{10000}',
        '\u{3FFFF}',
        '\u{40000}',
        '\u{100000}',
        '\u{10FFFF}',
    ];
    for c in chars {
        let text = format!("a{c}b");
        let expected = [print(&text.replace('\u{7F}', ""))];
        for reads in every_split(text.as_bytes()) {
            assert_eq!(parse_in_reads(&reads), expected, "{reads:02x?}");
        }
    }
}

#[test]
fn invalid_utf8_decodes_the_same_at_every_split() {
    let cases: [(&[u8], Vec<Action>); 7] = [
        // Characters cut short by ASCII, by another character, and by an
        // escape sequence.
        (b"\xe2\x82x", vec![print("\u{FFFD}x")]),
        (b"\xf0\x9f\xe2\x82\xac", vec![print("\u{FFFD}€")]),
        (
            b"\xe2\x82\x1b[m",
            vec![print("\u{FFFD}"), csi(&[&[0]], b"", b'm')],
        ),
        // An overlong form, a surrogate, a value past U+10FFFF, lone bytes.
        (b"\xc0\xaf", vec![print(&"\u{FFFD}".repeat(2))]),
        (b"\xed\xa0\x80", vec![print(&"\u{FFFD}".repeat(3))]),
        (b"\xf4\x90\x80\x80", vec![print(&"\u{FFFD}".repeat(4))]),
        (b"\x80\xbf\xfe\xff", vec![print(&"\u{FFFD}".repeat(4))]),
    ];
    for (input, expected) in cases {
        for reads in every_split(input) {
            assert_eq!(parse_in_reads(&reads), expected, "{reads:02x?}");
        }
    }
}

/// Every way to split `input` into reads, one per subset of the gaps
/// between its bytes.
fn every_split(input: &[u8]) -> impl Iterator<Item = Vec<&[u8]>> {
    let gaps = input.len().saturating_sub(1);
    (0..1_u32 << gaps).map(move |cuts| {
        let mut reads = Vec::new();
        let mut start = 0;
        for end in 1..input.len() {
            if cuts >> (end - 1) & 1 == 1 {
                reads.push(&input[start..end]);
                start = end;
            }
        }
        reads.push(&input[start..]);
        reads
    })
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

/// The private markers that can open a CSI or DCS sequence.
const MARKERS: [u8; 4] = *b"?><=";

/// `count` parameters numbered from 1, separated by `;`.
fn numbered(count: u16) -> String {
    let values: Vec<String> = (1..=count).map(|value| value.to_string()).collect();
    values.join(";")
}

/// The groups that [`numbered`] makes, as the parser keeps them.
fn numbered_groups(count: u16) -> Vec<Vec<u16>> {
    (1..=count).map(|value| vec![value]).collect()
}

#[test]
fn colon_colors_keep_their_sub_parameters_in_one_group() {
    assert_eq!(
        parse(b"\x1b[38:2::255:128:0m\x1b[48:2:1:1:2:3m\x1b[38:5:196m"),
        [
            csi(&[&[38, 2, 0, 255, 128, 0]], b"", b'm'),
            csi(&[&[48, 2, 1, 1, 2, 3]], b"", b'm'),
            csi(&[&[38, 5, 196]], b"", b'm'),
        ]
    );
}

#[test]
fn semicolon_colors_are_separate_parameters() {
    assert_eq!(
        parse(b"\x1b[38;2;255;128;0m"),
        [csi(&[&[38], &[2], &[255], &[128], &[0]], b"", b'm')]
    );
}

#[test]
fn underline_styles_are_sub_parameters() {
    assert_eq!(
        parse(b"\x1b[4:3m\x1b[4:0m\x1b[1;4:5;58:2::10:20:30m"),
        [
            csi(&[&[4, 3]], b"", b'm'),
            csi(&[&[4, 0]], b"", b'm'),
            csi(&[&[1], &[4, 5], &[58, 2, 0, 10, 20, 30]], b"", b'm'),
        ]
    );
}

#[test]
fn empty_values_read_as_zero() {
    assert_eq!(
        parse(b"\x1b[m\x1b[;5H\x1b[5;H\x1b[;;m\x1b[:1m\x1b[1:m\x1b[1::2m"),
        [
            csi(&[&[0]], b"", b'm'),
            csi(&[&[0], &[5]], b"", b'H'),
            csi(&[&[5], &[0]], b"", b'H'),
            csi(&[&[0], &[0], &[0]], b"", b'm'),
            csi(&[&[0, 1]], b"", b'm'),
            csi(&[&[1, 0]], b"", b'm'),
            csi(&[&[1, 0, 2]], b"", b'm'),
        ]
    );
}

#[test]
fn empty_zero_and_missing_values_take_the_default() {
    /// Reads the first parameter of each sequence with a default of 1, as
    /// cursor movements do.
    struct Counts(Vec<u16>);

    impl Handler for Counts {
        fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _final_byte: u8) {
            self.0.push(params.value_or(0, 1));
        }
    }

    let mut counts = Counts(Vec::new());
    Parser::new().advance(&mut counts, b"\x1b[A\x1b[0A\x1b[1A\x1b[;A\x1b[3A");
    assert_eq!(counts.0, [1, 1, 1, 1, 3]);
}

#[test]
fn values_too_large_for_a_u16_stop_at_the_maximum() {
    assert_eq!(
        parse(b"\x1b[65535;65536;99999999999A\x1b[0007A"),
        [
            csi(&[&[u16::MAX], &[u16::MAX], &[u16::MAX]], b"", b'A'),
            csi(&[&[7]], b"", b'A'),
        ]
    );
}

#[test]
fn values_past_the_limit_are_dropped_and_the_sequence_still_dispatches() {
    let input = format!("\x1b[?{}$p\x1b[5A", numbered(40));
    assert_eq!(
        parse(input.as_bytes()),
        [
            Action::Csi {
                params: numbered_groups(32),
                intermediates: b"?$".to_vec(),
                final_byte: b'p',
            },
            csi(&[&[5]], b"", b'A'),
        ]
    );
}

#[test]
fn sub_parameters_count_towards_the_limit() {
    // 31 values, then a group of three: only its first value fits.
    let input = format!("\x1b[{};7:8:9m", numbered(31));
    let mut groups = numbered_groups(31);
    groups.push(vec![7]);
    assert_eq!(
        parse(input.as_bytes()),
        [Action::Csi {
            params: groups,
            intermediates: Vec::new(),
            final_byte: b'm',
        }]
    );

    // 16 groups of two values fill the limit, and a 17th is dropped whole.
    let input = format!("\x1b[{}2m", "1:1;".repeat(16));
    assert_eq!(
        parse(input.as_bytes()),
        [Action::Csi {
            params: vec![vec![1, 1]; MAX_PARAMS / 2],
            intermediates: Vec::new(),
            final_byte: b'm',
        }]
    );
}

#[test]
fn every_private_marker_comes_first_among_the_intermediates() {
    for marker in MARKERS {
        let input = [
            b"\x1b[".as_slice(),
            &[marker],
            b"1;2h\x1b[",
            &[marker],
            b"u\x1b[",
            &[marker],
            b"2026$p\x1bP",
            &[marker],
            b"1qdata\x1b\\",
        ]
        .concat();
        assert_eq!(
            parse(&input),
            [
                csi(&[&[1], &[2]], &[marker], b'h'),
                csi(&[&[0]], &[marker], b'u'),
                csi(&[&[2026]], &[marker, b'$'], b'p'),
                dcs(&[&[1]], &[marker], b'q', b"data"),
            ],
            "{}",
            char::from(marker)
        );
    }
}

#[test]
fn a_misplaced_private_marker_voids_the_sequence() {
    for marker in MARKERS {
        // After a parameter, after another marker, and in a DCS string.
        let input = [
            b"\x1b[1".as_slice(),
            &[marker],
            b"ha\x1b[?",
            &[marker],
            b"1hb\x1bP1",
            &[marker],
            b"qdata\x1b\\c",
        ]
        .concat();
        assert_eq!(parse(&input), [print("abc")], "{}", char::from(marker));
    }
}

#[test]
fn intermediates_follow_the_parameters() {
    assert_eq!(
        parse(b"\x1b[2 q\x1b[61;1\"p\x1b[!p\x1b[>4;2m\x1bP$q\"q\x1b\\"),
        [
            csi(&[&[2]], b" ", b'q'),
            csi(&[&[61], &[1]], b"\"", b'p'),
            csi(&[&[0]], b"!", b'p'),
            csi(&[&[4], &[2]], b">", b'm'),
            dcs(&[&[0]], b"$", b'q', b"\"q"),
        ]
    );
}

#[test]
fn dcs_parameters_follow_the_same_rules() {
    assert_eq!(
        parse(b"\x1bP1;2:3;;qx\x1b\\"),
        [dcs(&[&[1], &[2, 3], &[0], &[0]], b"", b'q', b"x")]
    );
    let input = format!("\x1bP{}qx\x1b\\", numbered(40));
    assert_eq!(
        parse(input.as_bytes()),
        [Action::Dcs {
            params: numbered_groups(32),
            intermediates: Vec::new(),
            final_byte: b'q',
            payload: b"x".to_vec(),
            end: StringEnd::St,
        }]
    );
}

/// Records each call on its own, with its own chunk, where [`Recorder`]
/// merges them.
#[derive(Default)]
struct Calls(Vec<String>);

impl Handler for Calls {
    fn print(&mut self, c: char) {
        self.0.push(format!("print {c}"));
    }

    fn execute(&mut self, byte: u8) {
        self.0.push(format!("execute {byte:#04x}"));
    }

    fn csi_dispatch(&mut self, _params: &Params, _intermediates: &[u8], final_byte: u8) {
        self.0.push(format!("csi {}", char::from(final_byte)));
    }

    fn osc_start(&mut self) {
        self.0.push("osc_start".to_owned());
    }

    fn osc_put(&mut self, data: &[u8]) {
        self.0.push(format!("osc_put {}", data.escape_ascii()));
    }

    fn osc_end(&mut self, end: StringEnd) {
        self.0.push(format!("osc_end {end:?}"));
    }

    fn dcs_start(&mut self, _params: &Params, _intermediates: &[u8], final_byte: u8) {
        self.0.push(format!("dcs_start {}", char::from(final_byte)));
    }

    fn dcs_put(&mut self, data: &[u8]) {
        self.0.push(format!("dcs_put {}", data.escape_ascii()));
    }

    fn dcs_end(&mut self, end: StringEnd) {
        self.0.push(format!("dcs_end {end:?}"));
    }

    fn apc_start(&mut self) {
        self.0.push("apc_start".to_owned());
    }

    fn apc_put(&mut self, data: &[u8]) {
        self.0.push(format!("apc_put {}", data.escape_ascii()));
    }

    fn apc_end(&mut self, end: StringEnd) {
        self.0.push(format!("apc_end {end:?}"));
    }
}

fn calls_with(limits: Limits, reads: &[&[u8]]) -> Vec<String> {
    let mut parser = Parser::with_limits(limits);
    let mut calls = Calls::default();
    for read in reads {
        parser.advance(&mut calls, read);
    }
    calls.0
}

fn calls(reads: &[&[u8]]) -> Vec<String> {
    calls_with(Limits::default(), reads)
}

/// 4 bytes for every string, and 8 for OSC 52.
fn small_limits() -> Limits {
    Limits {
        osc: 4,
        osc_commands: vec![(52, 8)],
        dcs: 4,
        apc: 4,
    }
}

#[test]
fn payloads_arrive_in_a_chunk_per_read() {
    assert_eq!(
        calls(&[b"\x1b]0;ab", b"cd", b"e\x07"]),
        [
            "osc_start",
            "osc_put 0;ab",
            "osc_put cd",
            "osc_put e",
            "osc_end Bel"
        ]
    );
    assert_eq!(
        calls(&[b"\x1bPqab", b"c\x1b", b"\\"]),
        ["dcs_start q", "dcs_put ab", "dcs_put c", "dcs_end St"]
    );
    assert_eq!(
        calls(&[b"\x1b_Ga=q;", b"AAAA\x1b\\"]),
        ["apc_start", "apc_put Ga=q;", "apc_put AAAA", "apc_end St"]
    );
}

#[test]
fn dropped_bytes_split_a_chunk() {
    // C0 controls in OSC payloads, and DEL in DCS ones.
    assert_eq!(
        calls(&[b"\x1b]0;a\x01b\x07"]),
        ["osc_start", "osc_put 0;a", "osc_put b", "osc_end Bel"]
    );
    assert_eq!(
        calls(&[b"\x1bPqa\x7fb\x1b\\"]),
        ["dcs_start q", "dcs_put a", "dcs_put b", "dcs_end St"]
    );
}

#[test]
fn terminators_split_across_reads_are_recognized() {
    let cancelled_dcs = Action::Dcs {
        params: vec![vec![0]],
        intermediates: Vec::new(),
        final_byte: b'q',
        payload: b"d".to_vec(),
        end: StringEnd::Cancelled,
    };
    let cases: [(&[u8], Vec<Action>); 8] = [
        (
            b"\x1b]0;t\x07x",
            vec![osc(b"0;t", StringEnd::Bel), print("x")],
        ),
        (
            b"\x1b]0;t\x1b\\x",
            vec![osc(b"0;t", StringEnd::St), print("x")],
        ),
        (
            b"\x1b]0;t\x18x",
            vec![
                osc(b"0;t", StringEnd::Cancelled),
                Action::Execute(0x18),
                print("x"),
            ],
        ),
        (
            b"\x1bPqd\x1b\\x",
            vec![dcs(&[&[0]], b"", b'q', b"d"), print("x")],
        ),
        (
            b"\x1bPqd\x1ax",
            vec![cancelled_dcs, Action::Execute(0x1A), print("x")],
        ),
        (b"\x1b_d\x1b\\x", vec![apc(b"d", StringEnd::St), print("x")]),
        (
            b"\x1b_d\x18x",
            vec![
                apc(b"d", StringEnd::Cancelled),
                Action::Execute(0x18),
                print("x"),
            ],
        ),
        (b"\x1bXd\x1b\\x", vec![print("x")]),
    ];
    for (input, expected) in cases {
        for reads in every_split(input) {
            assert_eq!(parse_in_reads(&reads), expected, "{reads:02x?}");
        }
    }
}

#[test]
fn a_payload_at_its_limit_is_delivered() {
    assert_eq!(
        calls_with(
            small_limits(),
            &[b"\x1b]0;ab\x07\x1bPqabcd\x1b\\\x1b_abcd\x1b\\"]
        ),
        [
            "osc_start",
            "osc_put 0;ab",
            "osc_end Bel",
            "dcs_start q",
            "dcs_put abcd",
            "dcs_end St",
            "apc_start",
            "apc_put abcd",
            "apc_end St",
        ]
    );
}

#[test]
fn a_payload_past_its_limit_ends_at_once_and_the_rest_is_dropped() {
    // The chunk that would pass the limit is not delivered, the ones before
    // it are.
    assert_eq!(
        calls_with(small_limits(), &[b"\x1b]0;abc\x07x"]),
        ["osc_start", "osc_end TooLong", "print x"]
    );
    assert_eq!(
        calls_with(small_limits(), &[b"\x1b]0;a", b"bc", b"de\x07x"]),
        ["osc_start", "osc_put 0;a", "osc_end TooLong", "print x"]
    );
    assert_eq!(
        calls_with(small_limits(), &[b"\x1bPqabcde\x1b\\x\x1b_abcde\x1b\\y"]),
        [
            "dcs_start q",
            "dcs_end TooLong",
            "print x",
            "apc_start",
            "apc_end TooLong",
            "print y",
        ]
    );
}

#[test]
fn the_rest_of_a_string_past_its_limit_ends_as_the_string_would() {
    // CAN and SUB are executed, BEL ends only an OSC string, and ESC can
    // start a new sequence.
    assert_eq!(
        calls_with(small_limits(), &[b"\x1b]0;abcde\x18x"]),
        ["osc_start", "osc_end TooLong", "execute 0x18", "print x"]
    );
    assert_eq!(
        calls_with(small_limits(), &[b"\x1b_abcde\x07x\x1b\\y"]),
        ["apc_start", "apc_end TooLong", "print y"]
    );
    assert_eq!(
        calls_with(small_limits(), &[b"\x1bPqabcde\x1b[1m"]),
        ["dcs_start q", "dcs_end TooLong", "csi m"]
    );
}

#[test]
fn osc_commands_can_have_limits_of_their_own() {
    assert_eq!(
        calls_with(small_limits(), &[b"\x1b]52;c;ab\x07"]),
        ["osc_start", "osc_put 52;c;ab", "osc_end Bel"]
    );
    assert_eq!(
        calls_with(small_limits(), &[b"\x1b]52;c;abcd\x07"]),
        ["osc_start", "osc_end TooLong"]
    );
    assert_eq!(
        calls_with(small_limits(), &[b"\x1b]2;abc\x07"]),
        ["osc_start", "osc_end TooLong"]
    );
    // Until the number is read, the general limit applies.
    assert_eq!(
        calls_with(small_limits(), &[b"\x1b]5", b"2;c;ab\x07"]),
        ["osc_start", "osc_put 5", "osc_put 2;c;ab", "osc_end Bel"]
    );
}

#[test]
fn the_digits_of_an_osc_number_count_against_the_general_limit() {
    // With 2 bytes for OSC strings in general, `52;` passes the limit before
    // OSC 52's own applies, however the reads split it.
    let limits = Limits {
        osc: 2,
        ..small_limits()
    };
    let input = b"\x1b]52;c;a\x07";
    for reads in every_split(input) {
        let mut parser = Parser::with_limits(limits.clone());
        let mut recorder = Recorder::new();
        for read in &reads {
            parser.advance(&mut recorder, read);
        }
        assert_eq!(
            recorder.actions(),
            [osc(b"", StringEnd::TooLong)],
            "{reads:02x?}"
        );
    }
}

#[test]
fn new_limits_apply_from_the_next_string() {
    let mut parser = Parser::with_limits(small_limits());
    let mut calls = Calls::default();
    parser.advance(&mut calls, b"\x1b]0;ab");
    parser.set_limits(Limits::default());
    assert_eq!(parser.limits(), &Limits::default());
    parser.advance(&mut calls, b"cd\x07\x1b]0;abcd\x07");
    assert_eq!(
        calls.0,
        [
            "osc_start",
            "osc_put 0;ab",
            "osc_end TooLong",
            "osc_start",
            "osc_put 0;abcd",
            "osc_end Bel",
        ]
    );
}

#[test]
fn a_string_keeps_its_limits_when_its_osc_number_comes_after_new_ones() {
    let mut parser = Parser::with_limits(small_limits());
    let mut calls = Calls::default();
    parser.advance(&mut calls, b"\x1b]5");
    parser.set_limits(Limits::default());
    parser.advance(&mut calls, b"2;c;AAAAAAAA\x07");
    assert_eq!(calls.0.last().map(String::as_str), Some("osc_end TooLong"));
    // The next string has the new limits.
    calls.0.clear();
    parser.advance(&mut calls, b"\x1b]52;c;AAAAAAAA\x07");
    assert_eq!(calls.0.last().map(String::as_str), Some("osc_end Bel"));
}

#[test]
fn a_100_mib_payload_is_dropped_in_constant_memory() {
    /// Counts what arrives, so that the test keeps nothing either.
    #[derive(Default)]
    struct Tally {
        starts: usize,
        delivered: usize,
        ends: Vec<StringEnd>,
        after_the_end: usize,
        text: String,
    }

    impl Handler for Tally {
        fn print(&mut self, c: char) {
            self.text.push(c);
        }

        fn osc_start(&mut self) {
            self.starts += 1;
        }

        fn osc_put(&mut self, data: &[u8]) {
            if self.ends.is_empty() {
                self.delivered += data.len();
            } else {
                self.after_the_end += data.len();
            }
        }

        fn osc_end(&mut self, end: StringEnd) {
            self.ends.push(end);
        }
    }

    // The parser's state has a fixed size, and nothing past the limit
    // reaches the handler, so no one holds the payload.
    const READ: usize = 64 * 1024;
    let read = [b'A'; READ];
    let mut parser = Parser::new();
    let mut tally = Tally::default();
    parser.advance(&mut tally, b"\x1b]52;c;");
    for _ in 0..100 * 1024 * 1024 / READ {
        parser.advance(&mut tally, &read);
    }
    parser.advance(&mut tally, b"\x07done");

    assert_eq!(tally.starts, 1);
    assert_eq!(tally.ends, [StringEnd::TooLong]);
    assert!(tally.delivered <= parser.limits().osc_limit(Some(52)));
    assert_eq!(tally.after_the_end, 0);
    assert_eq!(tally.text, "done");
}
