-- WezTerm under F2-134's conditions. Its defaults that the measures depend
-- on: front_end = 'WebGpu', max_fps = 60.
local wezterm = require 'wezterm'
return {
  font = wezterm.font('Menlo'),
  font_size = 13,
  harfbuzz_features = { 'calt=0', 'clig=0', 'liga=0' },
  initial_cols = 120,
  initial_rows = 40,
  hide_tab_bar_if_only_one_tab = true,
  default_cursor_style = 'SteadyBlock',
  scrollback_lines = 10000,
  window_background_opacity = 1.0,
  macos_window_background_blur = 0,
  window_close_confirmation = 'NeverPrompt',
  check_for_updates = false,
}
