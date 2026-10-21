//! The known causes of the divergences between the two cores, and how a
//! step that the screens part at shows one.

use bolt_vt_core::page::Width;
use bolt_vt_core::parser::MAX_PARAMS;
use bolt_vt_core::terminal::{Charset, Shift};

use crate::{Before, HEAD, Screen, Step};

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
    /// The origin mode that DECRC brings back.
    SavedOrigin,
    /// The spacer head of a wide character that wraps over another.
    WrapHead,
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
                "CUU, CUD, CHA and their kin in origin mode move within the scroll region in the \
                 VT core, as in xterm; alacritty_terminal adds the top margin to the cursor's row a \
                 second time, as it moves to the row that it reached, or keeps, as if that were \
                 relative to the margin"
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
                "HT with a wrap pending leaves the cursor at the last column in the VT core, as \
                 xterm does without its mode 41; alacritty_terminal wraps to the first column of the \
                 next row, and stops there"
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
                "ICH, DCH, ECH, EL, ED, and IL and DL outside the scroll region, leave no wrap \
                 pending in the VT core, as xterm's edits do; \
                 alacritty_terminal keeps it, so that the next character wraps, and its EL 0 with \
                 a wrap pending erases nothing"
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
                "DECRC, and the end of the alternate screen that mode 1049 makes, bring back the \
                 shift of the character sets that DECSC saved, SO's or SI's, in the VT core, as in \
                 xterm and on DEC's terminals; alacritty_terminal keeps the shift that is on, as \
                 its saved cursor does not hold it: the comparison ends when the restore changes \
                 the shift"
            }
            Self::SavedOrigin => {
                "DECRC, and the end of the alternate screen that mode 1049 makes, bring back the \
                 origin mode that DECSC saved in the VT core, as in xterm and on DEC's terminals; \
                 alacritty_terminal keeps the mode that is on: the comparison ends when the \
                 restore changes it"
            }
            Self::WrapHead => {
                "a wide character that does not fit at the last column, and wraps over a wide \
                 character at the start of the next row, keeps the spacer head that it leaves in \
                 the VT core, which stands for it; alacritty_terminal empties the head again as it \
                 writes over the other wide character"
            }
        }
    }
}

/// The modes that the VT core has and `alacritty_terminal` has not, which
/// change what later sequences do: xterm's fix for curses, reverse wrap and
/// its extension, the alternate screens without the cursor's save and the
/// save alone, left and right margins, and grapheme clustering.
const MODES: [u16; 8] = [41, 45, 47, 69, 1045, 1047, 1048, 2027];

/// What the comparison saw at a step.
pub(crate) struct Seen<'a> {
    /// What the byte made the VT core do.
    pub(crate) step: &'a Step,
    /// The byte.
    pub(crate) byte: u8,
    /// The VT core's state before the step, and after it.
    pub(crate) before: &'a Before,
    pub(crate) after: &'a Before,
    /// The VT core's screen and `alacritty_terminal`'s, if they differ.
    pub(crate) screens: Option<(&'a Screen, &'a Screen)>,
    /// Whether the character that the step printed, if any, is wide.
    pub(crate) wide: bool,
}

/// A cursor as [`Screen::cursor`] has it.
type At = (usize, usize, bool);

impl Seen<'_> {
    /// Whether the screens are the same.
    fn same(&self) -> bool {
        self.screens.is_none()
    }

    /// Whether the screens' cells are the same, their cursors aside.
    fn cells_same(&self) -> bool {
        self.screens
            .is_none_or(|(bolt, alacritty)| bolt.rows == alacritty.rows)
    }

    /// Whether the screens' rows are the same but for the rows in `rows`.
    fn rows_same_but(&self, rows: &[usize]) -> bool {
        self.screens.is_none_or(|(bolt, alacritty)| {
            bolt.rows.len() == alacritty.rows.len()
                && (0..bolt.rows.len())
                    .filter(|y| !rows.contains(y))
                    .all(|y| bolt.rows[y] == alacritty.rows[y])
        })
    }

    /// The VT core's cursor and `alacritty_terminal`'s, the one before the
    /// step for both if the screens are the same.
    fn cursors(&self) -> (At, At) {
        self.screens.map_or(
            (self.before.cursor, self.before.cursor),
            |(bolt, alacritty)| (bolt.cursor, alacritty.cursor),
        )
    }

    /// Whether the cursors are the same, or differ as an edit makes them
    /// when the wrap was pending: in that alone, which the VT core ended
    /// and `alacritty_terminal` kept.
    fn cursors_same_but_the_wrap(&self) -> bool {
        let (bolt, alacritty) = self.cursors();
        bolt == alacritty || (self.before.cursor.2 && self.wrap_kept())
    }

    /// Whether the cursors differ in their pending wrap alone, which the VT
    /// core ended and `alacritty_terminal` kept.
    fn wrap_kept(&self) -> bool {
        let (bolt, alacritty) = self.cursors();
        (bolt.0, bolt.1) == (alacritty.0, alacritty.1) && !bolt.2 && alacritty.2
    }
}

