//! A plugin on a thread of its own, for a build of tests, as `--features
//! sample-worker` builds it: its status bar renders without pause, each
//! render reading the focused pane's first rows, one request a row, and
//! asking for the next render, so that its thread keeps the main thread
//! answering, as the latency's bench measures beside it. The bar tells the
//! renders of the last second, and the text of the first row.

use std::time::{Duration, Instant};

use lokyr_kernel::plugins::ui::ViewId;
use lokyr_kernel::plugins::view::{Align, Edges, Node, Style, Token};
use lokyr_kernel::plugins::{Cx, Plugin};

/// Its manifest, of the status bar, on a thread of its own.
pub(crate) const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/sample-worker\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "thread = \"worker\"\n[contributes]\nslots = [\"status-bar\"]\n",
);

/// The rows that each render reads, a request each.
const ROWS: i64 = 24;

/// The plugin: its renders, and those of the last whole second.
#[derive(Default)]
pub(crate) struct SampleWorker {
    renders: u64,
    second: Option<(Instant, u64)>,
    rate: u64,
}

impl Plugin for SampleWorker {
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::StatusBar(_) = view else {
            return None;
        };
        self.renders += 1;
        let now = Instant::now();
        match self.second {
            Some((since, at)) if now.duration_since(since) >= Duration::from_secs(1) => {
                self.rate = self.renders - at;
                self.second = Some((now, self.renders));
            }
            None => self.second = Some((now, self.renders)),
            Some(_) => {}
        }
        let mut first = String::new();
        if let Some(pane) = cx.focused_pane() {
            for row in 0..ROWS {
                if let Ok(Some(line)) = cx.line_text(pane, row, 0)
                    && row == 0
                {
                    first = line.text;
                }
            }
        }
        // The next render, as soon as this one returns.
        let _ = cx.request_render(view.clone());
        let mut text = Node::text(format!(
            "worker: {} renders a second, {ROWS} rows each · {}",
            self.rate,
            first.trim_end()
        ));
        text.style.color = Some(Token::from("text.secondary"));
        let mut bar = Node::container(vec![text]);
        bar.layout.grow = 1.0;
        bar.layout.align = Align::Center;
        bar.layout.padding = Edges::all(6.0);
        bar.style = Style {
            background: Some(Token::from("tabs.bar.bg")),
            ..Style::default()
        };
        Some(bar)
    }
}
