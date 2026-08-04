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
| `sample::process` | `/proc/[pid]/stat` → `Proc` |
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

### Resolved upstream — 2026-08-04

All eight were addressed in `ntui`, plus a ninth the fixes needed:

| Gap | Shipped as |
|---|---|
| 1. `props_eq` deep-compares | `ntui::Shared<T>` — `Arc` compared by `ptr_eq` |
| 2. Selectable table | `TableProps::selected` |
| 3. Virtualized table | `TableProps::viewport` + `Viewport` |
| 4. Per-cell styling | `TableProps::cell_style`, `CellStyler`, `CellStyle`, `CellContext` |
| 5. `frame_text` carries no styling | `TestTerminal::cell` / `buffer` / `row` |
| 6. `render` uncallable from a test | `testing::render_once::<C>(&props)` |
| 7. List cursor hook | `Hooks::use_list_selection` + `ListSelection` |
| 8. `Theme` too small | Already possible — `use_context::<T>()` takes any type; documented with a worked example |
| — | `Hooks::use_memo`, which the rtop fixes needed |

rtop dropped two of its own workarounds in turn: the `build` methods split
out of every component's `render` existed only so tests could inspect an
element tree, and are replaced by `render_once`. `ui::Shared` is now a
re-export of `ntui::Shared`.

rtop keeps its own `ProcessTable` rather than moving onto the upgraded
`ntui::widgets::Table`, and the reason is not inertia: the process table
right-aligns numeric columns and truncates them from the *left* (so a long
pid keeps its significant digits), left-aligns and right-truncates text
ones, and prefixes tree guides at depth. None of that is general enough to
push into a widget, and a `Table` that took a per-column alignment and
truncation policy would be a worse `Table`. The general case — selection,
virtualization, per-cell color — is what went upstream.

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
| `/` | Incremental filter (`n` is renice; filtering narrows rather than cycles) |
| `:` | Command line — `:sort cpu`, `:filter <text>`, `:user <name>`, `:tree`, `:q` |
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

**Measured on 2026-08-04**, ~790 processes, release build: **7.7–8.4 ms per
sample, a 0.51–0.56% duty cycle** at the 1500 ms refresh. Within budget.
These are targets to measure against, not assertions that fail CI on a
noisy machine.

The budget did real work here: adding the sensors reader pushed the figure
to 2.5% and the bench is what caught it. See *What phase 6 actually cost*.

## 8. Configuration

`~/.config/rtop/config.toml`, following the XDG base directory spec. A
missing file is not an error — defaults apply and nothing is written.

Every key below is one the implementation accepts. Both structs carry
`deny_unknown_fields`, so an example with an aspirational key in it is not
a harmless illustration — pasting it is a fatal startup error. `columns`
and `show_threads` were in the original draft of this section and are not
implemented; they have been removed rather than left to be copied.

