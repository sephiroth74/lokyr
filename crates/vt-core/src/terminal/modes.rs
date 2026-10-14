//! The modes that programs set and reset.

use super::Terminal;
use crate::parser::Params;

impl Terminal {
    /// DECSET and DECRST: sets, if `on`, or resets the DEC private modes in
    /// `params`. Those that the terminal does not know do nothing.
    pub(super) fn set_private_modes(&mut self, params: &Params, on: bool) {
        for mode in params.iter().map(|group| group[0]) {
            if mode == 69 {
                self.set_left_right_margin_mode(on);
            }
        }
    }
}
