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
        IoView::build(props)
    }
}

impl IoView {
    pub fn build(props: &IoViewProps) -> Element {
        let palette = &props.palette;
        let sample = &props.sample;

        // Devices that have never moved a byte are noise: a machine has
        // dozens of loop devices and a handful of down interfaces.
        let disks = active(&sample.disks);
        let nets = active(&sample.nets);

        // Split the available rows between the two lists, leaving each its
        // heading and column header.
        let half = (props.height as usize).saturating_sub(4) / 2;

        let mut children = section("Disks", "READ", "WRITE", &disks, half, palette);
        children.extend(section("Network", "RX", "TX", &nets, half, palette));

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

/// Devices currently moving data, busiest first.
fn active(rates: &[IoRate]) -> Vec<&IoRate> {
    let mut active: Vec<&IoRate> = rates
        .iter()
        .filter(|r| r.read_per_sec > 0 || r.write_per_sec > 0)
        .collect();
    active.sort_by_key(|r| std::cmp::Reverse(r.read_per_sec + r.write_per_sec));
    active
}

fn section(
    title: &str,
    read_label: &str,
    write_label: &str,
    rates: &[&IoRate],
    limit: usize,
    palette: &Palette,
) -> Vec<Element> {
    let mut rows = vec![
        heading(title, palette),
        header(read_label, write_label, palette),
    ];

    if rates.is_empty() {
        rows.push(Element::view(
            ViewProps {
                height: Dimension::Cells(1),
                ..Default::default()
            },
            vec![Element::text(TextProps {
                content: String::from("  (idle)"),
                color: palette.muted,
                wrap: TextWrap::Truncate,
                ..Default::default()
            })],
        ));
        return rows;
    }

    for rate in rates.iter().take(limit) {
        rows.push(device_row(rate, palette));
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
