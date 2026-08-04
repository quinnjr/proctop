//! `/proc/net/{tcp,tcp6,udp,udp6}` — sockets the machine is listening on.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::model::{Protocol, Socket};

/// TCP state `0A` is `TCP_LISTEN`.
const TCP_LISTEN: &str = "0A";

/// Column offsets within a `/proc/net/*` record.
mod field {
    pub const LOCAL: usize = 1;
    pub const REMOTE: usize = 2;
    pub const STATE: usize = 3;
    /// `tx_queue:rx_queue`; on a listener the second half is the accept
    /// queue depth.
    pub const QUEUES: usize = 4;
    pub const UID: usize = 7;
    pub const INODE: usize = 9;
    pub const REQUIRED: usize = INODE + 1;
}

/// Parse the sockets this machine is listening on out of one `/proc/net`
/// file.
///
/// Connected sockets are excluded: for TCP that means anything not in
/// `TCP_LISTEN`, and for UDP — which has no listening state — anything with
/// a peer address. A UDP socket bound with no peer is how a DNS resolver or
/// a DHCP client appears, and omitting those would make the view wrong
/// about what is bound.
///
/// The header line and any malformed record are skipped; the rest of the
/// file is kept.
pub fn parse(text: &str, protocol: Protocol) -> Vec<Socket> {
    text.lines()
        .filter_map(|line| parse_line(line, protocol))
        .collect()
}

fn parse_line(line: &str, protocol: Protocol) -> Option<Socket> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < field::REQUIRED {
        return None;
    }
    // The header's first column is `sl`, not `<n>:`.
    if !fields[0].ends_with(':') {
        return None;
    }

    let local = parse_addr(fields[field::LOCAL], protocol)?;
    let remote = parse_addr(fields[field::REMOTE], protocol)?;

    let accept_queue = if protocol.is_tcp() {
        if fields[field::STATE] != TCP_LISTEN {
            return None;
        }
        // `tx_queue:rx_queue` — the accept queue is the second half.
        let (_, rx) = fields[field::QUEUES].split_once(':')?;
        Some(u32::from_str_radix(rx, 16).ok()?)
    } else {
        // A UDP socket with a peer is connected, not listening.
        if remote.port() != 0 || !remote.ip().is_unspecified() {
            return None;
        }
        None
    };

    Some(Socket {
        protocol,
        local,
        uid: fields[field::UID].parse().ok()?,
        inode: fields[field::INODE].parse().ok()?,
        accept_queue,
    })
}

/// Decode an `<address>:<port>` pair.
///
/// The port is big-endian hex. The address is not: IPv4 is a little-endian
/// `u32`, and IPv6 is four little-endian `u32` words rather than one
/// 128-bit little-endian value — reversing all sixteen bytes gives the
/// wrong address, and is the usual way to get this wrong.
fn parse_addr(field: &str, protocol: Protocol) -> Option<SocketAddr> {
    let (addr, port) = field.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;

    let ip = if protocol.is_v6() {
        if addr.len() != 32 {
            return None;
        }
        let mut octets = [0u8; 16];
        for (word, chunk) in addr.as_bytes().chunks(8).enumerate() {
            let value = u32::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
            octets[word * 4..word * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        IpAddr::V6(Ipv6Addr::from(octets))
    } else {
        if addr.len() != 8 {
            return None;
        }
        IpAddr::V4(Ipv4Addr::from(
            u32::from_str_radix(addr, 16).ok()?.to_le_bytes(),
        ))
    };

    Some(SocketAddr::new(ip, port))
}

/// Order sockets so the attack surface reads first: wildcard binds, then
/// interface-specific ones, then loopback — and by port within each.
pub fn sort(sockets: &mut [Socket]) {
    sockets.sort_by(|a, b| {
        a.exposure()
            .cmp(&b.exposure())
            .then_with(|| a.local.port().cmp(&b.local.port()))
            .then_with(|| a.protocol.cmp(&b.protocol))
    });
}
