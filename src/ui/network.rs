//! The Network tab: per-interface throughput, and what the machine is
//! listening on.
//!
//! Two questions, one subsystem. Splitting them across tabs would mean
//! pressing a key to correlate a busy interface with what is bound to it.

use ntui::props::{Dimension, FlexDirection, ViewProps};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks};

use crate::model::{Exposure, ListeningSocket, Sample};
use crate::ui::palette::Palette;
use crate::ui::section::{CHROME, active, heading, is_busy, placeholder, section};
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
        let palette = &props.palette;
        let nets = props.sample.nets.as_deref().map(active);
        let (interface_rows, listening_rows) = layout(props.height as usize, &props.sample);

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

/// How the tab's rows divide between the two sections.
///
/// Interfaces take what they need, up to half the pane; the listeners get
/// everything else, since there are usually many more of them. A fixed half
/// each left the tab half blank while hiding listeners that had something to
/// show. `App` needs the same answer to clamp the socket cursor, so this
/// lives here rather than inside the render.
pub(crate) fn layout(height: usize, sample: &Sample) -> (usize, usize) {
    // Counted rather than collected: `active` allocates and sorts, and this
    // runs twice per frame on this tab plus once more for `App`'s clamp.
    let busy = sample
        .nets
        .as_deref()
        .map(|n| n.iter().filter(|r| is_busy(r)).count());
    let wanted = busy.map_or(1, |n| n.max(1)) + CHROME;
    // A share too small to hold a heading and one device is rendered as
    // nothing by `section`, so give it to the listeners rather than leaving
    // a blank row.
    let interfaces = wanted.min(height / 2);
    let interfaces = if interfaces < CHROME { 0 } else { interfaces };
    (interfaces, height - interfaces)
}

/// How many socket rows the listening section can show in `available` rows.
///
/// The footer costs a row when it appears, so the capacity depends on the
/// list itself — which is why `App` cannot approximate it.
fn capacity(available: usize, sockets: &[ListeningSocket]) -> usize {
    let footer = usize::from(sockets.iter().any(|s| s.process.is_none()));
    available.saturating_sub(CHROME + footer)
}

/// How many socket rows the listening section shows in a tab `height` rows
/// tall.
///
/// `App` clamps the socket cursor with this and `listening` windows with
/// it, from the same argument the `height` prop carries. One entry point so
/// the two cannot drift: a cursor clamped against a viewport the renderer
/// does not use is how rows scroll off with no key to bring them back.
pub(crate) fn socket_capacity(height: usize, sample: &Sample) -> usize {
    let sockets = sample.sockets.as_deref().map_or(&[][..], |s| s.as_slice());
    capacity(layout(height, sample).1, sockets)
}

/// The listening-socket half, rendered into at most `available` rows.
fn listening(props: &NetworkViewProps, available: usize) -> Vec<Element> {
    let palette = &props.palette;
    if available < CHROME {
        return Vec::new();
    }

    let mut rows = vec![heading("Listening", palette), header(palette)];

    // The heading and column header are paid for. With nothing left, the
    // section stops here: a one-line notice below a full budget is one row
    // more than we were given, and `Overflow::Visible` would paint it over
    // the status bar.
    if available == CHROME {
        return rows;
    }

    // `None` is "not read yet", not "could not read": sockets are only
    // read while this tab is showing, so the first frame after switching
    // to it has none. Saying "unavailable" here would be the same lie the
    // disk view used to tell.
    let Some(sockets) = props.sample.sockets.as_deref() else {
        rows.push(placeholder("  reading sockets…", palette.muted));
        return rows;
    };

    if sockets.is_empty() {
        rows.push(placeholder("  nothing is listening", palette.muted));
        return rows;
    }

    // Rows that could not be attributed get a one-line explanation rather
    // than a mysteriously empty column, and it costs a row.
    let unattributed = sockets.iter().filter(|s| s.process.is_none()).count();
    let limit = capacity(available, sockets);

    let window = props.selection.visible(sockets.len(), limit);
    for i in window.clone() {
        rows.push(socket_row(&sockets[i], i == props.selection.index, palette));
    }

    if unattributed > 0 {
        rows.push(placeholder(
            &attribution_note(unattributed, sockets, crate::sampler::our_euid()),
            palette.muted,
        ));
    }

    rows
}

