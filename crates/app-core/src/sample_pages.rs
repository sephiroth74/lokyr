//! A plugin of two pages of the settings tab, for a build of tests, as
//! `--features sample-pages` builds it, and for the app's tests: General,
//! of a field, a toggle and a button that shows the other page, and
//! Terminal, of two fields; each tells how many times it was shown, which
//! it hears, and the command `sample-pages.show-terminal`, Show the
//! Terminal Page, shows the second, as a plugin's ask of its own page.

use std::collections::BTreeMap;

use lokyr_kernel::commands::{Args, Category, Command, Error};
use lokyr_kernel::plugins::ui::{UiEvent, ViewId};
use lokyr_kernel::plugins::view::{
    Direction, Edges, Layout, Length, Node, Style, TextSize, Token, UiEventKind,
};
use lokyr_kernel::plugins::{Catalogs, Cx, Data, Event, Plugin, built_in};

/// Its manifest, of its two pages.
pub(crate) const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/sample-pages\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\n",
    "pages = [{ id = \"general\", icon = \"settings\" }, { id = \"terminal\", icon = \"terminal\" }]\n",
);

/// Its catalogs, of its pages' titles and its command's.
static CATALOGS: [(&str, &str); 2] = [
    (
        "en",
        "page-general = General\npage-terminal = Terminal\ncommand-sample-pages-show-terminal = Show the Terminal Page\nshown = Shown { $times } times\n",
    ),
    (
        "it",
        "page-general = Generale\npage-terminal = Terminale\ncommand-sample-pages-show-terminal = Mostra la pagina Terminale\nshown = Mostrata { $times } volte\n",
    ),
];

/// Its command, which shows its second page.
const SHOW: &str = "sample-pages.show-terminal";

/// The plugin, and how many times each page was shown.
#[derive(Default)]
pub(crate) struct SamplePages {
    shown: BTreeMap<String, u64>,
}

impl Plugin for SamplePages {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: SHOW.to_owned(),
            title: "Show the Terminal Page".to_owned(),
            category: Category::App,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        cx.show_page("terminal")
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            if let Event::PageShown(page) = event {
                *self.shown.entry(page.clone()).or_default() += 1;
                cx.request_render(ViewId::Page(page.clone()))?;
            }
        }
        Ok(())
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        if event.key.0 == "show-terminal" && event.kind == UiEventKind::Click {
            cx.show_page("terminal")?;
        }
        Ok(())
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::Page(page) = view else {
            return None;
        };
        let title = cx.message(&format!("page-{page}"), &[]).ok()?;
        let times = self.shown.get(page).copied().unwrap_or(0);
        let shown = cx
            .message(
                "shown",
                &[(
                    "times",
                    Data::Integer(i64::try_from(times).unwrap_or(i64::MAX)),
                )],
            )
            .ok()?;
        let mut children = vec![
            Node::text(title).key("title").style(Style {
                text_size: Some(TextSize::Large),
                ..Style::default()
            }),
            Node::text(shown).key("shown").style(Style {
                color: Some(Token::from("text.secondary")),
                ..Style::default()
            }),
        ];
        let field = |key: &str, placeholder: &str| {
            Node::field(placeholder)
                .key(key)
                .label(placeholder)
                .layout(Layout {
                    width: Length::Points(260.0),
                    ..Layout::default()
                })
        };
        if page == "general" {
            children.push(field("name", "Name"));
            children.push(Node::toggle("lightbulb", true).key("tips").label("Tips"));
            children.push(Node::button("Show the Terminal Page").key("show-terminal"));
        } else {
            children.push(field("shell", "Shell"));
            children.push(field("term", "TERM"));
        }
        Some(Node::container(children).key(page.as_str()).layout(Layout {
            direction: Direction::Column,
            gap: 12.0,
            padding: Edges::all(20.0),
            width: Length::Fraction(1.0),
            ..Layout::default()
        }))
    }

    fn catalogs(&self) -> Catalogs {
        built_in(&CATALOGS)
    }
}
