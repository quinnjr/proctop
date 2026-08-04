# rtop — Design

**Date:** 2026-08-04
**Status:** Approved, ready for implementation planning

## Summary

`rtop` is an htop-inspired Linux system monitor written in Rust, built on
[`ntui`](https://github.com/quinnjr/ntui) — an Ink-style TUI library with
components, hooks, and flexbox layout.

The project has two equal goals:

1. **A daily-driver system monitor.** Something that replaces htop on this
   machine, not a demo.
2. **A demanding dogfood harness for `ntui`.** A monitor re-renders a
   500-row table twice a second under a hard CPU budget. That pressure
   exposes API and performance gaps that a counter example never will.

Where those goals conflict, rtop ships the thing that works and files the
`ntui` gap as an upstream issue. rtop does not contort itself to avoid
finding problems, and it does not block on upstream fixes.

## Scope

**In scope for v1:**

- CPU / memory / swap meters, load average, uptime, task counts
- Sortable, filterable, scrollable process table
- Per-process detail pane (cmdline, threads, FDs, cgroup, I/O)
- Disk and network throughput tab
- Sensors tab (thermal, fan, battery)
- Kill and renice
- TOML config file and bundled themes

**Out of scope:**

- Non-Linux platforms. rtop reads `/proc` and `/sys` directly and makes no
  attempt at portability. This is a deliberate trade: full control and no
  abstraction tax, at the cost of macOS and BSD.
- htop's F-key legacy bindings as the primary interaction model (see
  *Keybindings*). They are aliases, not the design center.
- htop's interactive setup screen. Configuration is a file.

## 1. Crate layout

A single crate, split by module rather than by crate boundary:

| Module | Responsibility |
|---|---|
| `model` | Shared types: `Proc`, `Sample`, `CpuTimes`, `MemInfo`, `LoadAvg` |
| `sample::*` | Parsers, one per `/proc` file — pure `&str` in, types out |
| `sampler` | The only module that performs I/O; walks `/proc` and feeds the parsers |
| `delta` | Rate derivation from consecutive readings |
| `sort`, `format` | Pure comparators and display formatting |
| `ui::*` | ntui components, keybindings, layout |

The important boundary is not between crates but between *parsing* and
*reading*. Process accounting is where the subtle bugs live — jiffy diffs
against a moving clock, PID reuse, counter wraparound, units that differ
per field — and it must be testable without a terminal or a live machine.
Every parser therefore takes a `&str` rather than a path, so the file read
is a separate, thin layer confined to `sampler`.

A two-crate workspace was considered and rejected as ceremony: the
`&str`-in parser convention already buys the testability, and nothing else
in the split was carrying weight.

## 2. Sampling architecture

### Data flow

```
use_interval(refresh_ms)
      │
      ▼
spawn_blocking(Sampler::sample)   ← blocking /proc syscalls, off the render thread
      │
      ▼
Sampler diffs prev ↔ current, computes rates
      │
      ▼
State<Arc<Sample>>                ← Send handle, written from the blocking task
      │
      ▼
ntui re-render                    ← pointer clone, not a deep copy
```

### Key decisions

**Blocking reads never touch the render thread.** `procfs` performs
synchronous file I/O across hundreds of files per tick. Every sample runs
inside `tokio::task::spawn_blocking`, driven by `Hooks::use_interval` at
the configured refresh period (default 1500 ms, matching htop).

**Rates are computed in core, not in the UI.** A raw `/proc` read yields
monotonic counters — jiffies, sectors, bytes. `Sampler` retains the
previous `Snapshot` and emits a `Sample` in which CPU percentages, disk
throughput, and network throughput are already derived. The UI layer
formats numbers and never performs accounting arithmetic. This keeps the
math in the crate that has tests for it.

**Process identity is `(pid, starttime)`, never `pid` alone.** Linux
recycles PIDs. Keying deltas on the bare PID silently attributes a dead
process's accumulated CPU time to whatever new process inherits its number,
producing impossible percentages. `starttime` from `/proc/[pid]/stat`
disambiguates.

**Counter wraparound and negative deltas are clamped, not trusted.** A
delta that comes back negative means a counter reset, a process
substitution, or a suspended machine. Core emits zero for that field on
that tick rather than propagating a nonsense rate.

**`Arc<Sample>` in state.** A re-render clones a pointer. A 500-process
snapshot is never deep-copied on the render path.

### Sampling module map

