//! The numeric parameters of control sequences.

use std::fmt;

/// The most values that [`Params`] keeps, sub-parameters included. A
/// sequence with more keeps the first ones.
pub const MAX_PARAMS: usize = 32;

/// The parameters of a CSI or DCS sequence.
///
/// Parameters are separated by `;`. A parameter can have sub-parameters,
/// separated by `:`, as in `38:2::255:0:0`; a parameter and its
/// sub-parameters form a group. An empty value reads as 0, so a sequence
/// without parameters has one: `CSI m` reads as `CSI 0 m`.
#[derive(Clone, Default)]
pub struct Params {
    /// Every value, one group after the other.
    values: [u16; MAX_PARAMS],
    /// How many values each group has, the one being parsed included.
    group_lens: [u8; MAX_PARAMS],
    /// How many values are in use.
    value_count: usize,
    /// How many groups are complete.
    group_count: usize,
    /// The value being parsed, which is not in `values` yet.
    current: u16,
}

impl Params {
    /// Parameters made of `groups`, each a parameter followed by its
    /// sub-parameters. An empty group reads as a single 0. Values past
    /// [`MAX_PARAMS`] are dropped, as the parser drops them.
    pub fn from_groups(groups: &[&[u16]]) -> Self {
        let mut params = Self::default();
        for &group in groups {
            if let Some((&last, rest)) = group.split_last() {
                for &value in rest {
                    params.current = value;
                    params.end_sub_param();
                }
                params.current = last;
            }
            params.end_param();
        }
        params
    }

    /// The number of parameters, each counted once with its sub-parameters.
    pub fn len(&self) -> usize {
        self.group_count
    }

    /// Whether there are no parameters.
    pub fn is_empty(&self) -> bool {
        self.group_count == 0
    }

    /// The parameters, each with its sub-parameters: `38:2::255:0:0` yields
    /// a single slice of six values.
    pub fn iter(&self) -> impl Iterator<Item = &[u16]> + '_ {
        let mut start = 0;
        self.group_lens[..self.group_count].iter().map(move |&len| {
            let end = start + usize::from(len);
            let group = &self.values[start..end];
            start = end;
            group
        })
    }

    /// Forgets every parameter, before a new sequence.
    pub(crate) fn clear(&mut self) {
        self.value_count = 0;
        self.group_count = 0;
        self.group_lens[0] = 0;
        self.current = 0;
    }

    /// Appends a decimal digit to the value being parsed. A value too large
    /// for a `u16` stops at `u16::MAX`.
    pub(crate) fn push_digit(&mut self, digit: u8) {
        self.current = self
            .current
            .saturating_mul(10)
            .saturating_add(u16::from(digit));
    }

    /// Ends the value being parsed, and the parameter with it: `;`, or the
    /// end of the sequence.
    pub(crate) fn end_param(&mut self) {
        self.end_sub_param();
        // A group whose values were all dropped does not count.
        if self.group_count < MAX_PARAMS && self.group_lens[self.group_count] > 0 {
            self.group_count += 1;
            if self.group_count < MAX_PARAMS {
                self.group_lens[self.group_count] = 0;
            }
        }
    }

    /// Ends the value being parsed, so that a sub-parameter of the same
    /// parameter follows: `:`.
    pub(crate) fn end_sub_param(&mut self) {
        if self.value_count < MAX_PARAMS {
            // Every complete group has a value, so `group_count` is at most
            // `value_count`, which is below `MAX_PARAMS`.
            self.group_lens[self.group_count] += 1;
            self.values[self.value_count] = self.current;
            self.value_count += 1;
        }
        self.current = 0;
    }
}

/// Parameters are equal when their groups are.
impl PartialEq for Params {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for Params {}

/// Shows the groups, as in `[[38, 2, 0, 255, 0, 0], [1]]`.
impl fmt::Debug for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups(params: &Params) -> Vec<Vec<u16>> {
        params.iter().map(<[u16]>::to_vec).collect()
    }

    #[test]
    fn groups_keep_their_sub_parameters() {
        let params = Params::from_groups(&[&[38, 2, 0, 255, 0, 0], &[1], &[4, 3]]);
        assert_eq!(params.len(), 3);
        assert_eq!(
            groups(&params),
            [vec![38, 2, 0, 255, 0, 0], vec![1], vec![4, 3]]
        );
    }

    #[test]
    fn an_empty_group_reads_as_zero() {
        let params = Params::from_groups(&[&[], &[5]]);
        assert_eq!(groups(&params), [vec![0], vec![5]]);
    }

    #[test]
    fn values_past_the_limit_are_dropped() {
        let many = [7; 40];
        let params = Params::from_groups(&[&[1], &many]);
        assert_eq!(params.len(), 2);
        let kept: Vec<Vec<u16>> = groups(&params);
        assert_eq!(kept[1].len(), MAX_PARAMS - 1);
        assert_eq!(Params::from_groups(&[&many, &[2]]).len(), 1);
    }

    #[test]
    fn no_parameters() {
        let params = Params::default();
        assert!(params.is_empty());
        assert_eq!(params.iter().count(), 0);
    }

    #[test]
    fn digits_and_separators_build_groups() {
        // `38:2::255:0:0;1`, then the same sequence again after a clear.
        let mut params = Params::default();
        for _ in 0..2 {
            params.clear();
            for &byte in b"38:2::255:0:0;1" {
                match byte {
                    b':' => params.end_sub_param(),
                    b';' => params.end_param(),
                    digit => params.push_digit(digit - b'0'),
                }
            }
            params.end_param();
            assert_eq!(params, Params::from_groups(&[&[38, 2, 0, 255, 0, 0], &[1]]));
        }
    }

    #[test]
    fn a_large_value_stops_at_the_maximum() {
        let mut params = Params::default();
        for digit in [9; 7] {
            params.push_digit(digit);
        }
        params.end_param();
        assert_eq!(groups(&params), [vec![u16::MAX]]);
    }

    #[test]
    fn debug_shows_the_groups() {
        let params = Params::from_groups(&[&[4, 3], &[1]]);
        assert_eq!(format!("{params:?}"), "[[4, 3], [1]]");
    }
}
