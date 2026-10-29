//! The performance's panel: what the app measures of its windows' frames,
//! of the GPU, of the process and of its programs' output, a second at a
//! time, and the panel that shows it over a window, in a corner.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use lokyr_kernel::config::Corner;
use lokyr_kernel::l10n::{Arg, Texts};
use lokyr_platform_core::{TimerId, Usage, WindowId};
use lokyr_render_core::{Color, GpuTimes, Item, PlacedGlyph, Rect};
use lokyr_text_core::CellMetrics;
use lokyr_ui_core::pane::{AtlasGlyph, CellText};
use lokyr_vt_core::page::Style;

/// How often the panel measures again, and changes.
pub(crate) const INTERVAL: Duration = Duration::from_secs(1);

/// The panel's background, a dark gray that lets a little of the window
/// through, and its text.
const BACKGROUND: Color = Color {
    red: 0.08,
    green: 0.08,
    blue: 0.09,
    alpha: 0.82,
};
const TEXT: Color = Color {
    red: 0.92,
    green: 0.92,
    blue: 0.92,
    alpha: 1.0,
};

/// A megabyte, as the panel counts them.
const MEGABYTE: f64 = 1_000_000.0;

/// What the panel measures while it shows.
#[derive(Debug)]
pub(crate) struct Performance {
    /// The timer of the next measure.
    pub(crate) timer: Option<TimerId>,
    /// When the last measure was, and what the process had used then, and
    /// the bytes that its programs had written.
    since: Instant,
    usage: Option<Usage>,
    bytes: u64,
    /// The CPU's time of each frame that each window presented since then.
    frames: BTreeMap<WindowId, Vec<Duration>>,
    /// Each window's lines, as the last measure gave them.
    lines: BTreeMap<WindowId, Vec<String>>,
}

/// What a measure found of a window, over its interval.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Measure {
    /// The frames presented a second.
    pub(crate) rate: f64,
    /// The CPU's time of a frame, its median and its longest; none without
    /// a frame.
    pub(crate) cpu: Option<(Duration, Duration)>,
    /// The GPU's, its mean and its longest, of every window's frames.
    pub(crate) gpu: Option<(Duration, Duration)>,
    /// The wakeups a second, all of them and those out of idle, where the
    /// system tells them.
    pub(crate) wakeups: Option<(f64, f64)>,
    /// The memory, in MB, where the system tells it.
    pub(crate) memory: Option<f64>,
    /// The programs' output, in MB a second.
    pub(crate) output: f64,
}

impl Performance {
    /// Measures from `now`, of the process's `usage` and the programs'
    /// `bytes` then.
    pub(crate) fn new(now: Instant, usage: Option<Usage>, bytes: u64) -> Self {
        Self {
            timer: None,
            since: now,
            usage,
            bytes,
            frames: BTreeMap::new(),
            lines: BTreeMap::new(),
        }
    }

    /// Records a frame that `window` presented, which took the CPU `time`.
    pub(crate) fn frame(&mut self, window: WindowId, time: Duration) {
        self.frames.entry(window).or_default().push(time);
    }

    /// Measures at `now`, of the GPU's times since the last measure, and of
    /// the process's `usage` and the programs' `bytes` now: what each window
    /// of `windows` found, which starts the next interval.
    pub(crate) fn measure(
        &mut self,
        now: Instant,
        windows: &[WindowId],
        gpu: GpuTimes,
        usage: Option<Usage>,
        bytes: u64,
    ) -> Vec<(WindowId, Measure)> {
        let elapsed = now.saturating_duration_since(self.since).as_secs_f64();
        let per_second = |count: f64| if elapsed > 0.0 { count / elapsed } else { 0.0 };
        let gpu = (gpu.frames > 0).then(|| (gpu.total / gpu.frames, gpu.longest));
        let wakeups = self.usage.zip(usage).map(|(before, after)| {
            let idle = after.idle_wakeups.saturating_sub(before.idle_wakeups) as f64;
            let interrupts = after
                .interrupt_wakeups
                .saturating_sub(before.interrupt_wakeups) as f64;
            (per_second(idle + interrupts), per_second(idle))
        });
        let memory = usage.map(|usage| usage.footprint as f64 / MEGABYTE);
        let output = per_second(bytes.saturating_sub(self.bytes) as f64) / MEGABYTE;
        let measures = windows
            .iter()
            .map(|&window| {
                let mut times = self.frames.remove(&window).unwrap_or_default();
                times.sort_unstable();
                let cpu = times
                    .last()
                    .map(|&longest| (times[times.len() / 2], longest));
                let measure = Measure {
                    rate: per_second(times.len() as f64),
                    cpu,
                    gpu,
                    wakeups,
                    memory,
                    output,
                };
                (window, measure)
            })
            .collect();
        self.frames.clear();
        (self.since, self.usage, self.bytes) = (now, usage, bytes);
        measures
    }

    /// Gives `window`'s panel `lines`.
    pub(crate) fn set_lines(&mut self, window: WindowId, lines: Vec<String>) {
        self.lines.insert(window, lines);
    }

