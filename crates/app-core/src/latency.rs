//! The latency's bench: the time from each key press, as the system
//! stamped it, to the presentation of the frame that shows what it typed,
//! as a histogram, and its phases: the press to the app's handling of its
//! key, that to the frame that shows its echo, ready to present, and that
//! to its presentation; which the app prints as JSON as it quits.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lokyr_platform_core::{KeyCode, TimerId, WindowId};

/// What the bench keeps.
#[derive(Debug, Default)]
pub(crate) struct LatencyBench {
    /// The keys that the bench types itself; none for the user's.
    pub(crate) keys: usize,
    /// How many of them it typed.
    pub(crate) sent: usize,
    /// Whether another window floods its output meanwhile.
    pub(crate) flood: bool,
    /// The window whose keys it measures.
    pub(crate) window: Option<WindowId>,
    /// The press that waits for the frame that shows it, and when the
    /// bench typed it, which it gives up a second after.
    pub(crate) pending: Option<Instant>,
    pub(crate) typed_at: Option<Instant>,
    /// When the app handled the pending press's key.
    pub(crate) handled: Option<Instant>,
    /// The latencies, which the display's thread adds to as frames show.
    pub(crate) samples: Arc<Mutex<Vec<Duration>>>,
    /// Each latency's phases, as [`Phases`] tells them.
    pub(crate) phases: Arc<Mutex<Vec<Phases>>>,
    /// The timer of the bench's next key.
    pub(crate) timer: Option<TimerId>,
}

/// A latency's phases: the press to the app's handling of its key, that to
/// the frame that shows its echo, ready to present, and that to the frame's
/// presentation.
pub(crate) type Phases = [Duration; 3];

/// How long a key waits for its frame before the bench gives it up.
pub(crate) const GIVE_UP: Duration = Duration::from_secs(1);

/// The key that the bench types as its `n`th, and its letter: `a` to `z`,
/// over and over.
pub(crate) fn key_of(n: usize) -> (KeyCode, char) {
    const LETTERS: [KeyCode; 26] = [
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::KeyC,
        KeyCode::KeyD,
        KeyCode::KeyE,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyI,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyN,
        KeyCode::KeyO,
        KeyCode::KeyP,
        KeyCode::KeyQ,
        KeyCode::KeyR,
        KeyCode::KeyS,
        KeyCode::KeyT,
        KeyCode::KeyU,
        KeyCode::KeyV,
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyY,
        KeyCode::KeyZ,
    ];
    let at = n % LETTERS.len();
    let letter = char::from(b'a' + u8::try_from(at).unwrap_or(0));
    (LETTERS[at], letter)
}

/// The pause before the bench's key after its `n`th: 40 to 62 ms, so that
/// the keys fall on every phase of the display's refresh.
pub(crate) fn pause(n: usize) -> Duration {
    let n = u64::try_from(n).unwrap_or(0);
    Duration::from_millis(40 + (n * 7) % 23)
}