/// Why some rows have no process, in the terms that are actually true.
///
/// Suggesting root to a user who is already root is unfollowable advice:
/// once privileged, an unattributed row means the owner exited between the
/// `/proc/net` read and the fd walk, which no privilege fixes.
///
/// `ours` is passed rather than read so both branches are testable without
/// depending on the uid the suite happens to run as.
fn attribution_note(unattributed: usize, sockets: &[ListeningSocket], ours: u32) -> String {
    let plural = if unattributed == 1 { "" } else { "s" };
    let others = sockets
        .iter()
        .any(|s| s.process.is_none() && s.socket.uid != ours);
    if ours != 0 && others {
        format!(
            "  {unattributed} socket{plural} owned by other users — run as root to attribute them"
        )
    } else {
        format!("  {unattributed} socket{plural} could not be attributed")
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::IoRate;

    fn sample(interfaces: usize) -> Sample {
        Sample {
            nets: Some(
                (0..interfaces)
                    .map(|i| IoRate {
                        name: format!("net{i}"),
                        read_per_sec: 1,
                        write_per_sec: 1,
                    })
                    .collect(),
            ),
            ..Sample::default()
        }
    }

    #[test]
    fn a_quiet_interface_list_hands_its_unused_rows_to_the_listeners() {
        // The failure this replaces: a fixed half each left the tab half
        // blank while hiding listeners that had something to show.
        let (interfaces, listening) = layout(20, &sample(1));

        assert_eq!(interfaces, 1 + CHROME);
        assert_eq!(listening, 20 - (1 + CHROME));
    }

    #[test]
    fn a_crowded_interface_list_is_capped_at_half_the_pane() {
        let (interfaces, listening) = layout(20, &sample(40));

        assert_eq!(interfaces, 10);
        assert_eq!(listening, 10);
    }

    #[test]
    fn never_hands_out_more_than_the_budget() {
        for height in 0..60usize {
            for interfaces in 0..12 {
                let (a, b) = layout(height, &sample(interfaces));
                assert_eq!(a + b, height, "height={height} gave {a}+{b}");
            }
        }
    }

    #[test]
    fn a_budget_of_nothing_hands_out_nothing() {
        assert_eq!(layout(0, &sample(4)), (0, 0));
    }

    #[test]
    fn an_interface_share_too_small_for_its_own_chrome_goes_to_the_listeners() {
        // `section` renders nothing below `CHROME`, so those rows would be
        // blank space taken from a list that could use them.
        for height in 0..CHROME * 2 {
            let (interfaces, _) = layout(height, &sample(4));
            assert!(
                interfaces == 0 || interfaces >= CHROME,
                "height={height} gave interfaces {interfaces}"
            );
        }
    }

    #[test]
    fn an_absent_interface_list_still_asks_for_one_row() {
        assert_eq!(
            layout(20, &Sample::default()),
            (1 + CHROME, 20 - (1 + CHROME))
        );
    }

    fn owned_by(uid: u32, attributed: bool) -> ListeningSocket {
        ListeningSocket {
            socket: crate::model::Socket {
                protocol: crate::model::Protocol::Tcp,
                local: "0.0.0.0:80".parse().unwrap(),
                uid,
                inode: 1,
                accept_queue: Some(0),
            },
            user: "root".into(),
            process: attributed.then(|| (1, "init".into())),
        }
    }

    #[test]
    fn the_footer_offers_root_only_when_root_would_help() {
        let theirs = [owned_by(0, false)];
        let mine = [owned_by(1000, false)];

        // Unprivileged, looking at someone else's socket: root is the fix.
        assert!(
            attribution_note(1, &theirs, 1000).contains("run as root"),
            "should suggest root"
        );
        // Already root: an unattributed row means the owner exited between
        // the two reads, and no privilege fixes that.
        assert!(
            !attribution_note(1, &theirs, 0).contains("run as root"),
            "root cannot become more root"
        );
        // Our own socket, unattributed: also not a permissions problem.
        assert!(
            !attribution_note(1, &mine, 1000).contains("run as root"),
            "our own process needs no privilege"
        );
    }

    #[test]
    fn the_footer_counts_in_the_right_number() {
        let mine = [owned_by(1000, false)];
        assert!(attribution_note(1, &mine, 1000).contains("1 socket "));
        assert!(attribution_note(2, &mine, 1000).contains("2 sockets "));
    }

    #[test]
    fn the_capacity_reserves_a_row_for_the_footer_only_when_one_is_needed() {
        let attributed = [owned_by(0, true), owned_by(0, true)];
        let mixed = [owned_by(0, true), owned_by(0, false)];

        assert_eq!(capacity(10, &attributed), 8);
        assert_eq!(capacity(10, &mixed), 7, "the footer costs a row");
        assert_eq!(capacity(2, &mixed), 0, "saturates rather than underflowing");
        assert_eq!(capacity(0, &mixed), 0);
    }
}
