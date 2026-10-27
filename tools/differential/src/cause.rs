//! The known causes of the divergences between the two cores, and how a
//! step that the screens part at shows one.

use lokyr_vt_core::page::Width;
use lokyr_vt_core::parser::MAX_PARAMS;
use lokyr_vt_core::terminal::{Charset, Shift};
use lokyr_vt_core::unicode::Widths;

use crate::{Before, HEAD, Screen, Step, TAIL};

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
    /// The cursor that DECRC brings back below the scroll region.
    RestoredBelow,
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
                "An edit that would split a wide character, text inserted among them too, empties it whole in the VT core, as in xterm; alacritty_terminal empties or moves the cells that the edit takes alone, which leaves half of the character. Text inserted at the start of a row moves the wide character there in the VT core, which keeps the spacer head above it; alacritty_terminal empties the head"
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
            Self::RestoredBelow => {
                "DECRC, and the end of the alternate screen that mode 1049 makes, put the cursor \
                 that DECSC saved below the scroll region at its bottom margin in the VT core, \
                 when they bring origin mode back on, as xterm does; alacritty_terminal puts it \
                 back where it was saved, which its next move takes to the margin"
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
            .is_none_or(|(lokyr, alacritty)| lokyr.rows == alacritty.rows)
    }

    /// Whether the screens' rows are the same but for the rows in `rows`.
    fn rows_same_but(&self, rows: &[usize]) -> bool {
        self.screens.is_none_or(|(lokyr, alacritty)| {
            lokyr.rows.len() == alacritty.rows.len()
                && (0..lokyr.rows.len())
                    .filter(|y| !rows.contains(y))
                    .all(|y| lokyr.rows[y] == alacritty.rows[y])
        })
    }

    /// The VT core's cursor and `alacritty_terminal`'s, the VT core's after
    /// the step for both if the screens are the same.
    fn cursors(&self) -> (At, At) {
        self.screens.map_or(
            (self.after.cursor, self.after.cursor),
            |(lokyr, alacritty)| (lokyr.cursor, alacritty.cursor),
        )
    }

    /// Whether the cursors are the same, or differ as an edit makes them
    /// when the wrap was pending: in that alone, which the VT core ended
    /// and `alacritty_terminal` kept.
    fn cursors_same_but_the_wrap(&self) -> bool {
        let (lokyr, alacritty) = self.cursors();
        lokyr == alacritty || (self.before.cursor.2 && self.wrap_kept())
    }

    /// Whether the cursors differ in their pending wrap alone, which the VT
    /// core ended and `alacritty_terminal` kept.
    fn wrap_kept(&self) -> bool {
        let (lokyr, alacritty) = self.cursors();
        (lokyr.0, lokyr.1) == (alacritty.0, alacritty.1) && !lokyr.2 && alacritty.2
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
    if restores(step) && seen.same() {
        if after.shift != before.shift {
            return Some(Cause::Shift);
        }
        if after.origin != before.origin {
            return Some(Cause::SavedOrigin);
        }
    }
    let (lokyr, alacritty) = seen.cursors();
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
                    && lokyr == before.cursor =>
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
                    && lokyr == before.cursor =>
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
        } if intermediates == b"#"
            && seen.cells_same()
            && lokyr == (0, 0, false)
            && alacritty == before.cursor =>
        {
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

/// Whether `step` brings back the cursor that DECSC saved: DECRC, SCORC,
/// and the alternate screen's end that 1049 makes.
fn restores(step: &Step) -> bool {
    match step {
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
    }
}

/// The known cause of the difference between the screens at a step that
/// they part at, recognized from what the step made the VT core do, its
/// state before it, and the difference's shape, as each cause makes it.
pub(crate) fn cause(seen: &Seen<'_>) -> Option<Cause> {
    let (lokyr_screen, alacritty_screen) = seen.screens?;
    let before = seen.before;
    let (x, y, pending) = before.cursor;
    let (lokyr, alacritty) = (lokyr_screen.cursor, alacritty_screen.cursor);
    let place = |at: At| (at.0, at.1);
    let first = |params: &[u16]| usize::from(params.first().copied().unwrap_or(0).max(1));
    let (top, bottom) = before.region;
    match seen.step {
        // A restore that brings origin mode back on, which puts the VT
        // core's cursor at the bottom margin, and `alacritty_terminal`'s
        // back where it was saved, below it, in the same column.
        step if restores(step)
            && seen.after.origin
            && seen.cells_same()
            && lokyr.0 == alacritty.0
            && lokyr.2 == alacritty.2
            && lokyr.1 == bottom
            && alacritty.1 > bottom =>
        {
            Some(Cause::RestoredBelow)
        }
        Step::Csi {
            params,
            intermediates,
            final_byte,
            ..
        } => match (intermediates.as_slice(), final_byte) {
            // DECOM's reset, which `alacritty_terminal`'s cursor stays
            // through: where it was, unless a reset of 1049 in the sequence
            // leaves the alternate screen, which brings the saved cursor back
            // in both cores; or, after a reset of 1049 before it, where 1049
            // saved it on the primary screen.
            (b"?", b'l')
                if params.contains(&6)
                    && seen.cells_same()
                    && lokyr == (0, 0, false)
                    && ((place(alacritty) == (x, y)
                        && !(params.contains(&1049) && before.restored.is_some()))
                        || (params
                            .iter()
                            .position(|&mode| mode == 1049)
                            .is_some_and(|at| params[at..].contains(&6))
                            && before.restored == Some(place(alacritty)))) =>
            {
                Some(Cause::Decom)
            }
            // CUU, CUD, CNL, CPL, VPR, CHA and HPA in origin mode, below a
            // top margin, where the rows part as each core moves, and the
            // columns do not.
            (b"", b'A' | b'B' | b'E' | b'F' | b'e' | b'G' | b'`')
                if before.origin
                    && top > 0
                    && seen.cells_same()
                    && lokyr.0 == alacritty.0
                    && !lokyr.2
                    && !alacritty.2
                    && lokyr.1 != alacritty.1
                    && (lokyr.1, alacritty.1)
                        == origin_rows(*final_byte, first(params), y, before.region) =>
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
                    && lokyr.0 == alacritty.0
                    && !lokyr.2
                    && !alacritty.2
                    && lokyr.1 == top
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
                    && lokyr.0 == alacritty.0
                    && !lokyr.2
                    && !alacritty.2
                    && lokyr.1 == bottom
                    && alacritty.1 > bottom =>
            {
                Some(Cause::Margins)
            }
            // DCH past the row's end, which empties the VT core's row from the
            // cursor, and `alacritty_terminal`'s from as many cells back from
            // the row's end, which keep what the VT core keeps.
            (b"", b'P')
                if first(params) > before.cols - x
                    && seen.rows_same_but(&[y])
                    && seen.cursors_same_but_the_wrap()
                    && lokyr_screen.rows[y].len() <= x
                    && alacritty_screen.rows[y]
                        == prefix(
                            &lokyr_screen.rows[y],
                            before.cols - first(params).min(before.cols),
                        ) =>
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
                    && lokyr == (0, y, false)
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
                    && lokyr_screen.rows[0].is_empty() =>
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
                    && but_last(&alacritty_screen.rows[y], before.cols) == lokyr_screen.rows[y] =>
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
                    && lokyr.0 == 0
                    && alacritty.0 > 0
                    && lokyr.1 == alacritty.1
                    && !lokyr.2
                    && alacritty.2 == pending =>
            {
                Some(Cause::CbtFirst)
            }
            // REP in insert mode, whose copies meet a wide character in a row
            // that they go in: on the rows that they change, and in the
            // spacer heads above the rows that they insert at the start of.
            (b"", b'b')
                if before.insert
                    && meets_a_wide_character(params, before)
                    && lokyr == alacritty
                    && before.last_printed.is_some_and(|(c, _)| {
                        inserted_over_wide(seen, &copies(params, before, None), c)
                    }) =>
            {
                Some(Cause::WideEdit)
            }
            // SD, which `alacritty_terminal` takes, and the cursor stays.
            (b"", b'T') if params.len() > 1 && lokyr == alacritty => Some(Cause::Highlight),
            // REP in insert mode of a wide character, one of whose copies
            // lands at the last column alone: the first from it, or a later
            // one when the cells left from the cursor are odd, or, when they
            // are even and the rows' cells odd, one on a row after the
            // cursor's. That copy wraps as InsertWrap tells, inserted in the
            // next row in the VT core and written over its first cells in
            // `alacritty_terminal`; the copies after it insert in both.
            (b"", b'b')
                if before.insert
                    && before.autowrap
                    && before.last_printed.is_some_and(|(_, width)| width == 2)
                    && !pending
                    && if (before.cols - x) % 2 == 1 {
                        first(params) > (before.cols - 1 - x) / 2
                    } else {
                        before.cols % 2 == 1
                            && first(params) > (before.cols - x) / 2 + (before.cols - 1) / 2
                    }
                    && lokyr == alacritty
                    && before.last_printed.is_some_and(|(c, _)| {
                        inserted_on_wrapping(seen, &copies(params, before, None), c)
                    }) =>
            {
                Some(Cause::InsertWrap)
            }
            // REP of a wide character whose copies wrap from the last column
            // over half of a wide character at the start of a row below: the
            // spacer heads that they left, at the ends of the rows that they
            // wrap from, alone differ.
            (b"", b'b')
                if before.autowrap
                    && !before.insert
                    && lokyr == alacritty
                    && before.last_printed.is_some_and(|(_, width)| width == 2)
                    && heads_left(seen, &copies(params, before, None)) =>
            {
                Some(Cause::WrapHead)
            }
            // REP of a wide character without autowrap, whose copies reach
            // the last column with a cell left, in insert mode too: the VT
            // core's last copy goes one column back, over half of the copy
            // before it, and `alacritty_terminal` drops it. The row's last
            // three cells alone differ.
            (b"", b'b')
                if before.last_printed.is_some_and(|(_, width)| width == 2)
                    && !before.autowrap
                    && (before.cols - x) % 2 == 1
                    && 2 * first(params) > before.cols - x
                    && lokyr == alacritty
                    && seen.rows_same_but(&[y])
                    && before.last_printed.is_some_and(|(c, _)| {
                        same_but_the_end(lokyr_screen, alacritty_screen, y, before.cols, c)
                    }) =>
            {
                Some(Cause::NoWrapWide)
            }
            // REP after RIS, which repeats the character that the VT core
            // forgot in `alacritty_terminal` alone: the VT core leaves its
            // cursor.
            (b"", b'b')
                if lokyr == before.cursor
                    && before.reset.is_some_and(|width| {
                        printed_alone(seen, &copies(params, before, Some(usize::from(width))))
                    }) =>
            {
                Some(Cause::RepReset)
            }
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
                && lokyr == before.cursor
                && alacritty.0 == 0
                && (seen.cells_same() || y == bottom) =>
        {
            Some(Cause::Tab)
        }
        // The characters whose widths differ, on the rows that they print on.
        // The VT core prints U+FFFD, one cell, and `alacritty_terminal`
        // nothing.
        Step::Print('\u{FFFD}')
            if (0x80..=0x9F).contains(&seen.byte)
                && lokyr == advanced(before, 1, 1)
                && alacritty == before.cursor
                && seen.rows_same_but(&[y, lokyr.1]) =>
        {
            Some(Cause::C1)
        }
        // The VT core takes no cell, and `alacritty_terminal` one.
        Step::Print('\u{D7C7}'..='\u{D7CA}' | '\u{D7FC}'..='\u{D7FF}')
            if lokyr == before.cursor
                && alacritty == advanced(before, 1, 1)
                && seen.rows_same_but(&[y, alacritty.1]) =>
        {
            Some(Cause::Jamo)
        }
        // The VT core widens the cluster by a cell, and `alacritty_terminal`
        // takes none.
        Step::Print('\u{FE0F}')
            if lokyr == advanced(before, 1, 1)
                && alacritty == before.cursor
                && seen.rows_same_but(&[y, lokyr.1]) =>
        {
            Some(Cause::Emoji)
        }
        // Text inserted at the second half of a wide character, or that
        // pushes one past the row's end, on the row that it goes in; or at
        // the start of a row, under the spacer head above it.
        Step::Print(c)
            if before.insert && lokyr == alacritty && printed_in_insert_mode(seen, *c) =>
        {
            Some(Cause::WideEdit)
        }
        // A wide character that does not fit at the last column, in insert
        // mode.
        Step::Print(c)
            if seen.wide
                && before.insert
                && before.autowrap
                && !pending
                && x + 1 == before.cols
                && lokyr == alacritty
                && inserted_on_wrapping(seen, &copies(&[1], before, Some(2)), *c) =>
        {
            Some(Cause::InsertWrap)
        }
        // A wide character that does not fit at the last column, without
        // autowrap.
        Step::Print(c)
            if seen.wide
                && !before.autowrap
                && x + 1 == before.cols
                && lokyr == alacritty
                && seen.rows_same_but(&[y])
                && same_but_the_end(lokyr_screen, alacritty_screen, y, before.cols, *c) =>
        {
            Some(Cause::NoWrapWide)
        }
        // A wide character that wraps from the last column over half of a
        // wide character in the first two columns of the next row, where
        // `alacritty_terminal` empties a spacer head above: the head that
        // it left, at the end of the cursor's row, alone differs.
        Step::Print(_)
            if seen.wide
                && before.autowrap
                && !before.insert
                && !pending
                && x + 1 == before.cols
                && before.below[..2.min(before.below.len())]
                    .iter()
                    .any(|width| matches!(width, Width::Wide | Width::SpacerTail))
                && seen.rows_same_but(&[y])
                && lokyr == alacritty
                && lokyr_screen.rows[y].get(x).is_some_and(|cell| cell == HEAD)
                && but_last(&lokyr_screen.rows[y], before.cols)
                    == but_last(&alacritty_screen.rows[y], before.cols) =>
        {
            Some(Cause::WrapHead)
        }
        _ => None,
    }
}

/// The rows that a move with `final_byte` by `n`, or CHA and HPA, takes the
/// cursor to from row `y` in origin mode, in the VT core and in
/// `alacritty_terminal`, with the scroll region `region`: the VT core moves
/// within the region; `alacritty_terminal` adds the region's top to the row
/// that it reaches, as if that were relative to it, and stops at its bottom
/// or at the first row.
fn origin_rows(
    final_byte: u8,
    n: usize,
    y: usize,
    (top, bottom): (usize, usize),
) -> (usize, usize) {
    let target = match final_byte {
        b'A' | b'F' => y as isize - n as isize,
        b'B' | b'E' | b'e' => (y + n) as isize,
        _ => y as isize,
    };
    let lokyr = target.clamp(top as isize, bottom as isize);
    let alacritty = (target + top as isize).clamp(0, bottom as isize);
    (lokyr as usize, alacritty as usize)
}

/// Where printing `n` characters `width` cells wide takes the cursor from
/// where it was `before`, on a screen without left and right margins: a
/// wide character that does not fit at the last column leaves it for the
/// next row, a wrap past the bottom margin scrolls and leaves the cursor
/// there, and without autowrap the last column takes each character, a
/// wide one from a column back.
fn advanced(before: &Before, n: usize, width: usize) -> At {
    let (mut x, mut y, mut pending) = before.cursor;
    let (cols, bottom) = (before.cols, before.region.1);
    let next_row = |y: usize| {
        if y != bottom && y + 1 < before.rows {
            y + 1
        } else {
            y
        }
    };
    for _ in 0..n {
        if pending && before.autowrap {
            (x, y, pending) = (0, next_row(y), false);
        }
        if width == 2 && x + 1 == cols {
            if before.autowrap {
                (x, y) = (0, next_row(y));
            } else {
                x -= 1;
            }
        }
        if x + width >= cols {
            (x, pending) = (cols - 1, true);
        } else {
            x += width;
        }
    }
    (x, y, pending)
}

/// Whether the screens differ as `copies` that `alacritty_terminal` printed
/// alone make them, after RIS, which leaves the VT core's screen empty, as
/// nothing that writes a cell comes between them without ending the reset:
/// the VT core's screen is empty, and `alacritty_terminal`'s is too but for
/// the cells that the copies went in, those of each pass in a row as the
/// scrolls leave the rows, the cells right of them in insert mode, which
/// the copies pushed, and the spacer head at the end of a row that a wide
/// copy wrapped from.
fn printed_alone(seen: &Seen<'_>, copies: &Copies) -> bool {
    let Some((other, printed)) = seen.screens else {
        return true;
    };
    let (cols, insert) = (seen.before.cols, seen.before.insert);
    other.rows.iter().all(Vec::is_empty)
        && (0..printed.rows.len()).all(|row| {
            let theirs = padded(&printed.rows[row], cols);
            let passes = copies.passes(row);
            (0..cols).all(|col| {
                theirs[col] == " "
                    || passes
                        .iter()
                        .any(|pass| col >= pass.from && (insert || col < pass.from + pass.cells))
                    || (col + 1 == cols && theirs[col] == HEAD && copies.wraps_from.contains(&row))
            })
        })
}

/// Whether `printed`, a row of the core that printed, is `other`, the same
/// row of the other core, but for the spacer head at its end, which the
/// printing emptied.
fn head_emptied(printed: &[String], other: &[String], cols: usize) -> bool {
    other.get(cols - 1).is_some_and(|cell| cell == HEAD)
        && printed.get(cols - 1).is_none_or(|cell| cell == " ")
        && but_last(printed, cols) == but_last(other, cols)
}

/// Whether the screens differ as `copies` of `c` inserted in insert mode
/// make them differ where they meet wide characters. In each row that they
/// write, `alacritty_terminal` empties the cell left of where they start,
/// which the VT core keeps as it was, and copies of its own, next to the
/// half of a wide character that it writes over, where the VT core keeps
/// them; and keeps half of a wide character that the VT core empties: the
/// first half of one that they split where they start, the second half of
/// one pushed past them, and at the row's end the first half of one that
/// they push off it, or a second half that they leave there without its
/// first. At the end of a row that a wide copy wraps from, and above each
/// row whose start they write in, where the row ended with one, the VT core
/// keeps the spacer head that `alacritty_terminal` empties as it writes
/// over the start of the row below. The VT core's row holds no half of a
/// wide character alone; in a row below that the copies wrap to, its two
/// cells past them hold what the row started with, which the insert
/// pushed; and copies of a wide character that reach the row's end end it
/// with a copy, where one that did not fit went a column back, whatever
/// `alacritty_terminal`'s row holds there. A row that they come back to the
/// start of differs as [`came_back`] tells, the copies taken as inserted in
/// both.
fn inserted_over_wide(seen: &Seen<'_>, copies: &Copies, c: char) -> bool {
    let Some((lokyr, alacritty)) = seen.screens else {
        return true;
    };
    let cols = seen.before.cols;
    let copy = c.to_string();
    (0..lokyr.rows.len()).all(|row| {
        if lokyr.rows[row] == alacritty.rows[row] {
            return true;
        }
        let passes = copies.passes(row);
        if let Some(&&Stretch {
            from, cells, start, ..
        }) = passes.last()
        {
            let (ours_row, theirs_row) = (
                padded(&lokyr.rows[row], cols),
                padded(&alacritty.rows[row], cols),
            );
            // Each copy is taken as inserted in both cores: where
            // `alacritty_terminal` wrote one over the row's start, rows of
            // copies alike look the same, and other rows are InsertWrap's.
            if start == Start::Back {
                return came_back(seen.before, &passes, &ours_row, &theirs_row, &copy, false);
            }
            // In a row below that the copies wrap to, the VT core's two cells
            // after them hold what the row started with, which the insert
            // pushed: as far as a later wrap's head at its end lets them. A
            // new row held nothing, which the cells' comparison sees.
            let end = if copies.wraps_from.contains(&row) {
                cols - 1
            } else {
                cols
            };
            let pushed = match start {
                Start::Below(y) => seen
                    .before
                    .starts
                    .get(y - seen.before.cursor.1)
                    .is_none_or(|start| pushed_right(&ours_row[..end], from + cells, start)),
                Start::Cursor | Start::New | Start::Back => true,
            };
            // Copies of a wide character that reach the row's end: one that
            // does not fit there, as without autowrap, goes one column back
            // in the VT core, over the copy before it, whose first half it
            // empties where they left an odd number of cells, and
            // `alacritty_terminal` drops it. The VT core's row ends with a
            // copy, whatever `alacritty_terminal`'s holds there.
            let back = first_half(&copy) && from + cells == cols;
            return pushed
                && whole(&ours_row)
                && (!back || ours_row.ends_with(&[copy.as_str(), TAIL]))
                && (0..cols).all(|col| {
                    let (ours, theirs) = (ours_row[col], theirs_row[col]);
                    ours == theirs
                        || (back
                            && (col + 2 >= cols
                                || (col + 3 == cols && ours == " " && first_half(theirs))))
                        || (theirs == " "
                            && ((col + 1 == from && seen.before.left.as_deref() == Some(ours))
                                || ((from..from + cells).contains(&col)
                                    && (ours == copy || ours == TAIL))))
                        || (ours == " "
                            && ((col + 1 == from && first_half(theirs))
                            || (col == from + cells && theirs == TAIL)
                            || (col + 1 == cols && first_half(theirs))
                            // A second half that the copies left without its
                            // first, as they reach the row's end without
                            // autowrap.
                            || (col + 1 == cols
                                && theirs == TAIL
                                && !col
                                    .checked_sub(1)
                                    .is_some_and(|left| first_half(theirs_row[left])))))
                        || (col + 1 == cols
                            && ours == HEAD
                            && theirs == " "
                            && copies.wraps_from.contains(&row))
                });
        }
        head_above(seen, copies, row)
    })
}

/// Whether the screens' rows differ in their last cells alone, in rows
/// that `copies` wrapped from at the last column: the VT core's has a
/// spacer head there and `alacritty_terminal`'s an empty cell.
fn heads_left(seen: &Seen<'_>, copies: &Copies) -> bool {
    let Some((lokyr, alacritty)) = seen.screens else {
        return true;
    };
    let cols = seen.before.cols;
    (0..lokyr.rows.len()).all(|row| {
        lokyr.rows[row] == alacritty.rows[row]
            || (copies.wraps_from.contains(&row)
                && lokyr.rows[row]
                    .get(cols - 1)
                    .is_some_and(|cell| cell == HEAD)
                && alacritty.rows[row]
                    .get(cols - 1)
                    .is_none_or(|cell| cell == " ")
                && but_last(&lokyr.rows[row], cols) == but_last(&alacritty.rows[row], cols))
    })
}

/// Whether the screens differ as `copies` of the wide character `c` make
/// them differ when one wraps from the last column in insert mode: the
/// rows that they wrap to differ in that the VT core's has the copies
/// inserted, and `alacritty_terminal`'s the first written over its cells.
/// Both start with the copies, of which `alacritty_terminal` may have
/// emptied one, and past them the VT core's cells are what the row started
/// with, and then `alacritty_terminal`'s two cells right, but for a wide
/// character that the insert split at the row's end. The first row, which
/// they wrapped from, differs in its last cell, and in the cell left of
/// the first copy: when that copy went over the second half of a wide
/// character, whose first half the VT core empties, or where
/// `alacritty_terminal` empties it as its insert turns the second half of a
/// wide character at the row's end round to the cursor. A row that a later
/// copy wraps from in turn, on a screen of an odd number of columns, ends
/// with the spacer head that the VT core leaves, which `alacritty_terminal`
/// may empty. The rows are as the region's scrolls leave them. A row that
/// they come back to the start of, the screen's last below the region,
/// differs as [`came_back`] tells, and the row above it ends with a spacer
/// head that `alacritty_terminal` empties as it writes over the wide
/// character at that start.
fn inserted_on_wrapping(seen: &Seen<'_>, copies: &Copies, c: char) -> bool {
    let Some((lokyr, alacritty)) = seen.screens else {
        return true;
    };
    // A wrap from the bottom margin scrolls, and the copy goes into a new
    // row, where both cores write it alike.
    if seen.before.cursor.1 == seen.before.region.1 {
        return false;
    }
    let cols = seen.before.cols;
    let copy = c.to_string();
    (0..lokyr.rows.len()).all(|row| {
        if lokyr.rows[row] == alacritty.rows[row] {
            return true;
        }
        let passes = copies.passes(row);
        let Some(stretch) = passes.last() else {
            return head_above(seen, copies, row);
        };
        let (ours, theirs) = (
            padded(&lokyr.rows[row], cols),
            padded(&alacritty.rows[row], cols),
        );
        // A row that a later copy wraps from ends with its head, which
        // takes the cell that the insert pushed there.
        let end = if copies.wraps_from.contains(&row) {
            cols - 1
        } else {
            cols
        };
        // Among the copies, `alacritty_terminal` empties one next to the
        // half of a wide character that the insert brought to it.
        let inserted = |cells: usize| {
            (0..cells).all(|col| {
                ours[col] == theirs[col]
                    || (theirs[col] == " " && (ours[col] == copy || ours[col] == TAIL))
            }) && two_cells_right(&ours, &theirs, cells)
        };
        match stretch.start {
            Start::Cursor => {
                // The first copy, written over the second half of a wide
                // character, empties its first half in the VT core, which
                // `alacritty_terminal` keeps as it was.
                let from = stretch.from;
                let mut ours = lokyr.rows[row].clone();
                if seen.before.widths.get(from) == Some(&Width::SpacerTail)
                    && let (Some(left), Some(theirs)) = (
                        from.checked_sub(1).and_then(|left| ours.get_mut(left)),
                        from.checked_sub(1)
                            .and_then(|left| alacritty.rows[row].get(left)),
                    )
                    && left == " "
                    && seen.before.left.as_ref() == Some(theirs)
                {
                    left.clone_from(theirs);
                }
                // It differs in its last cell, a spacer head in the VT core,
                // among its copies as the rows that they wrap to do, and in
                // the cell left of them, which `alacritty_terminal` empties
                // as its insert turns the second half of a wide character
                // at the row's end round to the cursor.
                let ours = padded(&ours, cols);
                (0..cols).all(|col| {
                    ours[col] == theirs[col]
                        || (col + 1 == cols && ours[col] == HEAD && theirs[col] == " ")
                        || (col + 1 == from
                            && theirs[col] == " "
                            && seen.before.left.as_deref() == Some(ours[col]))
                        || ((from..from + stretch.cells).contains(&col)
                            && theirs[col] == " "
                            && (ours[col] == copy || ours[col] == TAIL))
                })
            }
            Start::Back => came_back(seen.before, &passes, &ours, &theirs, &copy, true),
            start => {
                let cells = stretch.cells;
                let started = match start {
                    Start::Below(y) => seen
                        .before
                        .starts
                        .get(y - seen.before.cursor.1)
                        .is_none_or(|start| pushed_right(&ours[..end], cells, start)),
                    _ => pushed_right(&ours[..end], cells, &[" ".to_owned(), " ".to_owned()]),
                };
                started
                    && inserted(cells)
                    && (!copies.wraps_from.contains(&row)
                        || (ours[cols - 1] == HEAD && [HEAD, " "].contains(&theirs[cols - 1])))
            }
        }
    })
}

/// Whether the screens' row `row`, which no copy of `copies` writes in,
/// differs as they make it when they write over the start of the row
/// below: the row ended with a spacer head, which the VT core keeps and
/// `alacritty_terminal` empties. The rows are as the region's scrolls leave
/// them, and `Before` and `copies.starts` tell them as they were.
fn head_above(seen: &Seen<'_>, copies: &Copies, row: usize) -> bool {
    let Some((lokyr, alacritty)) = seen.screens else {
        return true;
    };
    let before_them = copies.before_scrolls(row, seen.before.region);
    copies.starts.contains(&(before_them + 1))
        && seen.before.ends_with_head.get(before_them) == Some(&true)
        && head_emptied(&alacritty.rows[row], &lokyr.rows[row], seen.before.cols)
}

/// Whether the screens' row that copies came back to the start of, the
/// screen's last below the region, differs as their passes in insert mode
/// make it differ: `ours` and `theirs` are its cells, and `passes` those
/// that wrote in it. The last pass's copies come first, of which
/// `alacritty_terminal` may have emptied some. Past them, the VT core's row
/// holds what the row held before that pass, which it pushed right, and so
/// does `alacritty_terminal`'s, with which the VT core's cells are compared
/// but for the two right past the copies, compared with what the row held
/// at its start. After a wrap that left a spacer head,
/// `alacritty_terminal` wrote the first copy over the row's first two
/// cells, and its row holds the rest from the third, if `writes_over`;
/// otherwise it is taken to have pushed them too, as rows of copies alike
/// show it, but for the VT core's two cells past the copies, which hold
/// the row's first two. What the row held is the cursor's row as it was,
/// up to where the pass before started, but for the first half of a wide
/// character that the pass split there, which the VT core empties, and the
/// cell left of it, which `alacritty_terminal` may empty; and then the
/// copies of the pass before, of which `alacritty_terminal` may have
/// emptied some. The cells pushed past the row's end are gone, but for the
/// first half of a wide character that the push split, which the VT core
/// empties: its row holds no half of a wide character alone.
fn came_back(
    before: &Before,
    passes: &[&Stretch],
    ours: &[&str],
    theirs: &[&str],
    copy: &str,
    writes_over: bool,
) -> bool {
    let Some((last, earlier)) = passes.split_last() else {
        return true;
    };
    let cols = ours.len();
    // Where the pass before started; none when the copies started with a
    // wrap pending at the end of the row.
    let from = earlier.last().map_or(cols, |pass| pass.from);
    let (pushed, skipped) = (last.cells, if writes_over && last.over { 2 } else { 0 });
    let copied = |ours: &str, theirs: &str| {
        ours == theirs || (theirs == " " && (ours == copy || ours == TAIL))
    };
    // What the row held at column `col` before the last pass.
    let held = |col: usize| {
        if col < from {
            before
                .starts
                .first()
                .map_or(" ", |start| start.get(col).map_or(" ", String::as_str))
        } else if first_half(copy) && (col - from) % 2 == 1 {
            TAIL
        } else {
            copy
        }
    };
    whole(ours)
        && (0..pushed).all(|col| copied(ours[col], theirs[col]))
        && (0..cols - pushed).all(|col| {
            let ours_cell = ours[pushed + col];
            let split = |cell: &str| ours_cell == " " && first_half(cell);
            let at_end = pushed + col + 1 == cols;
            // After a wrap that left a spacer head, the VT core's two cells
            // past the copies hold the row's first two, which
            // `alacritty_terminal` wrote over.
            if last.over
                && col < 2
                && ours_cell != held(col)
                && !((col + 1 == from || at_end) && split(held(col)))
            {
                return false;
            }
            if col < skipped {
                return true;
            }
            let theirs_at = pushed + col - skipped;
            let theirs_cell = theirs[theirs_at];
            (if col < from {
                ours_cell == theirs_cell
            } else {
                copied(ours_cell, theirs_cell)
            }) || (col + 1 == from
                    && (split(theirs_cell)
                        || (theirs_cell == " " && before.left.as_deref() == Some(ours_cell))))
                // The second half of a wide character whose first half
                // `alacritty_terminal` wrote over, which it emptied.
                || (skipped == 2 && col == 2 && ours_cell == TAIL && theirs_cell == " ")
                || (at_end && split(theirs_cell))
        })
}

/// Whether the VT core's two cells from column `from`, in a row that copies
/// inserted in wrap to, hold what the row started with, `start`, which the
/// insert pushed right and `alacritty_terminal` wrote over: as far as
/// `ours`, the row's cells, go, and but for the first half of a wide
/// character that the insert split at their end, which it empties.
fn pushed_right(ours: &[&str], from: usize, start: &[String; 2]) -> bool {
    let cols = ours.len();
    (0..2).all(|i| {
        let col = from + i;
        col >= cols
            || ours[col] == start[i]
            || (col + 1 == cols
                && ours[col] == " "
                && start[i]
                    .chars()
                    .next()
                    .is_some_and(|c| Widths::default().char_width(c) == 2))
    })
}

/// Whether `row`, a row's cells, holds no half of a wide character without
/// its other half.
fn whole(row: &[&str]) -> bool {
    (0..row.len()).all(|col| {
        if first_half(row[col]) {
            row.get(col + 1) == Some(&TAIL)
        } else if row[col] == TAIL {
            col.checked_sub(1).is_some_and(|left| first_half(row[left]))
        } else {
            true
        }
    })
}

/// Whether `cell` holds the first half of a wide character.
fn first_half(cell: &str) -> bool {
    cell.chars()
        .next()
        .is_some_and(|c| Widths::default().char_width(c) == 2)
}

/// The `cols` cells of `row`, with the empty ones at its end that
/// [`Screen`] leaves out.
fn padded(row: &[String], cols: usize) -> Vec<&str> {
    let mut cells: Vec<&str> = row.iter().map(String::as_str).collect();
    cells.resize(cols, " ");
    cells
}

/// Whether the cells of `ours` from column `from + 2` are those of
/// `theirs` from `from`: but for the first, which may be the second half
/// of a wide character whose first half `alacritty_terminal` wrote over,
/// and emptied in `theirs`, and the last, which may be empty in `ours`
/// where `theirs` has the first half of a wide character that the insert
/// split at the row's end.
fn two_cells_right(ours: &[&str], theirs: &[&str], from: usize) -> bool {
    let cols = ours.len();
    if from + 2 >= cols {
        return true;
    }
    let cells = cols - from - 2;
    (0..cells).all(|i| {
        let (ours, theirs_cell) = (ours[from + 2 + i], theirs[from + i]);
        ours == theirs_cell
            || (i == 0 && ours == TAIL && theirs_cell == " ")
            || (i + 1 == cells && ours == " " && theirs.get(from + i + 1) == Some(&TAIL))
    })
}

/// Whether row `y` of the screens is the same in both but for its last
/// three cells, where a wide character `c` at the last column without
/// autowrap goes one column back in the VT core: its last two cells hold
/// it, and the one before is as `alacritty_terminal`'s, or empty where it
/// held the first half of a wide character whose second half the character
/// went over, which `alacritty_terminal` keeps.
fn same_but_the_end(lokyr: &Screen, alacritty: &Screen, y: usize, cols: usize, c: char) -> bool {
    let start = cols.saturating_sub(3);
    let (ours, theirs) = (
        padded(&lokyr.rows[y], cols),
        padded(&alacritty.rows[y], cols),
    );
    prefix(&lokyr.rows[y], start) == prefix(&alacritty.rows[y], start)
        && ours[cols - 2] == c.to_string()
        && ours[cols - 1] == TAIL
        && (cols < 3
            || ours[start] == theirs[start]
            || (ours[start] == " " && first_half(theirs[start])))
}

/// `row`'s first `len` cells, without the empty ones at the end, as
/// [`Screen`] keeps a row.
fn prefix(row: &[String], len: usize) -> Vec<String> {
    let mut cells = row[..row.len().min(len)].to_vec();
    while cells.last().is_some_and(|cell| cell == " ") {
        cells.pop();
    }
    cells
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
/// wide character, or pushes one past the row's end: on the cursor's row,
/// or, when a wrap is pending, on the row that the wrap lands in: the next,
/// or the cursor's own at the last row below the scroll region, which does
/// not scroll; a wrap at the bottom margin lands in a new row.
fn inserts_at_a_tail(before: &Before) -> bool {
    let (x, y, pending) = before.cursor;
    let bottom = before.region.1;
    let (row, x) = if !pending {
        (&before.widths, x)
    } else if y > bottom && y + 1 == before.rows {
        (&before.widths, 0)
    } else if y == bottom {
        return false;
    } else {
        (&before.below, 0)
    };
    let tail = |x: usize| row.get(x) == Some(&Width::SpacerTail);
    tail(x) || tail(before.cols - 1) || tail(before.cols - 2)
}

/// Where REP's copies go on the screen that `before` holds.
struct Copies {
    /// The rows that they write as the rows were before them, the cursor's
    /// first unless a wrap was pending, with no row twice.
    rows: Vec<usize>,
    /// Whether they wrap back to the start of their last row, the screen's
    /// last below the region, where a wrap stays.
    again: bool,
    /// The rows whose first two columns they write in, as they were: the
    /// cursor's, from there, and those that a wrap takes them to the start
    /// of.
    starts: Vec<usize>,
    /// What each pass of them writes, in order, in the rows as the screen
    /// shows them after the copies, the region's scrolls taken: one pass a
    /// row, but in the row that they come back to the start of.
    stretches: Vec<Stretch>,
    /// The rows that a wide copy wraps from at their last column, leaving
    /// a spacer head there, as the screen shows them after the copies.
    wraps_from: Vec<usize>,
    /// How many times they scroll the region.
    scroll_count: usize,
}

impl Copies {
    /// The passes that write in `row`, in order.
    fn passes(&self, row: usize) -> Vec<&Stretch> {
        self.stretches
            .iter()
            .filter(|stretch| stretch.row == row)
            .collect()
    }

    /// The row that the screen's `row` was before the copies scrolled the
    /// region from `top` to `bottom`: past the region for one that they
    /// brought in, which no copy wrote the start of the row below.
    fn before_scrolls(&self, row: usize, (top, bottom): (usize, usize)) -> usize {
        if (top..=bottom).contains(&row) {
            row + self.scroll_count
        } else {
            row
        }
    }
}

/// What a pass of copies writes in a row, as the screen shows the rows
/// after them.
#[derive(Clone, Copy, Debug)]
struct Stretch {
    /// The row, the region's scrolls taken.
    row: usize,
    /// The column that the copies start from.
    from: usize,
    /// The cells that they take.
    cells: usize,
    /// What the row held before them.
    start: Start,
    /// Whether the wrap that took them there left a spacer head, as a wide
    /// copy did not fit at the last column: `alacritty_terminal` writes that
    /// copy over the row's first two cells, and inserts the others.
    over: bool,
}

/// What a row that a pass of copies writes in held before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    /// The cursor's row, which they start in.
    Cursor,
    /// Row `y`, below the cursor's, which a wrap takes them to the start of.
    Below(usize),
    /// Nothing: a row that a scroll of the region brings in.
    New,
    /// The screen's last row, below the region, which a wrap takes them
    /// back to the start of, with what the passes before wrote there.
    Back,
}

/// Follows REP with `params` as the cores print its copies, of `width`
/// cells each, or of the width of the character before it: a pending wrap,
/// or a wide character at the last column, wraps first, to the next row,
/// or scrolls the region at its bottom margin, or stays on the screen's
/// last row below the region, at its start. Without autowrap, or for a
/// character of no width, the copies stay on the cursor's row.
fn copies(params: &[u16], before: &Before, width: Option<usize>) -> Copies {
    let (mut x, mut y, mut pending) = before.cursor;
    let (top, bottom) = before.region;
    let width = width.unwrap_or_else(|| {
        before
            .last_printed
            .map_or(0, |(_, width)| usize::from(width))
    });
    let mut copies = Copies {
        rows: if pending { Vec::new() } else { vec![y] },
        again: false,
        starts: if !pending && x <= 1 {
            vec![y]
        } else {
            Vec::new()
        },
        stretches: Vec::new(),
        wraps_from: Vec::new(),
        scroll_count: 0,
    };
    if width == 0 || !before.autowrap {
        copies.rows = vec![y];
        copies.stretches = vec![Stretch {
            row: y,
            from: x,
            cells: if width == 0 {
                before.cols - x
            } else {
                (first_count(params) * width).min(before.cols - x)
            },
            start: Start::Cursor,
            over: false,
        }];
        return copies;
    }
    // The passes written, each with its row before the scrolls after it
    // and the scrolls before it, and the one being written.
    let mut written = Vec::new();
    let mut open = (!pending).then_some((
        Stretch {
            row: y,
            from: x,
            cells: 0,
            start: Start::Cursor,
            over: false,
        },
        0,
    ));
    let mut scrolls = 0;
    let mut heads = Vec::new();
    for _ in 0..first_count(params) {
        if pending || (width == 2 && x + 1 == before.cols) {
            if !pending {
                heads.push((y, scrolls));
            }
            written.extend(open.take());
            let start = if y == bottom {
                // The copies after it go into a new row.
                scrolls += 1;
                Start::New
            } else {
                let start = if y + 1 == before.rows {
                    copies.again = true;
                    Start::Back
                } else {
                    y += 1;
                    Start::Below(y)
                };
                if !copies.rows.contains(&y) {
                    copies.rows.push(y);
                }
                if !copies.starts.contains(&y) {
                    copies.starts.push(y);
                }
                start
            };
            let stretch = Stretch {
                row: y,
                from: 0,
                cells: 0,
                start,
                over: !pending,
            };
            (x, pending) = (0, false);
            open = Some((stretch, scrolls));
        }
        if let Some((stretch, _)) = open.as_mut() {
            stretch.cells += width;
        }
        x += width;
        if x >= before.cols {
            (x, pending) = (before.cols - 1, true);
        }
    }
    written.extend(open);
    // A row of the region goes up a row at each scroll after it, and out of
    // the screen past the region's top.
    let shown = |row: usize, before_it: usize| {
        if (top..=bottom).contains(&row) {
            row.checked_sub(scrolls - before_it)
                .filter(|&row| row >= top)
        } else {
            Some(row)
        }
    };
    copies.wraps_from = heads
        .into_iter()
        .filter_map(|(row, before_it)| shown(row, before_it))
        .collect();
    copies.scroll_count = scrolls;
    copies.stretches = written
        .into_iter()
        .filter_map(|(stretch, before_it)| {
            shown(stretch.row, before_it).map(|row| Stretch {
                row,
                cells: stretch.cells.min(before.cols - stretch.from),
                ..stretch
            })
        })
        .collect();
    copies
}

/// REP's count, 1 for none or 0.
fn first_count(params: &[u16]) -> usize {
    usize::from(params.first().copied().unwrap_or(0).max(1))
}

/// Whether the copies of REP with `params`, in insert mode, meet a wide
/// character, which they may push, split or write over, as the shape of
/// the difference tells: right of the cursor, on the cursor's row when no
/// wrap is pending; in a row below that they wrap to; or in the screen's
/// last row below the region, which they wrap back to the start of, one
/// that was there or one of their own.
fn meets_a_wide_character(params: &[u16], before: &Before) -> bool {
    let (x, y, pending) = before.cursor;
    let copies = copies(params, before, None);
    let holds_wide = |row: usize| {
        if row == y {
            before.widths.contains(&Width::SpacerTail)
        } else {
            before.wide_below.get(row - y - 1) == Some(&true)
        }
    };
    (!pending && before.widths[x..].contains(&Width::SpacerTail))
        || copies.rows.iter().any(|&row| row > y && holds_wide(row))
        || (copies.again
            && (copies.rows.last().is_some_and(|&row| holds_wide(row))
                || before.last_printed.is_some_and(|(_, width)| width == 2)))
}

/// Whether the screens differ as a character printed in insert mode makes
/// them, when it lands on the second half of a wide character or pushes one
/// past the row's end: on the row that it goes in, and, when it goes in at
/// the row's first two columns, in the spacer head above that row, which
/// the VT core keeps. `alacritty_terminal` shifts the row by turning its
/// cells, so that the wide character's half at the end comes to the cursor,
/// and writing over it there empties the head. A wrap at the bottom margin,
/// which scrolls, is not taken.
fn printed_in_insert_mode(seen: &Seen<'_>, c: char) -> bool {
    let width = if seen.wide { 2 } else { 1 };
    inserts_at_a_tail(seen.before)
        && inserted_over_wide(seen, &copies(&[1], seen.before, Some(width)), c)
}

/// The sets that G0 to G3 hold.
pub(crate) fn sets(charsets: lokyr_vt_core::terminal::Charsets) -> [Charset; 4] {
    [Shift::G0, Shift::G1, Shift::G2, Shift::G3].map(|slot| charsets.get(slot))
}

#[cfg(test)]
mod tests {
    use super::Cause::*;
    use super::*;
    use crate::explore::{Parted, parted, stepped};

    /// The cause that the rules give the screens that `parted` saw, with
    /// the VT core's as `lokyr`.
    fn explained(parted: &Parted, lokyr: &Screen) -> Option<Cause> {
        let seen = Seen {
            step: &parted.step,
            byte: parted.byte,
            before: &parted.before,
            after: &parted.after,
            screens: (*lokyr != parted.alacritty).then_some((lokyr, &parted.alacritty)),
            wide: parted.wide,
        };
        ending(&seen).or_else(|| cause(&seen))
    }

    /// The inputs that show each cause that the screens show, at the byte
    /// where they part, on a screen of 80 by 24.
    fn cases() -> Vec<(&'static str, Cause, Vec<u8>)> {
        let line_feed = [&[b'a'; 80][..], b"\n"].concat();
        let split_at_the_end = format!(
            "\x1b[2;1Hx{}\x1b[1;80H\x1b[4h\u{4e2d}",
            "\u{4e2d}".repeat(39)
        );
        // REP in insert mode from the second half of a wide character, on a
        // row that ends with a spacer head, below one that wraps into it.
        let from_a_tail = |count: usize| {
            format!(
                "\x1b[2;2H{}\u{4e2d}\x1b[H{}\u{4e2d}\x1b[4h\x08\x1b[{count}b",
                "a".repeat(78),
                "b".repeat(79)
            )
        };
        let (insert_wrap_from_a_tail, repeat_from_a_tail) = (from_a_tail(40), from_a_tail(80));
        // A wide character that wraps in insert mode from the last row,
        // below the region, back to its start, under a spacer head.
        let wrap_back = format!(
            "\x1b[1;2r\x1b[23;1H{}\u{4e2d}\x1b[24;80H\x1b[4h\u{4e2d}",
            "a".repeat(79)
        );
        let cases: [(&str, Cause, &[u8]); 53] = [
            ("c1", C1, b"a\x80b"),
            ("jamo", Jamo, "a\u{D7FF}b".as_bytes()),
            ("emoji", Emoji, "x\u{2600}\u{FE0F}y\r\n".as_bytes()),
            (
                "origin-moves",
                OriginMoves,
                b"\x1b[5;20r\x1b[?6h\x1b[2;1H\x1b[BX",
            ),
            (
                "origin-moves-up",
                OriginMoves,
                b"\x1b[3;10r\x1b[?6h\x1b[4;1H\x1b[2A",
            ),
            ("cha-in-origin", OriginMoves, b"\x1b[2;5r\x1b[?6h\x1b[5GX"),
            ("dch", Dch, b"ABCDEF\x1b[1;2H\x1b[130P"),
            ("line-feed", LineFeed, &line_feed),
            ("decom", Decom, b"\x1b[5;10H\x1b[?6lX"),
            (
                "decom-after-the-restore",
                Decom,
                b"\x1b[5;10H\x1b[?1049h\x1b[2;3H\x1b[?1049;6l",
            ),
            (
                "restored-below",
                RestoredBelow,
                b"\x1b[?6h\x1b[20;1H\x1b7\x1b[5;10r\x1b8",
            ),
            ("decaln", Decaln, b"7\x1b#8"),
            ("tab", Tab, b"\x1b[80GN\t"),
            ("scorc", Scorc, b"x\x1b[2u"),
            ("lines", Lines, b"M\x1b[M"),
            ("edit-wrap", EditWrap, b"\x1b[80GN\x1b[P"),
            ("region", Region, b"y\x1b[24;30r"),
            ("highlight", Highlight, b"x\x1b[;T"),
            ("ed1", Ed1, b"3\n\x1b[1J"),
            ("margins", Margins, b"\x1b[5;20r\x1b[5H\x1b[AX"),
            ("margins-down", Margins, b"\x1b[5;20r\x1b[20H\x1b[BX"),
            ("wide-edit", WideEdit, b"\xe4\xb8\xad\x08\x1b[K"),
            ("cbt", Cbt, b"\x1b[80GN\x1b[Z"),
            ("wide-insert", WideEdit, b"\x1b[4h\xe4\xb8\xad\x08\xc3\xa9"),
            (
                "moved-head",
                MovedHead,
                "\x1b[79Gx\u{4e2d}\x1b[1;1H\x1b[P".as_bytes(),
            ),
            ("cbt-first", CbtFirst, b"\x1b[3gd\x1b[Z"),
            (
                "insert-wrap",
                InsertWrap,
                "\x1b[2;1Hxy\x1b[4h\x1b[1;80H\u{4e2d}".as_bytes(),
            ),
            (
                "insert-wrap-splits-the-end",
                InsertWrap,
                split_at_the_end.as_bytes(),
            ),
            ("rep-reset", RepReset, b" \x1bc\x1b[b"),
            ("rep-reset-twice", RepReset, b" \x1bc\x1bc\x1b[b"),
            (
                "no-wrap-wide",
                NoWrapWide,
                "\x1b[?7l\x1b[80G\u{4e2d}".as_bytes(),
            ),
            (
                "wide-insert-wrap",
                WideEdit,
                "\x1b[2;79H\u{4e2d}\x1b[4h\x1b[1;80Hab".as_bytes(),
            ),
            (
                "wide-repeat",
                WideEdit,
                "\x1b[78G\u{4e2d}\x1b[1G[\x1b[4h\x1b[2b".as_bytes(),
            ),
            (
                "wide-repeat-below",
                WideEdit,
                "\x1b[3;71H\u{4e2d}\x1b[H[\x1b[4h\x1b[169b".as_bytes(),
            ),
            (
                "wrap-over-wide",
                WrapHead,
                "\x1b[2;1H\u{4e2d}\x1b[1;80H\u{6587}".as_bytes(),
            ),
            (
                "wrap-over-a-tail",
                WrapHead,
                "\x1b[2;2H\u{4e2d}\x1b[1;80H\u{6587}".as_bytes(),
            ),
            (
                "repeat-insert-wrap",
                InsertWrap,
                "\x1b[2;1Hxy\x1b[1;1H\u{4e2d}\x1b[1;80H\x1b[4h\x1b[2b".as_bytes(),
            ),
            (
                "repeat-over-wide",
                WrapHead,
                "\x1b[2;1H\u{4e2d}\x1b[1;78H\u{4e2d}\x1b[b".as_bytes(),
            ),
            (
                "insert-at-the-last-row",
                WideEdit,
                "\x1b[1;2r\x1b[24;79H\u{4e2d}\x1b[4h\u{e9}".as_bytes(),
            ),
            (
                "repeat-over-its-row",
                WideEdit,
                "\x1b[1;2r\x1b[24;2H\u{4e2d}\x1b[4h\x1b[40b".as_bytes(),
            ),
            (
                "repeat-under-a-head",
                WideEdit,
                "\x1b[1;2r\x1b[23;80H\u{4e2d}\x1b[4h\x1b[40b".as_bytes(),
            ),
            (
                "repeat-pushes-and-scrolls",
                WideEdit,
                "\x1b[24;79H\u{6587}\x1b[24;1Ha\x1b[4h\x1b[99b".as_bytes(),
            ),
            (
                "insert-under-a-head",
                WideEdit,
                "\x1b[1;80H\u{4e2d}\x1b[2;79H\u{6587}\x1b[2;1H\x1b[4ha".as_bytes(),
            ),
            (
                "repeat-no-wrap-wide",
                NoWrapWide,
                "\x1b[?7l\x1b[2G\u{4e2d}\x1b[40b".as_bytes(),
            ),
            (
                "repeat-no-wrap-wide-insert",
                NoWrapWide,
                "\x1b[H\u{4e2d}\x1b[1;78H\x1b[?7l\x1b[4h\x1b[2b".as_bytes(),
            ),
            (
                "rep-reset-at-the-bottom",
                RepReset,
                b"x\x1bc\x1b[24;1H\x1b[100b",
            ),
            (
                "rep-reset-in-a-region",
                RepReset,
                b"x\x1bc\x1b[2;5r\x1b[5;1H\x1b[100b",
            ),
            (
                "repeat-insert-wrap-later",
                InsertWrap,
                "\x1b[2;1Hxy\x1b[1;1H\u{4e2d}\x1b[1;76H\x1b[4h\x1b[4b".as_bytes(),
            ),
            (
                "insert-wrap-from-a-tail",
                InsertWrap,
                insert_wrap_from_a_tail.as_bytes(),
            ),
            (
                "repeat-from-a-tail",
                WideEdit,
                repeat_from_a_tail.as_bytes(),
            ),
            ("insert-wrap-back", InsertWrap, wrap_back.as_bytes()),
            (
                "repeat-a-graphic",
                WideEdit,
                "\x1b[1;78H\u{4e2d}\x1b[1;1H\x1b(0q\x1b[4h\x1b[2b".as_bytes(),
            ),
            ("highlight-twice", Highlight, b"x\x1b[1;2T"),
        ];
        cases
            .into_iter()
            .map(|(name, cause, input)| (name, cause, input.to_vec()))
            .collect()
    }

    /// Bugs that a VT core could have, as its screen at the byte where the
    /// screens part shows them, each named: its cursor a column off or a
    /// row off, its pending wrap the other way, a row far from the cursor
    /// written to, and the row after the cursor's, or the one before it at
    /// the last row.
    fn bugs(parted: &Parted) -> Vec<(&'static str, Screen)> {
        let (cols, rows) = (parted.before.cols, parted.before.rows);
        let screen = &parted.lokyr;
        let (x, y, pending) = screen.cursor;
        let mut bugs = Vec::new();
        let mut moved = |name, cursor| {
            bugs.push((
                name,
                Screen {
                    cursor,
                    ..screen.clone()
                },
            ));
        };
        moved(
            "a column off",
            (if x + 1 < cols { x + 1 } else { x - 1 }, y, pending),
        );
        moved(
            "a row off",
            (x, if y + 1 < rows { y + 1 } else { y - 1 }, pending),
        );
        moved("the wrap flipped", (x, y, !pending));
        let far = if parted.before.cursor.1 < rows / 2 {
            rows - 1
        } else {
            0
        };
        let written = |row: usize| {
            let mut written = screen.clone();
            match written.rows[row].first_mut() {
                Some(cell) => *cell = "#".to_owned(),
                None => written.rows[row].push("#".to_owned()),
            }
            written
        };
        bugs.push(("a far row written", written(far)));
        let near = if y + 1 < rows { y + 1 } else { y - 1 };
        bugs.push(("a near row written", written(near)));
        bugs
    }

    #[test]
    fn each_rule_explains_its_cause_and_none_of_the_bugs_that_look_like_it() {
        for (name, expected, input) in cases() {
            let parted = parted(80, 24, &input).expect("the screens part");
            assert_eq!(explained(&parted, &parted.lokyr), Some(expected), "{name}");
            for (bug, screen) in bugs(&parted) {
                if expected == Highlight && bug.ends_with("row written") {
                    // SD's rows, which `alacritty_terminal` scrolls, are any.
                    continue;
                }
                if name == "rep-reset-at-the-bottom" && bug == "a far row written" {
                    // The first row, which `alacritty_terminal`'s copies
                    // scroll out of its screen, leaves nothing to compare
                    // the VT core's with.
                    continue;
                }
                assert_eq!(explained(&parted, &screen), None, "{name}, {bug}");
            }
        }
    }

    #[test]
    fn rep_explains_nothing_on_the_rows_that_its_copies_do_not_reach() {
        // Each input, and the rows that a VT core's bug writes: a REP that
        // does not wrap, above a row that holds a wide character; one with
        // a wrap pending, after a wide character at the end of the row; one
        // at the last row below the region, whose wraps stay on that row;
        // and one that RIS made the VT core forget its character for, whose
        // copies scroll the region a row in `alacritty_terminal`: the rows
        // that come to its top and below.
        let inputs: [(&str, &[usize]); 4] = [
            ("\x1b[2;5H\u{4e2d}\x1b[Ha\x1b[4h\x1b[b", &[0, 1, 2]),
            ("\x1b[1;79H\u{4e2d}\x1b[4h\x1b[3b", &[0, 1, 2]),
            ("\x1b[1;2r\x1b[24;2H\u{4e2d}\x1b[4h\x1b[40b", &[21, 22]),
            ("x\x1bc\x1b[2;5r\x1b[5;1H\x1b[100b", &[2, 3]),
        ];
        for (input, rows) in inputs {
            let step = stepped(80, 24, input.as_bytes()).expect("the screens part no sooner");
            for &row in rows {
                let mut bug = step.lokyr.clone();
                match bug.rows[row].get_mut(5) {
                    Some(cell) => *cell = "#".to_owned(),
                    None => bug.rows[row].push("#".to_owned()),
                }
                assert_eq!(explained(&step, &bug), None, "{input:?}, row {row}");
            }
        }
    }

    /// The screen `screen` with the cell at column `x` of row `y` set to
    /// `cell`.
    fn with_cell(screen: &Screen, x: usize, y: usize, cell: &str) -> Screen {
        let mut changed = screen.clone();
        let cols = changed.rows[y].len().max(x + 1);
        changed.rows[y].resize(cols, " ".to_owned());
        changed.rows[y][x] = cell.to_owned();
        changed
    }

    #[test]
    fn rep_in_insert_mode_explains_its_shapes_alone() {
        // A REP that pushes the wide character on its row, with the screens
        // the same: a cell written, or the wide character emptied where it
        // was pushed to, is no part of WideEdit.
        let pushed = stepped(
            80,
            24,
            "\x1b[1;10H\u{4e2d}\x1b[1;1Ha\x1b[4h\x1b[b".as_bytes(),
        )
        .expect("no parting");
        assert_eq!(
            explained(&pushed, &with_cell(&pushed.lokyr, 5, 0, "#")),
            None
        );
        let emptied = with_cell(&with_cell(&pushed.lokyr, 10, 0, " "), 11, 0, " ");
        assert_eq!(explained(&pushed, &emptied), None);
        // A spacer head at the end of a row that the copies do not wrap
        // from: theirs, or one below whose start they do not write.
        for y in [0, 5] {
            assert_eq!(
                explained(&pushed, &with_cell(&pushed.lokyr, 79, y, HEAD)),
                None,
                "row {y}"
            );
        }
        // Copies that scroll a region below the screen's top past it: a
        // copy in a row above the region, which the rows that they wrote
        // first scroll out of the screen beyond, is no part of them.
        let past = stepped(
            20,
            6,
            "\x1b[3;6r\x1b[6;19H\u{6587}\x1b[6;1Ha\x1b[4h\x1b[99b".as_bytes(),
        )
        .expect("no parting");
        assert_eq!(explained(&past, &with_cell(&past.lokyr, 1, 1, "a")), None);
        // Copies that wrap in insert mode over a row with a wide character:
        // InsertWrap's, whose shape a cell off in the next row is no part
        // of.
        let wrap = parted(
            80,
            24,
            "\x1b[2;10H\u{4e2d}\x1b[1;1H\u{4e2d}\x1b[1;80H\x1b[4h\x1b[2b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&wrap, &wrap.lokyr), Some(InsertWrap));
        let mut shifted = wrap.lokyr.clone();
        shifted.rows[1].insert(0, " ".to_owned());
        assert_eq!(explained(&wrap, &shifted), None);
        // Copies that scroll the screen: a cell written in any row is no
        // part of them.
        let scrolled = stepped(
            20,
            6,
            "\x1b[6;1H\u{6587}\x1b[1;1H\x1b[4h\x1b[200b".as_bytes(),
        )
        .expect("no parting before");
        for y in 0..6 {
            assert_eq!(
                explained(&scrolled, &with_cell(&scrolled.lokyr, 0, y, "#")),
                None,
                "row {y}"
            );
        }
        // On a row of an odd number of cells, copies that wrap in insert
        // mode take two cells fewer of the next row's in `alacritty_terminal`,
        // which writes the first over them, so that a wide character there
        // splits at the row's end, and mangles a copy; with one more, they
        // wrap from that row's last column again.
        for count in [10, 11] {
            let odd = parted(
                21,
                24,
                format!(
                    "\x1b[2;1Hab\u{6587}cd\x1b[3;1Hab\u{6587}cd\x1b[1;1H\u{4e2d}\x1b[1;21H\x1b[4h\x1b[{count}b"
                )
                .as_bytes(),
            )
            .expect("the screens part");
            assert_eq!(explained(&odd, &odd.lokyr), Some(InsertWrap), "{count}");
            assert_eq!(
                explained(&odd, &with_cell(&odd.lokyr, 3, 1, "#")),
                None,
                "{count}"
            );
        }
        // On a row of an odd number of cells, below the cursor's, which
        // copies fill to its end, a copy lands at the last column alone and
        // wraps in insert mode: a cell written at that column of the row is
        // no part of InsertWrap; nor is a row written above the last, below
        // the region, which copies wrap back to the start of.
        for (input, x, y) in [
            (
                "\x1b[5;1Hxy\x1b[1;1H\u{4e2d}\x1b[3;16H\x1b[4h\x1b[12b",
                18,
                3,
            ),
            ("\x1b[1;2r\x1b[6;1H\u{4e2d}\x1b[6;16H\x1b[4h\x1b[39b", 0, 4),
        ] {
            let later = parted(19, 6, input.as_bytes()).expect("the screens part");
            assert_eq!(
                explained(&later, &later.lokyr),
                Some(InsertWrap),
                "{input:?}"
            );
            assert_eq!(
                explained(&later, &with_cell(&later.lokyr, x, y, "#")),
                None,
                "{input:?}"
            );
        }
        // The row that a later copy wraps from, whose spacer head
        // `alacritty_terminal` keeps or empties: a cell written there in its
        // screen is no part of InsertWrap.
        let mut later = parted(
            19,
            6,
            "\x1b[5;1Hxy\x1b[1;1H\u{4e2d}\x1b[3;16H\x1b[4h\x1b[12b".as_bytes(),
        )
        .expect("the screens part");
        later.alacritty = with_cell(&later.alacritty, 18, 3, "#");
        assert_eq!(explained(&later, &later.lokyr), None);
        // Fewer copies, which reach no row's last column alone: a spacer
        // head at the end of their row is no part of InsertWrap.
        let short =
            stepped(19, 6, "\x1b[3;14H\x1b[4h\u{4e2d}\x1b[b".as_bytes()).expect("no parting");
        assert_eq!(
            explained(&short, &with_cell(&short.lokyr, 18, 2, HEAD)),
            None
        );
        // A wrap in insert mode from the bottom margin, which scrolls, and
        // whose copy goes into a new row in both: a row written is no part
        // of InsertWrap, for a printed character and for REP.
        for input in [
            "\x1b[5;1Hk\x1b[4h\x1b[24;80H\u{4e2d}",
            "\x1b[5;1Hk\x1b[1;1H\u{4e2d}\x1b[24;80H\x1b[4h\x1b[2b",
        ] {
            let bottom = stepped(80, 24, input.as_bytes()).expect("no parting before");
            assert_eq!(
                explained(&bottom, &with_cell(&bottom.lokyr, 3, 0, "#")),
                None,
                "{input:?}"
            );
        }
    }

    #[test]
    fn wrap_heads_and_insert_wraps_explain_their_shapes_alone() {
        let head = |screen: &Screen, row: usize| {
            let mut bug = screen.clone();
            bug.rows[row].resize(80, " ".to_owned());
            bug.rows[row][79] = HEAD.to_owned();
            bug
        };
        // A copy that wraps over the wide character at the start of the
        // next row, whose head `alacritty_terminal` empties: heads on the
        // row that it goes to, over the `x` at the end of row 4, and at the
        // last row are no part of it.
        let wrap = parted(
            80,
            24,
            "\x1b[5;80Hx\x1b[2;1H\u{4e2d}\x1b[1;78H\u{4e2d}\x1b[b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&wrap, &wrap.lokyr), Some(WrapHead));
        for row in [1, 4, 23] {
            assert_eq!(explained(&wrap, &head(&wrap.lokyr, row)), None, "row {row}");
        }
        // Two copies, the second on the row that the first goes to, which
        // ends with an `x`, and rows that start with 中 after it and lower:
        // a head over the `x`, and one above a row that the copies did not
        // write, are no part of it either.
        let wrap = parted(
            80,
            24,
            "\x1b[2;80Hx\x1b[3;1H\u{4e2d}\x1b[11;1H\u{4e2d}\x1b[2;1H\u{4e2d}\x1b[1;78H\u{4e2d}\x1b[2b"
                .as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&wrap, &wrap.lokyr), Some(WrapHead));
        for row in [1, 9] {
            assert_eq!(explained(&wrap, &head(&wrap.lokyr, row)), None, "row {row}");
        }
        // Nor a head after a copy that does not wrap.
        let stays = stepped(80, 24, "\x1b[1;10H\u{4e2d}\x1b[b".as_bytes()).expect("no parting");
        assert_eq!(explained(&stays, &head(&stays.lokyr, 0)), None);
        // Copies that wrap in insert mode into an empty row, which both
        // cores show the same: a cell written in the row that they leave,
        // or the next row's cells a column off, are no part of InsertWrap.
        let insert = stepped(80, 24, "\x1b[H\u{4e2d}\x1b[1;78H\x1b[4h\x1b[2b".as_bytes())
            .expect("no parting");
        let mut written = insert.lokyr.clone();
        written.rows[0][5] = "#".to_owned();
        assert_eq!(explained(&insert, &written), None);
        let mut shifted = insert.lokyr.clone();
        shifted.rows[1].insert(0, " ".to_owned());
        assert_eq!(explained(&insert, &shifted), None);
    }

    #[test]
    fn no_wrap_wide_explains_the_row_s_end_alone() {
        // REP without autowrap with an even number of cells left, whose
        // copies fit: the screens do not part, and a cell changed in the
        // row's last three is no part of NoWrapWide.
        let even = stepped(80, 24, "\x1b[?7l\x1b[H\u{4e2d}\x1b[1;77H\x1b[3b".as_bytes())
            .expect("no parting");
        let mut changed = even.lokyr.clone();
        changed.rows[0][77] = "#".to_owned();
        assert_eq!(explained(&even, &changed), None);
        // A wide character at the last column without autowrap: a cell
        // changed at the row's start is no part of it.
        let printed =
            parted(80, 24, "\x1b[?7lab\x1b[80G\u{4e2d}".as_bytes()).expect("the screens part");
        assert_eq!(explained(&printed, &printed.lokyr), Some(NoWrapWide));
        let mut changed = printed.lokyr.clone();
        changed.rows[0][0] = "#".to_owned();
        assert_eq!(explained(&printed, &changed), None);
    }

    #[test]
    fn the_edges_of_the_shapes_take_their_cells_alone() {
        // The row that a wide character in insert mode wraps from: a cell
        // other than the head that the VT core leaves, printed and
        // repeated.
        for input in [
            "\x1b[2;1Hxy\x1b[4h\x1b[1;80H\u{4e2d}",
            "\x1b[2;1Hxy\x1b[1;1H\u{4e2d}\x1b[1;78H\x1b[4h\x1b[2b",
        ] {
            let wrap = parted(80, 24, input.as_bytes()).expect("the screens part");
            assert_eq!(explained(&wrap, &wrap.lokyr), Some(InsertWrap), "{input:?}");
            assert_eq!(
                explained(&wrap, &with_cell(&wrap.lokyr, 79, 0, "#")),
                None,
                "{input:?}"
            );
        }
        // A wide character at the last column without autowrap: a cell
        // other than its halves where it goes, or than the empty one left
        // of them.
        let back =
            parted(80, 24, "\x1b[?7lab\x1b[80G\u{4e2d}".as_bytes()).expect("the screens part");
        for x in 77..80 {
            assert_eq!(
                explained(&back, &with_cell(&back.lokyr, x, 0, "#")),
                None,
                "column {x}"
            );
        }
        // A head that the VT core keeps above a row that copies insert at
        // the start of, over a cell that `alacritty_terminal` keeps.
        let kept = stepped(
            80,
            24,
            "\x1b[1;80Hx\x1b[2;3H\u{4e2d}\x1b[2;1Ha\x1b[4h\x1b[b".as_bytes(),
        )
        .expect("no parting");
        assert_eq!(explained(&kept, &with_cell(&kept.lokyr, 79, 0, HEAD)), None);
        // A head left on a row that the copies wrote and did not wrap from.
        let wrap = parted(
            80,
            24,
            "\x1b[3;1H\u{4e2d}\x1b[2;1H\u{4e2d}\x1b[1;78H\u{4e2d}\x1b[2b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&wrap, &wrap.lokyr), Some(WrapHead));
        assert_eq!(explained(&wrap, &with_cell(&wrap.lokyr, 79, 1, HEAD)), None);
        // The cell left of where copies that wrap in insert mode start: a
        // narrow one emptied, and the first half of a wide character that
        // they went over, which `alacritty_terminal` did not keep as it was.
        let narrow = format!(
            "\x1b[2;1Hz{}\u{4e2d}\x1b[2;2H\x1b[4h\x1b[40b",
            "a".repeat(78)
        );
        let narrow = parted(80, 24, narrow.as_bytes()).expect("the screens part");
        assert_eq!(explained(&narrow, &narrow.lokyr), Some(InsertWrap));
        assert_eq!(
            explained(&narrow, &with_cell(&narrow.lokyr, 0, 1, " ")),
            None
        );
        let from_a_tail = format!(
            "\x1b[2;2H{}\u{4e2d}\x1b[H{}\u{4e2d}\x1b[4h\x08\x1b[40b",
            "a".repeat(78),
            "b".repeat(79)
        );
        let mut split = parted(80, 24, from_a_tail.as_bytes()).expect("the screens part");
        assert_eq!(explained(&split, &split.lokyr), Some(InsertWrap));
        assert_eq!(explained(&split, &with_cell(&split.lokyr, 0, 1, "#")), None);
        split.alacritty = with_cell(&split.alacritty, 0, 1, "\u{6587}");
        assert_eq!(explained(&split, &split.lokyr), None);
        // A wide character that wraps in insert mode back to the start of
        // the last row, below the region: a head left at the end of a row
        // other than the one above it.
        let wrap_back = format!(
            "\x1b[1;2r\x1b[23;1H{}\u{4e2d}\x1b[24;80H\x1b[4h\u{4e2d}",
            "a".repeat(79)
        );
        let back = parted(80, 24, wrap_back.as_bytes()).expect("the screens part");
        assert_eq!(explained(&back, &back.lokyr), Some(InsertWrap));
        assert_eq!(
            explained(&back, &with_cell(&back.lokyr, 79, 21, HEAD)),
            None
        );
        // A cell written in the row that it wraps back to, past the copy.
        assert_eq!(explained(&back, &with_cell(&back.lokyr, 40, 23, "#")), None);
    }

    #[test]
    fn halves_heads_counts_and_ends_that_only_one_check_rejects() {
        // A first half kept alone, which `whole` alone rejects: at the end of
        // copies that go back a column without autowrap, and where the copy
        // split the wide character at the cursor.
        let back = stepped(19, 6, "\u{4e2d}\x1b[?7l\x1b[1;1H\x1b[4h\x1b[11b".as_bytes())
            .expect("no parting before");
        assert_eq!(
            explained(&back, &with_cell(&back.lokyr, 16, 0, "\u{4e2d}")),
            None
        );
        let split =
            parted(20, 6, "\u{4e2d}\x1b[1;2H\x1b[4h\x1b[b".as_bytes()).expect("the screens part");
        assert_eq!(
            explained(&split, &with_cell(&split.lokyr, 0, 0, "\u{4e2d}")),
            None
        );
        // A head at the end of a row of the region that the copies' scrolls
        // moved, above none that they started.
        let scrolled = stepped(
            19,
            6,
            format!(
                "\x1b[2;1H{}\u{4e2d}\x1b[2;5r\x1b[2;1H\x1b[4h\x1b[41b",
                "q".repeat(18)
            )
            .as_bytes(),
        )
        .expect("no parting before");
        assert_eq!(
            explained(&scrolled, &with_cell(&scrolled.lokyr, 18, 0, HEAD)),
            None
        );
        // A copy that fits, without autowrap, which no copy goes back over.
        let fits = stepped(20, 6, "\u{4e2d}\x1b[?7l\x1b[1;2H\x1b[b".as_bytes())
            .expect("no parting before");
        let extra = with_cell(&with_cell(&fits.lokyr, 18, 0, "\u{4e2d}"), 19, 0, TAIL);
        assert_eq!(explained(&fits, &extra), None);
        // A wide character that a row below starts with, emptied where the
        // insert does not push it to the row's end.
        let rows: String = (1..=6)
            .map(|row| format!("\x1b[{row};1H\u{4e2d}"))
            .collect();
        let below = parted(20, 6, format!("{rows}\x1b[1;2H\x1b[4h\x1b[10b").as_bytes())
            .expect("the screens part");
        assert_eq!(explained(&below, &with_cell(&below.lokyr, 2, 1, " ")), None);
    }

    #[test]
    fn a_pass_takes_the_wrap_that_took_it_back() {
        // On the screen's last row, below the region: copies that fill the
        // row and come back after a wrap pending at its end, or after one
        // that leaves a spacer head at its last column.
        for (column, over) in [(3, false), (2, true)] {
            let input = format!("\x1b[1;3r\x1b[1;1H\u{4e2d}\x1b[6;{column}H\x1b[4h\x1b[10b");
            let step = stepped(20, 6, input.as_bytes()).expect("no parting before");
            let passes = copies(&[10], &step.before, None).stretches;
            let last = passes.last().expect("a pass");
            assert_eq!((last.start, last.over), (Start::Back, over), "{input:?}");
        }
        // After a wrap pending, both cores insert the copy at the row's
        // start: past it, `ab`, `cd` and copies are pushed two cells right in
        // both, but for a copy that `alacritty_terminal` emptied. After a
        // wrap that left a head, it would have written the copy over the
        // first two cells, which these rows are not.
        let before = stepped(20, 6, b"x").expect("no parting").before;
        let copy = "\u{4e2d}";
        let mut ours = vec![copy, TAIL, "a", "b", "c", "d"];
        for _ in 0..7 {
            ours.extend([copy, TAIL]);
        }
        let mut theirs = ours.clone();
        theirs[8] = " ";
        let cursor = Stretch {
            row: 5,
            from: 4,
            cells: 16,
            start: Start::Cursor,
            over: false,
        };
        for over in [false, true] {
            let back = Stretch {
                row: 5,
                from: 0,
                cells: 2,
                start: Start::Back,
                over,
            };
            assert_eq!(
                came_back(&before, &[&cursor, &back], &ours, &theirs, copy, true),
                !over
            );
        }
    }

    #[test]
    fn shapes_without_autowrap_left_cells_and_scrolled_heads_take_no_bugs() {
        let written = |screen: &Screen, x: usize, y: usize| with_cell(screen, x, y, "#");
        // Without autowrap, the second half of a wide character pushed
        // right after the copies, which the VT core empties: repeated, and
        // printed.
        for input in [
            "\u{4e2d}\x1b[?7l\x1b[1;2H\x1b[4h\x1b[b",
            "a\u{4e2d}\x1b[?7l\x1b[1;3H\x1b[4h\u{e9}",
        ] {
            let pushed = parted(20, 6, input.as_bytes()).expect("the screens part");
            assert_eq!(
                explained(&pushed, &pushed.lokyr),
                Some(WideEdit),
                "{input:?}"
            );
            assert_eq!(explained(&pushed, &written(&pushed.lokyr, 3, 0)), None);
        }
        // Without autowrap, a last copy that goes one column back in the VT
        // core, after copies that split a wide character at the cursor: the
        // row's last cells, and the one before them.
        let back = parted(
            20,
            6,
            "\x1b[1;1H\u{4e2d}\x1b[?7l\x1b[1;2H\x1b[4h\x1b[10b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&back, &back.lokyr), Some(WideEdit));
        for x in [17, 18, 19] {
            assert_eq!(explained(&back, &written(&back.lokyr, x, 0)), None);
        }
        let other = with_cell(&back.lokyr, 18, 0, "\u{6587}");
        assert_eq!(explained(&back, &other), None);
        // Copies that fill the row's end exactly, which none goes back
        // from; and narrow ones, which never do.
        let exact = stepped(
            20,
            6,
            "ab\u{6587}cd\x1b[2;1H\u{4e2d}\x1b[?7l\x1b[1;3H\x1b[4h\x1b[9b".as_bytes(),
        )
        .expect("no parting before");
        assert_eq!(
            explained(&exact, &with_cell(&exact.lokyr, 17, 0, " ")),
            None
        );
        let narrow = parted(
            20,
            6,
            "\x1b[1;1H\u{4e2d}\x1b[2;1Hh\x1b[?7l\x1b[1;2H\x1b[4h\x1b[19b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&narrow, &narrow.lokyr), Some(WideEdit));
        // A single copy that goes back over a wide character at the row's
        // end, past which the VT core keeps the cell before.
        let over = parted(
            20,
            6,
            format!(
                "{}\u{6587}\x1b[1;1H\u{4e2d}\x1b[?7l\x1b[1;20H\x1b[4h\x1b[b",
                "q".repeat(18)
            )
            .as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&over, &over.lokyr), Some(WideEdit));
        assert_eq!(explained(&over, &with_cell(&over.lokyr, 17, 0, " ")), None);
        // InsertWrap's first row, whose cell left of the copies
        // `alacritty_terminal` empties as its insert turns the second half
        // of the wide character at the row's end round to the cursor.
        let left = parted(
            20,
            6,
            format!(
                "\x1b[2;1H{}\u{4e2d}\x1b[3;1H\u{4e2d}\x1b[2;18H\x1b[4h\x1b[2b",
                "q".repeat(18)
            )
            .as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&left, &left.lokyr), Some(InsertWrap));
        assert_eq!(explained(&left, &written(&left.lokyr, 16, 1)), None);
        // The head above the row whose start the copies write in, after
        // they scroll the screen a row: the row that it moved to, alone.
        let head = format!(
            "\x1b[5;1H{}\u{4e2d}{}\u{4e2d}\x1b[6;3Hh\x1b[6;1H\x1b[4h",
            "h".repeat(19),
            "h".repeat(16)
        );
        // The head stands at the end of row 4, or of row 3 once the 21st
        // copy scrolled the screen: one at the end of the row above it
        // stands for nothing.
        for (count, head_row) in [(20, 4), (21, 3)] {
            let input = format!("{head}\x1b[{count}b");
            let scrolled = parted(20, 6, input.as_bytes()).expect("the screens part");
            assert_eq!(
                explained(&scrolled, &scrolled.lokyr),
                Some(WideEdit),
                "{input:?}"
            );
            assert_eq!(
                explained(
                    &scrolled,
                    &with_cell(&scrolled.lokyr, 19, head_row - 1, HEAD)
                ),
                None
            );
        }
    }

    #[test]
    fn the_rows_that_copies_come_back_to_take_no_bugs() {
        let written = |screen: &Screen, x: usize, y: usize| with_cell(screen, x, y, "#");
        let rows = |content: &str| {
            (1..=6)
                .map(|row| format!("\x1b[{row};1H{content}"))
                .collect::<String>()
        };
        // A wide character that wraps back to the start of the last row,
        // below the region: the two cells past it hold what the row
        // started with, which `alacritty_terminal` wrote over.
        let printed = parted(
            20,
            6,
            "\x1b[1;3r\x1b[6;1Hxy\x1b[1;1H\u{4e2d}\x1b[6;20H\x1b[4h\u{4e2d}".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&printed, &printed.lokyr), Some(InsertWrap));
        for x in [2, 3] {
            assert_eq!(explained(&printed, &written(&printed.lokyr, x, 5)), None);
        }
        // Another wide character in the VT core where the copy goes, and
        // a copy where the row held nothing.
        let other = with_cell(&printed.lokyr, 0, 5, "\u{65e5}");
        assert_eq!(explained(&printed, &other), None);
        let copied = with_cell(&with_cell(&printed.lokyr, 10, 5, "\u{4e2d}"), 11, 5, TAIL);
        assert_eq!(explained(&printed, &copied), None);
        let wrap_back = format!(
            "\x1b[1;2r\x1b[23;1H{}\u{4e2d}\x1b[24;80H\x1b[4h\u{4e2d}",
            "a".repeat(79)
        );
        let back = parted(80, 24, wrap_back.as_bytes()).expect("the screens part");
        assert_eq!(explained(&back, &written(&back.lokyr, 2, 23)), None);
        // Copies that come back after a pass in the cursor's row, which ended
        // with a spacer head: the insert pushed it off the row, and the
        // cell there is what came before it.
        let agreed = stepped(
            20,
            6,
            "\x1b[1;3r\x1b[1;1H\u{4e2d}\x1b[6;18H\x1b[4h\x1b[5b".as_bytes(),
        )
        .expect("no parting before");
        assert_eq!(
            explained(&agreed, &with_cell(&agreed.lokyr, 19, 5, HEAD)),
            None
        );
        let kept = parted(
            20,
            6,
            "\x1b[1;3r\x1b[1;1H\u{4e2d}\x1b[6;2H\x1b[4h\x1b[10b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&kept, &kept.lokyr), Some(InsertWrap));
        assert_eq!(explained(&kept, &written(&kept.lokyr, 19, 5)), None);
        // After more passes, a copy of the pass before whose first half
        // `alacritty_terminal` empties, and the VT core turns into a second
        // half alone.
        let passes = parted(
            20,
            6,
            "\x1b[1;3r\x1b[1;1H\u{4e2d}\x1b[6;2H\x1b[4h\x1b[30b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&passes, &passes.lokyr), Some(WideEdit));
        assert_eq!(
            explained(&passes, &with_cell(&passes.lokyr, 2, 5, TAIL)),
            None
        );
        // The first half of a wide character at the row's start, which the
        // first pass split and the VT core emptied, pushed past the copy
        // that comes back.
        let first = parted(
            20,
            6,
            "\x1b[1;3r\x1b[6;1H\u{4e2d}\x1b[6;2H\x1b[4h\x1b[10b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&first, &first.lokyr), Some(InsertWrap));
        // On 19 columns, from a row below the cursor's: copies that come
        // back after a spacer head, past which the VT core empties the
        // first half that the push split at the row's end; and, after more
        // passes, copies of their own that `alacritty_terminal` empties.
        for count in [30, 45] {
            let input = format!("\x1b[1;3r\x1b[1;1H\u{4e2d}\x1b[4;1H\x1b[4h\x1b[{count}b");
            let more = parted(19, 6, input.as_bytes()).expect("the screens part");
            assert_eq!(explained(&more, &more.lokyr), Some(WideEdit), "{input:?}");
        }
        // Copies that come back with no pass before them, from a wrap
        // pending at the end of the row: what it held, pushed right.
        let pending = parted(
            20,
            6,
            format!(
                "\x1b[1;3r\x1b[6;1Hab{}\u{6587}\x1b[6;19H\u{4e2d}\x1b[4h\x1b[2b",
                " ".repeat(13)
            )
            .as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&pending, &pending.lokyr), Some(WideEdit));
        let copied = with_cell(&with_cell(&pending.lokyr, 8, 5, "\u{4e2d}"), 9, 5, TAIL);
        assert_eq!(explained(&pending, &copied), None);
        // Each pass keeps its own cells: copies after RIS at the last
        // column and at the start of the row; and the cursor's row of `q`,
        // which the copies that come back push right.
        let reset = parted(20, 6, b"x\x1bc\x1b[1;3r\x1b[6;20H\x1b[2b").expect("the screens part");
        assert_eq!(explained(&reset, &reset.lokyr), Some(RepReset));
        let pushed = parted(
            20,
            6,
            format!(
                "\x1b[1;3r\x1b[6;1H{}\u{4e2d}\x1b[6;18H\x1b[4h\x1b[2b",
                "q".repeat(18)
            )
            .as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&pushed, &pushed.lokyr), Some(InsertWrap));
        assert_eq!(explained(&pushed, &written(&pushed.lokyr, 10, 5)), None);
        // On 19 columns, the first half of the wide character that the
        // first pass split, which `alacritty_terminal` keeps, pushed right
        // by the copies that come back.
        let split = parted(
            19,
            6,
            format!(
                "\x1b[1;3r{}\x1b[1;1H\u{4e2d}\x1b[6;2H\x1b[4h\x1b[10b",
                rows("\u{65e5}x")
            )
            .as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&split, &split.lokyr), Some(WideEdit));
        assert_eq!(explained(&split, &written(&split.lokyr, 2, 5)), None);
        // Copies that come back after a spacer head, past which the VT core
        // keeps the cell left of the cursor, which `alacritty_terminal`
        // empties: emptied in the VT core too.
        let left = parted(
            20,
            6,
            format!(
                "\x1b[2;5r{}\x1b[1;1H\u{4e2d}\x1b[6;2H\x1b[4h\x1b[11b",
                rows("ab\u{6587}cd")
            )
            .as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&left, &left.lokyr), Some(WideEdit));
        assert_eq!(explained(&left, &with_cell(&left.lokyr, 4, 5, " ")), None);
    }

    #[test]
    fn halves_odd_widths_and_resets_take_no_bugs() {
        let emptied = |screen: &Screen, x: usize, y: usize| with_cell(screen, x, y, " ");
        // A VT core that empties half of a wide character and keeps the
        // other: the second half of the last copy, without autowrap, and
        // the first half of the character that the cursor stands right of.
        let tail = parted(
            20,
            6,
            "\x1b[2;1Ha\u{4e2d}\x1b[1;1H\u{4e2d}\x1b[?7l\x1b[2;1H\x1b[4h\x1b[10b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&tail, &emptied(&tail.lokyr, 19, 1)), None);
        let row = "\u{4e2d}".repeat(10);
        let head = stepped(
            20,
            6,
            format!("\x1b[1;1H{row}\x1b[?7l\x1b[1;20H\x1b[4h\u{4e2d}").as_bytes(),
        )
        .expect("no parting");
        assert_eq!(explained(&head, &emptied(&head.lokyr, 18, 0)), None);
        // On 19 columns, a cell that the insert pushed to the end of a row
        // that the copies wrap to, emptied in the VT core.
        let rows: String = (1..=6)
            .map(|row| format!("\x1b[{row};1Hpq\u{4e2d}rs\u{4e2d}tu"))
            .collect();
        let odd = stepped(
            19,
            6,
            format!("{rows}\x1b[1;1H\u{4e2d}\x1b[1;16H\x1b[4h\x1b[20b").as_bytes(),
        )
        .expect("no parting before");
        assert_eq!(explained(&odd, &emptied(&odd.lokyr, 18, 2)), None);
        // A wide character that the insert pushed to the last column of a
        // row that the copies wrap to, which it splits there: the VT core
        // empties its first half, for both rules.
        let split = parted(
            19,
            6,
            "\x1b[2;1H\u{4e2d}\x1b[1;1H\u{4e2d}\x1b[1;18H\x1b[4h\x1b[10b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&split, &split.lokyr), Some(WideEdit));
        let written_over = parted(
            19,
            6,
            "\x1b[2;1H\u{65e5}x\x1b[1;1H\u{4e2d}\x1b[1;17H\x1b[4h\x1b[10b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(
            explained(&written_over, &written_over.lokyr),
            Some(InsertWrap)
        );
        // A row that the copies wrap to and from, whose last cell, the
        // spacer head, holds nothing that the insert pushed.
        let through = parted(
            19,
            6,
            "\x1b[2;1H\u{4e2d}\x1b[1;1H\u{4e2d}\x1b[1;16H\x1b[4h\x1b[20b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&through, &through.lokyr), Some(WideEdit));
        // Wide copies that come back to the start of the last row, below
        // the region, which held no wide character: their own meet them.
        let own = parted(
            20,
            6,
            "\x1b[1;2r\x1b[1;1H\u{4e2d}\x1b[6;2H\x1b[4h\x1b[11b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&own, &own.lokyr), Some(WideEdit));
        // REP after RIS, which leaves the VT core's screen empty: a cell
        // written where the copy goes in `alacritty_terminal`.
        let reset = parted(80, 24, b"x\x1bc\x1b[b").expect("the screens part");
        assert_eq!(explained(&reset, &reset.lokyr), Some(RepReset));
        assert_eq!(explained(&reset, &with_cell(&reset.lokyr, 0, 0, "#")), None);
    }

    #[test]
    fn the_edges_of_wide_edits_restores_and_resets_take_no_bugs() {
        let emptied = |screen: &Screen, y: usize, cells: &[usize]| {
            cells
                .iter()
                .fold(screen.clone(), |screen, &x| with_cell(&screen, x, y, " "))
        };
        // A wide character that the copies do not push off the row: gone,
        // or its second half emptied, in the VT core, repeated and printed.
        for input in [
            "\x1b[1;10H\u{4e2d}\x1b[1;9Ha\x1b[4h\x1b[69b",
            "\x1b[1;78H\u{4e2d}\x1b[1;78H\x1b[4ha",
        ] {
            let step = stepped(80, 24, input.as_bytes()).expect("no parting");
            for cells in [&[78, 79][..], &[79]] {
                assert_eq!(
                    explained(&step, &emptied(&step.lokyr, 0, cells)),
                    None,
                    "{input:?} {cells:?}"
                );
            }
        }
        // A wide character that the copies push short of the row's end: its
        // first half emptied in the VT core, where they end.
        let step = stepped(
            80,
            24,
            "\x1b[1;10H\u{4e2d}\x1b[1;9Ha\x1b[4h\x1b[9b".as_bytes(),
        )
        .expect("no parting");
        assert_eq!(explained(&step, &emptied(&step.lokyr, 0, &[18])), None);
        // The second half of a wide character left of where the copies
        // start, which they do not split: emptied in the VT core.
        for input in [
            "\x1b[1;20H\u{4e2d}\x1b[1;1H\u{6587}\x1b[4ha\x1b[1;3H\x1b[b",
            "\x1b[1;79H\u{4e2d}\x1b[1;1H\u{6587}\x1b[4hb",
        ] {
            let step = stepped(80, 24, input.as_bytes()).expect("no parting");
            assert_eq!(
                explained(&step, &emptied(&step.lokyr, 0, &[1])),
                None,
                "{input:?}"
            );
        }
        // A spacer head that the VT core adds above a row whose start the
        // copies write, over a cell empty in both.
        let step = stepped(
            80,
            24,
            "\x1b[2;3H\u{4e2d}\x1b[2;1Ha\x1b[4h\x1b[b".as_bytes(),
        )
        .expect("no parting");
        assert_eq!(explained(&step, &with_cell(&step.lokyr, 79, 0, HEAD)), None);
        // A wide character at the last column without autowrap, which the
        // VT core sets one column back: the `x` before it, which it does not
        // go over, erased.
        let back = parted(80, 24, "\x1b[?7lab\x1b[78Gx\x1b[80G\u{4e2d}".as_bytes())
            .expect("the screens part");
        assert_eq!(explained(&back, &back.lokyr), Some(NoWrapWide));
        assert_eq!(explained(&back, &emptied(&back.lokyr, 0, &[77])), None);
        // DECOM's reset before 1049's on the alternate screen, which both
        // cores leave where 1049 saved the cursor: a VT core that stays home.
        let step = stepped(80, 24, b"\x1b[5;10H\x1b[?1049h\x1b[?6;1049l").expect("no parting");
        let home = Screen {
            cursor: (0, 0, false),
            ..step.lokyr.clone()
        };
        assert_eq!(explained(&step, &home), None);
        // REP after RIS, which the VT core does nothing for: a cell written
        // in its row past where the copy goes.
        let reset = parted(80, 24, b" \x1bc\x1b[b").expect("the screens part");
        assert_eq!(explained(&reset, &reset.lokyr), Some(RepReset));
        assert_eq!(
            explained(&reset, &with_cell(&reset.lokyr, 50, 0, "#")),
            None
        );
        // A wide copy after RIS that wraps from the last column, where
        // `alacritty_terminal` leaves its spacer head.
        let wrap =
            parted(80, 24, "\u{4e2d}\x1bc\x1b[1;80H\x1b[b".as_bytes()).expect("the screens part");
        assert_eq!(explained(&wrap, &wrap.lokyr), Some(RepReset));
        // An insert in the last row below the region, which wraps back to
        // its start, under a row that ends with no head: a head that the
        // VT core adds there.
        let back = parted(
            80,
            24,
            "\x1b[1;2r\x1b[24;1H\u{4e2d}\x1b[24;80H\x1b[4h\u{4e2d}".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&back, &back.lokyr), Some(InsertWrap));
        assert_eq!(
            explained(&back, &with_cell(&back.lokyr, 79, 22, HEAD)),
            None
        );
        // A character inserted over the second half of a wide character,
        // without autowrap, which `alacritty_terminal` pushes to the row's
        // end without its first half, and the VT core empties.
        let orphan = parted(
            20,
            6,
            "\x1b[?7l\x1b[1;18H\u{4e2d}\u{e9}\x1b[4h\x1b[1;19H%".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&orphan, &orphan.lokyr), Some(WideEdit));
    }

    #[test]
    fn insert_wraps_compare_the_rows_that_they_scroll_and_push() {
        // Copies that wrap in insert mode and scroll the screen, which the
        // screens agree on: a VT core whose screen is empty is no part of
        // InsertWrap.
        let scrolled = stepped(
            20,
            6,
            "\x1b[3;1Hxyz\x1b[1;1H\u{6587}\x1b[1;4H\x1b[4h\x1b[200b".as_bytes(),
        )
        .expect("no parting");
        let empty = Screen {
            rows: vec![Vec::new(); 6],
            ..scrolled.lokyr.clone()
        };
        assert_eq!(explained(&scrolled, &empty), None);
        // Nor is a cell written past the copies in the new row that the
        // last scroll brought in, which held nothing.
        assert_eq!(
            explained(&scrolled, &with_cell(&scrolled.lokyr, 4, 5, "#")),
            None
        );
        // The cells that the copies push right in the row that they wrap
        // to, which `alacritty_terminal` writes over: written, or emptied,
        // in the VT core, printed and repeated.
        for (input, x) in [
            ("\x1b[2;1Hxy\x1b[4h\x1b[1;80H\u{4e2d}", 2),
            ("\x1b[2;1Hxy\x1b[1;1H\u{4e2d}\x1b[1;78H\x1b[4h\x1b[2b", 2),
        ] {
            let wrap = parted(80, 24, input.as_bytes()).expect("the screens part");
            assert_eq!(explained(&wrap, &wrap.lokyr), Some(InsertWrap), "{input:?}");
            for cell in ["#", " "] {
                for x in [x, x + 1] {
                    assert_eq!(
                        explained(&wrap, &with_cell(&wrap.lokyr, x, 1, cell)),
                        None,
                        "{input:?} {x} {cell:?}"
                    );
                }
            }
        }
        // The first row, whose last cell `alacritty_terminal` keeps when
        // the next row starts with a narrow character: a VT core that
        // drops its head there is no part of InsertWrap.
        let kept = parted(
            20,
            6,
            "\x1b[2;1Hxy\x1b[1;1H\u{4e2d}\x1b[1;2H\x1b[4h\x1b[12b".as_bytes(),
        )
        .expect("the screens part");
        assert_eq!(explained(&kept, &kept.lokyr), Some(InsertWrap));
        assert_eq!(explained(&kept, &with_cell(&kept.lokyr, 19, 0, " ")), None);
        // On rows of `a中` below the region, on 19 by 6: copies that come
        // back to the start of the last row push the ones before them right
        // with the rest of the row; and, on 21 by 6, copies in the cursor's
        // row of `中`, among which `alacritty_terminal` empties one, as in
        // the rows that they wrap to.
        let rows = |content: &str| {
            (1..=6)
                .map(|row| format!("\x1b[{row};1H{content}"))
                .collect::<String>()
        };
        for (cols, input) in [
            (
                19,
                format!(
                    "\x1b[1;3r{}\x1b[1;1H\u{4e2d}\x1b[6;15H\x1b[4h\x1b[3b",
                    rows("a\u{4e2d}")
                ),
            ),
            (
                21,
                format!(
                    "\x1b[1;3r{}\x1b[1;1H\u{4e2d}\x1b[5;1H\x1b[4h\x1b[21b",
                    rows("\u{4e2d}")
                ),
            ),
        ] {
            let wrap = parted(cols, 6, input.as_bytes()).expect("the screens part");
            assert_eq!(explained(&wrap, &wrap.lokyr), Some(InsertWrap), "{input:?}");
        }
    }

    #[test]
    fn restores_explain_the_cursors_that_their_modes_make() {
        let moved = |step: &Parted, cursor| Screen {
            cursor,
            ..step.lokyr.clone()
        };
        // A restore outside origin mode, which caps nothing in either core:
        // a VT core that caps it is no part of RestoredBelow.
        let restore = stepped(80, 24, b"\x1b[20;1H\x1b7\x1b[5;10r\x1b8").expect("no parting");
        assert_eq!(explained(&restore, &moved(&restore, (0, 9, false))), None);
        // DECOM's reset before 1049's, whose restore both cores make last:
        // a VT core that skips it, and stays home, is no part of Decom.
        let reset =
            stepped(80, 24, b"\x1b[5;10H\x1b[?1049h\x1b[2;3H\x1b[?6;1049l").expect("no parting");
        assert_eq!(explained(&reset, &moved(&reset, (0, 0, false))), None);
        // DECOM's reset after 1049's: an `alacritty_terminal` cursor neither
        // where it was nor where 1049 saved it is no part of Decom.
        let mut reset = parted(80, 24, b"\x1b[5;10H\x1b[?1049h\x1b[2;3H\x1b[?1049;6l")
            .expect("the screens part");
        assert_eq!(explained(&reset, &reset.lokyr), Some(Decom));
        reset.alacritty.cursor = (5, 5, false);
        assert_eq!(explained(&reset, &reset.lokyr), None);
        // The same on the primary screen, where 1049's reset brings back
        // nothing: an `alacritty_terminal` cursor where DECSC saved it is
        // no part of Decom.
        let mut reset =
            parted(80, 24, b"\x1b[5;10H\x1b7\x1b[2;3H\x1b[?1049;6l").expect("the screens part");
        assert_eq!(explained(&reset, &reset.lokyr), Some(Decom));
        reset.alacritty.cursor = (9, 4, false);
        assert_eq!(explained(&reset, &reset.lokyr), None);
    }

    #[test]
    fn stepped_sees_nothing_past_a_parting() {
        // DECOM's reset parts the cursors; a character after it is past
        // the step where the screens part.
        assert!(stepped(80, 24, b"\x1b[2;3H\x1b[?6l").is_some());
        assert!(stepped(80, 24, b"\x1b[2;3H\x1b[?6lx").is_none());
    }

    #[test]
    fn dch_explains_nothing_of_a_vt_core_that_keeps_the_cell_under_the_cursor() {
        // DCH past the row's end that keeps the cell under the cursor.
        let dch = parted(80, 24, b"ABCDEFGH\x1b[1;5H\x1b[77P").expect("the screens part");
        assert_eq!(explained(&dch, &dch.lokyr), Some(Dch));
        let mut kept = dch.lokyr.clone();
        kept.rows[0].push("E".to_owned());
        assert_eq!(explained(&dch, &kept), None);
    }

    #[test]
    fn rep_without_autowrap_explains_nothing_of_a_vt_core_that_changes_the_row_before_its_end() {
        let rep =
            parted(80, 24, "\x1b[?7l\x1b[2G\u{4e2d}\x1b[40b".as_bytes()).expect("the screens part");
        assert_eq!(explained(&rep, &rep.lokyr), Some(NoWrapWide));
        let mut changed = rep.lokyr.clone();
        changed.rows[0][0] = "#".to_owned();
        assert_eq!(explained(&rep, &changed), None);
    }

    #[test]
    fn a_region_past_the_screen_that_moves_both_cursors_does_not_end_the_comparison() {
        // The screens are the same, as both cursors went home: the step
        // shows the VT core's, not the one before it.
        let parted = parted(80, 24, b"y\x1b[24;30r").expect("the screens part");
        let mut after = parted.after.clone();
        after.cursor = (0, 0, false);
        let seen = Seen {
            step: &parted.step,
            byte: parted.byte,
            before: &parted.before,
            after: &after,
            screens: None,
            wide: false,
        };
        assert_eq!(ending(&seen), None);
    }
}