    /// The lines of `window`'s panel, once a measure gave them.
    pub(crate) fn lines(&self, window: WindowId) -> Option<&[String]> {
        self.lines.get(&window).map(Vec::as_slice)
    }

    /// Lets the windows go that are not in `windows`, which closed.
    pub(crate) fn keep(&mut self, windows: &[WindowId]) {
        self.lines.retain(|window, _| windows.contains(window));
        self.frames.retain(|window, _| windows.contains(window));
    }
}

/// The panel's lines of `measure`, in `texts`' language: the numbers as
/// text, which the catalogs place.
pub(crate) fn lines(measure: &Measure, texts: &Texts) -> Vec<String> {
    let ms = |time: Duration| format!("{:.2}", time.as_secs_f64() * 1000.0);
    let none = "–".to_owned();
    let pair = |times: Option<(Duration, Duration)>| {
        times.map_or((none.clone(), none.clone()), |(typical, longest)| {
            (ms(typical), ms(longest))
        })
    };
    let (cpu, cpu_longest) = pair(measure.cpu);
    let (gpu, gpu_longest) = pair(measure.gpu);
    let (wakeups, idle) = measure
        .wakeups
        .map_or((none.clone(), none.clone()), |(all, idle)| {
            (format!("{all:.1}"), format!("{idle:.1}"))
        });
    let memory = measure
        .memory
        .map_or(none.clone(), |memory| format!("{memory:.1}"));
    let rate = format!("{:.0}", measure.rate);
    let output = format!("{:.2}", measure.output);
    vec![
        texts.text_with("performance-frames", &[("rate", Arg::Text(&rate))]),
        texts.text_with(
            "performance-cpu",
            &[
                ("median", Arg::Text(&cpu)),
                ("longest", Arg::Text(&cpu_longest)),
            ],
        ),
        texts.text_with(
            "performance-gpu",
            &[
                ("mean", Arg::Text(&gpu)),
                ("longest", Arg::Text(&gpu_longest)),
            ],
        ),
        texts.text_with(
            "performance-wakeups",
            &[("wakeups", Arg::Text(&wakeups)), ("idle", Arg::Text(&idle))],
        ),
        texts.text_with("performance-memory", &[("memory", Arg::Text(&memory))]),
        texts.text_with("performance-output", &[("output", Arg::Text(&output))]),
    ]
}

