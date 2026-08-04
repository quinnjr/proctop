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

```
cargo run --release
```

Three tabs — processes, I/O, sensors — plus search, a command line, a
detail pane, kill and renice, tree view, a config file, and themes. All
seven phases of [the plan](docs/superpowers/specs/2026-08-04-rtop-design.md#9-implementation-phases)
are built.

## Keys

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | Move the selection |
| `g` / `G`, `Home` / `End` | First / last row |
| `C-d` / `C-u` | Half page down / up |
| `PgDn` / `PgUp` | Full page down / up |
| `Tab`, `1`–`3` | Switch tab |
| `<` / `>` | Previous / next sort column |
| `I` | Reverse the sort direction |
| `t` | Tree view |
| `H` | Hide kernel threads |
| `u` | Filter to the selected process's user |
| `Enter` | Process details |
| `dd` | Kill — asks which signal |
| `n` | Renice |
| `/` | Incremental search |
| `:` | Command line |
| `?` | Help |
| `q` | Quit |
| `Esc` | Close an overlay, else clear the filter, else quit |

`:sort <col>`, `:filter <text>`, `:user <name>`, `:tree`, `:q`.

Column names — `pid`, `name`/`command`, `cpu`, `mem`/`memory`/`res`, `time` —
mean the same thing in the config file, in `--sort`, and in `:sort`.

`dd` rather than `k` for kill is deliberate: `k` is navigation, and binding
a destructive action to a navigation key in a list you are actively
scrolling is a footgun.

## What it measures

**Processes** — per-core CPU with the segments broken out, memory and swap,
load average, uptime, task and thread counts, and per-process PID / user /
priority / nice / virtual and resident memory / state / CPU% / MEM% /
accumulated CPU time / command.

**I/O** — per-device disk and per-interface network throughput, busiest
first, with idle devices hidden.

**Sensors** — temperatures, fan speeds, and battery from `hwmon`.

Two corrections in the accounting are worth knowing about, because a naive
subtract-and-divide gets both wrong:

- **Guest time is subtracted out of `user` and `nice` before deltas.** The
  kernel counts it in both places, which double-counts the interval on a
  virtualization host and makes every percentage come out too small.
- **Processes are identified by `(pid, starttime)`, never by PID alone.**
  Linux recycles PIDs; keying on the bare number attributes a dead
  process's accumulated CPU time to whatever inherits it, producing
  impossible percentages.

## Configuration

`~/.config/rtop/config.toml`, honouring `XDG_CONFIG_HOME`. A missing file
is fine; a *broken* one is fatal and reported, because a setting that
silently does nothing is worse than a refusal to start. Unknown keys are
rejected for the same reason.

```toml
refresh_ms = 1500
theme = "gruvbox"          # default, gruvbox, mono
tree_view = false
hide_kernel_threads = false

[processes]
sort_by = "cpu"            # pid, name, cpu, mem, time
sort_desc = true
```

Flags (`--refresh`, `--sort`, `--theme`, `--tree`, `-H`, `--config`)
override the file for one run and are never written back.
`rtop --show-config-path` prints where it looks.

## Cost

Sampling runs on `spawn_blocking`, never on the render thread. Measured on
2026-08-04 with ~790 processes, release build: **3.5–4.2 ms per sample, a
0.23–0.28% duty cycle** at the default 1500 ms refresh.

The largest single cost used to be reading `/proc/<pid>/status` for every
process just to recover its owner — one of the more expensive files in
procfs, since it makes the kernel walk memory accounting `stat` does not
touch. The owner is the `/proc/<pid>` directory's own uid, so one `stat(2)`
answers it, and dropping ~800 file reads per tick roughly halved the sample.

Sensors are the exception and are handled specially. This machine exposes
82 `hwmon` inputs and reading them costs ~30 ms — several times the whole
rest of a sample — because many are real I/O over SMBus rather than cached
kernel values. Reading them every tick took the duty cycle to 2.5%. They
are now read **only while the sensors tab is showing**, and at most every 2
seconds even then.

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

The keymap is a pure function — `ui::state::handle_key` takes the UI state
and a key and returns the next state plus an effect to perform — so every
mode, overlay, and edge of the process list is tested without a terminal.

`cargo run --release --example frames` prints each screen as plain text,
for checking layout without a terminal.

Deriving the visible rows — clone, filter, sort, nest — is memoized on the
sample identity and the filter/sort settings, so a keystroke that does not
change the list does not re-do that work for several hundred processes.

## License

MIT OR Apache-2.0.
