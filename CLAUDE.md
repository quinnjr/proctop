# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

rtop is an htop-inspired Linux system monitor built on
[`ntui`](https://github.com/quinnjr/ntui), an Ink-style TUI library. It has
two equal goals: a monitor worth using daily, and a demanding dogfood
harness for `ntui` — a table re-rendering several hundred rows twice a
second under a hard CPU budget finds API and performance gaps a small
example never will. When the two conflict, rtop ships what works and the
`ntui` gap is fixed upstream.

Linux only. rtop reads `/proc` and `/sys` directly with no portability
layer, and does not accept portability shims.

Design doc: `docs/superpowers/specs/2026-08-04-rtop-design.md`. Sections 10
and 11 record two audits and are the fastest way to understand why several
things are shaped the way they are.

`Shared`, `use_memo`, `use_list_selection`, `render_once`,
`TestTerminal::cell`, and `Table`'s selection/viewport/cell-style props were
all added to `ntui` *for* rtop and shipped in 0.3.0. When rtop needs
something from a TUI library, the default is to add it upstream rather than
work around it locally — but only after the shape has settled here. Develop
against a path dependency (`ntui = { version = "0.3", path = "../ntui/ntui" }`)
while iterating on both, and drop back to the plain version before
committing.

## Commands

```bash
cargo test                                   # everything
cargo test --test keymap                     # one integration test file
cargo test --test keymap a_single_d_opens    # one test by (partial) name
cargo test --lib                             # unit tests inside src/
cargo clippy --all-targets                   # lint gate; must stay clean
cargo fmt
cargo run --release                          # run it (needs a real TTY)
cargo run --release --example bench          # sample cost; re-run after touching sampler
cargo run --release --example frames         # every screen as plain text, no TTY needed
```

`--release` matters for `bench`: a debug build measures the wrong thing by a
wide margin.

`examples/frames.rs` is the fastest way to see a layout change — it drives
the real `App` headlessly and prints each tab and overlay as text.

## Architecture

### The parse / read split

The important boundary is not between crates but between *parsing* and
*reading*. Every parser under `sample::*` takes a `&str` and returns types —
never a path, never I/O. `sampler` is the only module in the crate that
touches the filesystem on the sampling path (`actions` and `config` do their
own I/O, off it).

This is what makes `tests/fixtures/` work: real `/proc` contents are checked
in, so the suite is deterministic and can cover cases that are hard to
produce live (a process named `weird)name`, a kernel that stops before the
guest columns, a machine with no swap, a counter mid-wraparound). Adding a
parser means adding a `&str -> T` function and a fixture, not a mock.

### Data flow, one frame

```
Sampler::sample(Wanted)          blocking /proc reads on spawn_blocking
  → Sample                        rates already derived; the UI does no accounting arithmetic
  → State<Shared<Sample>>         Shared compares by Arc::ptr_eq, not by value
  → App::render
      use_memo(deps) → visible_rows()   filter → sort → (tree) — skipped when deps are unchanged
      → Shared<Vec<ProcRow>>
      → ProcessTable                    slices to the visible window BEFORE building any row
```

Two separate mechanisms keep a frame cheap, and both matter: `use_memo`
stops the list being re-derived on a keystroke that changes nothing, and the
viewport slice stops offscreen rows being built at all.

### The keymap is a pure function

`ui::state::handle_key(state, key, rows, height) -> Effect` takes the UI
state and a key and returns the next state plus an `Effect` describing
anything it cannot do itself (quit, kill, renice). `App` performs the
effects. That is why `tests/keymap.rs` can cover every mode, overlay, and
list edge without a terminal — put new interaction logic there, not in
`app.rs`.

### Testing tiers

| Tier | Tool | For |
|---|---|---|
| Fixture | `tests/*.rs` + `tests/fixtures/` | parsing, deltas, sorting, filtering — deterministic |
| Structural | `ntui::testing::render_once` | what a component *built* — a frame cannot tell "never built" from "built then clipped" |
| Visual | `ntui::testing::TestTerminal` | what the user sees; `cell()`/`row()` for colour, since `frame_text()` carries no styling |
| Live | `tests/sampler.rs`, `tests/app.rs` | wiring only, never a particular figure |

Sampling runs on `spawn_blocking`, so whether it has landed after any given
tick is a race — the live tests poll (`tick_until`) rather than assume.

## Invariants that bite

Most of these are here because something already went wrong.

- **Process identity is `(pid, starttime)`, never a bare pid.** Linux
  recycles PIDs. This applies to deltas *and* to destructive actions:
  `actions::kill_if_unchanged` / `renice_if_unchanged` re-verify before
  signalling, because a dialog can sit open while its process exits.
- **Guest time is counted twice by the kernel, and the two corrections are
  different.** `cpu_usage` *subtracts* it out of `user`/`nice` and reports
  it as its own bucket. `total_jiffies` — the denominator — corrects by
  *omitting* the guest columns from the sum, and must not also subtract
  them: doing both removes guest time twice and inflates every percentage
  on a virtualization host.
- **Arithmetic on the sampling path saturates.** A debug-build overflow is a
  panic, and `ntui` swallows panics in `spawn_blocking` tasks — so it
  manifests as a UI that silently stops updating with no error anywhere.
  Same reason there is no `unwrap`/`expect` on that path.
- **`None` and `Some(vec![])` mean different things** for `Sample::disks`,
  `nets`, `sensors`, and `sockets`: "not read" versus "this machine
  genuinely has none". They render differently. For the two gated
  subsystems `None` is usually "this tab just opened", so the notice says
  so rather than claiming unavailability. `mem`/`load`/`uptime` are
  deliberately not `Option` — procfs guarantees those files.
- **Each counter stream owns its baseline timestamp.** A shared clock that
  advances on a failed read makes the next successful tick divide two
  intervals of counters by one and report double the throughput.
- **`Shared<T>` compares by pointer, and `Shared::default()` allocates.** A
  defaulted `Shared` prop field gets a fresh pointer every render and
  defeats `props_eq` permanently — the exact cost the type exists to avoid.
- **A `use_memo` dep tuple must cover everything the body reads.** Anything
  missing is stale data shown to the user.
- **`network::socket_capacity()` must equal the rows `NetworkView` draws.**
  `App` clamps the socket cursor with it and the renderer windows with it;
  disagree and the cursor scrolls rows off with no key to recover them.
  One function, called with the same argument the `height` prop carries.
- **`meters::height()` must equal what `Meters` actually renders**, and the
  row budget is passed to it as a prop. A clamp the component never sees
  shrinks the arithmetic without shrinking the header, and the chrome gets
  squeezed off the bottom anyway. `tests/components.rs` pins the two
  against each other across sizes.
- **Overlay panel rows must paint an opaque background.** A `View` with
  `Color::Reset` does not paint, so the process table shows straight through
  the dialog. `overlay::row`/`field` do this; there is no non-painting
  variant any more, deliberately.
- **Multi-key sequences must match on the whole `KeyEvent`.** `Ctrl-D`'s
  `KeyCode` is also `Char('d')`, so a code-only comparison let it complete
  `dd` and open the kill dialog from a navigation key.
- **`SortKey::SPELLINGS` is the single sort vocabulary**, shared by the
  config file, `--sort`, and `:sort`. Three tables drifted once and
  `sort_by = "memory"` became a fatal startup error while `--sort memory`
  worked.
- **Sorting tie-breaks on ascending pid in both directions**, or the idle
  bulk of the table reshuffles every refresh. A NaN percentage sorts as zero
  (`partial_cmp` reports incomparable; `total_cmp` floats NaN above
  infinity).
- **Sensors and sockets are read only while their own tab is showing**, at
  most every 2s, and `Sampler::sample` takes a `Wanted` saying which.
  Sensors are ~80 `hwmon` inputs, many of them real SMBus I/O; socket
  attribution walks every readable `/proc/<pid>/fd`. Each costs more than
  the whole rest of a sample.
- **`/proc/net` addresses are little-endian, and IPv6 is four little-endian
  *words*** — not one 128-bit little-endian value. `::1` is stored as
  `00000000000000000000000001000000`; reversing all sixteen bytes gives the
  wrong address.

## Conventions

- A broken config is fatal and reported; a missing one is fine. Unknown keys
  are rejected — a setting that appears not to work with nothing saying why
  is worse than a refusal to start. Themes are bundled with `include_str!`
  and selected by name; there is no user theme file to parse.
- `/proc` read failures are normal, not exceptional: a per-process error
  drops that process silently, a per-subsystem error degrades that view.
  Only config-parse and terminal-init are fatal.
- The README's Keys table and `ui::help::BINDINGS` are both claims about
  behavior. A test checks each entry fits its rendered column and that
  every entry appears in `help.rs`'s hand-maintained `BOUND` list — which
  is not derived from `handle_key`, so deleting a keymap arm fails nothing.
  Both the README and that list are checked by hand.
- rtop keeps its own `ProcessTable` rather than using `ntui::widgets::Table`:
  it right-aligns numeric columns and truncates them from the *left*,
  left-aligns text ones, and prefixes tree guides at depth. None of that is
  general enough to push upstream.
