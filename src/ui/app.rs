//! The root component: owns the sampling loop, the UI state, and the wiring
//! between the keymap's decisions and the world.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ntui::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks, element};

use crate::actions;
use crate::config::Config;
use crate::filter::{self, Filter};
use crate::model::{ProcRow, Sample};
use crate::sampler::Sampler;
use crate::sort::{SortKey, sort_procs};
use crate::tree;
use crate::ui::detail::{Detail, DetailProps, Kill, KillProps, Renice, ReniceProps};
use crate::ui::help::{Help, HelpProps};
use crate::ui::io::{IoView, IoViewProps};
use crate::ui::meters::{Meters, MetersProps};
use crate::ui::palette::Palette;
use crate::ui::sensors::{SensorView, SensorViewProps};
use crate::ui::state::{Effect, Mode, Overlay, Tab, UiState, handle_key};
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
        // `Sampler::sensors` for why. Keyed on the tab so switching to it
        // restarts the loop and the readings appear on the next tick rather
        // than at the next refresh.
        let showing_sensors = ui.get().tab == Tab::Sensors;
        {
            let (sink, sampler) = (sample.clone(), sampler.get());
            let refresh = Duration::from_millis(props.config.refresh_ms);
            hooks.use_task(showing_sensors, move || async move {
                loop {
                    let sink = sink.clone();
                    let sampler = sampler.clone();
                    // /proc reads are blocking syscalls across hundreds of
                    // files; on the render thread they would stall input for
                    // the duration of every sample.
                    let taken = tokio::task::spawn_blocking(move || {
                        sampler
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .sample(showing_sensors)
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

        let header_rows = meters::height(current.cores.len(), terminal_cols);
        let body_rows = terminal_rows
            .saturating_sub(header_rows)
            .saturating_sub(CHROME_ROWS);

        // The table spends one of its rows on its own column header, so it
        // can only show one fewer process than the body has room for.
        // Without this the status bar is pushed off the bottom of the
        // screen, since the table's flex_grow lets it take the space.
        let table_rows = body_rows.saturating_sub(1);

        let rows = Shared::new(visible_rows(&current, &state));

        // Processes exit between samples, so a cursor that was valid a
        // moment ago can now point past the end of the list.
        let mut cursor = state.selection;
        cursor.clamp(rows.len(), table_rows as usize);

        {
            let (ui, rows) = (ui.clone(), rows.clone());
            let height = table_rows as usize;
            hooks.use_input(move |ev, _| {
                let mut next = ui.get();
                next.selection = cursor;
                let effect = handle_key(&mut next, ev, &rows, height);

                match effect {
                    Effect::None => {}
                    Effect::Quit => app.exit(),
                    // Signals and renices are performed here rather than in
                    // the keymap so that the keymap stays a pure function.
                    Effect::Kill { pid, signal } => {
                        next.notice = Some(match actions::kill(pid, signal) {
                            Ok(()) => format!("sent {} to {pid}", signal.label()),
                            Err(e) => format!("{} to {pid}: {e}", signal.label()),
                        });
                    }
                    Effect::Renice { pid, nice } => {
                        next.notice = Some(match actions::renice(pid, nice) {
                            Ok(()) => format!("reniced {pid} to {nice}"),
                            Err(e) => format!("renice {pid}: {e}"),
                        });
                    }
                }
                ui.set(next);
            });
        }

        let body = match state.tab {
            Tab::Processes => element!(ProcessTable(
                rows: rows.clone(),
                selection: cursor,
                height: table_rows,
                sort: state.sort,
                palette: palette,
            )),
            Tab::Io => element!(IoView(
                sample: current.clone(),
                palette: palette,
                height: body_rows,
            )),
            Tab::Sensors => element!(SensorView(
                sample: current.clone(),
                palette: palette,
                height: body_rows,
            )),
        };

        let mut children = vec![
            element!(Meters(sample: current.clone(), palette: palette, width: terminal_cols)),
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
        Overlay::Detail { pid } => Some(element!(Detail(
            // Looked up fresh each frame so the pane keeps updating, and
            // says so rather than freezing if the process exits.
            row: procs.iter().find(|r| r.proc.pid == *pid).cloned(),
            pid: *pid,
            palette: *palette,
        ))),
        Overlay::Kill { pid, name, index } => Some(element!(Kill(
            pid: *pid,
            name: name.clone(),
            index: *index,
            palette: *palette,
        ))),
        Overlay::Renice { pid, name, input } => Some(element!(Renice(
            pid: *pid,
            name: name.clone(),
            input: input.clone(),
            palette: *palette,
        ))),
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
