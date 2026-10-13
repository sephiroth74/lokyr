//! Unicode normalization (UAX #15, <https://www.unicode.org/reports/tr15/>)
//! of grapheme clusters, as they are printed.

use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick, is_nfd_quick};

/// The normal form that printed text takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Normalization {
    /// The text stays as programs write it.
    #[default]
    None,
    /// Canonical composition: `e` and U+0301 become `é`, and Hangul jamo
    /// become syllables.
    Nfc,
    /// Canonical decomposition, the reverse.
    Nfd,
    /// The variant of NFD that Apple's HFS+ uses for file names, which
    /// leaves U+2000–U+2FFF, U+F900–U+FAFF and U+2F800–U+2FAFF alone.
    HfsPlus,
}

impl Normalization {
    /// Writes `cluster` in this normal form into `out`, unless it is in it
    /// already; returns whether it wrote it.
    pub(crate) fn normalize(self, cluster: &[char], out: &mut Vec<char>) -> bool {
        let quick = match self {
            Self::None => return false,
            Self::Nfc => is_nfc_quick(cluster.iter().copied()),
            Self::Nfd => is_nfd_quick(cluster.iter().copied()),
            Self::HfsPlus => {
                if cluster.iter().all(|&c| kept_by_hfs_plus(c)) {
                    return false;
                }
                is_nfd_quick(cluster.iter().copied().filter(|&c| !kept_by_hfs_plus(c)))
            }
        };
        if quick == IsNormalized::Yes {
            return false;
        }
        out.clear();
        match self {
            Self::None => {}
            Self::Nfc => out.extend(cluster.iter().copied().nfc()),
            Self::Nfd => out.extend(cluster.iter().copied().nfd()),
            Self::HfsPlus => {
                // Decompose the runs between the characters that HFS+ keeps.
                for run in cluster.split_inclusive(|&c| kept_by_hfs_plus(c)) {
                    let (decomposed, kept) = match run.split_last() {
                        Some((&last, rest)) if kept_by_hfs_plus(last) => (rest, Some(last)),
                        _ => (run, None),
                    };
                    out.extend(decomposed.iter().copied().nfd());
                    out.extend(kept);
                }
            }
        }
        out.as_slice() != cluster
    }
}

/// Whether HFS+ leaves `c` as it is, rather than decomposing it.
fn kept_by_hfs_plus(c: char) -> bool {
    matches!(c, '\u{2000}'..='\u{2FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{2F800}'..='\u{2FAFF}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(form: Normalization, text: &str) -> String {
        let cluster: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        if form.normalize(&cluster, &mut out) {
            out.into_iter().collect()
        } else {
            text.to_owned()
        }
    }

    #[test]
    fn each_form_composes_or_decomposes() {
        assert_eq!(normalized(Normalization::Nfc, "e\u{301}"), "\u{E9}");
        assert_eq!(normalized(Normalization::Nfd, "\u{E9}"), "e\u{301}");
        assert_eq!(normalized(Normalization::HfsPlus, "\u{E9}"), "e\u{301}");
        for form in [
            Normalization::Nfc,
            Normalization::Nfd,
            Normalization::HfsPlus,
        ] {
            assert_eq!(normalized(form, "a"), "a");
        }
        assert_eq!(normalized(Normalization::None, "e\u{301}"), "e\u{301}");
        assert_eq!(normalized(Normalization::None, "\u{E9}"), "\u{E9}");
    }

    #[test]
    fn hangul_jamo_and_syllables() {
        assert_eq!(
            normalized(Normalization::Nfc, "\u{1100}\u{1161}\u{11A8}"),
            "\u{AC01}"
        );
        assert_eq!(
            normalized(Normalization::Nfd, "\u{AC01}"),
            "\u{1100}\u{1161}\u{11A8}"
        );
    }

    #[test]
    fn hfs_plus_leaves_its_ranges_alone() {
        // OHM SIGN, a CJK compatibility ideograph, and one from the
        // supplement: NFD decomposes them, HFS+ does not.
        for c in ['\u{2126}', '\u{F900}', '\u{2F800}'] {
            let text = c.to_string();
            assert_ne!(
                normalized(Normalization::Nfd, &text),
                text,
                "{:X}",
                u32::from(c)
            );
            assert_eq!(
                normalized(Normalization::HfsPlus, &text),
                text,
                "{:X}",
                u32::from(c)
            );
        }
        // Around them, it decomposes as NFD does.
        assert_eq!(
            normalized(Normalization::HfsPlus, "\u{E9}\u{2126}\u{E9}"),
            "e\u{301}\u{2126}e\u{301}"
        );
    }

    #[test]
    fn marks_in_any_order_come_out_in_canonical_order() {
        // DOT BELOW (class 220) goes before ACUTE (class 230).
        assert_eq!(
            normalized(Normalization::Nfd, "a\u{301}\u{323}"),
            "a\u{323}\u{301}"
        );
    }
}
