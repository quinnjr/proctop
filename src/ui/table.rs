//! The process table.
//!
//! ntui's built-in `Table` widget is static: it takes owned `String` cells,
//! sizes every column by scanning all rows, has no notion of a selected row,
//! and styles every cell alike. A process table needs the opposite of all
//! four — so this builds on `View`/`Text` directly, and slices the list to
//! the visible window *before* constructing any row, keeping the cost of a
//! frame proportional to what is on screen rather than to the process count.

use ntui::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks};

use crate::format;
use crate::model::ProcRow;
use crate::sort::SortKey;
use crate::ui::selection::Selection;
use crate::ui::{Shared, theme};

/// A fixed-width column of the table.
struct Column {
    header: &'static str,
    width: u16,
    /// Whether the cell content is right-aligned, as numbers should be.
    numeric: bool,
    sort: Option<SortKey>,
}

const COLUMNS: [Column; 10] = [
    Column {
        header: "PID",
        width: 7,
        numeric: true,
        sort: Some(SortKey::Pid),
    },
    Column {
        header: "USER",
        width: 9,
        numeric: false,
        sort: None,
    },
    Column {
        header: "PRI",
        width: 3,
        numeric: true,
        sort: None,
    },
    Column {
        header: "NI",
        width: 3,
        numeric: true,
        sort: None,
    },
    Column {
        header: "VIRT",
        width: 6,
        numeric: true,
        sort: None,
    },
    Column {
        header: "RES",
        width: 6,
        numeric: true,
        sort: Some(SortKey::Memory),
    },
    Column {
        header: "S",
        width: 1,
        numeric: false,
        sort: None,
    },
    Column {
        header: "CPU%",
        width: 5,
        numeric: true,
        sort: Some(SortKey::Cpu),
    },
    Column {
        header: "MEM%",
        width: 5,
        numeric: true,
        sort: Some(SortKey::Memory),
    },
    Column {
        header: "TIME+",
        width: 9,
        numeric: true,
        sort: Some(SortKey::Time),
    },
];

/// The name column is not in `COLUMNS` because it takes the remaining width
/// rather than a fixed one.
const COMMAND_HEADER: &str = "Command";

#[derive(Clone, PartialEq, Default)]
pub struct ProcessTableProps {
    /// Already sorted and filtered.
    pub rows: Shared<Vec<ProcRow>>,
    pub selection: Selection,
    /// How many rows fit on screen.
    pub height: u16,
    /// Which column is driving the order, so its header can be marked.
    pub sort: SortKey,
}

pub struct ProcessTable;

impl Component for ProcessTable {
    type Props = ProcessTableProps;

    fn render(props: &ProcessTableProps, _hooks: &mut Hooks) -> Element {
        ProcessTable::build(props)
    }
}

impl ProcessTable {
    /// The table's output as a pure function of its props.
    ///
    /// Exposed separately from `Component::render` because a `Hooks` cannot
    /// be constructed outside ntui, and the property worth testing here —
    /// that offscreen rows are never built at all — is invisible in a
    /// rendered frame, where "never built" and "built then clipped" look
    /// identical.
    pub fn build(props: &ProcessTableProps) -> Element {
        let window = props
            .selection
            .visible(props.rows.len(), props.height as usize);

        let mut children = Vec::with_capacity(window.len() + 1);
        children.push(header_row(props.sort));

        for i in window {
            let row = &props.rows[i];
            children.push(process_row(row, i == props.selection.index));
        }

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

fn header_row(sort: SortKey) -> Element {
    let mut cells = Vec::with_capacity(COLUMNS.len() + 1);

    for column in &COLUMNS {
        let active = column.sort == Some(sort);
        cells.push(cell(
            &pad(column.header, column.width, column.numeric),
            column.width,
            if active { Color::Black } else { theme::HEADER },
            if active { Weight::Bold } else { Weight::Normal },
            if active {
                Color::Cyan
            } else {
                theme::HEADER_BG
            },
        ));
    }
    cells.push(cell_flex(
        COMMAND_HEADER,
        theme::HEADER,
        Weight::Normal,
        theme::HEADER_BG,
    ));

    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 1,
            height: Dimension::Cells(1),
            background: theme::HEADER_BG,
            ..Default::default()
        },
        cells,
    )
}

fn process_row(row: &ProcRow, selected: bool) -> Element {
    let p = &row.proc;
    let bg = if selected {
        theme::SELECTED_BG
    } else {
        Color::Reset
    };
    // On the selection bar, per-column colors would be unreadable against
    // the highlight, so the whole row takes the contrasting foreground.
    let fg = |c: Color| if selected { theme::SELECTED_FG } else { c };

    let values = [
        (p.pid.to_string(), fg(theme::TEXT)),
        (row.user.clone(), fg(theme::LABEL)),
        (p.priority.to_string(), fg(theme::TEXT)),
        (p.nice.to_string(), fg(theme::TEXT)),
        (format::bytes(p.vsize), fg(theme::MUTED)),
        (format::bytes(p.rss), fg(theme::TEXT)),
        (p.state.as_char().to_string(), fg(theme::TEXT)),
        (
            format!("{:.1}", row.cpu * 100.0),
            fg(theme::cpu_color(row.cpu)),
        ),
        (
            format!("{:.1}", row.mem * 100.0),
            fg(theme::mem_color(row.mem)),
        ),
        (format::cpu_time(p.cpu_time()), fg(theme::TEXT)),
    ];

    let mut cells: Vec<Element> = values
        .iter()
        .zip(&COLUMNS)
        .map(|((value, color), column)| {
            cell(
                &pad(value, column.width, column.numeric),
                column.width,
                *color,
                Weight::Normal,
                bg,
            )
        })
        .collect();
    cells.push(cell_flex(&p.name, fg(theme::TEXT), Weight::Normal, bg));

    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 1,
            height: Dimension::Cells(1),
            background: bg,
            ..Default::default()
        },
        cells,
    )
}

/// Pad or truncate to exactly `width`, so columns line up regardless of
/// content. Truncation is on the left for numbers (keeping the significant
/// digits) and on the right for text.
fn pad(value: &str, width: u16, numeric: bool) -> String {
    let width = width as usize;
    let len = value.chars().count();

    if len > width {
        return if numeric {
            value.chars().skip(len - width).collect()
        } else {
            value.chars().take(width).collect()
        };
    }
    let padding = " ".repeat(width - len);
    if numeric {
        format!("{padding}{value}")
    } else {
        format!("{value}{padding}")
    }
}

fn cell(content: &str, width: u16, color: Color, weight: Weight, background: Color) -> Element {
    Element::view(
        ViewProps {
            width: Dimension::Cells(width),
            height: Dimension::Cells(1),
            background,
            ..Default::default()
        },
        vec![Element::text(TextProps {
            content: content.to_string(),
            color,
            weight,
            wrap: TextWrap::Truncate,
            ..Default::default()
        })],
    )
}

/// The trailing column, which takes whatever width is left. Truncated rather
/// than wrapped: a long command name must not push every later row down.
fn cell_flex(content: &str, color: Color, weight: Weight, background: Color) -> Element {
    Element::view(
        ViewProps {
            flex_grow: 1.0,
            height: Dimension::Cells(1),
            background,
            overflow: ntui::props::Overflow::Clip,
            ..Default::default()
        },
        vec![Element::text(TextProps {
            content: content.to_string(),
            color,
            weight,
            wrap: TextWrap::Truncate,
            ..Default::default()
        })],
    )
}
