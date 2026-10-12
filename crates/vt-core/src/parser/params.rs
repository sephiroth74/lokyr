//! The numeric parameters of control sequences.

/// The most values that [`Params`] keeps, sub-parameters included. A
/// sequence with more keeps the first ones.
pub const MAX_PARAMS: usize = 32;

/// The parameters of a CSI or DCS sequence.
///
/// Parameters are separated by `;`. A parameter can have sub-parameters,
/// separated by `:`, as in `38:2::255:0:0`; a parameter and its
/// sub-parameters form a group. An empty value reads as 0.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Params {
    /// Every value, one group after the other.
    values: [u16; MAX_PARAMS],
    /// How many values each group has.
    group_lens: [u8; MAX_PARAMS],
    /// How many values are in use.
    value_count: usize,
    /// How many groups are in use.
    group_count: usize,
}

impl Params {
    /// Parameters made of `groups`, each a parameter followed by its
    /// sub-parameters. An empty group reads as a single 0. Values past
    /// [`MAX_PARAMS`] are dropped, as the parser drops them.
    pub fn from_groups(groups: &[&[u16]]) -> Self {
        let mut params = Self::default();
        for &group in groups {
            let group: &[u16] = if group.is_empty() { &[0] } else { group };
            let take = group.len().min(MAX_PARAMS - params.value_count);
            if take == 0 {
                break;
            }
            params.values[params.value_count..params.value_count + take]
                .copy_from_slice(&group[..take]);
            params.group_lens[params.group_count] =
                u8::try_from(take).expect("MAX_PARAMS fits in a u8");
            params.value_count += take;
            params.group_count += 1;
        }
        params
    }

    /// The number of parameters, each counted once with its sub-parameters.
    pub fn len(&self) -> usize {
        self.group_count
    }

    /// Whether the sequence has no parameters.
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
}
