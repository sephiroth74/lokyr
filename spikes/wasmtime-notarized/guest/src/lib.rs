//! The WebAssembly component hosted by the spike. Its exports measure the
//! cost of calling into a plugin and the speed of plugin code.

wit_bindgen::generate!({
    path: "../wit",
    world: "plugin",
});

use exports::lokyr::spike::bench::{Guest, Node, Span};
use lokyr::spike::host_log;

struct Bench;

impl Guest for Bench {
    fn init() {
        // Replaces the default hook, which reads `RUST_BACKTRACE` from the
        // environment and prints to stderr; a plugin has neither. The host
        // gets the report instead, and the panic then aborts into a trap.
        std::panic::set_hook(Box::new(|info| host_log::log(&info.to_string())));
    }

    fn noop() {}

    fn render(n: u32) -> Vec<Node> {
        // A root whose children are all the other nodes, as in a list view.
        let children = (1..n).collect();
        let mut nodes = Vec::with_capacity(n as usize);
        nodes.push(Node {
            key: 0,
            kind: 0,
            label: String::from("root"),
            children,
        });
        nodes.extend((1..n).map(|key| Node {
            key,
            kind: 1,
            label: format!("item {key}"),
            children: Vec::new(),
        }));
        nodes
    }

    fn work(iterations: u32) -> u64 {
        // xorshift64: cheap, and its result depends on every iteration, so
        // the loop cannot be optimized away.
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut sum: u64 = 0;
        for _ in 0..iterations {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            sum = sum.wrapping_add(state);
        }
        sum
    }

    fn fail() {
        panic!("the spike guest failed on purpose");
    }

    fn line_count(lines: Vec<String>) -> u32 {
        lines.len() as u32
    }

    fn fuzzy(lines: Vec<String>, query: String) -> Vec<u32> {
        lines
            .iter()
            .enumerate()
            .filter(|(_, line)| spike_workload::fuzzy_score(line, &query).is_some())
            .map(|(index, _)| index as u32)
            .collect()
    }

    fn highlight(lines: Vec<String>) -> Vec<Vec<Span>> {
        lines
            .iter()
            .map(|line| {
                spike_workload::highlight(line)
                    .into_iter()
                    .map(|(start, len, kind)| Span { start, len, kind })
                    .collect()
            })
            .collect()
    }
}

export!(Bench);
