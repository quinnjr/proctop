//! The Network tab: per-interface throughput, and what the machine is
//! listening on.
//!
//! Two questions, one subsystem. Splitting them across tabs would mean
//! pressing a key to correlate a busy interface with what is bound to it.

use ntui::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks};

use crate::model::{Exposure, ListeningSocket, Sample};
use crate::ui::io::{CHROME, active, section, split};
use crate::ui::palette::Palette;
use crate::ui::table::{cell, cell_flex, pad};
use crate::ui::{Selection, Shared};

/// Column widths for the listening table.
const PROTO: u16 = 5;
const ADDRESS: u16 = 26;
const PORT: u16 = 6;
const QUEUE: u16 = 3;
const USER: u16 = 9;

#[derive(Clone, PartialEq, Default)]
pub struct NetworkViewProps {
    pub sample: Shared<Sample>,
    pub palette: Palette,
    /// Rows available for the whole tab.
    pub height: u16,
    /// Cursor and scroll position within the listening list.
    pub selection: Selection,
}

pub struct NetworkView;

impl Component for NetworkView {
    type Props = NetworkViewProps;

    fn render(props: &NetworkViewProps, _hooks: &mut Hooks) -> Element {
        NetworkView::render_tree(props)
    }
}

impl NetworkView {
    fn render_tree(props: &NetworkViewProps) -> Element {
        let palette = &props.palette;
        let nets = props.sample.nets.as_deref().map(active);

        // Interfaces take what they need; the listeners get the rest, since
        // there are usually many more of them.
        let budget = props.height as usize;
        let interface_rows = nets.as_ref().map_or(1, |n| n.len().max(1)) + CHROME;
        let (interface_rows, listening_rows) = split(budget, interface_rows, budget);

        let mut children = section(
            "Interfaces",
            "RX",
            "TX",
            nets.as_deref(),
            interface_rows,
            palette,
        );
        children.extend(listening(props, listening_rows));

        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                ..Default::default()
            },
            children,
        )
    }
}

/// The listening-socket half, rendered into at most `available` rows.
fn listening(props: &NetworkViewProps, available: usize) -> Vec<Element> {
    let palette = &props.palette;
    if available < CHROME {
        return Vec::new();
    }

    let mut rows = vec![
        heading("Listening", palette),
        header(palette),
        // The heading and column header are paid for; what remains is the
        // list, less one row reserved for the attribution note when one is
        // needed.
    ];

    // `None` is "not read yet", not "could not read": sockets are only
    // read while this tab is showing, so the first frame after switching
    // to it has none. Saying "unavailable" here would be the same lie the
    // disk view used to tell.
    let Some(sockets) = props.sample.sockets.as_deref() else {
        rows.push(note("  reading sockets…", palette.muted, palette));
        return rows;
    };

    if sockets.is_empty() {
        rows.push(note("  nothing is listening", palette.muted, palette));
        return rows;
    }

    // Rows that could not be attributed get a one-line explanation rather
    // than a mysteriously empty column, and it costs a row.
    let unattributed = sockets.iter().filter(|s| s.process.is_none()).count();
    let footer = usize::from(unattributed > 0);
    let limit = available.saturating_sub(CHROME + footer);

    let window = props.selection.visible(sockets.len(), limit);
    for i in window.clone() {
        rows.push(socket_row(&sockets[i], i == props.selection.index, palette));
    }

    if unattributed > 0 {
        rows.push(note(
            &format!("  {unattributed} owned by other users — run as root to attribute them"),
            palette.muted,
            palette,
        ));
    }

    rows
}

fn heading(title: &str, palette: &Palette) -> Element {
    Element::view(
        ViewProps {
            height: Dimension::Cells(1),
            ..Default::default()
        },
        vec![Element::text(TextProps {
            content: title.to_string(),
            color: palette.label,
            weight: Weight::Bold,
            wrap: TextWrap::Truncate,
            ..Default::default()
        })],
    )
}

fn header(palette: &Palette) -> Element {
    let head = |text: &str, width: u16, numeric: bool| {
        cell(
            &pad(text, width, numeric),
            width,
            palette.header_fg,
            Weight::Normal,
            palette.header_bg,
        )
    };
    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 1,
            height: Dimension::Cells(1),
            background: palette.header_bg,
            ..Default::default()
        },
        vec![
            head("PROTO", PROTO, false),
            head("LOCAL ADDRESS", ADDRESS, false),
            head("PORT", PORT, true),
            head("Q", QUEUE, true),
            head("USER", USER, false),
            cell_flex(
                "PROCESS",
                palette.header_fg,
                Weight::Normal,
                palette.header_bg,
            ),
        ],
    )
}

fn socket_row(listening: &ListeningSocket, selected: bool, palette: &Palette) -> Element {
    let socket = &listening.socket;
    let bg = if selected {
        palette.selected_bg
    } else {
        Color::Reset
    };
    let fg = |c: Color| if selected { palette.selected_fg } else { c };

    // Exposure is the point of this view, so it colours the address: a
    // wildcard bind is reachable from anywhere, a loopback bind from
    // nowhere, and that difference is invisible in every other rtop view.
    let exposure = socket.exposure();
    let address_color = match exposure {
        Exposure::Exposed => palette.warn,
        Exposure::Loopback => palette.muted,
        Exposure::Interface => palette.text,
    };

    let queue = match socket.accept_queue {
        // UDP has no accept queue at all, which is not the same as an empty
        // one — an em dash rather than a zero.
        None => String::from("–"),
        Some(depth) => depth.to_string(),
    };
    let process = match &listening.process {
        Some((pid, name)) => format!("{name} ({pid})"),
        None => String::from("–"),
    };

    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 1,
            height: Dimension::Cells(1),
            background: bg,
            ..Default::default()
        },
        vec![
            cell(
                &pad(socket.protocol.as_str(), PROTO, false),
                PROTO,
                fg(palette.label),
                Weight::Normal,
                bg,
            ),
            cell(
                &pad(&socket.local.ip().to_string(), ADDRESS, false),
                ADDRESS,
                fg(address_color),
                if exposure == Exposure::Exposed {
                    Weight::Bold
                } else {
                    Weight::Normal
                },
                bg,
            ),
            cell(
                &pad(&socket.local.port().to_string(), PORT, true),
                PORT,
                fg(palette.text),
                Weight::Normal,
                bg,
            ),
            cell(
                &pad(&queue, QUEUE, true),
                QUEUE,
                fg(queue_color(socket.accept_queue, palette)),
                Weight::Normal,
                bg,
            ),
            cell(
                &pad(&listening.user, USER, false),
                USER,
                fg(palette.label),
                Weight::Normal,
                bg,
            ),
            cell_flex(
                &process,
                fg(if listening.process.is_some() {
                    palette.text
                } else {
                    palette.muted
                }),
                Weight::Normal,
                bg,
            ),
        ],
    )
}

/// A non-empty accept queue means connections are arriving faster than the
/// application is accepting them, which is worth seeing.
fn queue_color(depth: Option<u32>, palette: &Palette) -> Color {
    match depth {
        Some(0) | None => palette.muted,
        Some(_) => palette.warn,
    }
}

fn note(text: &str, color: Color, _palette: &Palette) -> Element {
    Element::view(
        ViewProps {
            height: Dimension::Cells(1),
            ..Default::default()
        },
        vec![Element::text(TextProps {
            content: text.to_string(),
            color,
            wrap: TextWrap::Truncate,
            ..Default::default()
        })],
    )
}
