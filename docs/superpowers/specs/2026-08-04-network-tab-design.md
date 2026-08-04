# Network tab — Design

**Date:** 2026-08-04
**Status:** Approved, implementing

## Summary

rtop gains a **Network** tab holding both halves of the network story: the
per-interface throughput that currently lives in the I/O tab, and a list of
**listening sockets** — what is bound on this machine, who owns it, and
whether it is reachable from outside.

The I/O tab loses its network half and becomes **Disk**.

## Why listening sockets

The process table answers "what is running". The throughput rows answer
"how fast". Neither answers "what is this machine exposing", which is the
question you currently drop to `ss -tlnp` to answer. It is also the one
with a security dimension: a service bound to `0.0.0.0` is reachable from
the network, and one bound to `127.0.0.1` is not, and the difference is
invisible in every other view rtop has.

Established connections are deliberately **out of scope**. There are ~690
of them on the machine this was designed on against ~50 listeners, they
churn constantly, and the interesting ones (a process holding a thousand
sockets) are better seen as a per-process count than as a thousand rows.
That is a separate feature if it is ever wanted.

## 1. Tab layout

Four tabs: `Processes │ Disk │ Network │ Sensors`, selected by `1`–`4`.

`Disk` is the old `Io` view with the network section removed — the split
helper, the row budget and the "unavailable" versus "idle" distinction all
survive unchanged, with one section instead of two.

`Network` stacks two sections:

```
Interfaces
DEVICE                 RX         TX
eno1                11K/s     4.1K/s [||||                ]

Listening
PROTO LOCAL ADDRESS              PORT   Q USER      PROCESS
tcp   0.0.0.0                    5434   0 root      postgres
tcp6  ::                         4200   0 joseph    node
tcp   127.0.0.1                 39697   0 root      —
udp   0.0.0.0                     5353   - avahi     avahi-daemon
```

The interfaces section is the throughput block moved verbatim; the
listening section is new. The row budget is split between them by the same
demand-following `split` the Disk tab uses, so a machine with one interface
and forty listeners does not waste half the pane.

## 2. What counts as listening

**TCP:** state `0A` (`TCP_LISTEN`).

**UDP:** there is no listening state. A UDP socket with no connected peer —
`rem_address` all zeros — is the equivalent, and is how a DNS resolver, an
mDNS responder or a DHCP client appears. Omitting them would make the tab
quietly wrong about what is bound.

Both IPv4 and IPv6 files are read: `/proc/net/tcp`, `tcp6`, `udp`, `udp6`.

## 3. Exposure is the point

The most useful fact in the view is whether a port is reachable from
outside the machine, so it drives both colour and order.

| Local address | Meaning | Colour |
|---|---|---|
| `0.0.0.0`, `::` | reachable from any interface | `warn` |
| `127.0.0.0/8`, `::1` | loopback only | `muted` |
| anything else | bound to one interface | `text` |

Default sort is **exposure first, then port ascending** — wildcards at the
top — so the attack surface is the first thing on screen rather than
something to hunt for.

## 4. Address parsing

`/proc/net/tcp` stores addresses as hex, and the encoding is the part worth
being careful about:

- **IPv4** is a little-endian `u32`: `0100007F` is `127.0.0.1`.
- **IPv6** is four little-endian `u32` words, *not* one 128-bit
  little-endian value: `::1` is stored as
  `00000000000000000000000001000000`. Reversing the whole 16 bytes gives
  the wrong address, and it is the classic way to get this wrong.
- The port is a big-endian `u16` in hex.

Parsed into `std::net::IpAddr` rather than kept as text, so `Ipv6Addr`'s
own `Display` produces correct `::` compression instead of hand-rolled
formatting that gets it subtly wrong.

## 5. Model

```rust
pub enum Protocol { Tcp, Tcp6, Udp, Udp6 }

pub struct Socket {
    pub protocol: Protocol,
    pub local: SocketAddr,
    pub uid: u32,
    /// Socket inode, the join key to a process via /proc/<pid>/fd.
    pub inode: u64,
    /// For a TCP listener, connections established and waiting to be
    /// accepted. Meaningless for UDP.
    pub accept_queue: u32,
}

pub struct ListeningSocket {
    pub socket: Socket,
    pub user: Arc<str>,
    /// `None` when the socket's owner could not be determined — which is
    /// the normal unprivileged case, not an error.
    pub process: Option<(i32, Arc<str>)>,
}
```