/// The known cause that a step has, at which the comparison ends whether
/// the screens differ yet or not, as its effect can show later only, when
/// the screens show of it at the step what it makes them show, if any:
/// [`Cause::Unsupported`], a sequence that vte may ignore for its values,
/// and DECCOLM, which can show anything; the British set and LNM, which
/// show nothing; SCORC with parameters, and a region that only
/// `alacritty_terminal` sets, which move its cursor alone; DECALN, which
/// moves the VT core's alone; and a restore of the cursor that changes the
/// VT core's shift of the character sets or its origin mode, which shows
/// nothing.
pub(crate) fn ending(seen: &Seen<'_>) -> Option<Cause> {
    let (step, before, after) = (seen.step, seen.before, seen.after);
    // DECRC, SCORC, and the alternate screen's end that 1049 makes.
    let restores = match step {
        Step::Esc {
            intermediates,
            final_byte: b'8',
        } => intermediates.is_empty(),
        Step::Csi {
            params,
            intermediates,
            final_byte,
            ..
        } => match (intermediates.as_slice(), final_byte) {
            (b"", b'u') => params.len() <= 1 && params.first().is_none_or(|&value| value == 0),
            (b"?", b'l') => params.contains(&1049),
            _ => false,
        },
        _ => false,
    };
    if restores && seen.same() {
        if after.shift != before.shift {
            return Some(Cause::Shift);
        }
        if after.origin != before.origin {
            return Some(Cause::SavedOrigin);
        }
    }
    let (bolt, alacritty) = seen.cursors();
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
                    && usize::from(params.first().copied().unwrap_or(0)) >= before.rows
                    && seen.cells_same()
                    && bolt == before.cursor =>
            {
                Some(Cause::Region)
            }
            (b"?", b'h' | b'l') if params.contains(&3) => Some(Cause::Deccolm),
            (b"", b'h') if params.contains(&20) && seen.same() => Some(Cause::Lnm),
            // Not the VT core's SCORC, which has one parameter at most, and 0,
            // and restores the character sets too in `alacritty_terminal`.
            (b"", b'u')
                if (params.len() > 1 || params.first().is_some_and(|&value| value > 0))
                    && seen.cells_same()
                    && bolt == before.cursor =>
            {
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
        // DECALN, which fills both screens and puts the VT core's cursor
        // home.
        Step::Esc {
            intermediates,
            final_byte: b'8',
        } if intermediates == b"#" && seen.cells_same() && alacritty == before.cursor => {
            Some(Cause::Decaln)
        }
        // The British set, which `alacritty_terminal` does not designate,
        // so that the set before stays.
        Step::Esc {
            intermediates,
            final_byte: b'A',
        } if matches!(intermediates.as_slice(), b"(" | b")" | b"*" | b"+") && seen.same() => {
            Some(Cause::British)
        }
        // SPA and EPA, DECBI and DECFI, SS2 and SS3, LS2 and LS3.
        Step::Esc {
            intermediates,
            final_byte: b'V' | b'W' | b'6' | b'9' | b'N' | b'O' | b'n' | b'o',
        } if intermediates.is_empty() => Some(Cause::Unsupported),
        _ => None,
    }
}

