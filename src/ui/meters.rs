//! The header: per-core CPU meters, memory, swap, and a summary line.

use ntui::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks};

use crate::delta::CpuUsage;
use crate::format;
use crate::model::Sample;
use crate::ui::Shared;
use crate::ui::palette::Palette;

/// Most rows of meters to show before adding another column, so a 32-core
/// machine does not push the process table off the screen.
const MAX_ROWS: usize = 9;
/// Width of one meter column, including its label and figure.
const METER_WIDTH: u16 = 30;
/// Cells between columns.
const METER_GAP: u16 = 2;

/// Memory and swap are laid out as two more meters after the cores, so they
/// share the column budget instead of always claiming a column of their own.
const EXTRA_METERS: usize = 2;

#[derive(Clone, PartialEq, Default)]
pub struct MetersProps {
    pub sample: Shared<Sample>,
    pub palette: Palette,
    /// Terminal width, which decides how many meter columns fit.
    pub width: u16,
}

pub struct Meters;

impl Component for Meters {
    type Props = MetersProps;

    fn render(props: &MetersProps, _hooks: &mut Hooks) -> Element {
        let sample = &props.sample;
        let palette = &props.palette;
        let cores = &sample.cores;

        // Memory and swap are just two more meters on the end, so they
        // flow into whatever column has room rather than always claiming
        // one of their own.
        let mut meters: Vec<Element> = cores
            .iter()
            .enumerate()
            .map(|(i, usage)| cpu_meter(i, usage, palette))
            .collect();
        meters.push(meter("Mem", memory_bar(sample), palette.mem_used, palette));
        meters.push(meter("Swp", swap_bar(sample), palette.swap, palette));

        // Fill each column top to bottom, the way htop does, so core 0 and
        // core 1 sit next to each other vertically rather than across the
        // screen from each other.
        let rows = rows_per_column(cores.len(), props.width);
        let mut columns: Vec<Vec<Element>> = Vec::new();
        for (i, meter) in meters.into_iter().enumerate() {
            let column = i / rows;
            if columns.len() <= column {
                columns.push(Vec::new());
            }
            columns[column].push(meter);
        }

        let columns: Vec<Element> = columns
            .into_iter()
            .map(|cells| {
                Element::view(
                    ViewProps {
                        flex_direction: FlexDirection::Column,
                        width: Dimension::Cells(METER_WIDTH),
                        ..Default::default()
                    },
                    cells,
                )
            })
            .collect();

        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            vec![
                Element::view(
                    ViewProps {
                        flex_direction: FlexDirection::Row,
                        gap: 2,
                        ..Default::default()
                    },
                    columns,
                ),
                summary(sample, palette),
            ],
        )
    }
}

/// How many rows each meter column holds.
///
/// Columns are added rather than rows once a machine has more cores than fit
/// vertically — but only as many columns as the terminal is actually wide
/// enough for. Without that ceiling a 32-core machine lays out five columns
/// and the last one, holding memory and swap, is clipped off the right edge.
fn rows_per_column(cores: usize, width: u16) -> usize {
    let items = cores + EXTRA_METERS;
    let fits = ((width + METER_GAP) / (METER_WIDTH + METER_GAP)).max(1) as usize;
    let wanted = items.div_ceil(MAX_ROWS).max(1);
    let columns = wanted.min(fits);
    items.div_ceil(columns).max(1)
}

fn cpu_meter(index: usize, usage: &CpuUsage, palette: &Palette) -> Element {
    meter(
        &index.to_string(),
        format!(
            "{}{:>5.1}%",
            format::bar(usage.busy(), 16),
            usage.busy() * 100.0
        ),
        segment_color(usage, palette),
        palette,
    )
}

/// Color the bar by whatever it is mostly doing, so a machine pinned in
/// system time is distinguishable from one doing user work at a glance.
fn segment_color(usage: &CpuUsage, palette: &Palette) -> Color {
    let candidates = [
        (usage.system, palette.cpu_system),
        (usage.nice, palette.cpu_nice),
        (usage.irq + usage.softirq, palette.cpu_irq),
        (usage.iowait, palette.cpu_iowait),
        (usage.user, palette.cpu_user),
    ];
    candidates
        .iter()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, color)| *color)
        .unwrap_or(palette.cpu_user)
}

fn memory_bar(sample: &Sample) -> String {
    let mem = &sample.mem;
    let used = fraction(mem.used(), mem.total);
    // Kept within METER_WIDTH: content wider than the column makes
    // flexbox shrink the label box beside it, and the label loses a cell.
    format!(
        "{}{:>5}/{}",
        format::bar(used, 16),
        format::bytes(mem.used()),
        format::bytes(mem.total)
    )
}

fn swap_bar(sample: &Sample) -> String {
    let mem = &sample.mem;
    let used = fraction(mem.swap_used(), mem.swap_total);
    format!(
        "{}{:>5}/{}",
        format::bar(used, 16),
        format::bytes(mem.swap_used()),
        format::bytes(mem.swap_total)
    )
}

/// A machine with no swap has a zero denominator; report an empty meter
/// rather than NaN.
fn fraction(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        0.0
    } else {
        part as f32 / whole as f32
    }
}

fn meter(label: &str, content: String, color: Color, palette: &Palette) -> Element {
    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            height: Dimension::Cells(1),
            ..Default::default()
        },
        vec![
            Element::view(
                ViewProps {
                    width: Dimension::Cells(4),
                    ..Default::default()
                },
                vec![Element::text(TextProps {
                    content: format!("{label:>3}"),
                    color: palette.label,
                    weight: Weight::Bold,
                    wrap: TextWrap::Truncate,
                    ..Default::default()
                })],
            ),
            Element::text(TextProps {
                content,
                color,
                wrap: TextWrap::Truncate,
                ..Default::default()
            }),
        ],
    )
}

fn summary(sample: &Sample, palette: &Palette) -> Element {
    let load = &sample.load;
    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 2,
            height: Dimension::Cells(1),
            ..Default::default()
        },
        vec![
            Element::text(TextProps {
                content: format!(
                    "Tasks: {}, {} thr, {} running",
                    sample.procs.len(),
                    sample.threads,
                    sample.running
                ),
                color: palette.text,
                wrap: TextWrap::Truncate,
                ..Default::default()
            }),
            Element::text(TextProps {
                content: format!("Load: {:.2} {:.2} {:.2}", load.one, load.five, load.fifteen),
                color: palette.text,
                wrap: TextWrap::Truncate,
                ..Default::default()
            }),
            Element::text(TextProps {
                content: format!("Uptime: {}", format::uptime(sample.uptime)),
                color: palette.muted,
                wrap: TextWrap::Truncate,
                ..Default::default()
            }),
        ],
    )
}

/// How many terminal rows the header occupies, so the body knows how much
/// room is left. The `+ 1` is the summary line beneath the meters.
pub fn height(cores: usize, width: u16) -> u16 {
    let rows = rows_per_column(cores, width);
    // The tallest column is the full row count unless everything fits in
    // one short column.
    let items = cores + EXTRA_METERS;
    (rows.min(items) + 1) as u16
}
