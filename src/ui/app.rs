//! The root component: owns the sampling loop, the UI state, and the wiring
//! between the keymap's decisions and the world.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ntui::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks, element};

use crate::actions;
use crate::config::Config;
use crate::filter::{self, Filter};
use crate::model::{ProcRow, Sample};
use crate::sampler::{Sampler, Wanted};
use crate::sort::{SortKey, sort_procs};
use crate::tree;
use crate::ui::detail::{Detail, DetailProps, Kill, KillProps, Renice, ReniceProps};
use crate::ui::help::{Help, HelpProps};
use crate::ui::io::{DiskView, DiskViewProps};
use crate::ui::meters::{Meters, MetersProps};
use crate::ui::network::{self, NetworkView, NetworkViewProps};
use crate::ui::palette::Palette;
use crate::ui::sensors::{SensorView, SensorViewProps};
use crate::ui::state::{Effect, Lists, Mode, Overlay, Tab, UiState, handle_key};
use crate::ui::table::{ProcessTable, ProcessTableProps, cell};
use crate::ui::{Shared, meters};

/// Rows the body gives up to chrome: the tab bar and the status bar.
const CHROME_ROWS: u16 = 2;

#[derive(Clone, PartialEq, Default)]
pub struct AppProps {
    pub config: Config,
    pub palette: Palette,
}

pub struct App;

impl Component for App {
    type Props = AppProps;

