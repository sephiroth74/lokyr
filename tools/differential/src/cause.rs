//! The known causes of the divergences between the two cores, and how a
//! step that the screens part at shows one.

use bolt_vt_core::page::Width;
use bolt_vt_core::parser::MAX_PARAMS;
use bolt_vt_core::terminal::{Charset, Shift};

use crate::{Before, Step};

/// A way in which the two cores differ on purpose, each for its reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// Lone bytes from 0x80 to 0x9F.
    C1,
    /// U+D7FF's width.
    Jamo,
    /// The width of an emoji presentation sequence.
    Emoji,
    /// The British set.
    British,
    /// DECCOLM without the window's operations.
    Deccolm,
    /// CUU, CUD and their kin in origin mode.
    OriginMoves,
    /// DCH past the row's end.
    Dch,
    /// LF, its kin and RI, with a wrap pending.
    LineFeed,
    /// DECOM's reset.
    Decom,
    /// DECALN's cursor.
    Decaln,
    /// HT with a wrap pending.
    Tab,
    /// What the VT core does and `alacritty_terminal` does not.
    Unsupported,
    /// SCORC with parameters.
    Scorc,
    /// IL and DL's cursor.
    Lines,
    /// The pending wrap through edits.
    EditWrap,
    /// DECSTBM's margins past the screen.
    Region,
    /// CSI T with several parameters.
    Highlight,
    /// ED 1 from the second row.
    Ed1,
    /// Relative moves at the scroll region's margins.
    Margins,
    /// Edits that split a wide character.
    WideEdit,
    /// CBT with a wrap pending.
    Cbt,
    /// A control sequence of more than 32 values.
    Params,
    /// A spacer head that DCH moves.
    MovedHead,
    /// LNM's line feeds.
    Lnm,
    /// CBT with no tab stop before the cursor.
    CbtFirst,
    /// REP after DEL.
    Del,
    /// A wide character that wraps in insert mode.
    InsertWrap,
    /// REP after RIS.
    RepReset,
    /// A wide character at the last column without autowrap.
    NoWrapWide,
    /// The shift that DECRC brings back.
    Shift,
}