/// The report of `samples`, as JSON: the latencies in milliseconds, their
/// least, median, 90th and 99th percentiles and most, as nearest ranks; the
/// median and the 99th percentile of each of their `phases`, the handling,
/// the echo's frame and the presentation; the median and the 99th
/// percentile in frames of `refresh`; and how many samples took each number
/// of frames, rounded up.
pub(crate) fn report(
    samples: &[Duration],
    phases: &[Phases],
    refresh: Duration,
    flood: bool,
    keys: usize,
) -> String {
    let mut sorted: Vec<f64> = samples
        .iter()
        .map(|sample| sample.as_secs_f64() * 1000.0)
        .collect();
    sorted.sort_by(f64::total_cmp);
    let refresh_ms = refresh.as_secs_f64() * 1000.0;
    let rank = |share: f64| -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        // Nearest ranks, counted from 1: ceil(share × n).
        let at = (share * sorted.len() as f64).ceil().max(1.0) as usize;
        sorted[at.min(sorted.len()) - 1]
    };
    let mut histogram: Vec<(u64, usize)> = Vec::new();
    for &sample in &sorted {
        let frames = (sample / refresh_ms).ceil().max(1.0) as u64;
        match histogram.last_mut() {
            Some((last, count)) if *last == frames => *count += 1,
            _ => histogram.push((frames, 1)),
        }
    }
    let mut text = String::new();
    let _ = write!(
        text,
        "{{\"refresh_ms\":{refresh_ms:.2},\"flood\":{flood},\"keys\":{keys},\"samples\":{},",
        sorted.len()
    );
    let _ = write!(
        text,
        "\"latency_ms\":{{\"min\":{:.2},\"p50\":{:.2},\"p90\":{:.2},\"p99\":{:.2},\"max\":{:.2}}},",
        sorted.first().copied().unwrap_or(0.0),
        rank(0.5),
        rank(0.9),
        rank(0.99),
        sorted.last().copied().unwrap_or(0.0)
    );
    text.push_str("\"phases_ms\":{");
    for (at, name) in ["handled", "framed", "presented"].into_iter().enumerate() {
        let mut phase: Vec<f64> = phases
            .iter()
            .map(|phases| phases[at].as_secs_f64() * 1000.0)
            .collect();
        phase.sort_by(f64::total_cmp);
        let rank = |share: f64| -> f64 {
            if phase.is_empty() {
                return 0.0;
            }
            let at = (share * phase.len() as f64).ceil().max(1.0) as usize;
            phase[at.min(phase.len()) - 1]
        };
        let comma = if at == 0 { "" } else { "," };
        let _ = write!(
            text,
            "{comma}\"{name}\":{{\"p50\":{:.2},\"p99\":{:.2}}}",
            rank(0.5),
            rank(0.99)
        );
    }
    text.push_str("},");
    let _ = write!(
        text,
        "\"frames\":{{\"p50\":{:.2},\"p99\":{:.2}}},\"histogram\":{{",
        rank(0.5) / refresh_ms,
        rank(0.99) / refresh_ms
    );
    for (at, (frames, count)) in histogram.iter().enumerate() {
        let comma = if at == 0 { "" } else { "," };
        let _ = write!(text, "{comma}\"{frames}\":{count}");
    }
    text.push_str("}}");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_go_round_the_letters_on_every_phase() {
        assert_eq!(key_of(0), (KeyCode::KeyA, 'a'));
        assert_eq!(key_of(27), (KeyCode::KeyB, 'b'));
        let pauses: Vec<u64> = (0..23).map(|n| pause(n).as_millis() as u64).collect();
        assert!(pauses.iter().all(|&ms| (40..=62).contains(&ms)));
        // Every millisecond of the range, over 23 keys.
        let mut seen = pauses.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 23);
    }

    #[test]
    fn the_report_tells_the_percentiles_in_milliseconds_and_frames() {
        let samples: Vec<Duration> = [10, 20, 30, 12, 14, 16, 40, 18, 22, 24]
            .into_iter()
            .map(Duration::from_millis)
            .collect();
        let phases: Vec<Phases> = (1..=10)
            .map(|n| {
                [
                    Duration::from_millis(1),
                    Duration::from_millis(n),
                    Duration::from_millis(10 * n),
                ]
            })
            .collect();
        let report = report(&samples, &phases, Duration::from_micros(16_667), true, 10);
        assert!(
            report.starts_with("{\"refresh_ms\":16.67,\"flood\":true,\"keys\":10,\"samples\":10,"),
            "{report}"
        );
        assert!(report.contains("\"latency_ms\":{\"min\":10.00,\"p50\":18.00,\"p90\":30.00,\"p99\":40.00,\"max\":40.00}"), "{report}");
        assert!(
            report.contains("\"phases_ms\":{\"handled\":{\"p50\":1.00,\"p99\":1.00},\"framed\":{\"p50\":5.00,\"p99\":10.00},\"presented\":{\"p50\":50.00,\"p99\":100.00}},"),
            "{report}"
        );
        assert!(
            report.contains("\"frames\":{\"p50\":1.08,\"p99\":2.40}"),
            "{report}"
        );
        // 10, 12, 14 and 16 within a frame; 18 to 30 within two; 40 three.
        assert!(
            report.ends_with("\"histogram\":{\"1\":4,\"2\":5,\"3\":1}}"),
            "{report}"
        );
        assert_eq!(
            super::report(&[], &[], Duration::from_millis(16), false, 0),
            "{\"refresh_ms\":16.00,\"flood\":false,\"keys\":0,\"samples\":0,\"latency_ms\":{\"min\":0.00,\"p50\":0.00,\"p90\":0.00,\"p99\":0.00,\"max\":0.00},\"phases_ms\":{\"handled\":{\"p50\":0.00,\"p99\":0.00},\"framed\":{\"p50\":0.00,\"p99\":0.00},\"presented\":{\"p50\":0.00,\"p99\":0.00}},\"frames\":{\"p50\":0.00,\"p99\":0.00},\"histogram\":{}}"
        );
    }
}