    fn render(props: &AppProps, hooks: &mut Hooks) -> Element {
        let sample = hooks.use_state(Shared::<Sample>::default);
        let ui = hooks.use_state(|| UiState {
            sort: props.config.processes.sort_by,
            descending: props.config.processes.sort_desc,
            tree_view: props.config.tree_view,
            filter: Filter {
                hide_kernel_threads: props.config.hide_kernel_threads,
                ..Filter::default()
            },
            ..UiState::default()
        });
        let (terminal_cols, terminal_rows) = hooks.use_terminal_size();
        let app = hooks.use_app();

        // The sampler is retained across ticks because rates are derived by
        // diffing against its previous reading.
        let sampler = hooks.use_state(|| Arc::new(Mutex::new(Sampler::new())));

        // Sensors are only read while their tab is showing; see
        // `Sampler::sensors` for why.
        //
        // The flag is a shared atomic rather than a dependency of the
        // sampling task. Keying the task on the tab restarted the loop on
        // every switch, and restarting it is not free: the in-flight
        // `spawn_blocking` is abandoned at its await point (the /proc work
        // still runs, its result is dropped, and the next sample's rates
        // then cover a doubled interval), while the fresh loop samples
        // immediately — so holding Tab down sampled far faster than
        // `refresh_ms`. Whether to read sensors is a parameter of a sample,
        // not a reason to tear the loop down.
        let want_sensors = hooks.use_state(|| Arc::new(AtomicBool::new(false)));
        let want_sockets = hooks.use_state(|| Arc::new(AtomicBool::new(false)));
        // Written, not `set`: this must not itself schedule a render.
        let tab = ui.get().tab;
        want_sensors
            .get()
            .store(tab == Tab::Sensors, Ordering::Relaxed);
        want_sockets
            .get()
            .store(tab == Tab::Network, Ordering::Relaxed);

        {
            let (sink, sampler) = (sample.clone(), sampler.get());
            let (sensors, sockets) = (want_sensors.get(), want_sockets.get());
            let refresh = Duration::from_millis(props.config.refresh_ms);
            hooks.use_future(move || async move {
                loop {
                    let sink = sink.clone();
                    let sampler = sampler.clone();
                    let (sensors, sockets) = (sensors.clone(), sockets.clone());
                    // /proc reads are blocking syscalls across hundreds of
                    // files; on the render thread they would stall input for
                    // the duration of every sample.
                    let taken = tokio::task::spawn_blocking(move || {
                        sampler
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .sample(Wanted {
                                sensors: sensors.load(Ordering::Relaxed),
                                sockets: sockets.load(Ordering::Relaxed),
                            })
                    })
                    .await;
                    if let Ok(taken) = taken {
                        sink.set(Shared::new(taken));
                    }
                    tokio::time::sleep(refresh).await;
                }
            });
        }

        let current = sample.get();
        let state = ui.get();
        let palette = props.palette;

        // The budget is passed to `Meters`, not just subtracted here: a
        // clamp the component never sees shrinks the arithmetic without
        // shrinking the header, which leaves `body_rows` over-stated and
        // the chrome squeezed off the bottom anyway.
        let header_budget = Some(terminal_rows.saturating_sub(CHROME_ROWS + 1));
        let header_rows = meters::height(current.cores.len(), terminal_cols, header_budget);
        let body_rows = terminal_rows
            .saturating_sub(header_rows)
            .saturating_sub(CHROME_ROWS);

        // The table spends one of its rows on its own column header, so it
        // can only show one fewer process than the body has room for.
        // Without this the status bar is pushed off the bottom of the
        // screen, since the table's flex_grow lets it take the space.
        let table_rows = body_rows.saturating_sub(1);

        // Memoized on what the list actually depends on. Without this the
        // whole process list is cloned, filtered and sorted on *every*
        // render — including a keystroke that moves the cursor one row, or
        // one that is not bound to anything. It also keeps the `Shared`
        // stable across those renders, so the table's props compare equal
        // and its subtree is skipped entirely.
        let rows = hooks.use_memo(
            (
                current.clone(),
                state.filter.clone(),
                state.sort,
                state.descending,
                state.tree_view,
            ),
            || Shared::new(visible_rows(&current, &state)),
        );

        // Processes exit between samples, so a cursor that was valid a
        // moment ago can now point past the end of the list.
        let mut cursor = state.selection;
        cursor.clamp(rows.len(), table_rows as usize);

        // Clamped against the listening list's own length and its own share
        // of the pane — see `UiState::socket_selection` for why it is not
        // the process cursor.
        let listening: &[_] = current.sockets.as_deref().map_or(&[], |s| s.as_slice());
        let socket_height = network::socket_capacity(body_rows as usize, &current);
        let mut socket_cursor = state.socket_selection;
        // Clamped only while this tab owns the data. Off it, `sockets` is
        // `None`, and clamping against a length of zero would rewind the
        // cursor — so a glance at another tab lost your place in a
        // forty-row list, which the process cursor never does.
        if state.tab == Tab::Network {
            socket_cursor.clamp(listening.len(), socket_height);
        }

        {
            let (ui, rows) = (ui.clone(), rows.clone());
            let proc_height = table_rows as usize;
            let socket_count = listening.len();
            hooks.use_input(move |ev, _| {
                let mut next = ui.get();
                next.selection = cursor;
                next.socket_selection = socket_cursor;
                let effect = handle_key(
                    &mut next,
                    ev,
                    Lists::processes(&rows, proc_height).with_sockets(socket_count, socket_height),
                );

                match effect {
                    Effect::None => {}
                    Effect::Quit => app.exit(),
                    // Signals and renices are performed here rather than in
                    // the keymap so that the keymap stays a pure function.
                    // Both re-check identity first: the dialog may have been
                    // open long enough for the process to exit and its pid
                    // to be recycled.
                    Effect::Kill {
                        pid,
                        starttime,
                        signal,
                    } => {
                        next.notice = Some(action_status(
                            signal.label(),
                            pid,
                            &actions::kill_if_unchanged(pid, starttime, signal),
                        ));
                    }
                    Effect::Renice {
                        pid,
                        starttime,
                        nice,
                    } => {
                        next.notice =
                            Some(match actions::renice_if_unchanged(pid, starttime, nice) {
                                Ok(()) => format!("reniced {pid} to {nice}"),
                                Err(e) => format!("renice {pid}: {}", actions::explain(&e)),
                            });
                    }
                }
                ui.set(next);
            });
        }

        // With no rows to give it, the body is omitted rather than rendered
        // empty: every view still draws its own column header, which would
        // cost a row the terminal does not have and push the status bar off.
        let body = if body_rows == 0 {
            Element::fragment(Vec::new())
        } else {
            match state.tab {
                Tab::Processes => element!(ProcessTable(
                    rows: rows.clone(),
                    selection: cursor,
                    height: table_rows,
                    sort: state.sort,
                    palette: palette,
                )),
                Tab::Disk => element!(DiskView(
                    sample: current.clone(),
                    palette: palette,
                    height: body_rows,
                )),
                Tab::Network => element!(NetworkView(
                    sample: current.clone(),
                    palette: palette,
                    height: body_rows,
                    selection: socket_cursor,
                )),
                Tab::Sensors => element!(SensorView(
                    sample: current.clone(),
                    palette: palette,
                    height: body_rows,
                )),
            }
        };

        let mut children = vec![
            element!(Meters(
                sample: current.clone(),
                palette: palette,
                width: terminal_cols,
                max_rows: header_budget,
            )),
            tab_bar(&state, &palette),
            body,
            status_bar(&state, rows.len(), &palette),
        ];

        if let Some(overlay) = overlay_element(&state, &current.procs, &palette) {
            children.push(overlay);
        }

        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Column,
                height: Dimension::Percent(1.0),
                ..Default::default()
            },
            children,
        )
    }
}

/// The process list as the table should show it: filtered, sorted, and — in
/// tree view — nested.
///
/// Filtering comes before sorting because sorting is the expensive step and
/// there is no reason to order rows about to be discarded.
pub fn visible_rows(sample: &Sample, state: &UiState) -> Vec<ProcRow> {
    let mut rows = filter::apply(sample.procs.clone(), &state.filter);
    sort_procs(&mut rows, state.sort, state.descending);
    if state.tree_view {
        rows = tree::flatten(rows);
    }
    rows
}

