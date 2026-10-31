//! The output's bench: each frame's time on the CPU, by its phases, as a
//! program's output draws on screen, which the app prints as JSON as the
//! program ends.

use std::fmt::Write as _;
use std::time::Duration;

/// A frame's time on the CPU, by its phases.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Phases {
    /// The copy of the screen, under the pane's lock.
    pub(crate) snapshot: Duration,
    /// The cells laid out, the glyphs shaped and rasterized that the atlas
    /// lacked.
    pub(crate) layout: Duration,
    /// The rest of the display list, the notice and the overlays.
    pub(crate) list: Duration,
    /// The atlas's uploads, the encoding of the GPU's commands, and their
    /// commit, but the wait for a drawable.
    pub(crate) encode: Duration,
    /// The whole frame, from its start to its commit, but the wait for a
    /// drawable.
    pub(crate) total: Duration,
}

/// What the bench keeps: the frames that it measured.
#[derive(Debug, Default)]
pub(crate) struct OutputBench {
    pub(crate) frames: Vec<Phases>,
}

/// The report of `frames`, of a window of `pixels`, as JSON: their count,
/// and of each phase and of the whole, in milliseconds, the median, the
/// 90th and 99th percentiles and the longest, as nearest ranks.
pub(crate) fn report(frames: &[Phases], pixels: (u32, u32)) -> String {
    let stats = |pick: fn(&Phases) -> Duration| {
        let mut values: Vec<f64> = frames
            .iter()
            .map(|frame| pick(frame).as_secs_f64() * 1000.0)
            .collect();
        values.sort_by(f64::total_cmp);
        let rank = |share: f64| -> f64 {
            if values.is_empty() {
                return 0.0;
            }
            let at = (share * values.len() as f64).ceil().max(1.0) as usize;
            values[at.min(values.len()) - 1]
        };
        format!(
            "{{\"p50\":{:.3},\"p90\":{:.3},\"p99\":{:.3},\"max\":{:.3}}}",
            rank(0.5),
            rank(0.9),
            rank(0.99),
            values.last().copied().unwrap_or(0.0)
        )
    };
    let mut text = String::new();
    let _ = write!(
        text,
        "{{\"pixels\":[{},{}],\"frames\":{},\"total_ms\":{},\"snapshot_ms\":{},\"layout_ms\":{},\"list_ms\":{},\"encode_ms\":{}}}",
        pixels.0,
        pixels.1,
        frames.len(),
        stats(|frame| frame.total),
        stats(|frame| frame.snapshot),
        stats(|frame| frame.layout),
        stats(|frame| frame.list),
        stats(|frame| frame.encode),
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_tells_each_phase_s_percentiles() {
        let frame = |ms: u64| Phases {
            snapshot: Duration::from_micros(ms * 100),
            layout: Duration::from_micros(ms * 500),
            list: Duration::from_micros(ms * 50),
            encode: Duration::from_micros(ms * 350),
            total: Duration::from_millis(ms),
        };
        let frames: Vec<Phases> = (1..=10).map(frame).collect();
        let report = report(&frames, (3840, 2160));
        assert!(
            report.starts_with("{\"pixels\":[3840,2160],\"frames\":10,"),
            "{report}"
        );
        assert!(
            report.contains(
                "\"total_ms\":{\"p50\":5.000,\"p90\":9.000,\"p99\":10.000,\"max\":10.000}"
            ),
            "{report}"
        );
        assert!(
            report.contains(
                "\"layout_ms\":{\"p50\":2.500,\"p90\":4.500,\"p99\":5.000,\"max\":5.000}"
            ),
            "{report}"
        );
        assert!(
            report.ends_with(
                "\"encode_ms\":{\"p50\":1.750,\"p90\":3.150,\"p99\":3.500,\"max\":3.500}}"
            ),
            "{report}"
        );
    }
}
