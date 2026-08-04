//! `/proc/net/dev` — per-interface cumulative byte and packet counters.

use crate::model::NetStat;

/// Parse the contents of `/proc/net/dev`.
///
/// The file opens with two header lines, and each data line is
/// `name: <8 receive fields> <8 transmit fields>`.
///
/// The name is split on the colon rather than on whitespace: the kernel
/// right-aligns short names into a fixed field but does not pad long ones,
/// so `wlp0s20f3:` has no space before its colon and a whitespace split
/// would swallow the first counter into the name.
pub fn parse_netdev(text: &str) -> Vec<NetStat> {
    text.lines().filter_map(parse_line).collect()
}

/// Offsets within the counter fields that follow the colon.
mod field {
    pub const RX_BYTES: usize = 0;
    pub const RX_PACKETS: usize = 1;
    pub const TX_BYTES: usize = 8;
    pub const TX_PACKETS: usize = 9;
    pub const REQUIRED: usize = TX_PACKETS + 1;
}

fn parse_line(line: &str) -> Option<NetStat> {
    let (name, counters) = line.split_once(':')?;
    let name = name.trim();
    // The second header line contains a colon too ("face |bytes ...").
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }

    let fields: Vec<&str> = counters.split_whitespace().collect();
    if fields.len() < field::REQUIRED {
        return None;
    }
    let num = |i: usize| fields[i].parse().unwrap_or(0);

    Some(NetStat {
        name: name.to_string(),
        rx_bytes: num(field::RX_BYTES),
        rx_packets: num(field::RX_PACKETS),
        tx_bytes: num(field::TX_BYTES),
        tx_packets: num(field::TX_PACKETS),
    })
}
