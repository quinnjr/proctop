//! Shared chrome for the modal layers.
//!
//! ntui paints an overlay `View` against the whole viewport, out of normal
//! flow, and explicitly does not support nesting one inside another — it
//! `debug_assert!`s on the attempt. That matches the interaction model
//! anyway: at most one overlay is open at a time.
//!
//! Every row inside a panel paints its own opaque background across the full
//! panel width. A `View` with `Color::Reset` does not paint at all, so
//! without this the process table shows straight through the dialog.

use ntui::Element;
use ntui::props::{Anchor, Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use ntui::style::{BorderStyle, Color, Weight};

use crate::ui::palette::Palette;

/// Width of a panel's content area. Fixed rather than content-sized so every
/// row can paint the same width, leaving no transparent gaps.
const PANEL_WIDTH: u16 = 54;

/// A centered, bordered panel above the rest of the UI.
pub fn panel(title: &str, palette: &Palette, mut body: Vec<Element>) -> Element {
    let mut children = vec![
        row(title, palette.label, Weight::Bold, palette),
        row("", palette.text, Weight::Normal, palette),
    ];
    children.append(&mut body);

    Element::view(
        ViewProps {
            overlay: Some(Anchor::Center),
            flex_direction: FlexDirection::Column,
            padding: 1,
            width: Dimension::Cells(PANEL_WIDTH + 4),
            border_style: BorderStyle::Round,
            border_color: palette.label,
            background: palette.panel_bg,
            ..Default::default()
        },
        children,
    )
}

/// One line of text, painting the panel background across the full width.
pub fn row(content: impl Into<String>, color: Color, weight: Weight, palette: &Palette) -> Element {
    Element::view(
        ViewProps {
            height: Dimension::Cells(1),
            width: Dimension::Cells(PANEL_WIDTH),
            background: palette.panel_bg,
            ..Default::default()
        },
        vec![Element::text(TextProps {
            content: content.into(),
            color,
            weight,
            wrap: TextWrap::Truncate,
            ..Default::default()
        })],
    )
}

/// A `label: value` row, with the labels aligned into a column.
pub fn field(label: &str, value: impl Into<String>, palette: &Palette) -> Element {
    const LABEL_WIDTH: u16 = 15;

    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            height: Dimension::Cells(1),
            width: Dimension::Cells(PANEL_WIDTH),
            background: palette.panel_bg,
            ..Default::default()
        },
        vec![
            Element::view(
                ViewProps {
                    width: Dimension::Cells(LABEL_WIDTH),
                    background: palette.panel_bg,
                    ..Default::default()
                },
                vec![Element::text(TextProps {
                    content: label.to_string(),
                    color: palette.muted,
                    wrap: TextWrap::Truncate,
                    ..Default::default()
                })],
            ),
            Element::view(
                ViewProps {
                    width: Dimension::Cells(PANEL_WIDTH - LABEL_WIDTH),
                    background: palette.panel_bg,
                    ..Default::default()
                },
                vec![Element::text(TextProps {
                    content: value.into(),
                    color: palette.text,
                    wrap: TextWrap::Truncate,
                    ..Default::default()
                })],
            ),
        ],
    )
}
