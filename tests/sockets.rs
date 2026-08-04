use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use rtop::model::{Exposure, Protocol};
use rtop::sample::sockets;

const TCP: &str = include_str!("fixtures/net_tcp.txt");
const TCP6: &str = include_str!("fixtures/net_tcp6.txt");
const UDP: &str = include_str!("fixtures/net_udp.txt");
const UDP6: &str = include_str!("fixtures/net_udp6.txt");

fn ports(sockets: &[rtop::model::Socket]) -> Vec<u16> {
    sockets.iter().map(|s| s.local.port()).collect()
}

// ---------- what counts as listening ----------

#[test]
fn keeps_only_tcp_sockets_in_the_listen_state() {
    // The fixture has two non-listening rows: an established connection and
    // a SYN_SENT. Neither is something the machine is offering.
    let listening = sockets::parse(TCP, Protocol::Tcp);

    assert_eq!(ports(&listening), vec![39697, 5434, 80, 65535]);
}

#[test]
fn keeps_only_udp_sockets_with_no_connected_peer() {
    // UDP has no LISTEN state; a socket with no remote address is the
    // equivalent. The fixture's third row is a connected DHCP exchange.
    let listening = sockets::parse(UDP, Protocol::Udp);

    assert_eq!(ports(&listening), vec![36218, 5353]);
}

#[test]
fn skips_a_malformed_line_without_losing_the_rest() {
    // The fixture ends with a line that is not a socket record at all.
    assert_eq!(sockets::parse(TCP, Protocol::Tcp).len(), 4);
}

#[test]
fn an_empty_file_yields_no_sockets() {
    assert!(sockets::parse("", Protocol::Tcp).is_empty());
    assert!(sockets::parse("  sl  local_address\n", Protocol::Tcp).is_empty());
}

// ---------- address decoding ----------

#[test]
fn decodes_ipv4_from_little_endian_hex() {
    // 0100007F is 127.0.0.1, not 1.0.0.127.
    let listening = sockets::parse(TCP, Protocol::Tcp);

    assert_eq!(
        listening[0].local.ip(),
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
    );
    assert_eq!(listening[1].local.ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    assert_eq!(
        listening[2].local.ip(),
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))
    );
}

#[test]
fn decodes_ipv6_as_four_little_endian_words() {
    // The classic way to get this wrong is reversing all sixteen bytes.
    // `::1` is stored as 00000000000000000000000001000000 — the last word
    // byte-swapped, not the whole address.
    let listening = sockets::parse(TCP6, Protocol::Tcp6);

    assert_eq!(listening[0].local.ip(), IpAddr::V6(Ipv6Addr::UNSPECIFIED));
    assert_eq!(listening[1].local.ip(), IpAddr::V6(Ipv6Addr::LOCALHOST));
    assert_eq!(
        listening[2].local.ip(),
        IpAddr::V6("2001:db8::1".parse().unwrap())
    );
}

#[test]
fn decodes_a_port_above_the_single_byte_range() {
    // A byte-order mistake in the port is invisible below 256.
    let listening = sockets::parse(TCP, Protocol::Tcp);

    assert_eq!(listening[0].local.port(), 39697);
    assert_eq!(listening[3].local.port(), 65535);
}

#[test]
fn reads_the_owning_uid_and_inode() {
    let listening = sockets::parse(TCP, Protocol::Tcp);

    assert_eq!(listening[0].uid, 0);
    assert_eq!(listening[0].inode, 29032);
    assert_eq!(listening[2].uid, 1000);
    assert_eq!(listening[3].uid, 33);
}

#[test]
fn records_the_protocol_it_was_told_to_parse() {
    assert!(
        sockets::parse(TCP6, Protocol::Tcp6)
            .iter()
            .all(|s| s.protocol == Protocol::Tcp6)
    );
    assert!(
        sockets::parse(UDP6, Protocol::Udp6)
            .iter()
            .all(|s| s.protocol == Protocol::Udp6)
    );
}

// ---------- accept queue ----------

#[test]
fn reads_the_accept_queue_from_a_tcp_listener() {
    // rx_queue on a LISTEN socket is the count of connections established
    // and waiting to be accepted.
    let listening = sockets::parse(TCP, Protocol::Tcp);

    assert_eq!(listening[0].accept_queue, Some(0));
    assert_eq!(listening[2].accept_queue, Some(7));
}

#[test]
fn reports_no_accept_queue_for_udp() {
    // UDP has no accept queue at all, which is different from having an
    // empty one.
    let listening = sockets::parse(UDP, Protocol::Udp);

    assert!(listening.iter().all(|s| s.accept_queue.is_none()));
}

// ---------- exposure ----------

#[test]
fn classifies_a_wildcard_bind_as_exposed() {
    for addr in ["0.0.0.0:80".parse().unwrap(), "[::]:80".parse().unwrap()] {
        assert_eq!(Exposure::of(&addr), Exposure::Exposed, "{addr}");
    }
}

#[test]
fn classifies_loopback_as_local_only() {
    for addr in [
        "127.0.0.1:80".parse().unwrap(),
        "127.0.0.53:53".parse().unwrap(),
        "[::1]:80".parse().unwrap(),
    ] {
        assert_eq!(Exposure::of(&addr), Exposure::Loopback, "{addr}");
    }
}

#[test]
fn classifies_a_specific_address_as_bound_to_one_interface() {
    for addr in [
        "10.0.0.5:80".parse().unwrap(),
        "[2001:db8::1]:80".parse().unwrap(),
    ] {
        assert_eq!(Exposure::of(&addr), Exposure::Interface, "{addr}");
    }
}

#[test]
fn orders_exposed_sockets_before_loopback_ones() {
    // The attack surface should be the first thing on screen, not something
    // to hunt for.
    let mut all = sockets::parse(TCP, Protocol::Tcp);
    all.extend(sockets::parse(TCP6, Protocol::Tcp6));
    sockets::sort(&mut all);

    let exposures: Vec<Exposure> = all.iter().map(|s| s.exposure()).collect();
    let mut sorted = exposures.clone();
    sorted.sort();
    assert_eq!(
        exposures, sorted,
        "exposed first, then interface, then loopback"
    );
}

#[test]
fn orders_by_port_within_an_exposure_class() {
    let mut all = sockets::parse(TCP, Protocol::Tcp);
    sockets::sort(&mut all);

    let exposed: Vec<u16> = all
        .iter()
        .filter(|s| s.exposure() == Exposure::Exposed)
        .map(|s| s.local.port())
        .collect();
    let mut sorted = exposed.clone();
    sorted.sort();
    assert_eq!(exposed, sorted);
}