impl Cause {
    /// Why the screens differ.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::C1 => {
                "vte takes a lone byte from 0x80 to 0x9F, which is ill-formed UTF-8, for a C1 \
                 control, which alacritty_terminal ignores; the VT core replaces it with U+FFFD, \
                 as it does each maximal subpart of ill-formed UTF-8, as the Unicode Standard \
                 recommends"
            }
            Self::Jamo => {
                "U+D7FF, of Hangul Jamo Extended-B, takes no cell in the VT core, as the whole \
                 block does with glibc's wcwidth; unicode-width, which alacritty_terminal uses, \
                 gives its unassigned code points one"
            }
            Self::Emoji => {
                "VS16 after a pictographic base makes the cluster an emoji presentation sequence \
                 in the VT core, two cells wide, as Unicode's emoji presentation says; \
                 alacritty_terminal gives each codepoint the width of its own, one for U+2600 and \
                 none for U+FE0F"
            }
            Self::British => {
                "ESC ( A and its kin designate the British set in the VT core, whose # is a pound \
                 sign, as on DEC's terminals and in xterm; alacritty_terminal knows only ASCII and \
                 DEC's line drawing, and keeps the set that was there: the comparison ends at the \
                 designation"
            }
            Self::Deccolm => {
                "DECCOLM needs the window's operations in the VT core, which these tests leave \
                 off, as they are by default, and does nothing without them; alacritty_terminal \
                 clears the screen and the scroll region, and puts the cursor home, without \
                 changing the width"
            }
            Self::OriginMoves => {
                "CUU and CUD in origin mode move within the scroll region in the VT core, as in \
                 xterm; alacritty_terminal adds the top margin to the cursor's row a second time, \
                 as it moves to the row that it reached as if that were relative to the margin"
            }
            Self::Dch => {
                "DCH of more cells than are right of the cursor deletes those in the VT core, as \
                 in xterm; alacritty_terminal clears as many cells back from the row's end, some \
                 of them left of the cursor"
            }
            Self::LineFeed => {
                "LF, VT, FF, IND and RI at the last column end the pending wrap in the VT core, as \
                 in xterm; alacritty_terminal keeps it, so that the next character goes to the row \
                 after the one that it moved to"
            }
            Self::Decom => {
                "DECOM's reset puts the cursor home in the VT core, as its set does, as in xterm \
                 and on DEC's terminals; alacritty_terminal puts it home only on the set"
            }
            Self::Decaln => {
                "DECALN puts the cursor home in the VT core, as DEC STD 070 and xterm do; alacritty_terminal leaves it where it was"
            }
            Self::Tab => {
                "HT with a wrap pending leaves the cursor at the last column in the VT core, as xterm does without its mode 41; alacritty_terminal wraps to the next row first, as that mode does"
            }
            Self::Unsupported => {
                "the VT core does as xterm and DEC's terminals do, and alacritty_terminal does \
                 nothing, for DECSED and DECSEL, DECIC and DECDC, DECCRA, DECERA, DECFRA and \
                 DECSERA, DECSACE, DECSCA, DECSCL and DECSTR, XTSAVE and XTRESTORE, SPA and EPA, \
                 DECBI and DECFI, SS2, SS3, LS2 and LS3, and the modes 41, 45, 47, 69, 1045, 1047, \
                 1048 and 2027: the comparison ends at the first of them, as what it changes can \
                 show later"
            }
            Self::Scorc => {
                "CSI u restores the cursor in the VT core without parameters alone, as SCORC, and with them belongs to the kitty keyboard protocol; alacritty_terminal restores the cursor whatever the parameters"
            }
            Self::Lines => {
                "IL and DL put the cursor at the left margin in the VT core, as DEC's terminals and xterm do; alacritty_terminal leaves it where it was"
            }
            Self::EditWrap => {
                "ICH, DCH, ECH, EL and ED leave no wrap pending in the VT core, as xterm's do; alacritty_terminal keeps it, so that the next character wraps"
            }
            Self::Region => {
                "DECSTBM with its top margin at the last row or past it does nothing in the VT \
                 core, as in xterm, which takes a bottom margin past the screen for the last row \
                 and then finds no room for a region; alacritty_terminal compares the margins \
                 before it cuts the bottom one to the screen, and so sets a region and puts the \
                 cursor home"
            }
            Self::Highlight => {
                "CSI T with more than one parameter is xterm's highlight mouse tracking, which the VT core leaves out and does nothing for; alacritty_terminal scrolls down"
            }
            Self::Ed1 => {
                "ED 1 with the cursor on the second row erases the first in the VT core, as in xterm; alacritty_terminal leaves it, as it erases the rows above the cursor's only when there are two of them or more"
            }
            Self::Margins => {
                "CUU, CUD and their kin stop at the scroll region's margin that they move toward when they start at it or before it in the VT core, as in xterm; alacritty_terminal goes on to the screen's edge"
            }
            Self::WideEdit => {
                "An edit that would split a wide character, text inserted among them too, empties it whole in the VT core, as in xterm; alacritty_terminal empties or moves the cells that the edit takes alone, which leaves half of the character"
            }
            Self::Cbt => {
                "CBT leaves no wrap pending in the VT core, as every move does in xterm; alacritty_terminal moves the cursor back and keeps the wrap pending"
            }
            Self::Params => {
                "a control sequence of more than 32 values is carried out with the first 32 in the \
                 VT core, as DEC's terminals keep the first parameters, and vte ignores it whole: \
                 the comparison ends at a sequence of 32 values or more, which the VT core's \
                 parser may have cut"
            }
            Self::MovedHead => {
                "a spacer head that DCH moves from the last column becomes an empty cell in the VT core, as it no longer leads to the wide character that went to the next row; alacritty_terminal moves it with the other cells"
            }
            Self::Lnm => {
                "with LNM set, LF, VT and FF return the carriage too in the VT core, as in xterm \
                 and on DEC's terminals; alacritty_terminal reads LNM for the keyboard alone: the \
                 comparison ends as LNM is set"
            }
            Self::CbtFirst => {
                "with no tab stop left before the cursor, CBT goes to the first column in the VT \
                 core, as in xterm; alacritty_terminal has a tab stop at the first column, which TBC \
                 clears, and without it stays at the last tab stop that it reached"
            }
            Self::Del => {
                "DEL is nothing in the VT core's parser, as in DEC's, so that REP after it repeats the character before; vte prints DEL, which alacritty_terminal does not show, and REP repeats it"
            }
            Self::InsertWrap => {
                "a wide character that does not fit at the last column in insert mode goes to the \
                 next row in the VT core, as in xterm, and is inserted there, pushing that row's \
                 cells right; alacritty_terminal looks for room to insert in on the row that it \
                 leaves, and then writes the character over the next row's first cells"
            }
            Self::RepReset => {
                "RIS forgets the character that REP repeats in the VT core, which starts over; vte keeps it in its parser, which RIS does not reach"
            }
            Self::NoWrapWide => {
                "a wide character at the last column without autowrap goes one column back in the VT core, as in xterm; alacritty_terminal drops it"
            }
            Self::Shift => {
                "DECRC brings back the shift of the character sets that DECSC saved, SO's or \
                 SI's, in the VT core, as in xterm and on DEC's terminals; alacritty_terminal \
                 keeps the shift that is on, as its saved cursor does not hold it: the comparison \
                 ends when DECRC changes the shift"
            }
        }
    }
}

