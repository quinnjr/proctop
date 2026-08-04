//! The I/O tab: per-device disk and per-interface network throughput.

use ntui::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks};

use crate::format;
use crate::model::{IoRate, Sample};
use crate::ui::Shared;
use crate::ui::palette::Palette;
use crate::ui::table::{cell, cell_flex, pad};

/// Bar width for the throughput sparkline.
const BAR: usize = 20;

/// Rows a section spends on its heading and column header before it can
/// show a single device.
const CHROME: usize = 2;

/// The scale each bar is drawn against, in bytes per second.
///
/// Throughput has no natural maximum the way a CPU meter does, so the bars
/// are drawn against a fixed reference rather than against the fastest
/// device present — otherwise an idle machine shows one device at "full"
/// and the display implies load that is not there.
const FULL_SCALE: f32 = 100.0 * 1024.0 * 1024.0;

#[derive(Clone, PartialEq, Default)]
pub struct IoViewProps {
    pub sample: Shared<Sample>,
    pub palette: Palette,
    /// Rows available for the whole tab.
    pub height: u16,
}

pub struct IoView;

impl Component for IoView {
    type Props = IoViewProps;

    fn render(props: &IoViewProps, _hooks: &mut Hooks) -> Element {
        IoView::render_tree(props)
    }
}

impl IoView {
    fn render_tree(props: &IoViewProps) -> Element {
        let palette = &props.palette;
        let sample = &props.sample;

        // Devices that have never moved a byte are noise: a machine has
        // dozens of loop devices and a handful of down interfaces.
        let disks = sample.disks.as_deref().map(active);
        let nets = sample.nets.as_deref().map(active);

        // Each section wants CHROME rows for its heading and column header
        // plus one per device. The split follows demand rather than being a
        // fixed half each — one busy disk and eight busy interfaces should
        // not leave half the tab blank — and the chrome counts against the
        // budget, so a pane too short for a section shows fewer sections
        // rather than overflowing.
        let wants =
            |rates: &Option<Vec<&IoRate>>| CHROME + rates.as_ref().map_or(1, |r| r.len().max(1));
        let (disk_rows, net_rows) = split(props.height as usize, wants(&disks), wants(&nets));

        let mut children = section(
            "Disks",
            "READ",
            "WRITE",
            disks.as_deref(),
            disk_rows,
            palette,
        );
        children.extend(section(
            "Network",
            "RX",
            "TX",
            nets.as_deref(),
            net_rows,
            palette,
        ));

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

/// Divide `budget` between two lists by demand, giving neither more than it
/// can use and preferring an even split when both want more than their half.
fn split(budget: usize, first: usize, second: usize) -> (usize, usize) {
    if first + second <= budget {
        return (first, second);
    }
    let half = budget / 2;
    if first <= half {
        return (first, budget - first);
    }
    if second <= half {
        return (budget - second, second);
    }
    (half, budget - half)
}

/// Devices currently moving data, busiest first.
fn active(rates: &[IoRate]) -> Vec<&IoRate> {
    let mut active: Vec<&IoRate> = rates
        .iter()
        .filter(|r| r.read_per_sec > 0 || r.write_per_sec > 0)
        .collect();
    active.sort_by_key(|r| std::cmp::Reverse(r.read_per_sec + r.write_per_sec));
    active
}

/// One titled list, rendered into at most `available` terminal rows —
/// heading and column header included, so nothing here can overflow the
/// budget it was given.
fn section(
    title: &str,
    read_label: &str,
    write_label: &str,
    // `None` when the counters could not be read at all, which is a
    // different fact from "read them, nothing is moving".
    rates: Option<&[&IoRate]>,
    available: usize,
    palette: &Palette,
) -> Vec<Element> {
    // Too short even for the heading: show nothing rather than a title with
    // no list under it.
    if available < CHROME {
        return Vec::new();
    }

    let mut rows = vec![
        heading(title, palette),
        header(read_label, write_label, palette),
    ];
    let limit = available - CHROME;
    if limit == 0 {
        return rows;
    }

    let Some(rates) = rates else {
        rows.push(notice("  (unavailable)", palette.alert));
        return rows;
    };

    if rates.is_empty() {
        rows.push(notice("  (idle)", palette.muted));
        return rows;
    }

    rows.extend(rates.iter().take(limit).map(|r| device_row(r, palette)));
    rows
}

/// A one-line message standing in for a list.
fn notice(text: &str, color: ntui::Color) -> Element {
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

fn header(read_label: &str, write_label: &str, palette: &Palette) -> Element {
    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 1,
            height: Dimension::Cells(1),
            background: palette.header_bg,
            ..Default::default()
        },
        vec![
            cell(
                &pad("DEVICE", 14, false),
                14,
                palette.header_fg,
                Weight::Normal,
                palette.header_bg,
            ),
            cell(
                &pad(read_label, 10, true),
                10,
                palette.header_fg,
                Weight::Normal,
                palette.header_bg,
            ),
            cell(
                &pad(write_label, 10, true),
                10,
                palette.header_fg,
                Weight::Normal,
                palette.header_bg,
            ),
            cell_flex("", palette.header_fg, Weight::Normal, palette.header_bg),
        ],
    )
}

fn device_row(rate: &IoRate, palette: &Palette) -> Element {
    let total = (rate.read_per_sec + rate.write_per_sec) as f32 / FULL_SCALE;
    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 1,
            height: Dimension::Cells(1),
            ..Default::default()
        },
        vec![
            cell(
                &pad(&rate.name, 14, false),
                14,
                palette.text,
                Weight::Normal,
                Color::Reset,
            ),
            cell(
                &pad(&per_second(rate.read_per_sec), 10, true),
                10,
                palette.cpu_user,
                Weight::Normal,
                Color::Reset,
            ),
            cell(
                &pad(&per_second(rate.write_per_sec), 10, true),
                10,
                palette.cpu_system,
                Weight::Normal,
                Color::Reset,
            ),
            cell_flex(
                &format!("[{}]", format::bar(total, BAR)),
                palette.mem_used,
                Weight::Normal,
                Color::Reset,
            ),
        ],
    )
}

fn per_second(bytes: u64) -> String {
    format!("{}/s", format::bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_budget_that_fits_both_gives_each_what_it_asked_for() {
        assert_eq!(split(20, 5, 6), (5, 6));
    }

    #[test]
    fn a_quiet_list_hands_its_unused_rows_to_the_busy_one() {
        // The failure this replaces: a fixed half each left the tab half
        // blank while hiding interfaces that had something to show.
        assert_eq!(split(12, 3, 20), (3, 9));
        assert_eq!(split(12, 20, 3), (9, 3));
    }

    #[test]
    fn two_busy_lists_share_evenly() {
        assert_eq!(split(12, 20, 20), (6, 6));
        assert_eq!(split(11, 20, 20), (5, 6));
    }

    #[test]
    fn never_hands_out_more_than_the_budget() {
        for budget in 0..40usize {
            for first in 0..12 {
                for second in 0..12 {
                    let (a, b) = split(budget, first, second);
                    assert!(a + b <= budget, "budget={budget} gave {a}+{b}");
                    assert!(a <= first.max(budget) && b <= second.max(budget));
                }
            }
        }
    }

    #[test]
    fn a_budget_of_nothing_hands_out_nothing() {
        assert_eq!(split(0, 5, 5), (0, 0));
    }
}