| Module | Contents |
|---|---|
| `model` | `Sample`, `Proc`, `ProcRow`, `ProcKey`, `CpuTimes`, `MemInfo`, `LoadAvg` |
| `sample::cpu` | `/proc/stat` → per-core and aggregate jiffy counters |
| `sample::memory` | `/proc/meminfo` → total/free/buffers/cached/swap |
| `sample::process` | `/proc/[pid]/{stat,status}` → `Proc` |
| `sample::system` | `/proc/loadavg`, `/proc/uptime` |
| `sample::users` | `/etc/passwd` → uid to username |
| `sample::disk` | `/proc/diskstats` → per-device sector counters *(phase 5)* |
| `sample::net` | `/proc/net/dev` → per-interface byte counters *(phase 5)* |
| `sample::sensors` | `/sys/class/hwmon`, `/sys/class/power_supply` *(phase 6)* |
| `delta` | `(prev, cur) → Sample`; all rate derivation |
| `sort` | Pure comparators, one per sortable column |
| `filter` | Substring, user, and state predicates *(phase 3)* |
| `tree` | Parent/child forest construction from PPID *(phase 3)* |
| `actions` | `kill(pid, signal)`, `renice(pid, nice)` *(phase 3)* |

Each `sample::*` module parses from a `&str`, not from a path. The reader
that supplies the string lives in `sampler` and is exercised separately.
This is what makes fixture-driven testing possible.

## 3. The process table

### The finding

`ntui`'s built-in `Table` widget (`ntui/src/widgets/table.rs`) is
documented as "a static (non-focusable) table". It accepts
`headers: Vec<String>` and `rows: Vec<Vec<String>>`, sizes every column to
its widest cell by scanning all rows, and applies uniform styling.

That shape cannot express what a process table needs:

- **No selection.** There is no selected row, no focus, no cursor.
- **No per-cell style.** A hot process cannot render its CPU cell in
  `danger`; a kernel thread cannot render dimmed.
- **No virtualization.** Column sizing scans every row, and every row is
  laid out whether or not it is on screen.
- **Owned `String` per cell.** At 500 processes × ~12 columns × 2 Hz, that
  is roughly 12,000 allocations per second spent on rows that are mostly
  offscreen.

### The decision

rtop implements its own `ProcessTable` component from `ntui`'s `View` and
`Text` primitives. It:

- slices the sorted, filtered process list **to the viewport before
  building any row**, so allocation is proportional to visible rows, not to
  process count;
- owns selection state and the highlight;
- colors cells by value (CPU/memory thresholds, kernel-thread dimming,
  state-dependent coloring).

Considered and rejected: extending `ntui::widgets::Table` with selection,
per-cell style, and virtualization *now*. Generalizing an API from a single
consumer, before that consumer's own requirements have settled, tends to
produce the wrong abstraction. rtop builds it locally first.

### Upstream feedback loop

Every `ntui` limitation rtop works around is filed as an issue on
`quinnjr/ntui`, with the rtop workaround referenced. Found so far, in
descending order of how much they cost:

1. **`props_eq` deep-compares large payloads.** ntui decides whether to
   re-render a subtree by comparing props with `PartialEq`, and
   `Arc<T>`'s own `PartialEq` compares the pointees. Passing the process
   list down as `Arc<Vec<ProcRow>>` therefore deep-compares several hundred
   processes every frame — more expensive than the render it is trying to
   skip. rtop works around it with `ui::Shared<T>`, a newtype whose
   `PartialEq` is `Arc::ptr_eq`. ntui may want either an identity-comparing
   wrapper of its own or a way for a component to opt out of `props_eq`.
2. **Selectable / focusable table** with a controlled selected index.
3. **Virtualized list or table** that lays out only the visible window.
4. **Per-cell (or row-callback) styling.**
5. **`TestTerminal::frame_text` carries no styling.** The frame comes back
   as plain text, so nothing about color, weight, or background can be
   asserted — which means the selected-row highlight, the sort-column
   marker, and the CPU threshold colors are all untestable through the
   harness. A styled-cell accessor (or a snapshot format that encodes
   attributes) would close this.
6. **A component's `render` cannot be called from a test**, because
   `Hooks` is not constructible outside the crate. rtop splits
   `ProcessTable::build(&props) -> Element` out from `Component::render` so
   its output tree can be inspected directly — which is the only way to
   assert that offscreen rows are never *built*, as opposed to built and
   then clipped. Something like a `Hooks::detached()` for hook-free
   components would remove the need for the split.
