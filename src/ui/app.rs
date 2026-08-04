//! The root component: owns the sampling loop, the selection, and the
//! keybindings.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ntui::props::{Dimension, FlexDirection, TextWrap};
use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks, KeyCode, element};

use crate::model::Sample;
use crate::sampler::Sampler;
use crate::sort::{SortKey, sort_procs};
use crate::ui::meters::{Meters, MetersProps};
use crate::ui::selection::Selection;
use crate::ui::table::{ProcessTable, ProcessTableProps};
use crate::ui::{Shared, meters, theme};

/// How often the machine is re-sampled. htop's default, and slow enough that
/// the sampler's own cost stays invisible.
const REFRESH: Duration = Duration::from_millis(1500);

/// Rows the table gives up to chrome: the column header and the status bar.
const CHROME_ROWS: u16 = 2;

#[derive(Clone, PartialEq, Default)]
pub struct AppProps {
    /// Column to sort by on startup.
    pub sort: SortKey,
}

pub struct App;

impl Component for App {
    type Props = AppProps;

    fn render(props: &AppProps, hooks: &mut Hooks) -> Element {
        let sample = hooks.use_state(Shared::<Sample>::default);
        let selection = hooks.use_state(Selection::default);
        let sort = hooks.use_state(|| props.sort);
        let descending = hooks.use_state(|| true);
        let (_, terminal_rows) = hooks.use_terminal_size();
        let app = hooks.use_app();

        // The sampler is retained across ticks because rates are derived by
        // diffing against its previous reading.
        let sampler = hooks.use_state(|| Arc::new(Mutex::new(Sampler::new())));

        {
            let (sink, sampler) = (sample.clone(), sampler.get());
            hooks.use_future(move || async move {
                loop {
                    let sink = sink.clone();
                    let sampler = sampler.clone();
                    // /proc reads are blocking syscalls across hundreds of
                    // files; running them on the render thread would stall
                    // input for the duration of every sample.
                    let taken = tokio::task::spawn_blocking(move || {
                        sampler
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .sample()
                    })
                    .await;
                    if let Ok(taken) = taken {
                        sink.set(Shared::new(taken));
                    }
                    tokio::time::sleep(REFRESH).await;
                }
            });
        }

        let current = sample.get();
        let header_rows = meters::height(current.cores.len());
        let table_rows = terminal_rows
            .saturating_sub(header_rows)
            .saturating_sub(CHROME_ROWS);

        let mut rows = current.procs.clone();
        sort_procs(&mut rows, sort.get(), descending.get());

        // Processes exit between samples, so a selection that was valid a
        // moment ago can now point past the end of the list.
        let mut cursor = selection.get();
        cursor.clamp(rows.len(), table_rows as usize);
        if cursor != selection.get() {
            selection.set(cursor);
        }

        {
            let (selection, sort, descending) =
                (selection.clone(), sort.clone(), descending.clone());
            let len = rows.len();
            let height = table_rows as usize;
            hooks.use_input(move |ev, _| {
                let page = height.max(1) as isize;
                match ev.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.exit(),

                    KeyCode::Char('j') | KeyCode::Down => {
                        selection.update(|s| s.move_by(1, len, height))
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        selection.update(|s| s.move_by(-1, len, height))
                    }
                    // Ctrl-modified rather than bare d/u: `d` is reserved
                    // for `dd` (kill), and `u` for filter-by-user.
                    KeyCode::Char('d') if ctrl(&ev) => {
                        selection.update(|s| s.move_by(page / 2, len, height))
                    }
                    KeyCode::Char('u') if ctrl(&ev) => {
                        selection.update(|s| s.move_by(-page / 2, len, height))
                    }
                    KeyCode::PageDown => selection.update(|s| s.move_by(page, len, height)),
                    KeyCode::PageUp => selection.update(|s| s.move_by(-page, len, height)),
                    KeyCode::Char('g') | KeyCode::Home => selection.update(Selection::to_top),
                    KeyCode::Char('G') | KeyCode::End => {
                        selection.update(|s| s.to_bottom(len, height))
                    }

                    // Re-sorting moves rows out from under the cursor, so
                    // the view returns to the top rather than landing the
                    // user somewhere arbitrary.
                    KeyCode::Char('<') => {
                        sort.update(|s| *s = previous_sort(*s));
                        selection.update(Selection::to_top);
                    }
                    KeyCode::Char('>') => {
                        sort.update(|s| *s = next_sort(*s));
                        selection.update(Selection::to_top);
                    }
                    KeyCode::Char('I') => {
                        descending.update(|d| *d = !*d);
                        selection.update(Selection::to_top);
                    }
                    _ => {}
                }
            });
        }

        let status = format!(
            " {} sorted by {} {} · j/k move · ^d/^u page · g/G ends · < > sort · I reverse · q quit",
            rows.len(),
            sort_name(sort.get()),
            if descending.get() { "desc" } else { "asc" },
        );

        element! {
            View(flex_direction: FlexDirection::Column, height: Dimension::Percent(1.0)) {
                Meters(sample: current.clone())
                ProcessTable(
                    rows: Shared::new(rows),
                    selection: cursor,
                    height: table_rows,
                    sort: sort.get(),
                )
                View(height: Dimension::Cells(1), background: Color::Blue) {
                    Text(
                        content: status,
                        color: theme::SELECTED_FG,
                        weight: Weight::Bold,
                        wrap: TextWrap::Truncate,
                    )
                }
            }
        }
    }
}

fn ctrl(ev: &ntui::KeyEvent) -> bool {
    ev.modifiers.contains(ntui::KeyModifiers::CONTROL)
}

const SORT_ORDER: [SortKey; 5] = [
    SortKey::Pid,
    SortKey::Name,
    SortKey::Cpu,
    SortKey::Memory,
    SortKey::Time,
];

fn next_sort(current: SortKey) -> SortKey {
    let i = SORT_ORDER.iter().position(|k| *k == current).unwrap_or(0);
    SORT_ORDER[(i + 1) % SORT_ORDER.len()]
}

fn previous_sort(current: SortKey) -> SortKey {
    let i = SORT_ORDER.iter().position(|k| *k == current).unwrap_or(0);
    SORT_ORDER[(i + SORT_ORDER.len() - 1) % SORT_ORDER.len()]
}

pub fn sort_name(key: SortKey) -> &'static str {
    match key {
        SortKey::Pid => "PID",
        SortKey::Name => "Command",
        SortKey::Cpu => "CPU%",
        SortKey::Memory => "RES",
        SortKey::Time => "TIME+",
    }
}