fn overlay_element(state: &UiState, procs: &[ProcRow], palette: &Palette) -> Option<Element> {
    match &state.overlay {
        Overlay::None => None,
        Overlay::Help => Some(element!(Help(palette: *palette))),
        // All three look the row up fresh each frame, so a pane keeps
        // updating and reports the exit rather than freezing on stale text.
        Overlay::Detail { key } => Some(element!(Detail(
            row: find(procs, *key),
            pid: key.pid,
            palette: *palette,
        ))),
        Overlay::Kill { key, name, index } => {
            let row = find(procs, *key);
            Some(element!(Kill(
                pid: key.pid,
                name: name.clone(),
                index: *index,
                alive: row.is_some(),
                owner: row.as_ref().and_then(foreign_owner),
                palette: *palette,
            )))
        }
        Overlay::Renice { key, name, input } => Some(element!(Renice(
            pid: key.pid,
            name: name.clone(),
            input: input.clone(),
            alive: find(procs, *key).is_some(),
            palette: *palette,
        ))),
    }
}

/// The row for a process identity, if it is still in the sample.
/// The status-bar line for a completed action.
///
/// Extracted from the effect handler so the failure wording is reachable
/// from a test: the handler itself needs a live process and a real syscall.
pub fn action_status(label: &str, pid: i32, result: &std::io::Result<()>) -> String {
    match result {
        Ok(()) => format!("sent {label} to {pid}"),
        Err(e) => format!("{label} to {pid}: {}", actions::explain(e)),
    }
}

fn find(procs: &[ProcRow], key: crate::model::ProcKey) -> Option<ProcRow> {
    procs.iter().find(|r| r.proc.key() == key).cloned()
}

/// The owner to warn about, or `None` when no warning is warranted.
///
/// Asks the kernel with `kill(pid, 0)` rather than comparing uids, so the
/// answer is the same one the real signal will get. A uid comparison is
/// necessarily approximate — `kill(2)` also accepts the target's saved-set
/// uid, `/proc` exposes only the effective one, and `CAP_KILL` need not come
/// with uid 0 — and an approximation here means warning about processes the
/// user can in fact signal.
///
/// Anything other than a permission refusal means no warning: the process
/// exiting is already reported by the dialog's own `alive` line.
pub fn foreign_owner(row: &ProcRow) -> Option<String> {
    match actions::signal_exists(row.proc.pid) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Some(row.user.to_string()),
        _ => None,
    }
}

fn tab_bar(state: &UiState, palette: &Palette) -> Element {
    let tabs: Vec<Element> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let active = *tab == state.tab;
            let label = format!(" {}:{} ", i + 1, tab.label());
            let width = label.chars().count() as u16;
            cell(
                &label,
                width,
                if active {
                    palette.selected_fg
                } else {
                    palette.muted
                },
                if active { Weight::Bold } else { Weight::Normal },
                if active {
                    palette.selected_bg
                } else {
                    Color::Reset
                },
            )
        })
        .collect();

    Element::view(
        ViewProps {
            flex_direction: FlexDirection::Row,
            gap: 1,
            height: Dimension::Cells(1),
            ..Default::default()
        },
        tabs,
    )
}

fn status_bar(state: &UiState, count: usize, palette: &Palette) -> Element {
    // A prompt or a notice displaces the usual hints: what the user is doing
    // right now matters more than the key list.
    let (content, background) = match (&state.mode, &state.notice) {
        (Mode::Search(query), _) => (format!(" /{query}_"), palette.selected_bg),
        (Mode::Command(buffer), _) => (format!(" :{buffer}_"), palette.selected_bg),
        (Mode::Normal, Some(notice)) => (format!(" {notice}"), palette.alert),
        (Mode::Normal, None) => (
            format!(
                " {count} procs{}{}{} · {} {} · ? keys · q quit",
                if state.filter.query.is_empty() {
                    String::new()
                } else {
                    format!(" · /{}", state.filter.query)
                },
                match &state.filter.user {
                    Some(user) => format!(" · user {user}"),
                    None => String::new(),
                },
                if state.tree_view { " · tree" } else { "" },
                sort_name(state.sort),
                if state.descending { "desc" } else { "asc" },
            ),
            palette.status_bg,
        ),
    };

    Element::view(
        ViewProps {
            height: Dimension::Cells(1),
            background,
            ..Default::default()
        },
        vec![Element::text(TextProps {
            content,
            color: palette.selected_fg,
            weight: Weight::Bold,
            wrap: TextWrap::Truncate,
            ..Default::default()
        })],
    )
}

/// The column name shown in the status bar.
pub fn sort_name(key: SortKey) -> &'static str {
    match key {
        SortKey::Pid => "PID",
        SortKey::Name => "Command",
        SortKey::Cpu => "CPU%",
        SortKey::Memory => "RES",
        SortKey::Time => "TIME+",
    }
}
