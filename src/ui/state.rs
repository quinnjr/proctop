//! The UI's state, and the keymap that drives it.
//!
//! Deliberately free of any dependency on the renderer: `handle_key` is a
//! pure function from state and a key to the next state plus an effect to
//! perform. That makes the entire interaction model — every mode, every
//! overlay, every edge of the process list — testable without a terminal.

use ntui::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::{NICE_MAX, NICE_MIN, SIGNALS, Signal};
use crate::filter::Filter;
use crate::model::ProcRow;
use crate::sort::SortKey;
use crate::ui::selection::Selection;

/// Which view is on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Processes,
    Io,
    Sensors,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Processes, Tab::Io, Tab::Sensors];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Processes => "Processes",
            Tab::Io => "I/O",
            Tab::Sensors => "Sensors",
        }
    }
}

/// What typing does right now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Normal,
    /// `/` — incremental filter; the table narrows as you type.
    Search(String),
    /// `:` — command line.
    Command(String),
}

/// A modal layer above everything else. At most one is open, because ntui
/// does not support nesting overlay views.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Overlay {
    #[default]
    None,
    Help,
    /// Confirming a signal for `pid`, with `index` into [`SIGNALS`].
    Kill {
        pid: i32,
        name: String,
        index: usize,
    },
    /// Typing a new nice value for `pid`.
    Renice {
        pid: i32,
        name: String,
        input: String,
    },
    /// Everything known about the selected process.
    Detail {
        pid: i32,
    },
}

/// Everything the UI remembers between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiState {
    pub tab: Tab,
    pub selection: Selection,
    pub sort: SortKey,
    pub descending: bool,
    pub filter: Filter,
    pub tree_view: bool,
    pub mode: Mode,
    pub overlay: Overlay,
    /// A one-line result or error from the last action.
    pub notice: Option<String>,
}

/// Something `handle_key` decided but cannot do itself, because it touches
/// the world outside the UI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Effect {
    #[default]
    None,
    Quit,
    Kill {
        pid: i32,
        signal: Signal,
    },
    Renice {
        pid: i32,
        nice: i32,
    },
}

/// The order `<` and `>` walk the sort columns in.
const SORT_ORDER: [SortKey; 5] = [
    SortKey::Pid,
    SortKey::Name,
    SortKey::Cpu,
    SortKey::Memory,
    SortKey::Time,
];

impl UiState {
    /// The process the cursor is on, if any.
    pub fn selected<'a>(&self, rows: &'a [ProcRow]) -> Option<&'a ProcRow> {
        rows.get(self.selection.index)
    }
}

/// Apply one key press.
///
/// `rows` is the list as currently displayed — already filtered and sorted —
/// and `height` is how many of them fit on screen.
pub fn handle_key(state: &mut UiState, key: KeyEvent, rows: &[ProcRow], height: usize) -> Effect {
    // Anything typed while an overlay is open belongs to the overlay.
    if state.overlay != Overlay::None {
        return handle_overlay_key(state, key);
    }
    match &state.mode {
        Mode::Normal => handle_normal_key(state, key, rows, height),
        Mode::Search(_) => handle_search_key(state, key),
        Mode::Command(_) => handle_command_key(state, key),
    }
}