7. **A `use_list_selection`-style hook**: cursor, clamping, page movement.
8. **`Theme` extension or escape hatch** — the built-in `Theme` carries
   eight tokens (`accent`, `surface`, `border`, `muted`, `foreground`,
   `danger`, `success`, `border_style`), which is not enough for htop-style
   meters needing distinct colors for user / system / nice / irq / iowait
   segments at once. rtop carries its own palette in `ui::theme`.

Once a workaround has proven its shape in rtop, it is a candidate to
promote into `ntui`. Promotion is a separate decision made per item, not an
obligation.

## 4. UI structure

```
App
├── Header          CpuMeters · MemMeter · SwapMeter · Load/Uptime · TaskCounts
├── TabBar          Processes │ I/O │ Sensors
├── Body
│   ├── ProcessView   ProcessTable + DetailPane (toggle)
│   ├── IoView        DiskTable + NetTable + sparklines
│   └── SensorsView   thermal / fan / battery
├── StatusBar       mode · active filter · sort column
└── Overlays        Help · Kill confirm · Renice · ThemePicker
```

Overlays use `ViewProps::overlay` with an `Anchor`, the same mechanism
`ntui`'s `Modal` uses. Overlays do not nest — `ntui` does not support it and
`debug_assert!`s on the attempt — so at most one is open at a time, which
matches the interaction model anyway.

The active tab, selection index, filter string, and sort key live in `App`
state and are passed down as props. Sampling state (`Arc<Sample>`) is
provided via context so meters and tables read it without prop-drilling
through every layer.

## 5. Keybindings

Vim-first and discoverable. htop's F-keys are bound as aliases for muscle
memory, but they are not the design center.

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | Move selection |
| `g` / `G`, `Home` / `End` | Top / bottom |
| `C-d` / `C-u` | Half-page down / up |
| `PgDn` / `PgUp` | Full page down / up |
| `I` | Reverse the sort direction |
| `/` | Incremental filter; `n` / `N` cycle matches |
| `:` | Command line — `:sort cpu`, `:tree`, `:kill`, `:q` |
| `dd` | Kill selected process (confirmation modal) |
| `<` / `>` | Previous / next sort column |
| `t` | Toggle tree view |
| `H` | Toggle thread visibility |
| `u` | Filter by user |
| `Enter` | Toggle detail pane |
| `1`–`3`, `Tab` | Switch tab |
| `?` | Help overlay |
| `q`, `Esc` | Quit / close overlay |

`dd` rather than `k` for kill is deliberate: `k` is navigation, and binding
a destructive action to a navigation key in a list you are actively
scrolling is a footgun. `dd` reads as vim's delete and requires two
keystrokes plus a confirmation. Half-page movement is `C-d`/`C-u` rather
than bare `d`/`u` for the same reason — `d` belongs to `dd`, and `u` to
filter-by-user.

`g` is bound directly rather than as `gg`, since there is no other `g`
prefix yet to disambiguate against. It becomes a pending-prefix state if
one appears.

## 6. Error handling

**`/proc` failures are normal, not exceptional.** A process exiting between
directory enumeration and file read yields `ENOENT` on every tick of a busy
system. Treating that as an error condition produces an unusable program.

| Failure | Behavior |
|---|---|
| Per-process read error (`ENOENT`, `EACCES`) | Process is dropped from this sample. No log, no user-visible error. |
| Per-subsystem unavailable (no `hwmon`, no permission for I/O counters) | That view renders an explanatory "unavailable" state. The rest of the app is unaffected. |
| Malformed line in a `/proc` file | Field is skipped; the rest of the record is kept. |
| Config parse error | Fatal, with a message naming the file and the problem. Silently ignoring a broken config is worse than refusing to start. |
| Terminal init failure | Fatal. |

**No `unwrap` or `expect` in the sampling path.** A panic in a sampler
running on a `spawn_blocking` task is particularly bad here: `ntui`'s
`use_future` documents that panics inside spawned tasks are swallowed and
the task is silently aborted, so a panicking sampler manifests as a UI that
quietly stops updating with no error anywhere. Sampling errors are values,
and the sampler task must be panic-free by construction.

## 7. Testing