/// The modes that the VT core has and `alacritty_terminal` has not, which
/// change what later sequences do: xterm's fix for curses, reverse wrap and
/// its extension, the alternate screens without the cursor's save and the
/// save alone, left and right margins, and grapheme clustering.
const MODES: [u16; 8] = [41, 45, 47, 69, 1045, 1047, 1048, 2027];

/// The known cause that `step` has, with the VT core's state `before` it
/// and the shift of its character sets after it, at which the comparison
/// ends whether the screens differ yet or not, as its effect can show later
/// only: [`Cause::Unsupported`], a sequence that vte may ignore for its
/// values, the British set, LNM, SCORC with parameters, DECRC that changes
/// the shift, DECCOLM, which resets the scroll region in
/// `alacritty_terminal`, DECALN, which resets it in the VT core, and a
/// region that only `alacritty_terminal` sets.
pub(crate) fn ending(step: &Step, before: &Before, shift: Shift) -> Option<Cause> {
    let restores = match step {
        Step::Esc {
            intermediates,
            final_byte: b'8',
        } => intermediates.is_empty(),
        Step::Csi {
            params,
            intermediates,
            final_byte: b'u',
            ..
        } => {
            intermediates.is_empty()
                && params.len() <= 1
                && params.first().is_none_or(|&value| value == 0)
        }
        _ => false,
    };
    if restores && shift != before.shift {
        return Some(Cause::Shift);
    }
    match step {
        Step::Csi { values, .. } if *values >= MAX_PARAMS => Some(Cause::Params),
        Step::Csi {
            params,
            intermediates,
            final_byte,
            ..
        } => match (intermediates.as_slice(), final_byte) {
            // A bottom margin past the screen, which the VT core takes for
            // the last row, and `alacritty_terminal` compares as it is.
            (b"", b'r')
                if params
                    .get(1)
                    .is_some_and(|&bottom| usize::from(bottom) > before.rows)
                    && usize::from(params.first().copied().unwrap_or(0)) >= before.rows =>
            {
                Some(Cause::Region)
            }
            (b"?", b'h' | b'l') if params.contains(&3) => Some(Cause::Deccolm),
            (b"", b'h') if params.contains(&20) => Some(Cause::Lnm),
            // Not the VT core's SCORC, which has one parameter at most, and 0,
            // and restores the character sets too in `alacritty_terminal`.
            (b"", b'u') if params.len() > 1 || params.first().is_some_and(|&value| value > 0) => {
                Some(Cause::Scorc)
            }
            (b"?", b'h' | b'l') if params.iter().any(|mode| MODES.contains(mode)) => {
                Some(Cause::Unsupported)
            }
            // DECSED, DECSEL, XTSAVE and XTRESTORE; DECIC and DECDC; DECCRA,
            // DECERA, DECSERA and DECFRA; DECSACE; DECSCA and DECSCL; DECSTR.
            (b"?", b'J' | b'K' | b's' | b'r')
            | (b"'", b'}' | b'~')
            | (b"$", b'v' | b'z' | b'{' | b'x')
            | (b"*", b'x')
            | (b"\"", b'q' | b'p')
            | (b"!", b'p') => Some(Cause::Unsupported),
            _ => None,
        },
        Step::Esc {
            intermediates,
            final_byte: b'8',
        } if intermediates == b"#" => Some(Cause::Decaln),
        // The British set, which `alacritty_terminal` does not designate,
        // so that the set before stays.
        Step::Esc {
            intermediates,
            final_byte: b'A',
        } if matches!(intermediates.as_slice(), b"(" | b")" | b"*" | b"+") => Some(Cause::British),
        // SPA and EPA, DECBI and DECFI, SS2 and SS3, LS2 and LS3.
        Step::Esc {
            intermediates,
            final_byte: b'V' | b'W' | b'6' | b'9' | b'N' | b'O' | b'n' | b'o',
        } if intermediates.is_empty() => Some(Cause::Unsupported),
        _ => None,
    }
}

