//! The numeric parameters of control sequences.

use std::fmt;

/// The most values that [`Params`] keeps, sub-parameters included. A
/// sequence with more keeps the first ones, and is still dispatched.
pub const MAX_PARAMS: usize = 32;

/// The parameters of a CSI or DCS sequence.
///
/// Parameters are separated by `;`. A parameter can have sub-parameters,
/// separated by `:`, as in `38:2::255:0:0`; a parameter and its
/// sub-parameters form a group. An empty value reads as 0, so a sequence
/// without parameters has one: `CSI m` reads as `CSI 0 m`.
///
/// A value too large for a `u16` reads as `u16::MAX`.
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
    /// Whether the value being parsed has digits.
    written: bool,
    /// The groups whose first value had digits, a bit each, as some
    /// sequences tell an empty parameter from 0.
    given: u32,
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
                    (params.current, params.written) = (value, true);
                    params.end_sub_param();
                }
                (params.current, params.written) = (last, true);
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

    /// The parameter at `index`, with its sub-parameters.
    pub fn get(&self, index: usize) -> Option<&[u16]> {
        self.iter().nth(index)
    }

    /// The value of the parameter at `index`, without its sub-parameters. A
    /// missing parameter reads as 0, like an empty one.
    pub fn value(&self, index: usize) -> u16 {
        self.get(index)
            .and_then(<[u16]>::first)
            .copied()
            .unwrap_or(0)
    }

    /// [`Params::value`], with 0 read as `default`. Most sequences give an
    /// empty or zero parameter their default meaning: `CSI A`, `CSI 0 A` and
    /// `CSI 1 A` all move the cursor up one line.
    pub fn value_or(&self, index: usize, default: u16) -> u16 {
        match self.value(index) {
            0 => default,
            value => value,
        }
    }

    /// Whether the parameter at `index` has digits, rather than being empty
    /// or missing, as XTWINOPS tells an omitted size from 0.
    pub fn has_value(&self, index: usize) -> bool {
        index < self.group_count && self.given & (1 << index) != 0
    }

    /// Forgets every parameter, before a new sequence.
    pub(crate) fn clear(&mut self) {
        self.value_count = 0;
        self.group_count = 0;
        self.group_lens[0] = 0;
        self.current = 0;
        self.written = false;
        self.given = 0;
    }

    /// Appends a decimal digit to the value being parsed. A value too large
    /// for a `u16` stops at `u16::MAX`.
    pub(crate) fn push_digit(&mut self, digit: u8) {
        self.current = self
            .current
            .saturating_mul(10)
            .saturating_add(u16::from(digit));
        self.written = true;
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
            if self.group_lens[self.group_count] == 0 && self.written {
                self.given |= 1 << self.group_count;
            }
            self.group_lens[self.group_count] += 1;
            self.values[self.value_count] = self.current;
            self.value_count += 1;
        }
        self.current = 0;
        self.written = false;
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
        assert!(!params.has_value(0) && params.has_value(1));
    }

    #[test]
    fn digits_tell_a_zero_from_an_empty_parameter() {
        // `8;;0;7:1`, the second parameter empty, the third 0.
        let mut params = Params::default();
        params.push_digit(8);
        params.end_param();
        params.end_param();
        params.push_digit(0);
        params.end_param();
        params.push_digit(7);
        params.end_sub_param();
        params.push_digit(1);
        params.end_param();
        let given: Vec<bool> = (0..5).map(|index| params.has_value(index)).collect();
        assert_eq!(given, [true, false, true, true, false]);
        params.clear();
        assert!(!params.has_value(0));
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
    fn a_parameter_can_be_read_by_index() {
        let params = Params::from_groups(&[&[4, 3], &[], &[7]]);
        assert_eq!(params.get(0), Some([4, 3].as_slice()));
        assert_eq!(params.get(3), None);
        assert_eq!([0, 1, 2, 3].map(|index| params.value(index)), [4, 0, 7, 0]);
    }

    #[test]
    fn zero_and_missing_values_take_the_default() {
        let params = Params::from_groups(&[&[0], &[5]]);
        assert_eq!([0, 1, 2].map(|index| params.value_or(index, 1)), [1, 5, 1]);
    }

    #[test]
    fn debug_shows_the_groups() {
        let params = Params::from_groups(&[&[4, 3], &[1]]);
        assert_eq!(format!("{params:?}"), "[[4, 3], [1]]");
    }
}
