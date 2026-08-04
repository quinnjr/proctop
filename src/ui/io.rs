//! The Disk tab: per-device throughput.
//!
//! Network throughput lives in the Network tab, beside the sockets it
//! explains. The section widgets both tabs draw with are in `ui::section`.

use ntui::props::{FlexDirection, ViewProps};
use ntui::{Component, Element, Hooks};

use crate::model::Sample;
use crate::ui::Shared;
use crate::ui::palette::Palette;
use crate::ui::section::{active, section};

#[derive(Clone, PartialEq, Default)]
pub struct DiskViewProps {
    pub sample: Shared<Sample>,
    pub palette: Palette,
    /// Rows available for the whole tab.
    pub height: u16,
}

pub struct DiskView;

impl Component for DiskView {
    type Props = DiskViewProps;

    fn render(props: &DiskViewProps, _hooks: &mut Hooks) -> Element {
        let palette = &props.palette;
        let sample = &props.sample;

        // Devices that have never moved a byte are noise: a machine has
        // dozens of loop devices.
        let disks = sample.disks.as_deref().map(active);

        // One section, so it gets the whole pane. `section` counts its own
        // chrome against the budget, so a pane too short to hold a heading
        // and a device shows nothing rather than overflowing.
        let children = section(
            "Disks",
            "READ",
            "WRITE",
            disks.as_deref(),
            props.height as usize,
            palette,
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
}