/// The drawings of the panel of `lines` over a window of `width` by `height`
/// pixels, whose content starts `top` pixels down, under its titlebar, in
/// `corner`: a background, then the lines' glyphs, of `cell`'s size, which
/// `glyph` gives, a cell a character.
pub(crate) fn panel(
    lines: &[String],
    cell: &CellMetrics,
    (width, height, top): (u32, u32, u32),
    corner: Corner,
    mut glyph: impl FnMut(&CellText<'_>) -> Option<AtlasGlyph>,
) -> Vec<Item> {
    let columns = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let (columns, rows) = (
        u32::try_from(columns).unwrap_or(u32::MAX),
        u32::try_from(lines.len()).unwrap_or(u32::MAX),
    );
    // A margin from the window's edges, and the text's from the panel's,
    // half a cell.
    let (margin, padding) = (cell.width, cell.width / 2);
    let panel_width = columns.saturating_mul(cell.width) + 2 * padding;
    let panel_height = rows.saturating_mul(cell.height) + 2 * padding;
    let x = match corner {
        Corner::TopLeft | Corner::BottomLeft => margin,
        Corner::TopRight | Corner::BottomRight => {
            width.saturating_sub(panel_width.saturating_add(margin))
        }
    };
    let y = match corner {
        Corner::TopLeft | Corner::TopRight => top.saturating_add(margin),
        Corner::BottomLeft | Corner::BottomRight => {
            height.saturating_sub(panel_height.saturating_add(margin))
        }
    };
    let style = Style::default();
    let mut glyphs = Vec::new();
    let mut text = [0; 4];
    for (row, line) in (0..).zip(lines) {
        let baseline = y + padding + row * cell.height + cell.baseline;
        for (column, character) in (0..).zip(line.chars()) {
            if character == ' ' {
                continue;
            }
            let shown = CellText {
                text: character.encode_utf8(&mut text),
                style: &style,
                wide: false,
                dark: true,
            };
            let Some(found) = glyph(&shown) else {
                continue;
            };
            let pen = x + padding + column * cell.width;
            glyphs.push(PlacedGlyph {
                region: found.region,
                x: i32::try_from(pen).unwrap_or(i32::MAX) + found.left,
                y: i32::try_from(baseline).unwrap_or(i32::MAX) - found.top,
                color: TEXT,
            });
        }
    }
    vec![
        Item::Rect {
            rect: Rect {
                x: i32::try_from(x).unwrap_or(i32::MAX),
                y: i32::try_from(y).unwrap_or(i32::MAX),
                width: panel_width,
                height: panel_height,
            },
            color: BACKGROUND,
        },
        Item::Glyphs(glyphs),
    ]
}

#[cfg(test)]
mod tests {
    use lokyr_render_core::{AtlasRegion, Texture};

    use super::*;

    fn window(id: u64) -> WindowId {
        WindowId::new(id)
    }

    fn usage(idle: u64, interrupts: u64, footprint: u64) -> Usage {
        Usage {
            idle_wakeups: idle,
            interrupt_wakeups: interrupts,
            footprint,
        }
    }

    #[test]
    fn a_measure_takes_each_window_s_frames_and_the_process_s_rates() {
        let start = Instant::now();
        let mut performance = Performance::new(start, Some(usage(10, 100, 0)), 1_000);
        let millis = Duration::from_millis;
        for time in [3, 1, 2, 9] {
            performance.frame(window(1), millis(time));
        }
        performance.frame(window(2), millis(4));
        let mut gpu = GpuTimes::default();
        gpu.add(millis(1));
        gpu.add(millis(3));
        // Over two seconds.
        let measures = performance.measure(
            start + Duration::from_secs(2),
            &[window(1), window(2), window(3)],
            gpu,
            Some(usage(14, 102, 48_200_000)),
            5_001_000,
        );
        let [(_, first), (_, second), (_, third)] = measures[..] else {
            panic!("three windows: {measures:?}");
        };
        assert_eq!(first.rate, 2.0);
        assert_eq!(first.cpu, Some((millis(3), millis(9))));
        assert_eq!(first.gpu, Some((millis(2), millis(3))));
        assert_eq!(first.wakeups, Some((3.0, 2.0)));
        assert_eq!(first.memory, Some(48.2));
        assert_eq!(first.output, 2.5);
        assert_eq!(
            (second.rate, second.cpu),
            (0.5, Some((millis(4), millis(4))))
        );
        // A window that drew nothing, and the next interval, from nothing.
        assert_eq!((third.rate, third.cpu), (0.0, None));
        let next = performance.measure(
            start + Duration::from_secs(3),
            &[window(1)],
            GpuTimes::default(),
            None,
            5_001_000,
        );
        assert_eq!(next[0].1.cpu, None);
        assert_eq!(next[0].1.gpu, None);
        assert_eq!((next[0].1.wakeups, next[0].1.memory), (None, None));
        assert_eq!(next[0].1.output, 0.0);
    }

    #[test]
    fn the_lines_tell_the_numbers_in_the_language() {
        let measure = Measure {
            rate: 119.6,
            cpu: Some((Duration::from_micros(420), Duration::from_micros(1_100))),
            gpu: None,
            wakeups: Some((2.04, 0.0)),
            memory: Some(48.24),
            output: 0.0,
        };
        let texts = Texts::new("en", &crate::app::CATALOGS);
        assert_eq!(
            lines(&measure, &texts),
            [
                "120 FPS",
                "CPU 0.42 ms, at most 1.10",
                "GPU – ms, at most –",
                "2.0 wakeups/s, 0.0 from idle",
                "Memory 48.2 MB",
                "Output 0.00 MB/s"
            ]
        );
        let texts = Texts::new("it", &crate::app::CATALOGS);
        assert_eq!(
            lines(&measure, &texts)[3],
            "2.0 risvegli/s, 0.0 dall'inattività"
        );
    }

    #[test]
    fn the_panel_sits_in_its_corner_with_a_glyph_a_character() {
        let cell = CellMetrics {
            width: 10,
            height: 20,
            baseline: 16,
            underline_top: 18,
            underline_thickness: 1,
            strikeout_top: 10,
            strikeout_thickness: 1,
        };
        let lines = ["ab c".to_owned(), "d".to_owned()];
        let mut asked = Vec::new();
        let glyph = |shown: &CellText<'_>| {
            asked.push(shown.text.to_owned());
            Some(AtlasGlyph {
                region: AtlasRegion {
                    texture: Texture::Coverage,
                    x: 0,
                    y: 0,
                    width: 6,
                    height: 12,
                },
                left: 1,
                top: 11,
            })
        };
        let items = panel(&lines, &cell, (800, 600, 56), Corner::TopRight, glyph);
        // Four columns and two rows, and half a cell of padding round them.
        let [Item::Rect { rect, .. }, Item::Glyphs(glyphs)] = &items[..] else {
            panic!("a background and glyphs: {items:?}");
        };
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (740, 66, 50, 50));
        assert_eq!(asked, ["a", "b", "c", "d"]);
        let places: Vec<(i32, i32)> = glyphs.iter().map(|glyph| (glyph.x, glyph.y)).collect();
        assert_eq!(places, [(746, 76), (756, 76), (776, 76), (746, 96)]);
        // The other corners.
        for (corner, place) in [
            (Corner::TopLeft, (10, 66)),
            (Corner::BottomRight, (740, 540)),
            (Corner::BottomLeft, (10, 540)),
        ] {
            let items = panel(&lines, &cell, (800, 600, 56), corner, |_: &CellText<'_>| {
                None
            });
            let Item::Rect { rect, .. } = &items[0] else {
                panic!("a background");
            };
            assert_eq!((rect.x, rect.y), place, "{corner:?}");
        }
    }
}
