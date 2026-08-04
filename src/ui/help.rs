//! The `?` overlay.

use ntui::style::Weight;
use ntui::{Component, Element, Hooks};

use crate::ui::overlay;
use crate::ui::palette::Palette;

/// Every binding, grouped the way someone looking for one would scan.
///
/// An empty key means a section heading.
pub const BINDINGS: &[(&str, &str)] = &[
    ("", "Navigate"),
    ("j / k, ↓ / ↑", "move the selection"),
    ("g / G, Home / End", "first / last row"),
    ("^d / ^u", "half page down / up"),
    ("PgDn / PgUp", "full page down / up"),
    ("", "View"),
    ("Tab / ⇧Tab, 1-4", "switch tab"),
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
    (":help", "this overlay"),
    (":q", "quit"),
    ("", "Leave"),
    ("q", "quit"),
    ("Esc", "close an overlay, else clear the filter, else quit"),
    ("", ""),
    ("? / any key", "close this help"),
];

/// Whether `key` — as spelled in [`BINDINGS`] — is something the keymap
/// actually answers to.
///
/// The help overlay is a claim about behavior, and nothing else keeps it
/// honest: a binding can be renamed or removed and the help will go on
/// advertising it. This is the list the test compares against, kept beside
/// the text it checks.
///
/// It catches one direction only — help advertising a key that is gone. A
/// key that is bound but undocumented passes, and so does an entry added
/// here without a matching binding, since this list is maintained by hand
/// rather than derived from `handle_key`.
pub fn is_documented_key(key: &str) -> bool {
    const BOUND: &[&str] = &[
        // movement
        "j", "k", "↓", "↑", "g", "G", "Home", "End", "^d", "^u", "PgDn", "PgUp", // view
        "Tab", "⇧Tab", "1-4", "<", ">", "I", "t", "H", "u", // act
        "Enter", "dd", "n", // find
        "any", "key", // leave
        "q", "Esc", "?",
    ];
    // Command spellings are checked by `SortKey::from_word`'s own tests and
    // by the command dispatcher; here only the key column matters.
    key.starts_with(':') || BOUND.contains(&key)
}

#[derive(Clone, PartialEq, Default)]
pub struct HelpProps {
    pub palette: Palette,
}

pub struct Help;

impl Component for Help {
    type Props = HelpProps;

    fn render(props: &HelpProps, _hooks: &mut Hooks) -> Element {
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

        overlay::panel("proctop — keys", palette, body)
    }
}