`Socket::exposure()` classifies the local address into the three cases in
§3, and is what both the colour and the sort key come from.

### The accept queue caveat

On a `TCP_LISTEN` socket the kernel documents `rx_queue` as the number of
established connections waiting to be accepted — a genuine "your
application is not accepting fast enough" signal. Every socket on the
machine this was designed on read zero, so **this column has not been
observed to move.** It is included because the reading is free and the
signal is valuable, but it is the one field here that has not been
verified against live non-zero data.

UDP has no accept queue; those rows render `-` rather than `0`.

## 6. Cost, and process attribution

Socket parsing is trivial — ~50 lines of text, no per-process work.

Attribution is not. The only way to map a socket inode to a process is to
walk `/proc/<pid>/fd` and readlink every entry looking for
`socket:[<inode>]`. Measured on the design machine: **12 ms across 190
readable processes and 4966 file descriptors** — over three times the
entire rest of a sample.

So it gets the treatment sensors got, for the same reason:

- built **only while the Network tab is showing**, and
- refreshed at most every 2 seconds even then.

`Sample::sockets` is `Option<Shared<Vec<ListeningSocket>>>`, matching
`sensors`: `None` means "not read" and renders "reading sockets…";
`Some(vec![])` means "read, nothing is listening". They render differently.

There is deliberately no third "unavailable" state. `/proc/net/*` is
guaranteed by procfs on every Linux, so it falls on the same side of the
line as `mem`/`load`/`uptime` rather than with `disks`/`nets`/`sensors`,
which a restricted container can legitimately deny.

### The privilege cliff

Unprivileged, `/proc/<pid>/fd` is readable only for your own processes — 190
of ~800 on the design machine. Every other socket's process is unknowable,
which is exactly what `ss -p` does too.

Rather than leave a column mysteriously blank, the tab reports it: when at
least one row could not be attributed, a footer line reads

> `N sockets owned by other users — run as root to attribute them`

The `USER` column never has this problem: `/proc/net/tcp` carries the
owning uid directly, so it is always correct and costs nothing.

## 7. Errors

A malformed line is skipped; the rest of the file is kept. An unreadable
`/proc/<pid>/fd` drops that process from the map silently — it is the
common case, not a failure. See §6 for why `/proc/net/*` gets no
"unavailable" state.

## 8. Scrolling

Fifty rows fits a normal terminal, but a container host with many bound
services will not. The listening section uses `ListSelection` and the
viewport slice rather than assuming it fits — both are already in `ntui`
and cost almost nothing to wire.

## 9. Module map

| Module | Contents |
|---|---|
| `sample::sockets` | `/proc/net/{tcp,tcp6,udp,udp6}` → `Vec<Socket>`; pure `&str` in |
| `sampler` | reads the four files, builds the inode→pid map, joins them |
| `ui::network` | the Network tab: interfaces section + listening section |
| `ui::io` | becomes Disk-only |

`sample::sockets` follows the existing convention exactly: a `&str`
parser with fixtures, no I/O. The fd walk is I/O and therefore lives in
`sampler`.

## 10. Testing

Fixtures capture real `/proc/net/*` contents covering the cases that are
easy to get wrong:

- a wildcard listener (`0.0.0.0`, `::`)
- a loopback listener (`127.0.0.1`, and `::1` for the word-swap)
- a listener bound to one specific interface address
- a high port, to catch a byte-order mistake that only shows above 255
- a non-`0A` TCP row, which must be excluded
- a connected UDP row, which must be excluded
- a malformed line

Plus: exposure classification and the sort order it drives, `-` versus `0`
for the accept queue, and the footer appearing only when a row is
unattributed. The fd walk gets a live smoke test asserting it finds rtop's
own listening socket if one exists, and does not panic if none does.

## 11. Out of scope

- Established connections (see *Why listening sockets*).
- Per-process bandwidth. `/proc` cannot answer it; it needs packet capture
  via libpcap and `CAP_NET_RAW`, which is a dependency and a privilege
  requirement out of proportion to the value here.
- Unix domain sockets. `/proc/net/unix` is a different shape and answers a
  different question.
