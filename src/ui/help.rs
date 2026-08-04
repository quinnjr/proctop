//! The `?` overlay.

use ntui::style::Weight;
use ntui::{Component, Element, Hooks};

use crate::ui::overlay;
use crate::ui::palette::Palette;

/// Every binding, grouped the way someone looking for one would scan.
///
/// An empty key means a section heading.
const BINDINGS: &[(&str, &str)] = &[
    ("", "Navigate"),
    ("j / k, ↓ / ↑", "move the selection"),
    ("g / G", "first / last row"),
    ("^d / ^u", "half page down / up"),
    ("PgDn / PgUp", "full page down / up"),
    ("", "View"),
    ("Tab, 1-3", "switch tab"),
    ("< / >", "previous / next sort column"),
    ("I", "reverse the sort direction"),
    ("t", "tree view"),
    ("H", "hide kernel threads"),
    ("u", "filter to the selected process's user"),
    ("", "Act"),
    ("Enter", "process details"),
    ("dd", "kill (asks which signal)"),
    ("n", "renice"),
    ("", "Find"),
    ("/", "incremental search"),
    (":", "command line"),
    ("", "Commands"),
    (":sort <col>", "pid, name, cpu, mem, time"),
    (":filter <text>", "set the search filter"),
    (":user <name>", "filter by user"),
    (":tree", "toggle tree view"),
    (":q", "quit"),
    ("", ""),
    ("? / any key", "close this help"),
];

#[derive(Clone, PartialEq, Default)]
pub struct HelpProps {
    pub palette: Palette,
}

pub struct Help;

impl Component for Help {
    type Props = HelpProps;

    fn render(props: &HelpProps, _hooks: &mut Hooks) -> Element {
        Help::build(props)
    }
}

impl Help {
    pub fn build(props: &HelpProps) -> Element {
        let palette = &props.palette;
        let body = BINDINGS
            .iter()
            .map(|(key, description)| match *key {
                "" if description.is_empty() => {
                    overlay::row("", palette.text, Weight::Normal, palette)
                }
                // A section heading: no key, so the description carries it.
                "" => overlay::row(*description, palette.label, Weight::Bold, palette),
                _ => overlay::field(key, *description, palette),
            })
            .collect();

        overlay::panel("rtop — keys", palette, body)
    }
}
