//! `/proc/loadavg` and `/proc/uptime` — machine-wide summary figures.

use std::time::Duration;

use crate::model::LoadAvg;

/// Parse the contents of `/proc/loadavg`.
///
/// The line is `<1min> <5min> <15min> <running>/<total> <last pid>`.
/// Returns `None` unless at least the three averages and the task counts are
/// present, since a partial load line is not worth displaying.
pub fn parse_loadavg(text: &str) -> Option<LoadAvg> {
    let mut fields = text.split_whitespace();

    let one = fields.next()?.parse().ok()?;
    let five = fields.next()?.parse().ok()?;
    let fifteen = fields.next()?.parse().ok()?;

    let (running, total) = fields.next()?.split_once('/')?;

    Some(LoadAvg {
        one,
        five,
        fifteen,
        running: running.parse().ok()?,
        total: total.parse().ok()?,
    })
}

/// Parse the contents of `/proc/uptime`, whose first field is seconds since
/// boot as a decimal. The second field (aggregate idle time) is not used.
pub fn parse_uptime(text: &str) -> Option<Duration> {
    let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some(Duration::from_secs(seconds as u64))
}
