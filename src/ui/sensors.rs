//! The sensors tab: temperatures, fans, and battery.

use ntui::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks};

use crate::format;
use crate::model::{Sample, Sensor, SensorKind};
use crate::ui::Shared;
use crate::ui::palette::Palette;
use crate::ui::table::{cell, cell_flex, pad};

/// The temperature a bar is drawn as full at. Silicon throttles well below
/// this, so a full bar always means something is wrong.
const MAX_CELSIUS: f32 = 100.0;
/// A fast case fan, for the same purpose.
const MAX_RPM: f32 = 3000.0;

#[derive(Clone, PartialEq, Default)]
pub struct SensorViewProps {
    pub sample: Shared<Sample>,
    pub palette: Palette,
    pub height: u16,
}

pub struct SensorView;

impl Component for SensorView {
    type Props = SensorViewProps;

    fn render(props: &SensorViewProps, _hooks: &mut Hooks) -> Element {
        let palette = &props.palette;
        let (headline, detail) = match &props.sample.sensors {
            // Readings are only taken while this tab is showing, so the
            // first frame after switching to it has none yet.
            None => (
                "Reading sensors…",
                "Hardware readings are only taken while this tab is open.",
            ),
            // Not an error: plenty of machines — VMs, containers, boards
            // with no hwmon driver — genuinely have nothing to report.
            Some(sensors) if sensors.is_empty() => (
                "No sensors available.",
                "Nothing under /sys/class/hwmon could be read. Virtual machines and \
                 containers usually have no thermal sensors at all.",
            ),
            Some(sensors) => return table(sensors, props),
        };

        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                padding: 1,
                ..Default::default()
            },
            vec![
                Element::text(TextProps {
                    content: headline.to_string(),
                    color: palette.text,
                    weight: Weight::Bold,
                    wrap: TextWrap::Truncate,
                    ..Default::default()
                }),
                Element::text(TextProps {
                    content: detail.to_string(),
                    color: palette.muted,
                    ..Default::default()
                }),
            ],
        )
    }
}

/// The readings themselves, once there are any.
fn table(sensors: &[Sensor], props: &SensorViewProps) -> Element {
    let palette = &props.palette;
    let mut children = vec![header(palette)];
    // Temperatures first, then fans, then battery — the order people
    // read them in. One chained pass with a single budget: taking
    // `height` per kind meant three times the rows the tab has room
    // for, pushing every fan and battery reading below the fold.
    let budget = (props.height as usize).saturating_sub(1);
    children.extend(
        [
            SensorKind::Temperature,
            SensorKind::Fan,
            SensorKind::Battery,
        ]
        .into_iter()
        .flat_map(|kind| sensors.iter().filter(move |s| s.kind == kind))
        .take(budget)
        .map(|s| sensor_row(s, palette)),
    );

    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            ..Default::default()
        },
        children,
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
            head("CHIP", 14, false),
            head("SENSOR", 20, false),
            head("VALUE", 9, true),
            cell_flex("", palette.header_fg, Weight::Normal, palette.header_bg),
        ],
    )
}

fn sensor_row(sensor: &Sensor, palette: &Palette) -> Element {
    let (value, fraction, color) = match sensor.kind {
        SensorKind::Temperature => (
            format!("{:.1}°C", sensor.value),
            sensor.value / MAX_CELSIUS,
            palette.temperature(sensor.value),
        ),
        SensorKind::Fan => (
            format!("{:.0} rpm", sensor.value),
            sensor.value / MAX_RPM,
            palette.text,
        ),
        SensorKind::Battery => (
            format!("{:.0}%", sensor.value),
            sensor.value / 100.0,
            // A nearly-flat battery is the one reading here that is urgent.
            if sensor.value <= 15.0 {
                palette.alert
            } else {
                palette.text
            },
        ),
    };

    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 1,
            height: Dimension::Cells(1),
            ..Default::default()
        },
        vec![
            cell(
                &pad(&sensor.chip, 14, false),
                14,
                palette.label,
                Weight::Normal,
                Color::Reset,
            ),
            cell(
                &pad(&sensor.label, 20, false),
                20,
                palette.text,
                Weight::Normal,
                Color::Reset,
            ),
            cell(
                &pad(&value, 9, true),
                9,
                color,
                Weight::Normal,
                Color::Reset,
            ),
            cell_flex(
                &format!("[{}]", format::bar(fraction, 20)),
                color,
                Weight::Normal,
                Color::Reset,
            ),
        ],
    )
}
