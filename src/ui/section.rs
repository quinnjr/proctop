//! A titled, row-budgeted list of throughput devices.
//!
//! Shared by the Disk and Network tabs: both draw a heading, a column
//! header and one row per device inside a fixed row budget, and both need
//! "could not read" to look different from "read it, nothing is moving".
//! It lives here rather than in either tab so neither depends on the other.

use ntui::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use ntui::style::{Color, Weight};

use crate::format;
use crate::model::IoRate;
use crate::ui::palette::Palette;
use crate::ui::table::{cell, cell_flex, pad};
use ntui::Element;

/// Bar width for the throughput sparkline.
const BAR: usize = 20;

/// Rows a section spends on its heading and column header before it can
/// show a single device.
pub(crate) const CHROME: usize = 2;

/// The scale each bar is drawn against, in bytes per second.
///
/// Throughput has no natural maximum the way a CPU meter does, so the bars
/// are drawn against a fixed reference rather than against the fastest
/// device present — otherwise an idle machine shows one device at "full"
/// and the display implies load that is not there.
const FULL_SCALE: f32 = 100.0 * 1024.0 * 1024.0;

/// Whether a device has moved a byte in the last interval.
pub(crate) fn is_busy(rate: &IoRate) -> bool {
    rate.read_per_sec > 0 || rate.write_per_sec > 0
}

/// Devices currently moving data, busiest first.
pub(crate) fn active(rates: &[IoRate]) -> Vec<&IoRate> {
    let mut active: Vec<&IoRate> = rates.iter().filter(|r| is_busy(r)).collect();
    active.sort_by_key(|r| std::cmp::Reverse(r.read_per_sec + r.write_per_sec));
    active
}

/// One titled list, rendered into at most `available` terminal rows —
/// heading and column header included, so nothing here can overflow the
/// budget it was given.
pub(crate) fn section(
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
        rows.push(placeholder("  (unavailable)", palette.alert));
        return rows;
    };

    if rates.is_empty() {
        rows.push(placeholder("  (idle)", palette.muted));
        return rows;
    }

    rows.extend(rates.iter().take(limit).map(|r| device_row(r, palette)));
    rows
}

/// A one-line message standing in for a list.
pub(crate) fn placeholder(text: &str, color: ntui::Color) -> Element {
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

pub(crate) fn heading(title: &str, palette: &Palette) -> Element {
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

pub(crate) fn header(read_label: &str, write_label: &str, palette: &Palette) -> Element {
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

pub(crate) fn device_row(rate: &IoRate, palette: &Palette) -> Element {
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

pub(crate) fn per_second(bytes: u64) -> String {
    format!("{}/s", format::bytes(bytes))
}