/// The known cause that `step` shows, the step that `byte` made, at which
/// the screens part, with the VT core's state `before` it.
pub(crate) fn cause(step: &Step, byte: u8, before: &Before) -> Option<Cause> {
    let first = |params: &[u16]| params.first().copied().unwrap_or(0).max(1);
    match step {
        Step::Csi {
            params,
            intermediates,
            final_byte,
            ..
        } => match (intermediates.as_slice(), final_byte) {
            (b"?", b'l') if params.contains(&6) => Some(Cause::Decom),
            // CUU, CUD, CNL, CPL and VPR.
            (b"", b'A' | b'B' | b'E' | b'F' | b'e') if before.origin => Some(Cause::OriginMoves),
            (b"", b'P') if usize::from(first(params)) > before.cols - before.cursor.0 => {
                Some(Cause::Dch)
            }
            (b"", b'L' | b'M') if before.cursor.0 > 0 || before.cursor.2 => Some(Cause::Lines),
            // ICH, DCH, ECH, EL and ED.
            (b"", b'@' | b'P' | b'X' | b'K' | b'J') if before.cursor.2 => Some(Cause::EditWrap),
            (b"", b'T') if params.len() > 1 => Some(Cause::Highlight),
            (b"", b'J') if params.first() == Some(&1) && before.cursor.1 == 1 => Some(Cause::Ed1),
            // CUU and CPL from the top margin or below it, CUD, CNL and VPR
            // from the bottom one or above it, past the margin.
            (b"", b'A' | b'F')
                if before.cursor.1 >= before.region.0
                    && before.cursor.1 < before.region.0 + usize::from(first(params)) =>
            {
                Some(Cause::Margins)
            }
            (b"", b'B' | b'E' | b'e')
                if before.cursor.1 <= before.region.1
                    && before.cursor.1 + usize::from(first(params)) > before.region.1 =>
            {
                Some(Cause::Margins)
            }
            // EL, ED, ECH, DCH and ICH whose cells start or end between the
            // halves of a wide character.
            (b"", b'K' | b'J' | b'X' | b'P' | b'@') if splits(params, *final_byte, before) => {
                Some(Cause::WideEdit)
            }
            (b"", b'Z') if before.cursor.2 => Some(Cause::Cbt),
            (b"", b'P') if before.widths.last() == Some(&Width::SpacerHead) => {
                Some(Cause::MovedHead)
            }
            // CBT of more tab stops than come before the cursor, past the first
            // column, which is no tab stop in the VT core.
            (b"", b'Z')
                if usize::from(first(params))
                    > before.tab_stops[1..before.cursor.0.max(1)]
                        .iter()
                        .filter(|&&stop| stop)
                        .count() =>
            {
                Some(Cause::CbtFirst)
            }
            (b"", b'b') if before.del => Some(Cause::Del),
            (b"", b'b') if before.reset => Some(Cause::RepReset),
            _ => None,
        },
        // LF, VT and FF, IND and RI.
        Step::Execute(0x0A..=0x0C)
        | Step::Esc {
            final_byte: b'D' | b'M',
            ..
        } if before.cursor.2 => Some(Cause::LineFeed),
        Step::Print('\u{FFFD}') if (0x80..=0x9F).contains(&byte) => Some(Cause::C1),
        Step::Print('\u{D7B0}'..='\u{D7FF}') => Some(Cause::Jamo),
        Step::Print('\u{FE0F}') => Some(Cause::Emoji),
        Step::Execute(0x09) if before.cursor.2 => Some(Cause::Tab),
        Step::Print(_) if before.insert && inserts_at_a_tail(before) => Some(Cause::WideEdit),
        // A wide character that does not fit at the last column, in insert
        // mode.
        Step::Print(_)
            if before.insert && !before.cursor.2 && before.cursor.0 + 1 == before.cols =>
        {
            Some(Cause::InsertWrap)
        }
        Step::Print(_) if !before.autowrap && before.cursor.0 + 1 == before.cols => {
            Some(Cause::NoWrapWide)
        }
        _ => None,
    }
}

/// Whether the edit `final_byte` with `params` would split a wide
/// character of the cursor's row: start at its second half, end before it,
/// or, for ICH, push its second half past the row's end.
fn splits(params: &[u16], final_byte: u8, before: &Before) -> bool {
    let tail = |x: usize| before.widths.get(x) == Some(&Width::SpacerTail);
    let x = before.cursor.0;
    let n = usize::from(params.first().copied().unwrap_or(0).max(1));
    match (final_byte, params.first().copied().unwrap_or(0)) {
        (b'K' | b'J', 0) => tail(x),
        (b'K' | b'J', 1) => tail(x + 1),
        (b'X' | b'P', _) => tail(x) || tail(x + n),
        (b'@', _) => tail(x) || before.cols.checked_sub(n).is_some_and(tail),
        _ => false,
    }
}

/// Whether text inserted at the cursor goes in at the second half of a
/// wide character, or pushes one past the row's end: on the cursor's row, or
/// on the next when a wrap is pending.
fn inserts_at_a_tail(before: &Before) -> bool {
    let (row, x) = if before.cursor.2 {
        (&before.below, 0)
    } else {
        (&before.widths, before.cursor.0)
    };
    let tail = |x: usize| row.get(x) == Some(&Width::SpacerTail);
    tail(x) || tail(before.cols - 1) || tail(before.cols - 2)
}

/// The sets that G0 to G3 hold.
pub(crate) fn sets(charsets: bolt_vt_core::terminal::Charsets) -> [Charset; 4] {
    [Shift::G0, Shift::G1, Shift::G2, Shift::G3].map(|slot| charsets.get(slot))
}
