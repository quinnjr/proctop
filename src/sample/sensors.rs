//! `/sys/class/hwmon` and `/sys/class/power_supply`.
//!
//! hwmon is the least uniform interface rtop reads. Chips expose whichever
//! of `tempN`/`fanN` they feel like, indices are sparse rather than
//! sequential, labels are optional, and every reading is surrounded by
//! `_max`/`_crit`/`_crit_alarm` files that look just like it. So the
//! directory contents are passed in rather than read here, which keeps all
//! of that testable against literal fixtures instead of against whatever
//! hardware happens to be running the tests.

use crate::model::{Sensor, SensorKind};

/// Build the readings for one hwmon chip from its directory contents.
///
/// `files` is `(file name, contents)` for everything in the chip's
/// directory. Only `*_input` files are readings; the thresholds beside them
/// are ignored, or the list comes back three times too long and full of
/// fictional sensors.
///
/// Ordered by index numerically. Sorted as text, `temp10` lands between
/// `temp1` and `temp2` and a core listing reads as scrambled.
pub fn parse_hwmon(chip: &str, files: &[(String, String)]) -> Vec<Sensor> {
    let mut readings: Vec<(SensorKind, u32, Sensor)> = files
        .iter()
        .filter_map(|(name, contents)| {
            let (kind, index) = classify(name)?;
            let raw: f32 = contents.trim().parse().ok()?;

            // Built once per reading, not once per file examined: this
            // search is linear over `files`, and a chip directory holds
            // scores of them.
            let stem = format!("{}{index}", kind.prefix());
            let label_file = format!("{stem}_label");
            let label = files
                .iter()
                .find(|(n, _)| *n == label_file)
                .map(|(_, l)| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .unwrap_or(stem);

            Some((
                kind,
                index,
                Sensor {
                    chip: chip.to_string(),
                    label,
                    kind,
                    value: kind.scale(raw),
                },
            ))
        })
        .collect();

    readings.sort_by_key(|(kind, index, _)| (kind.prefix(), *index));
    readings.into_iter().map(|(_, _, sensor)| sensor).collect()
}

/// Build a reading from one `/sys/class/power_supply` entry.
pub fn parse_battery(name: &str, capacity: &str, status: &str) -> Option<Sensor> {
    let percent: f32 = capacity.trim().parse().ok()?;
    Some(Sensor {
        chip: name.to_string(),
        label: status.trim().to_string(),
        kind: SensorKind::Battery,
        value: percent,
    })
}

/// Recognize `temp3_input` / `fan1_input` and nothing else.
fn classify(file: &str) -> Option<(SensorKind, u32)> {
    let stem = file.strip_suffix("_input")?;
    for kind in [SensorKind::Temperature, SensorKind::Fan] {
        if let Some(index) = stem.strip_prefix(kind.prefix())
            && let Ok(index) = index.parse()
        {
            return Some((kind, index));
        }
    }
    None
}

impl SensorKind {
    fn prefix(self) -> &'static str {
        match self {
            SensorKind::Temperature => "temp",
            SensorKind::Fan => "fan",
            SensorKind::Battery => "battery",
        }
    }

    /// hwmon reports temperatures in millidegrees; fan RPM and battery
    /// percent are already in their display units.
    fn scale(self, raw: f32) -> f32 {
        match self {
            SensorKind::Temperature => raw / 1000.0,
            SensorKind::Fan | SensorKind::Battery => raw,
        }
    }
}
