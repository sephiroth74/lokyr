//! The known causes of the divergences between the two cores, and how a
//! step that the screens part at shows one.

use bolt_vt_core::page::Width;
use bolt_vt_core::parser::MAX_PARAMS;
use bolt_vt_core::terminal::{Charset, Shift};

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

    /// The VT core's cursor and `alacritty_terminal`'s, the VT core's after
    /// the step for both if the screens are the same.
    fn cursors(&self) -> (At, At) {
        self.screens.map_or(
            (self.after.cursor, self.after.cursor),
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
    if restores(step) && seen.same() {
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
        } if intermediates == b"#"
            && seen.cells_same()
            && bolt == (0, 0, false)
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
    let (bolt_screen, alacritty_screen) = seen.screens?;
    let before = seen.before;
    let (x, y, pending) = before.cursor;
    let (bolt, alacritty) = (bolt_screen.cursor, alacritty_screen.cursor);
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
            && bolt.0 == alacritty.0
            && bolt.2 == alacritty.2
            && bolt.1 == bottom
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
            // through: where it was, or where a reset of 1049 before it in
            // the sequence brought back the saved cursor, in both cores.
            (b"?", b'l')
                if params.contains(&6)
                    && seen.cells_same()
                    && bolt == (0, 0, false)
                    && (place(alacritty) == (x, y)
                        || params
                            .iter()
                            .position(|&mode| mode == 1049)
                            .is_some_and(|at| params[at..].contains(&6))) =>
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
                    && bolt.0 == alacritty.0
                    && !bolt.2
                    && !alacritty.2
                    && bolt.1 != alacritty.1
                    && (bolt.1, alacritty.1)
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
                    && bolt.0 == alacritty.0
                    && !bolt.2
                    && !alacritty.2
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
                    && !bolt.2
                    && !alacritty.2
                    && bolt.1 == bottom
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
                    && bolt_screen.rows[y].len() <= x
                    && alacritty_screen.rows[y]
                        == prefix(
                            &bolt_screen.rows[y],
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
                    && bolt.1 == alacritty.1
                    && !bolt.2
                    && alacritty.2 == pending =>
            {
                Some(Cause::CbtFirst)
            }
            // REP in insert mode, whose characters push a wide character past
            // the end of a row that they go in, on the rows that they change,
            // and in the spacer heads above the rows that they insert at the
            // start of.
            (b"", b'b')
                if before.insert
                    && pushes_a_wide_character(params, before)
                    && bolt == alacritty
                    && heads_kept_but(seen, &copies(params, before, None)) =>
            {
                Some(Cause::WideEdit)
            }
            // SD, which `alacritty_terminal` takes, and the cursor stays.
            (b"", b'T') if params.len() > 1 && bolt == alacritty => Some(Cause::Highlight),
            // REP in insert mode of a wide character, one of whose copies
            // lands at the last column, as the first does from it, or a later
            // one when the cells left from the cursor are odd: that copy wraps
            // as InsertWrap tells, inserted in the next row in the VT core and
            // written over its first cells in `alacritty_terminal`; the copies
            // after it insert in both.
            (b"", b'b')
                if before.insert
                    && before.autowrap
                    && before.last_printed.is_some_and(|(_, width)| width == 2)
                    && !pending
                    && (before.cols - x) % 2 == 1
                    && first(params) > (before.cols - 1 - x) / 2
                    && bolt == alacritty
                    && inserted_on_wrapping(seen, &copies(params, before, None)) =>
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
                    && bolt == alacritty
                    && before.last_printed.is_some_and(|(c, width)| {
                        width == 2 && heads_left(seen, &copies(params, before, None), c)
                    }) =>
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
                    && bolt == alacritty
                    && seen.rows_same_but(&[y])
                    && same_but_the_end(bolt_screen, alacritty_screen, y, before.cols) =>
            {
                Some(Cause::NoWrapWide)
            }
            // REP, which repeats a character in one core alone: the one that
            // does nothing leaves its cursor.
            (b"", b'b')
                if before.reset
                    && bolt == before.cursor
                    && printed_alone(
                        seen,
                        &copies(params, before, Some(2)),
                        reach(params, before),
                    ) =>
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
                && bolt == before.cursor
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
                && bolt == advanced(before, 1, 1)
                && alacritty == before.cursor
                && seen.rows_same_but(&[y, bolt.1]) =>
        {
            Some(Cause::C1)
        }
        // The VT core takes no cell, and `alacritty_terminal` one.
        Step::Print('\u{D7C7}'..='\u{D7CA}' | '\u{D7FC}'..='\u{D7FF}')
            if bolt == before.cursor
                && alacritty == advanced(before, 1, 1)
                && seen.rows_same_but(&[y, alacritty.1]) =>
        {
            Some(Cause::Jamo)
        }
        // The VT core widens the cluster by a cell, and `alacritty_terminal`
        // takes none.
        Step::Print('\u{FE0F}')
            if bolt == advanced(before, 1, 1)
                && alacritty == before.cursor
                && seen.rows_same_but(&[y, bolt.1]) =>
        {
            Some(Cause::Emoji)
        }
        // Text inserted at the second half of a wide character, or that
        // pushes one past the row's end, on the row that it goes in; or at
        // the start of a row, under the spacer head above it.
        Step::Print(_) if before.insert && bolt == alacritty && printed_in_insert_mode(seen) => {
            Some(Cause::WideEdit)
        }
        // A wide character that does not fit at the last column, in insert
        // mode, and without autowrap.
        Step::Print(_)
            if seen.wide
                && before.insert
                && before.autowrap
                && !pending
                && x + 1 == before.cols
                && bolt == alacritty
                && inserted_on_wrapping(seen, &copies(&[1], before, Some(2))) =>
        {
            Some(Cause::InsertWrap)
        }
        Step::Print(_)
            if seen.wide
                && !before.autowrap
                && x + 1 == before.cols
                && bolt == alacritty
                && seen.rows_same_but(&[y])
                && same_but_the_end(bolt_screen, alacritty_screen, y, before.cols) =>
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
    let bolt = target.clamp(top as isize, bottom as isize);
    let alacritty = (target + top as isize).clamp(0, bottom as isize);
    (bolt as usize, alacritty as usize)
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

/// Whether the screens are the same but for the rows that `copies` went
/// in, in `alacritty_terminal`'s screen, which printed them alone: a
/// scroll at the bottom margin moved the other rows of its region up, by
/// as many rows as the copies, `reach` rows at most, passed it.
fn printed_alone(seen: &Seen<'_>, copies: &Copies, reach: usize) -> bool {
    let Some((other, printed)) = seen.screens else {
        return true;
    };
    let rows = printed.rows.len();
    let (top, bottom) = seen.before.region;
    let y = seen.before.cursor.1;
    if !copies.scrolls {
        return (0..rows)
            .all(|row| copies.rows.contains(&row) || printed.rows[row] == other.rows[row]);
    }
    (0..=reach + 1).any(|scrolled| {
        (0..rows).all(|row| {
            if row < top || row > bottom {
                // Above the region, text from the cursor's row on goes into
                // the rows down to it, which do not scroll.
                (row >= y && row < top) || printed.rows[row] == other.rows[row]
            } else {
                row + scrolled >= y || printed.rows[row] == other.rows[row + scrolled]
            }
        })
    })
}

/// Whether `printed`, a row of the core that printed, is `other`, the same
/// row of the other core, but for the spacer head at its end, which the
/// printing emptied.
fn head_emptied(printed: &[String], other: &[String], cols: usize) -> bool {
    other.get(cols - 1).is_some_and(|cell| cell == HEAD)
        && printed.get(cols - 1).is_none_or(|cell| cell != HEAD)
        && but_last(printed, cols) == but_last(other, cols)
}

/// Whether the screens are the same but for the rows that `copies` change,
/// and for the last cell of the row above each that they insert at the
/// start of: a spacer head that the VT core keeps, as text inserted there
/// moves the wide character that it stands for rather than writing over it,
/// and that `alacritty_terminal` empties.
fn heads_kept_but(seen: &Seen<'_>, copies: &Copies) -> bool {
    let Some((bolt, alacritty)) = seen.screens else {
        return true;
    };
    let changed = copies.changed(seen.before.region);
    (0..bolt.rows.len()).all(|row| {
        changed.contains(&row)
            || bolt.rows[row] == alacritty.rows[row]
            || (copies.starts.contains(&(row + 1))
                && head_emptied(&alacritty.rows[row], &bolt.rows[row], seen.before.cols))
    })
}

/// Whether the screens' rows differ in their last cells alone, in rows
/// that `copies` of `c` wrote and wrapped from: the VT core's has a spacer
/// head there and `alacritty_terminal`'s an empty cell, and the next row
/// starts with a copy in both.
fn heads_left(seen: &Seen<'_>, copies: &Copies, c: char) -> bool {
    let Some((bolt, alacritty)) = seen.screens else {
        return true;
    };
    let (cols, (top, bottom)) = (seen.before.cols, seen.before.region);
    let copy = c.to_string();
    let wrote = |row: usize| {
        copies.rows.contains(&row) || (copies.scrolls && (top..=bottom).contains(&row))
    };
    (0..bolt.rows.len()).all(|row| {
        bolt.rows[row] == alacritty.rows[row]
            || (wrote(row)
                && bolt.rows[row]
                    .get(cols - 1)
                    .is_some_and(|cell| cell == HEAD)
                && alacritty.rows[row]
                    .get(cols - 1)
                    .is_none_or(|cell| cell == " ")
                && but_last(&bolt.rows[row], cols) == but_last(&alacritty.rows[row], cols)
                && [bolt, alacritty].iter().all(|screen| {
                    screen.rows.get(row + 1).and_then(|next| next.first()) == Some(&copy)
                }))
    })
}

/// Whether the screens differ as `copies` of a wide character make them
/// differ when one wraps from the last column in insert mode: the rows
/// that they wrote but the first differ in that the VT core's has the
/// copies inserted, and `alacritty_terminal`'s the first written over its
/// cells. Both start with the same copies, and past them the VT core's
/// cells are `alacritty_terminal`'s two cells right, but for a wide
/// character that the insert split at the row's end. The first row, which
/// they wrapped from, differs in its last cell alone. When the region
/// scrolls, its rows differ, and so does a row that they wrap back to the
/// start of, the screen's last below the region.
fn inserted_on_wrapping(seen: &Seen<'_>, copies: &Copies) -> bool {
    let Some((bolt, alacritty)) = seen.screens else {
        return true;
    };
    if copies.scrolls || copies.again {
        return seen.rows_same_but(&copies.changed(seen.before.region));
    }
    let cols = seen.before.cols;
    (0..bolt.rows.len()).all(|row| {
        if bolt.rows[row] == alacritty.rows[row] {
            return true;
        }
        match copies.rows.iter().position(|&written| written == row) {
            Some(0) => but_last(&bolt.rows[row], cols) == but_last(&alacritty.rows[row], cols),
            Some(_) => {
                let (ours, theirs) = (
                    padded(&bolt.rows[row], cols),
                    padded(&alacritty.rows[row], cols),
                );
                // The copies at the row's start, of one character and its
                // second half each, as long as some old cells may match too.
                let copied = ours
                    .iter()
                    .zip(&theirs)
                    .take_while(|(a, b)| a == b && (**a == ours[0] || **a == TAIL))
                    .count();
                (2..=copied)
                    .step_by(2)
                    .any(|copied| two_cells_right(&ours, &theirs, copied))
            }
            None => false,
        }
    })
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
/// three cells, which a wide character at the last column without autowrap
/// takes in the VT core.
fn same_but_the_end(bolt: &Screen, alacritty: &Screen, y: usize, cols: usize) -> bool {
    let start = cols.saturating_sub(3);
    prefix(&bolt.rows[y], start) == prefix(&alacritty.rows[y], start)
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
    /// Whether a wrap at the bottom margin scrolls the region, which
    /// brings in new rows for the copies after it.
    scrolls: bool,
    /// Whether they wrap back to the start of their last row, the screen's
    /// last below the region, where a wrap stays.
    again: bool,
    /// The rows whose first two columns they write in, as they were: the
    /// cursor's, from there, and those that a wrap takes them to the start
    /// of.
    starts: Vec<usize>,
}

impl Copies {
    /// The rows that the copies change: those that they write, and the
    /// region's, `region`, when they scroll it.
    fn changed(&self, (top, bottom): (usize, usize)) -> Vec<usize> {
        let mut rows = self.rows.clone();
        if self.scrolls {
            rows.extend(top..=bottom);
        }
        rows
    }
}

/// Follows REP with `params` as the cores print its copies, of `width`
/// cells each, or of the width of the character before it: a pending wrap,
/// or a wide character at the last column, wraps first, to the next row,
/// or scrolls the region at its bottom margin, or stays on the screen's
/// last row below the region, at its start. Without autowrap, or for a
/// character of no width, the copies stay on the cursor's row.
fn copies(params: &[u16], before: &Before, width: Option<usize>) -> Copies {
    let (mut x, mut y, mut pending) = before.cursor;
    let bottom = before.region.1;
    let width = width.unwrap_or_else(|| {
        before
            .last_printed
            .map_or(0, |(_, width)| usize::from(width))
    });
    let mut copies = Copies {
        rows: if pending { Vec::new() } else { vec![y] },
        scrolls: false,
        again: false,
        starts: if !pending && x <= 1 {
            vec![y]
        } else {
            Vec::new()
        },
    };
    if width == 0 || !before.autowrap {
        copies.rows = vec![y];
        return copies;
    }
    for _ in 0..first_count(params) {
        if pending || (width == 2 && x + 1 == before.cols) {
            if y == bottom {
                // The copies after it go into a new row.
                copies.scrolls = true;
            } else {
                if y + 1 == before.rows {
                    copies.again = true;
                } else {
                    y += 1;
                }
                if !copies.rows.contains(&y) {
                    copies.rows.push(y);
                }
                if !copies.starts.contains(&y) {
                    copies.starts.push(y);
                }
            }
            // From then on the copies change no other row.
            if copies.scrolls || copies.again {
                break;
            }
            (x, pending) = (0, false);
        }
        x += width;
        if x >= before.cols {
            (x, pending) = (before.cols - 1, true);
        }
    }
    copies
}

/// REP's count, 1 for none or 0.
fn first_count(params: &[u16]) -> usize {
    usize::from(params.first().copied().unwrap_or(0).max(1))
}

/// Whether REP with `params`, in insert mode, pushes a wide character past
/// the end of a row that its copies go in: from the cursor, on the
/// cursor's row when no wrap is pending; in a row below that they wrap to;
/// or in the screen's last row below the region, which they wrap back to
/// the start of, a wide character that was there or one of their own.
fn pushes_a_wide_character(params: &[u16], before: &Before) -> bool {
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
fn printed_in_insert_mode(seen: &Seen<'_>) -> bool {
    let before = seen.before;
    let (x, y, pending) = before.cursor;
    if pending && y == before.region.1 {
        return false;
    }
    let row = row_written(before);
    let tail = inserts_at_a_tail(before);
    let at_start = (pending || x <= 1) && row > 0;
    let copies = Copies {
        rows: vec![row],
        scrolls: false,
        again: false,
        starts: if at_start { vec![row] } else { Vec::new() },
    };
    tail && heads_kept_but(seen, &copies)
}

/// The row that a character printed in insert mode goes in: the cursor's,
/// or with a wrap pending the next, but the screen's last below the
/// region, where the wrap stays.
fn row_written(before: &Before) -> usize {
    let (_, y, pending) = before.cursor;
    if !pending || (y > before.region.1 && y + 1 == before.rows) {
        y
    } else {
        y + 1
    }
}

/// The sets that G0 to G3 hold.
pub(crate) fn sets(charsets: bolt_vt_core::terminal::Charsets) -> [Charset; 4] {
    [Shift::G0, Shift::G1, Shift::G2, Shift::G3].map(|slot| charsets.get(slot))
}

#[cfg(test)]
mod tests {
    use super::Cause::*;
    use super::*;
    use crate::explore::{Parted, parted, stepped};

    /// The cause that the rules give the screens that `parted` saw, with
    /// the VT core's as `bolt`.
    fn explained(parted: &Parted, bolt: &Screen) -> Option<Cause> {
        let seen = Seen {
            step: &parted.step,
            byte: parted.byte,
            before: &parted.before,
            after: &parted.after,
            screens: (*bolt != parted.alacritty).then_some((bolt, &parted.alacritty)),
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
        let cases: [(&str, Cause, &[u8]); 47] = [
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
        let screen = &parted.bolt;
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
            assert_eq!(explained(&parted, &parted.bolt), Some(expected), "{name}");
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
        // Each input, from the review, and the rows that a VT core's bug
        // writes: a REP that does not wrap, above a row that holds a wide
        // character; one with a wrap pending, after a wide character at the
        // end of the row; and one at the last row below the region, whose
        // wraps stay on that row.
        let inputs: [(&str, &[usize]); 3] = [
            ("\x1b[2;5H\u{4e2d}\x1b[Ha\x1b[4h\x1b[b", &[0, 1, 2]),
            ("\x1b[1;79H\u{4e2d}\x1b[4h\x1b[3b", &[0, 1, 2]),
            ("\x1b[1;2r\x1b[24;2H\u{4e2d}\x1b[4h\x1b[40b", &[21, 22]),
        ];
        for (input, rows) in inputs {
            let step = stepped(80, 24, input.as_bytes()).expect("the screens part no sooner");
            for &row in rows {
                let mut bug = step.bolt.clone();
                match bug.rows[row].get_mut(5) {
                    Some(cell) => *cell = "#".to_owned(),
                    None => bug.rows[row].push("#".to_owned()),
                }
                assert_eq!(explained(&step, &bug), None, "{input:?}, row {row}");
            }
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
        assert_eq!(explained(&wrap, &wrap.bolt), Some(WrapHead));
        for row in [1, 4, 23] {
            assert_eq!(explained(&wrap, &head(&wrap.bolt, row)), None, "row {row}");
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
        assert_eq!(explained(&wrap, &wrap.bolt), Some(WrapHead));
        for row in [1, 9] {
            assert_eq!(explained(&wrap, &head(&wrap.bolt, row)), None, "row {row}");
        }
        // Nor a head after a copy that does not wrap.
        let stays = stepped(80, 24, "\x1b[1;10H\u{4e2d}\x1b[b".as_bytes()).expect("no parting");
        assert_eq!(explained(&stays, &head(&stays.bolt, 0)), None);
        // Copies that wrap in insert mode into an empty row, which both
        // cores show the same: a cell written in the row that they leave,
        // or the next row's cells a column off, are no part of InsertWrap.
        let insert = stepped(80, 24, "\x1b[H\u{4e2d}\x1b[1;78H\x1b[4h\x1b[2b".as_bytes())
            .expect("no parting");
        let mut written = insert.bolt.clone();
        written.rows[0][5] = "#".to_owned();
        assert_eq!(explained(&insert, &written), None);
        let mut shifted = insert.bolt.clone();
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
        let mut changed = even.bolt.clone();
        changed.rows[0][77] = "#".to_owned();
        assert_eq!(explained(&even, &changed), None);
        // A wide character at the last column without autowrap: a cell
        // changed at the row's start is no part of it.
        let printed =
            parted(80, 24, "\x1b[?7lab\x1b[80G\u{4e2d}".as_bytes()).expect("the screens part");
        assert_eq!(explained(&printed, &printed.bolt), Some(NoWrapWide));
        let mut changed = printed.bolt.clone();
        changed.rows[0][0] = "#".to_owned();
        assert_eq!(explained(&printed, &changed), None);
    }

    #[test]
    fn dch_explains_nothing_of_a_vt_core_that_keeps_the_cell_under_the_cursor() {
        // DCH past the row's end that keeps the cell under the cursor.
        let dch = parted(80, 24, b"ABCDEFGH\x1b[1;5H\x1b[77P").expect("the screens part");
        assert_eq!(explained(&dch, &dch.bolt), Some(Dch));
        let mut kept = dch.bolt.clone();
        kept.rows[0].push("E".to_owned());
        assert_eq!(explained(&dch, &kept), None);
    }

    #[test]
    fn rep_without_autowrap_explains_nothing_of_a_vt_core_that_changes_the_row_before_its_end() {
        let rep =
            parted(80, 24, "\x1b[?7l\x1b[2G\u{4e2d}\x1b[40b".as_bytes()).expect("the screens part");
        assert_eq!(explained(&rep, &rep.bolt), Some(NoWrapWide));
        let mut changed = rep.bolt.clone();
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
