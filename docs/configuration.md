# Configuring proctop

proctop runs with no configuration at all. Everything below is optional.

- [The file](#the-file)
- [Options](#options)
- [Themes](#themes)
- [Command-line flags](#command-line-flags)
- [Precedence](#precedence)
- [When something is wrong](#when-something-is-wrong)
- [Worked example](#worked-example)

## The file

`$XDG_CONFIG_HOME/proctop/config.toml`, falling back to
`~/.config/proctop/config.toml`.

```
proctop --show-config-path
```

prints the path proctop will actually read, including the effect of
`--config`. A missing file is not an error — the defaults apply and nothing
is written back. proctop never writes to this file.

Every key is optional; omit one and it takes its default. An empty file is
valid and means "all defaults".

## Options

| Key | Type | Default | Notes |
|---|---|---|---|
| `refresh_ms` | integer | `1500` | Milliseconds between samples. Minimum **100**. |
| `theme` | string | `"default"` | One of `default`, `gruvbox`, `mono`. |
| `tree_view` | boolean | `false` | Start with the process tree expanded. |
| `hide_kernel_threads` | boolean | `false` | Hide kernel threads from the table. |
| `processes.sort_by` | string | `"cpu"` | Column to sort on — see below. |
| `processes.sort_desc` | boolean | `true` | Largest first. |

### `refresh_ms`

A sample costs a few milliseconds of real work, so a very fast refresh
spends a meaningful slice of the machine watching itself. Values under 100
are refused rather than clamped, because silently doing something other than
what you asked is worse than saying no.

At the default 1500 ms, proctop costs roughly 0.24% of one core with the
Processes tab open. The Sensors and Network tabs read more (`hwmon` inputs
and a walk of every readable `/proc/<pid>/fd` respectively) and are sampled
only while their own tab is showing.

### `processes.sort_by`

The same vocabulary works in the config file, in `--sort`, and in the `:sort`
command. Aliases are equivalent:

| Column | Accepted spellings |
|---|---|
| PID | `pid` |
| Command | `name`, `command` |
| CPU% | `cpu` |
| MEM% | `mem`, `memory`, `res` |
| TIME+ | `time` |

Sorting always tie-breaks on ascending PID, in both directions, so the idle
bulk of the table does not reshuffle itself every refresh.

## Themes

Three are built in: `default`, `gruvbox`, `mono`. They are compiled into the
binary and selected by name.

**There is no user theme file.** Naming a theme proctop does not have is a
startup error listing the ones it does:

```
proctop: unknown theme "solarized"; available: default, gruvbox, mono
```

## Command-line flags

Flags override the config file **for that run only**.

| Flag | Overrides | Notes |
|---|---|---|
| `-r`, `--refresh <MS>` | `refresh_ms` | Same 100 ms floor. |
| `-s`, `--sort <COL>` | `processes.sort_by` | Any spelling from the table above. |
| `-t`, `--theme <NAME>` | `theme` | |
| `--tree` | `tree_view` | **Sets it on only** — see below. |
| `-H`, `--hide-kernel-threads` | `hide_kernel_threads` | **Sets it on only** — see below. |
| `-c`, `--config <PATH>` | the file location | The named file **must** exist. |
| `--show-config-path` | — | Print the path and exit. |
| `-V`, `--version` | — | |
| `-h`, `--help` | — | |

### The two boolean flags are one-way

`--tree` and `-H` can turn a setting **on** but not off. If your config says

```toml
tree_view = true
```

there is no flag that gives you a flat list for one run; edit the file, or
press `t` once proctop is running. The same applies to
`hide_kernel_threads` and `H`.

Both are toggleable from inside the program, which is why this has not been
worth a second flag each.

### There is no flag for sort direction

`--sort` picks the column; `processes.sort_desc` picks the direction and is
config-only. Press `I` to invert the order while running.

## Precedence

Lowest to highest:

1. Built-in defaults
2. The config file — `~/.config/proctop/config.toml`, or `--config <path>`
3. Command-line flags
4. Keys pressed while running (never written back)

## When something is wrong

**A broken config is fatal and reported.** proctop refuses to start rather
than ignoring the part it did not understand — a setting that appears not to
work with nothing anywhere saying why is a worse experience than a refusal.

Unknown keys are rejected, so a typo is caught rather than silently ignored:

```
$ proctop --config bad.toml          # contains: refresh_msec = 2000
proctop: bad.toml: TOML parse error at line 2, column 1
```

Out-of-range values name the bound and what you gave:

```
proctop: config.toml: refresh_ms must be at least 100, got 50
proctop: --refresh must be at least 100ms
proctop: --sort expects one of pid, name, command, cpu, mem, memory, res, time (got ram)
```

The distinction between the two file paths is deliberate:

- The **default** file may be missing. That is the normal case.
- A file named with `--config` **must** exist, because you asked for it
  specifically:

```
proctop: /nope.toml: No such file or directory (os error 2)
```

"Missing" is decided by the read's own error, not by checking whether the
path exists — a permissions error on a file that is right there is reported
as a permissions error, not quietly treated as absent.

## Worked example

Every key, at a non-default value:

```toml
# ~/.config/proctop/config.toml

refresh_ms = 2000
theme = "gruvbox"
tree_view = true
hide_kernel_threads = true

[processes]
sort_by = "mem"
sort_desc = true
```

The equivalent for a single run, as far as flags can express it:

```
proctop --refresh 2000 --theme gruvbox --tree -H --sort mem
```

`sort_desc` has no flag, but `true` is already its default.
