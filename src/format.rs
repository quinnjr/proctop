//! Formatting for display. Pure string functions, kept out of the
//! components so they can be tested without rendering anything.

use std::time::Duration;

/// The kernel reports process CPU time in jiffies; USER_HZ has been 100 on
/// Linux for every architecture rtop targets.
const USER_HZ: u64 = 100;

/// Render a byte count in the largest unit that keeps it to four characters
/// of value or fewer — `999B`, `1.0K`, `128M`, `1023K`.
///
/// Not *three*: the band from 1000 to 1023 stays in the smaller unit,
/// because 1023K is more informative than 1.0M and the column is budgeted
/// for four either way. What the bound below rules out is the rounding case
/// where a value under 1024 would print as `1024K`.
///
/// The decimal is dropped once the value reaches 10, where it stops adding
/// information: in a table column, `128M` says as much as `128.4M` in one
/// fewer cell.
pub fn bytes(n: u64) -> String {
    // Through exbibytes, so the claim holds for every `u64` rather than
    // only for the range /proc happens to produce.
    const UNITS: [&str; 6] = ["K", "M", "G", "T", "P", "E"];

    if n < 1024 {
        return format!("{n}B");
    }

    let mut value = n as f64 / 1024.0;
    let mut unit = 0;
    // Advances at 1023.5 rather than 1024 because the `{:.0}` branch below
    // rounds: at 1023.6 the smaller unit would print "1024K", four digits.
    while value >= 1023.5 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if value < 10.0 {
        format!("{value:.1}{}", UNITS[unit])
    } else {
        format!("{value:.0}{}", UNITS[unit])
    }
}

/// Render accumulated CPU time, given in jiffies, as htop's TIME+ column.
///
/// Below an hour the hundredths are shown, because that is the resolution
/// that distinguishes short-lived processes. Past an hour they are dropped,
/// which trades them for the hour count.
///
/// The width is not fixed and not bounded: `9:59.99` is 7, `1h00:00` is 7,
/// and `1000h00:00` is 10 — reachable after about six weeks of one saturated
/// core. The column pads to alignment and, being numeric, truncates from the
/// left, so an extreme value loses its high-order hours rather than its
/// shape.
pub fn cpu_time(jiffies: u64) -> String {
    let total_seconds = jiffies / USER_HZ;
    let hundredths = jiffies % USER_HZ;

    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}h{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}.{hundredths:02}")
    }
}

/// Render a meter bar of exactly `width` cells, filled in proportion to
/// `fraction`.
///
/// The width is exact because the bar sits in a fixed-width column and a bar
/// one cell too long reflows the whole meter row. Values outside `0.0..=1.0`
/// — including NaN, and a process using more than one core — are clamped
/// rather than overflowing.
pub fn bar(fraction: f32, width: usize) -> String {
    let fraction = if fraction.is_nan() {
        0.0
    } else {
        fraction.clamp(0.0, 1.0)
    };
    let filled = ((fraction * width as f32).round() as usize).min(width);

    let mut s = String::with_capacity(width);
    s.extend(std::iter::repeat_n('|', filled));
    s.extend(std::iter::repeat_n(' ', width - filled));
    s
}

/// Render a machine uptime as `[N day(s), ]HH:MM:SS`.
pub fn uptime(d: Duration) -> String {
    let total = d.as_secs();
    let days = total / 86_400;
    let hours = (total % 86_400) / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;

    let clock = format!("{hours:02}:{minutes:02}:{seconds:02}");
    match days {
        0 => clock,
        1 => format!("1 day, {clock}"),
        n => format!("{n} days, {clock}"),
    }
}