**Parsers — fixture-driven.** Real `/proc` file contents are captured and
checked into `tests/fixtures/`. Parsers are tested against those strings,
so tests are deterministic, run on any machine, and can cover cases that
are hard to produce live (a process named `weird)name`, a kernel that
stops before the guest columns, a machine with no swap, a counter
mid-wraparound).

**Delta math — hand-computed expectations.** Two fixture snapshots with
known timestamps and known jiffy counts, asserted against percentages
worked out by hand. This is the layer most likely to harbor a silent
correctness bug, and it gets the most direct tests.

**Sort and filter — property tests.** Sorting is a total order; filtering
is a subset of the input; neither loses or duplicates a process.

**Components — rendered via `TestTerminal`.** `ntui` ships a headless test
harness. Each component renders at fixed terminal sizes against a fixed
`Sample` fixture and its text output is asserted. Where the property is
structural rather than visual — offscreen rows never being built — the
test inspects the `Element` tree from `ProcessTable::build` instead, since
a rendered frame cannot distinguish "not built" from "built then clipped".

**Live-system tests are confined to two files**, `tests/sampler.rs` and
`tests/app.rs`, both `#![cfg(target_os = "linux")]`. They assert on wiring
rather than on any particular figure: that the sampler finds the process
doing the sampling, that percentages land in range, that the app mounts and
quits. Sampling runs on `spawn_blocking`, so whether it has landed after a
given tick is a race; these tests poll rather than assume.

**Performance budget.** `examples/bench.rs` times a full sample so the
figure can be re-measured whenever the sampler changes. Targets:

- Under 1% of one core, averaged, at a 1500 ms refresh with 500 processes
- Under 5 ms per frame render

**Measured on 2026-08-04**, 782 processes, release build: **6.0 ms per
sample, a 0.40% duty cycle** at the 1500 ms refresh. Within budget with
room to spare. These are targets to measure against, not assertions that
fail CI on a noisy machine.

## 8. Configuration

`~/.config/rtop/config.toml`, following the XDG base directory spec. A
missing file is not an error — defaults apply and nothing is written.

```toml
refresh_ms = 1500
theme = "gruvbox"
show_threads = false
tree_view = false

[processes]
columns = ["pid", "user", "cpu", "mem", "state", "time", "command"]
sort_by = "cpu"
sort_desc = true
```

Themes are bundled TOML color maps compiled into the binary, deserialized
into `ntui::widgets::Theme` plus rtop's extended meter palette, and
provided to the tree via `ContextProvider`. A user theme file in
`~/.config/rtop/themes/` overrides a bundled one of the same name.

CLI flags (`--refresh`, `--sort`, `--theme`) override config values for a
single run and never write back.

## 9. Implementation phases

Each phase is independently verifiable; phase 2 produces a program worth
using.

| # | Phase | Done when |
|---|---|---|
| 1 | ✅ Sampler, `Sample`, deltas | Fixture tests pass; benches run. No UI exists. |
| 2 | ✅ Header meters + `ProcessTable` + sort + scroll | **rtop is usable as a monitor.** |
| 3 | Search, command line, help overlay, kill, renice | Full process interaction. |
| 4 | Detail pane | Per-process drill-down. |
| 5 | I/O tab | Disk and network throughput with sparklines. |
| 6 | Sensors tab | Thermal, fan, battery. |
| 7 | Config file and themes | Persistence and theming. |

### Risk flag: phase 6

Sensors is the weakest phase and the first candidate to cut. `hwmon`
discovery is genuinely inconsistent across hardware — label files that may
or may not exist, per-driver naming conventions, units that vary by sensor
type — and it is the least htop-like feature in the set. It is sequenced
last so that cutting it costs nothing. Decide at the phase 5/6 boundary
whether it earns its complexity.

## Dependencies

| Crate | Version | Purpose | Phase |
|---|---|---|---|
| `ntui` | 0.2 | TUI framework | 2 |
| `procfs` | 0.18 | Page size, and typed `/proc` access where useful | 1 |
| `tokio` | 1 | Async runtime (required by `ntui`) | 2 |
| `serde` | 1 | Config and theme deserialization | 7 |
| `toml` | 1 | Config format | 7 |
| `clap` | 4 | CLI flags | 7 |

`procfs` earns its place for `page_size()` and for the phase 5–6 readers;
the hot-path parsers are hand-written against `&str` because that is what
makes them fixture-testable. `/sys/class/hwmon` in phase 6 uses `std::fs`
directly — `procfs` does not cover `/sys`.
