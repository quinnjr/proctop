//! The process table.
//!
//! ntui's built-in `Table` widget is static: it takes owned `String` cells,
//! sizes every column by scanning all rows, has no notion of a selected row,
//! and styles every cell alike. A process table needs the opposite of all
//! four — so this builds on `View`/`Text` directly, and slices the list to
//! the visible window *before* constructing any row, keeping the cost of a
//! frame proportional to what is on screen rather than to the process count.

use ntui::props::{Dimension, FlexDirection, Overflow, TextProps, TextWrap, ViewProps};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks};

use crate::format;
use crate::model::ProcRow;
use crate::sort::SortKey;
use crate::ui::Shared;
use crate::ui::palette::Palette;
use crate::ui::selection::Selection;

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

/// The command column is not in `COLUMNS` because it takes the remaining
/// width rather than a fixed one.
const COMMAND_HEADER: &str = "Command";

/// Cells of indent per tree level. Two reads as nesting without pushing long
/// command lines off the right edge.
const INDENT: usize = 2;

#[derive(Clone, PartialEq, Default)]
pub struct ProcessTableProps {
    /// Already filtered, sorted, and (in tree view) nested.
    pub rows: Shared<Vec<ProcRow>>,
    pub selection: Selection,
    /// How many rows fit on screen.
    pub height: u16,
    /// Which column is driving the order, so its header can be marked.
    pub sort: SortKey,
    pub palette: Palette,
}

pub struct ProcessTable;

impl Component for ProcessTable {
    type Props = ProcessTableProps;

    fn render(props: &ProcessTableProps, _hooks: &mut Hooks) -> Element {
        ProcessTable::render_tree(props)
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
    fn render_tree(props: &ProcessTableProps) -> Element {
        let window = props
            .selection
            .visible(props.rows.len(), props.height as usize);

        let mut children = Vec::with_capacity(window.len() + 1);
        children.push(header_row(props.sort, &props.palette));

        for i in window {
            let row = &props.rows[i];
            children.push(process_row(row, i == props.selection.index, &props.palette));
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

fn header_row(sort: SortKey, palette: &Palette) -> Element {
    let mut cells = Vec::with_capacity(COLUMNS.len() + 1);

    for column in &COLUMNS {
        let active = column.sort == Some(sort);
        cells.push(cell(
            &pad(column.header, column.width, column.numeric),
            column.width,
            if active {
                palette.selected_fg
            } else {
                palette.header_fg
            },
            if active { Weight::Bold } else { Weight::Normal },
            if active {
                palette.selected_bg
            } else {
                palette.header_bg
            },
        ));
    }
    cells.push(cell_flex(
        COMMAND_HEADER,
        palette.header_fg,
        Weight::Normal,
        palette.header_bg,
    ));

    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 1,
            height: Dimension::Cells(1),
            background: palette.header_bg,
            ..Default::default()
        },
        cells,
    )
}

fn process_row(row: &ProcRow, selected: bool, palette: &Palette) -> Element {
    let p = &row.proc;
    let bg = if selected {
        palette.selected_bg
    } else {
        Color::Reset
    };
    // Against the selection bar, per-column colors would be unreadable, so
    // the whole row takes the contrasting foreground.
    let fg = |c: Color| if selected { palette.selected_fg } else { c };

    let values = [
        (p.pid.to_string(), fg(palette.text)),
        (row.user.to_string(), fg(palette.label)),
        (p.priority.to_string(), fg(palette.text)),
        (p.nice.to_string(), fg(palette.text)),
        (format::bytes(p.vsize), fg(palette.muted)),
        (format::bytes(p.rss), fg(palette.text)),
        (p.state.as_char().to_string(), fg(palette.text)),
        (
            format!("{:.1}", row.cpu * 100.0),
            fg(palette.cpu_load(row.cpu)),
        ),
        (
            format!("{:.1}", row.mem * 100.0),
            fg(palette.mem_load(row.mem)),
        ),
        (format::cpu_time(p.cpu_time()), fg(palette.text)),
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

    cells.push(cell_flex(
        &indented(row),
        fg(palette.text),
        Weight::Normal,
        bg,
    ));

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

/// The command, prefixed with tree guides at depth.
fn indented(row: &ProcRow) -> String {
    if row.depth == 0 {
        return row.proc.name.clone();
    }
    format!(
        "{}└─ {}",
        " ".repeat((row.depth - 1) * INDENT),
        row.proc.name
    )
}

/// Pad or truncate to exactly `width`, so columns line up regardless of
/// content. Truncation is on the left for numbers (keeping the significant
/// digits) and on the right for text.
pub fn pad(value: &str, width: u16, numeric: bool) -> String {
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

/// One fixed-width cell.
pub fn cell(content: &str, width: u16, color: Color, weight: Weight, background: Color) -> Element {
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

/// A cell taking whatever width is left. Truncated rather than wrapped: a
/// long command name must not push every later row down.
pub fn cell_flex(content: &str, color: Color, weight: Weight, background: Color) -> Element {
    Element::view(
        ViewProps {
            flex_grow: 1.0,
            height: Dimension::Cells(1),
            background,
            overflow: Overflow::Clip,
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