/// The known cause of the difference between the screens at a step that
/// they part at, recognized from what the step made the VT core do, its
/// state before it, and the difference's shape, as each cause makes it.
pub(crate) fn cause(seen: &Seen<'_>) -> Option<Cause> {
    let (bolt_screen, alacritty_screen) = seen.screens?;
    let before = seen.before;
    let (x, y, pending) = before.cursor;
    let (bolt, alacritty) = (bolt_screen.cursor, alacritty_screen.cursor);
    let place = |at: At| (at.0, at.1);
    let first = |params: &[u16]| usize::from(params.first().copied().unwrap_or(0).max(1));
    let (top, bottom) = before.region;
    match seen.step {
        Step::Csi {
            params,
            intermediates,
            final_byte,
            ..
        } => match (intermediates.as_slice(), final_byte) {
            (b"?", b'l')
                if params.contains(&6)
                    && seen.cells_same()
                    && bolt == (0, 0, false)
                    && place(alacritty) == (x, y) =>
            {
                Some(Cause::Decom)
            }
            // CUU, CUD, CNL, CPL, VPR, CHA and HPA in origin mode, below a
            // top margin, where the rows part and the columns do not.
            (b"", b'A' | b'B' | b'E' | b'F' | b'e' | b'G' | b'`')
                if before.origin
                    && top > 0
                    && seen.cells_same()
                    && bolt.0 == alacritty.0
                    && bolt.1 != alacritty.1 =>
            {
                Some(Cause::OriginMoves)
            }
            // CUU and CPL from the top margin or below it, past it.
            (b"", b'A' | b'F')
                if !before.origin
                    && top > 0
                    && y >= top
                    && y < top + first(params)
                    && seen.cells_same()
                    && bolt.0 == alacritty.0
                    && bolt.1 == top
                    && alacritty.1 < top =>
            {
                Some(Cause::Margins)
            }
            // CUD, CNL and VPR from the bottom margin or above it, past it.
            (b"", b'B' | b'E' | b'e')
                if !before.origin
                    && bottom + 1 < before.rows
                    && y <= bottom
                    && y + first(params) > bottom
                    && seen.cells_same()
                    && bolt.0 == alacritty.0
                    && bolt.1 == bottom
                    && alacritty.1 > bottom =>
            {
                Some(Cause::Margins)
            }
            (b"", b'P')
                if first(params) > before.cols - x
                    && seen.rows_same_but(&[y])
                    && seen.cursors_same_but_the_wrap() =>
            {
                Some(Cause::Dch)
            }
            (b"", b'P')
                if before.widths.last() == Some(&Width::SpacerHead)
                    && seen.rows_same_but(&[y])
                    && seen.cursors_same_but_the_wrap() =>
            {
                Some(Cause::MovedHead)
            }
            (b"", b'L' | b'M')
                if (x > 0 || pending)
                    && seen.cells_same()
                    && bolt == (0, y, false)
                    && alacritty == before.cursor =>
            {
                Some(Cause::Lines)
            }
            // ED 1 from the second row, which `alacritty_terminal` leaves the
            // first, and the second when it splits a wide character there.
            (b"", b'J')
                if params.first() == Some(&1)
                    && y == 1
                    && seen.rows_same_but(if splits(params, b'J', before) {
                        &[0, 1]
                    } else {
                        &[0]
                    })
                    && seen.cursors_same_but_the_wrap()
                    && bolt_screen.rows[0].is_empty() =>
            {
                Some(Cause::Ed1)
            }
            // EL, ED, ECH, DCH and ICH whose cells start or end between the
            // halves of a wide character.
            (b"", b'K' | b'J' | b'X' | b'P' | b'@')
                if splits(params, *final_byte, before)
                    && seen.rows_same_but(&[y])
                    && seen.cursors_same_but_the_wrap() =>
            {
                Some(Cause::WideEdit)
            }
            // EL 0 with a wrap pending, which erases nothing in
            // `alacritty_terminal`: the last cell differs, and the wrap.
            (b"", b'K')
                if pending
                    && params.first().is_none_or(|&mode| mode == 0)
                    && seen.rows_same_but(&[y])
                    && seen.wrap_kept()
                    && but_last(&alacritty_screen.rows[y], before.cols) == bolt_screen.rows[y] =>
            {
                Some(Cause::EditWrap)
            }
            // ICH, DCH, ECH, EL and ED, and IL and DL outside the scroll
            // region, which move nothing there.
            (b"", b'@' | b'P' | b'X' | b'K' | b'J' | b'L' | b'M')
                if pending && seen.cells_same() && seen.wrap_kept() =>
            {
                Some(Cause::EditWrap)
            }
            (b"", b'Z') if pending && seen.cells_same() && seen.wrap_kept() => Some(Cause::Cbt),
            // CBT of more tab stops than come before the cursor, past the first
            // column, which is no tab stop in the VT core, and no longer one in
            // `alacritty_terminal`.
            (b"", b'Z')
                if !before.first_stop
                    && first(params)
                        > before.tab_stops[1..x.max(1)]
                            .iter()
                            .filter(|&&stop| stop)
                            .count()
                    && seen.cells_same()
                    && bolt.0 == 0
                    && alacritty.0 > 0
                    && bolt.1 == alacritty.1 =>
            {
                Some(Cause::CbtFirst)
            }
            // REP in insert mode, whose characters push a wide character of
            // the cursor's row, or of a row below that they wrap to, past the
            // row's end, on the rows that they go in, two cells a character
            // at most, and those that a scroll at the bottom moves them to.
            (b"", b'b')
                if before.insert
                    && (before.widths[x..].contains(&Width::SpacerTail)
                        || before
                            .wide_below
                            .iter()
                            .take(reach(params, before))
                            .any(|&wide| wide))
                    && seen.rows_same_but(&around(y, reach(params, before))) =>
            {
                Some(Cause::WideEdit)
            }
            // SD, which `alacritty_terminal` takes, and the cursor stays.
            (b"", b'T') if params.len() > 1 && bolt == alacritty => Some(Cause::Highlight),
            // REP, which repeats a character in one core alone: the one that
            // does nothing leaves its cursor.
            (b"", b'b') if before.del && alacritty == before.cursor => Some(Cause::Del),
            (b"", b'b') if before.reset && bolt == before.cursor => Some(Cause::RepReset),
            _ => None,
        },
        // LF, VT and FF, IND and RI.
        Step::Execute(0x0A..=0x0C) if pending && seen.cells_same() && seen.wrap_kept() => {
            Some(Cause::LineFeed)
        }
        Step::Esc {
            intermediates,
            final_byte: b'D' | b'M',
        } if intermediates.is_empty() && pending && seen.cells_same() && seen.wrap_kept() => {
            Some(Cause::LineFeed)
        }
        // HT, which leaves the VT core's cursor as it was and takes
        // `alacritty_terminal`'s to the next row's first column, scrolling at
        // the bottom margin.
        Step::Execute(0x09)
            if pending
                && bolt == before.cursor
                && alacritty.0 == 0
                && (seen.cells_same() || y == bottom) =>
        {
            Some(Cause::Tab)
        }
        // The characters whose widths differ, on the rows that they print on.
        Step::Print('\u{FFFD}')
            if (0x80..=0x9F).contains(&seen.byte) && seen.rows_same_but(&[y, bolt.1]) =>
        {
            Some(Cause::C1)
        }
        Step::Print('\u{D7C7}'..='\u{D7CA}' | '\u{D7FC}'..='\u{D7FF}')
            if seen.rows_same_but(&[y, bolt.1]) =>
        {
            Some(Cause::Jamo)
        }
        Step::Print('\u{FE0F}') if seen.rows_same_but(&[y, bolt.1]) => Some(Cause::Emoji),
        // Text inserted at the second half of a wide character, or that
        // pushes one past the row's end, on the row that it goes in.
        Step::Print(_)
            if before.insert && inserts_at_a_tail(before) && seen.rows_same_but(&around(y, 1)) =>
        {
            Some(Cause::WideEdit)
        }
        // A wide character that does not fit at the last column, in insert
        // mode, and without autowrap.
        Step::Print(_)
            if seen.wide
                && before.insert
                && !pending
                && x + 1 == before.cols
                && seen.rows_same_but(&[y.saturating_sub(1), y, y + 1]) =>
        {
            Some(Cause::InsertWrap)
        }
        Step::Print(_)
            if seen.wide
                && !before.autowrap
                && x + 1 == before.cols
                && seen.rows_same_but(&[y]) =>
        {
            Some(Cause::NoWrapWide)
        }
        // A wide character that wraps from the last column over a wide
        // character at the start of the next row: the spacer head that it
        // left, at the end of the cursor's row, alone differs.
        Step::Print(_)
            if seen.wide
                && before.autowrap
                && !before.insert
                && !pending
                && x + 1 == before.cols
                && before.below.first() == Some(&Width::Wide)
                && seen.rows_same_but(&[y])
                && bolt == alacritty
                && bolt_screen.rows[y].get(x).is_some_and(|cell| cell == HEAD)
                && but_last(&bolt_screen.rows[y], before.cols)
                    == but_last(&alacritty_screen.rows[y], before.cols) =>
        {
            Some(Cause::WrapHead)
        }
        _ => None,
    }
}

/// The rows from `reach` above row `y` to `reach` below it: those that
/// text from row `y` goes on in, and those that a scroll at the bottom
/// moves them to.
fn around(y: usize, reach: usize) -> Vec<usize> {
    (y.saturating_sub(reach)..=y + reach).collect()
}

/// How many rows below the cursor's the characters that REP with `params`
/// prints can reach, two cells a character at most.
fn reach(params: &[u16], before: &Before) -> usize {
    2 * usize::from(params.first().copied().unwrap_or(0).max(1)) / before.cols + 1
}

/// `row`'s cells but its cell at column `cols - 1`, without the empty ones
/// at the end, as [`Screen`] keeps a row.
fn but_last(row: &[String], cols: usize) -> Vec<String> {
    let mut cells = row[..row.len().min(cols - 1)].to_vec();
    while cells.last().is_some_and(|cell| cell == " ") {
        cells.pop();
    }
    cells
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