```toml
refresh_ms = 1500
theme = "gruvbox"
tree_view = false
hide_kernel_threads = false

[processes]
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
| 3 | ✅ Search, command line, help overlay, kill, renice | Full process interaction. |
| 4 | ✅ Detail pane | Per-process drill-down. |
| 5 | ✅ Disk tab | Per-device throughput. Split from the original I/O tab when the Network tab took the interface half — see the network-tab design doc. |
| 6 | ✅ Sensors tab | Thermal, fan, battery. |
| 7 | ✅ Config file and themes | Persistence and theming. |

All seven phases are built. Tree view (`t`) arrived alongside phase 3.

### What phase 6 actually cost

The flagged risk was real, and showed up as performance rather than
correctness. `hwmon` discovery is as inconsistent as expected — sparse
indices (`temp1`, `temp2`, `temp6`, `temp10`), optional labels, threshold
files that look exactly like readings — but that is all handled in a pure
function against literal fixtures.

The surprise was cost. This machine exposes **82 hwmon inputs, and reading
them takes ~30ms** — several times the whole rest of a sample — because
many are real I/O over SMBus rather than cached kernel values. Reading them
every tick took rtop from a 0.4% duty cycle to **2.5%**, well over budget.

Two changes bring it back:

1. **Sensors are read only while their tab is showing.** The sampling loop
   is keyed on the visible tab via `use_task`, so switching to the tab
   restarts it and readings appear on the next tick.
2. **They refresh at most every 2 seconds even then.** Temperatures do not
   move faster than that.

`Sample::sensors` is `Option<Vec<Sensor>>` rather than `Vec<Sensor>`
precisely because of this: `None` means "not read", `Some(vec![])` means
"this machine genuinely has none", and the view says something different
about each. Conflating them made a tab that had not sampled yet claim the
hardware did not exist.

## 10. Audit — 2026-08-04

A conformance and efficiency audit checked 202 units against this document
and the README. 165 conformed; 37 findings were raised and all were fixed.
The four that mattered:

**`Config::load_or_default` swallowed a broken config.** Its own doc said "a
file that *is* there but is broken is still an error", and it had no error
return type at all. Replaced with `load_from_optional`, which distinguishes
"missing" by the read's error kind rather than by `Path::exists` — the
latter reports `false` for a permission error, which would take a present
but unreadable file down the defaults branch.

**`dd` was a single `d`.** Three prose sources and the code's own comment
said two keystrokes; the binding took one, and the test encoded the
one-key behavior. `UiState::pending` now holds the prefix.

**The kill dialog was transparent.** It was the last caller of
`overlay::line`, the row builder that paints no background, so the process
table showed through the one dialog users read carefully before pressing
Enter. Invisible to tests, because `TestTerminal::frame_text` carries no
styling — which is why `TestTerminal::cell` was added to ntui. `line` is
deleted.

**The meter header could push the UI off screen.** `columns =
wanted.min(fits)` let a narrow pane defeat `MAX_ROWS`: 32 cores in a
50-column pane wanted 35 rows, leaving `body_rows = 0`. Row count is now
capped, the renderer drops meters past the cap rather than growing, and
`App` clamps the header so the chrome always survives.

Two findings changed the design rather than just the code:

- **`Sample::disks` and `Sample::nets` are now `Option`**, joining
  `sensors`. §6 promises an unavailable subsystem renders as "unavailable";
  an unreadable `/proc/diskstats` was parsing to an empty list and rendering
  as "idle", which is a lie a restricted container would be told.
- **One sort vocabulary.** `--sort memory` worked while `sort_by = "memory"`
  was a *fatal* startup error, because three independent spelling tables had
  drifted. `SortKey::SPELLINGS` is now the single source, used by the CLI,
  the command line, and the `Deserialize` impl.

### What the budget caught

Adding the sensors reader took the duty cycle from 0.4% to 2.5% (§9). The
audit found the opposite kind of win in the per-process loop: reading
`/proc/<pid>/status` for every process, solely to recover its owner, when
the owner is the `/proc/<pid>` directory's uid and one `stat(2)` answers it.
Removing ~800 reads per tick roughly halved the sample, to **3.5–4.2 ms, a
0.23–0.28% duty cycle**.

Deriving the visible rows is now memoized on `(sample identity, filter,
sort, direction, tree view)`. It ran on every render — so a keystroke that
moved the cursor one row re-cloned, re-filtered and re-sorted ~790 rows,
each holding two owned strings. `ProcRow::user` is an `Arc<str>` interned
per uid for the same reason.

The sampling loop no longer restarts on a tab switch. Keying it on the
visible tab abandoned the in-flight `spawn_blocking` at its await point (the
work still ran, its result was dropped, and the next sample's rates then
covered a doubled interval) and started a fresh loop that sampled
immediately — so holding Tab sampled far faster than `refresh_ms`. Whether
to read sensors is a parameter of a sample, passed through an atomic, not a
reason to tear the loop down.

## 11. Second audit — 2026-08-04

A re-audit of 177 units, run against the state left by §10, checked whether
those 37 fixes held and what they broke. 149 conformed; 31 findings, of
which **two were regressions introduced by the first round** and three were
fixes reported as landed that had not.

**`Ctrl-D` opened the kill dialog.** The `dd` completion compared only
`KeyCode`, and Ctrl-D's code *is* `Char('d')` — so the documented
half-page-down key finished the sequence and opened a destructive dialog.
The completion now requires an unmodified key, and the test that was
supposed to cover this used `j`, which never exercised a modifier.

**The uid optimization changed semantics.** `/proc/<pid>` is owned by the
process's *effective* uid (`task_dump_owner()` reads `cred->euid`), not its
real one, so a setuid process was silently reattributed. Kept, because it
is what htop shows and the speedup is large, but every claim that said
"real uid" is corrected and `parse_status_uid` is deleted rather than left
as dead code with two green tests asserting semantics the binary no longer
had.

Three fixes had not actually landed:

- **`MEM%` still claimed `SortKey::Memory`** — `cargo fmt` reformatted the
  array before the edit ran, the pattern missed, and it was reported fixed
  without checking. A test now asserts exactly one column claims each key,
  which also caught that the Command column was never marked at all.
- **The `App` header clamp was inert.** It shrank the number App subtracted
  while `Meters` went on rendering its full height, so `body_rows` was
  over-stated instead. `Meters` now takes the budget as a prop, and the body
  is omitted entirely when there is no room for it — every view draws its
  own header, which costs a row a 4-line terminal does not have.
- **`format::bytes` still printed four digits** in the `1000..1023` band.
  The property test asserted `digits <= 4`, encoding "no more than four"
  rather than the doc's "under four" — the test was weaker than the claim it
  was meant to pin. The claim is now the accurate one.

**The `Option<Vec<IoRate>>` change had a regression of its own**: the sample
clock advanced on a failed read while the counter baseline did not, so the
tick after a transient failure divided two intervals of counters by one and
reported roughly double the true throughput. Each stream now carries its own
baseline timestamp, pinned by a test that fails on the spike.

Also: `verify_unchanged` reported `EACCES` as "exited" (under `hidepid` a
live process was declared dead); `contains_ignore_case` claimed to avoid
allocating and did not, and now does; the `threads` accumulator was the one
sum the saturating sweep missed; an unreadable `/proc/stat` fabricated a
zero baseline that reported the since-boot average as the interval; and
`ui::Selection` is now a re-export of the `ListSelection` it was upstreamed
as, rather than a second copy of the same arithmetic.

Upstream, `use_memo` did not diagnose hook-order violations the way every
sibling hook does, and `render_once`'s documentation was wrong about
effects — it never runs them at all, so `use_task` and `use_interval` are
silent no-ops there rather than a runtime requirement.

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