fn handle_normal_key(
    state: &mut UiState,
    key: KeyEvent,
    rows: &[ProcRow],
    height: usize,
) -> Effect {
    let len = rows.len();
    let page = height.max(1) as isize;
    // Clearing on the next keystroke keeps a stale "killed 1234" from
    // sitting in the status bar indefinitely.
    state.notice = None;

    match key.code {
        KeyCode::Char('q') => return Effect::Quit,
        KeyCode::Esc => {
            // Esc backs out of a filter before it quits, so an incremental
            // search does not become a way to accidentally exit.
            if state.filter.is_active() {
                state.filter = Filter::default();
                state.selection.to_top();
            } else {
                return Effect::Quit;
            }
        }

        KeyCode::Char('j') | KeyCode::Down => state.selection.move_by(1, len, height),
        KeyCode::Char('k') | KeyCode::Up => state.selection.move_by(-1, len, height),
        KeyCode::Char('d') if ctrl(&key) => state.selection.move_by(page / 2, len, height),
        KeyCode::Char('u') if ctrl(&key) => state.selection.move_by(-page / 2, len, height),
        KeyCode::PageDown => state.selection.move_by(page, len, height),
        KeyCode::PageUp => state.selection.move_by(-page, len, height),
        KeyCode::Char('g') | KeyCode::Home => state.selection.to_top(),
        KeyCode::Char('G') | KeyCode::End => state.selection.to_bottom(len, height),

        KeyCode::Char('/') => state.mode = Mode::Search(state.filter.query.clone()),
        KeyCode::Char(':') => state.mode = Mode::Command(String::new()),
        KeyCode::Char('?') => state.overlay = Overlay::Help,

        // Sorting moves rows out from under the cursor, so the view returns
        // to the top rather than landing somewhere arbitrary.
        KeyCode::Char('<') => {
            state.sort = step_sort(state.sort, -1);
            state.selection.to_top();
        }
        KeyCode::Char('>') => {
            state.sort = step_sort(state.sort, 1);
            state.selection.to_top();
        }
        KeyCode::Char('I') => {
            state.descending = !state.descending;
            state.selection.to_top();
        }

        KeyCode::Char('t') => {
            state.tree_view = !state.tree_view;
            state.selection.to_top();
        }
        KeyCode::Char('H') => {
            state.filter.hide_kernel_threads = !state.filter.hide_kernel_threads;
            state.selection.to_top();
        }
        // Filter to the selected process's owner, or clear that filter if
        // it is already on.
        KeyCode::Char('u') => {
            state.filter.user = match state.filter.user {
                Some(_) => None,
                None => state.selected(rows).map(|r| r.user.clone()),
            };
            state.selection.to_top();
        }

        KeyCode::Tab | KeyCode::Char('\t') => state.tab = step_tab(state.tab, 1),
        KeyCode::BackTab => state.tab = step_tab(state.tab, -1),
        KeyCode::Char(c @ '1'..='3') => {
            state.tab = Tab::ALL[c as usize - '1' as usize];
        }

        KeyCode::Enter => {
            if let Some(row) = state.selected(rows) {
                state.overlay = Overlay::Detail { pid: row.proc.pid };
            }
        }
        // `dd`, vim's delete. Two keystrokes plus a confirmation, and
        // deliberately not `k`, which is navigation in a list being actively
        // scrolled.
        KeyCode::Char('d') => {
            if let Some(row) = state.selected(rows) {
                state.overlay = Overlay::Kill {
                    pid: row.proc.pid,
                    name: row.proc.name.clone(),
                    index: 0,
                };
            }
        }
        KeyCode::Char('n') => {
            if let Some(row) = state.selected(rows) {
                state.overlay = Overlay::Renice {
                    pid: row.proc.pid,
                    name: row.proc.name.clone(),
                    input: row.proc.nice.to_string(),
                };
            }
        }
        _ => {}
    }
    Effect::None
}

fn handle_search_key(state: &mut UiState, key: KeyEvent) -> Effect {
    let Mode::Search(query) = &mut state.mode else {
        return Effect::None;
    };
    match key.code {
        // Esc abandons the search and restores the unfiltered list; Enter
        // accepts it and leaves the filter in place.
        KeyCode::Esc => {
            state.mode = Mode::Normal;
            state.filter.query.clear();
        }
        KeyCode::Enter => state.mode = Mode::Normal,
        KeyCode::Backspace => {
            query.pop();
            state.filter.query = query.clone();
        }
        KeyCode::Char(c) => {
            query.push(c);
            state.filter.query = query.clone();
        }
        _ => return Effect::None,
    }
    state.selection.to_top();
    Effect::None
}

fn handle_command_key(state: &mut UiState, key: KeyEvent) -> Effect {
    let Mode::Command(buffer) = &mut state.mode else {
        return Effect::None;
    };
    match key.code {
        KeyCode::Esc => state.mode = Mode::Normal,
        KeyCode::Backspace => {
            // Backspacing past the start leaves command mode, so the prompt
            // cannot get stuck empty.
            if buffer.pop().is_none() {
                state.mode = Mode::Normal;
            }
        }
        KeyCode::Char(c) => buffer.push(c),
        KeyCode::Enter => {
            let command = buffer.clone();
            state.mode = Mode::Normal;
            return run_command(state, &command);
        }
        _ => {}
    }
    Effect::None
}

