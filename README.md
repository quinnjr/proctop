# rtop

An htop-inspired Linux system monitor, built on
[ntui](https://github.com/quinnjr/ntui).

Two goals, equally weighted: a monitor worth using daily, and a demanding
dogfood harness for ntui. A process table re-renders several hundred rows
twice a second under a hard CPU budget — that pressure finds API and
performance gaps a counter example never will. Every one it finds is
recorded in [the design doc](docs/superpowers/specs/2026-08-04-rtop-design.md).

Linux only. rtop reads `/proc` and `/sys` directly, with no portability
layer.

## Status

Phases 1 and 2 of [the plan](docs/superpowers/specs/2026-08-04-rtop-design.md#9-implementation-phases)
are done: rtop samples the machine and renders meters plus a sortable,
scrollable process table. Search, kill/renice, the detail pane, the I/O and
sensors tabs, and config files are not built yet.

```
cargo run --release
```

## Keys

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | Move the selection |
| `g` / `G`, `Home` / `End` | First / last row |
| `C-d` / `C-u` | Half page down / up |
| `PgDn` / `PgUp` | Full page down / up |
| `<` / `>` | Previous / next sort column |
| `I` | Reverse the sort direction |
| `q`, `Esc` | Quit |

## What it measures

Per-core CPU with the segments broken out, memory and swap, load average,
uptime, task and thread counts, and per-process PID / user / priority /
nice / virtual and resident memory / state / CPU% / MEM% / accumulated CPU
time / command.

Two corrections in the accounting are worth knowing about, because a naive
subtract-and-divide gets both wrong:

- **Guest time is subtracted out of `user` and `nice` before deltas.** The
  kernel counts it in both places, which double-counts the interval on a
  virtualization host and makes every percentage come out too small.
- **Processes are identified by `(pid, starttime)`, never by PID alone.**
  Linux recycles PIDs; keying on the bare number attributes a dead
  process's accumulated CPU time to whatever inherits it, producing
  impossible percentages.

## Cost

Sampling runs on `spawn_blocking`, never on the render thread. Measured on
2026-08-04 with 782 processes, release build: **6.0 ms per sample, a 0.40%
duty cycle** at the default 1500 ms refresh.

Re-measure after touching the sampler:

```
cargo run --release --example bench
```

## Tests

```
cargo test
```

Parsers are tested against real `/proc` contents captured into
`tests/fixtures/`, so the suite is deterministic and covers cases that are
awkward to produce live — a process named `weird)name`, a kernel that stops
before the guest columns, a machine with no swap, a counter mid-wraparound.
Only `tests/sampler.rs` and `tests/app.rs` touch the running machine, and
they assert on wiring rather than on any particular figure.

## License

MIT OR Apache-2.0.