/// Interpret a `:` command.
fn run_command(state: &mut UiState, command: &str) -> Effect {
    let mut words = command.split_whitespace();
    let Some(verb) = words.next() else {
        return Effect::None;
    };
    let argument = words.next();

    match verb {
        "q" | "quit" => return Effect::Quit,
        "tree" => {
            state.tree_view = !state.tree_view;
            state.selection.to_top();
        }
        "sort" => match argument.and_then(parse_sort) {
            Some(key) => {
                state.sort = key;
                state.selection.to_top();
            }
            None => {
                state.notice = Some(format!(
                    "sort: expected one of pid, name, cpu, mem, time (got {})",
                    argument.unwrap_or("nothing")
                ));
            }
        },
        "filter" => {
            state.filter.query = argument.unwrap_or_default().to_string();
            state.selection.to_top();
        }
        "user" => {
            state.filter.user = argument.map(str::to_string);
            state.selection.to_top();
        }
        "help" => state.overlay = Overlay::Help,
        other => state.notice = Some(format!("unknown command: {other}")),
    }
    Effect::None
}

fn parse_sort(word: &str) -> Option<SortKey> {
    Some(match word {
        "pid" => SortKey::Pid,
        "name" | "command" => SortKey::Name,
        "cpu" => SortKey::Cpu,
        "mem" | "memory" | "res" => SortKey::Memory,
        "time" => SortKey::Time,
        _ => return None,
    })
}

fn handle_overlay_key(state: &mut UiState, key: KeyEvent) -> Effect {
    match &mut state.overlay {
        Overlay::None => Effect::None,

        Overlay::Help | Overlay::Detail { .. } => {
            // Any key dismisses, since neither takes input of its own.
            state.overlay = Overlay::None;
            Effect::None
        }

        Overlay::Kill { pid, index, .. } => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.overlay = Overlay::None;
                Effect::None
            }
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Right => {
                *index = (*index + 1) % SIGNALS.len();
                Effect::None
            }
            KeyCode::Char('k') | KeyCode::Up | KeyCode::Left => {
                *index = (*index + SIGNALS.len() - 1) % SIGNALS.len();
                Effect::None
            }
            KeyCode::Enter => {
                let effect = Effect::Kill {
                    pid: *pid,
                    signal: SIGNALS[*index],
                };
                state.overlay = Overlay::None;
                effect
            }
            _ => Effect::None,
        },

        Overlay::Renice { pid, input, .. } => match key.code {
            KeyCode::Esc => {
                state.overlay = Overlay::None;
                Effect::None
            }
            KeyCode::Backspace => {
                input.pop();
                Effect::None
            }
            // Only the characters a nice value can contain, so the field
            // cannot be filled with text that could never parse.
            KeyCode::Char(c @ ('0'..='9' | '-')) => {
                input.push(c);
                Effect::None
            }
            KeyCode::Enter => match input.parse::<i32>() {
                Ok(nice) if (NICE_MIN..=NICE_MAX).contains(&nice) => {
                    let effect = Effect::Renice { pid: *pid, nice };
                    state.overlay = Overlay::None;
                    effect
                }
                _ => {
                    state.notice = Some(format!("nice must be between {NICE_MIN} and {NICE_MAX}"));
                    Effect::None
                }
            },
            _ => Effect::None,
        },
    }
}

fn step_sort(current: SortKey, by: isize) -> SortKey {
    let i = SORT_ORDER.iter().position(|k| *k == current).unwrap_or(0) as isize;
    let len = SORT_ORDER.len() as isize;
    SORT_ORDER[(i + by).rem_euclid(len) as usize]
}

fn step_tab(current: Tab, by: isize) -> Tab {
    let i = Tab::ALL.iter().position(|t| *t == current).unwrap_or(0) as isize;
    let len = Tab::ALL.len() as isize;
    Tab::ALL[(i + by).rem_euclid(len) as usize]
}

fn ctrl(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
}
