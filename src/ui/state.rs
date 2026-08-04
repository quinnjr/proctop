//! The UI's state, and the keymap that drives it.
//!
//! Deliberately free of any dependency on the renderer: `handle_key` is a
//! pure function from state and a key to the next state plus an effect to
//! perform. That makes the entire interaction model — every mode, every
//! overlay, every edge of the process list — testable without a terminal.

use ntui::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::{NICE_MAX, NICE_MIN, SIGNALS, Signal};
use crate::filter::Filter;
use crate::model::{ProcKey, ProcRow};
use crate::sort::SortKey;
use crate::ui::Selection;

/// Which view is on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Processes,
    Disk,
    Network,
    Sensors,
}

/// Which scrollable list a tab shows, if any.
///
/// `UiState::cursor` picks the cursor and `Lists::extent` picks the bounds
/// it moves against; both dispatch on this rather than on `Tab` directly,
/// so giving a tab a list is one edit that both sides read. Two independent
/// `match self.tab` arms is how a cursor ends up moving against another
/// list's length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Driven {
    Processes,
    Sockets,
    /// The tab shows no list: movement keys do nothing at all.
    Nothing,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Processes, Tab::Disk, Tab::Network, Tab::Sensors];

    /// The list this tab's movement keys drive.
    pub fn driven(self) -> Driven {
        match self {
            Tab::Processes => Driven::Processes,
            Tab::Network => Driven::Sockets,
            Tab::Disk | Tab::Sensors => Driven::Nothing,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::Processes => "Processes",
            Tab::Disk => "Disk",
            Tab::Network => "Network",
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
    /// Confirming a signal for a process, with `index` into [`SIGNALS`].
    Kill {
        key: ProcKey,
        name: String,
        index: usize,
    },
    /// Typing a new nice value for a process.
    Renice {
        key: ProcKey,
        name: String,
        input: String,
    },
    /// Everything known about the selected process.
    Detail {
        key: ProcKey,
    },
}

/// Everything the UI remembers between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiState {
    pub tab: Tab,
    /// Cursor into the process table.
    pub selection: Selection,
    /// Cursor into the Network tab's listening list.
    ///
    /// Separate from `selection` because the two lists have nothing to do
    /// with each other: sharing one cursor meant scrolling the process
    /// table scrolled the socket list off its own viewport, and there was
    /// no key that could bring it back.
    pub socket_selection: Selection,
    pub sort: SortKey,
    pub descending: bool,
    pub filter: Filter,
    pub tree_view: bool,
    pub mode: Mode,
    pub overlay: Overlay,
    /// A one-line result or error from the last action.
    pub notice: Option<String>,
    /// A multi-key sequence in progress — currently only `d`, awaiting the
    /// second `d` of `dd`. Cleared by any key that is not its continuation.
    pub pending: Option<char>,
}

/// Something `handle_key` decided but cannot do itself, because it touches
/// the world outside the UI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Effect {
    #[default]
    None,
    Quit,
    /// Signal a process. Carries the full identity, not a bare pid: a
    /// dialog can sit open while the process exits and its number is
    /// recycled, and signalling the stranger that inherited it would be the
    /// worst possible outcome of pressing Enter.
    Kill {
        pid: i32,
        starttime: u64,
        signal: Signal,
    },
    Renice {
        pid: i32,
        starttime: u64,
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

    /// The cursor the movement keys drive, or `None` on a tab with no list.
    ///
    /// `None` rather than falling back to the process cursor: `j` on the
    /// Disk tab used to scroll the process table off screen, so switching
    /// back left the cursor on a row the user never chose — with `dd` two
    /// keystrokes away from it.
    fn cursor(&mut self) -> Option<&mut Selection> {
        match self.tab.driven() {
            Driven::Processes => Some(&mut self.selection),
            Driven::Sockets => Some(&mut self.socket_selection),
            Driven::Nothing => None,
        }
    }

    /// Whether the process list is the thing on screen.
    ///
    /// The actions — `Enter`, `dd`, `n`, `u` — all name the selected
    /// process, so off this tab there is nothing for them to act on. Left
    /// ungated, they acted on whatever row the process cursor happened to
    /// rest on, which the user could not see.
    fn on_processes(&self) -> bool {
        self.tab.driven() == Driven::Processes
    }
}

/// The lists a cursor can move through, as currently displayed.
///
/// Which one the movement keys drive depends on the tab, so the keymap
/// needs the extent of both. Passing only the process list is what let `j`
/// on the Network tab scroll a table nobody was looking at.
#[derive(Clone, Copy, Default)]
#[non_exhaustive]
pub struct Lists<'a> {
    /// Processes, already filtered and sorted.
    pub procs: &'a [ProcRow],
    /// How many process rows fit on screen.
    pub proc_height: usize,
    /// How many listening sockets the Network tab is showing.
    ///
    /// A count rather than the slice: the only thing the keymap ever asks
    /// is how far the cursor may travel, and holding a borrow of the list
    /// forced `App` to clone every socket into the input closure once a
    /// frame.
    pub sockets: usize,
    /// How many socket rows fit on screen.
    pub socket_height: usize,
}

impl<'a> Lists<'a> {
    /// The process list and how many of its rows are on screen.
    pub fn processes(procs: &'a [ProcRow], height: usize) -> Self {
        Lists {
            procs,
            proc_height: height,
            ..Lists::default()
        }
    }

    /// Add the extent of the Network tab's listening list.
    pub fn with_sockets(self, sockets: usize, height: usize) -> Self {
        Lists {
            sockets,
            socket_height: height,
            ..self
        }
    }
}

impl Lists<'_> {
    /// The length and visible height of whichever list `tab` is showing.
    fn extent(&self, tab: Tab) -> (usize, usize) {
        match tab.driven() {
            Driven::Processes => (self.procs.len(), self.proc_height),
            Driven::Sockets => (self.sockets, self.socket_height),
            Driven::Nothing => (0, 0),
        }
    }
}

/// Apply one key press.
pub fn handle_key(state: &mut UiState, key: KeyEvent, lists: Lists) -> Effect {
    // Anything typed while an overlay is open belongs to the overlay.
    if state.overlay != Overlay::None {
        return handle_overlay_key(state, key);
    }
    match &state.mode {
        Mode::Normal => handle_normal_key(state, key, lists),
        Mode::Search(_) => handle_search_key(state, key),
        Mode::Command(_) => handle_command_key(state, key),
    }
}

fn handle_normal_key(state: &mut UiState, key: KeyEvent, lists: Lists) -> Effect {
    let rows = lists.procs;
    let (len, height) = lists.extent(state.tab);
    let page = height.max(1) as isize;
    // Clearing on the next keystroke keeps a stale "killed 1234" from
    // sitting in the status bar indefinitely.
    state.notice = None;

    // A multi-key sequence in progress claims this key, whether or not it
    // completes. Taken unconditionally so any other key abandons it rather
    // than leaving a prefix armed for later; anything that is not the
    // completion falls through and is treated as its own key.
    //
    // The modifier check is load-bearing: Ctrl-D's `KeyCode` is also
    // `Char('d')`, so comparing the code alone let the documented
    // half-page-down key finish `dd` and open a destructive dialog.
    if state.pending.take() == Some('d') && is_plain(&key, 'd') {
        if state.on_processes()
            && let Some(row) = state.selected(rows)
        {
            state.overlay = Overlay::Kill {
                key: row.proc.key(),
                name: row.proc.name.clone(),
                index: 0,
            };
        }
        return Effect::None;
    }

    match key.code {
        KeyCode::Char('q') => return Effect::Quit,
        KeyCode::Esc => {
            // Esc backs out of a filter before it quits, so an incremental
            // search does not become a way to accidentally exit.
            if state.filter.is_active() {
                state.filter = Filter::default();
                state.selection.to_start();
            } else {
                return Effect::Quit;
            }
        }

        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(c) = state.cursor() {
                c.move_by(1, len, height);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(c) = state.cursor() {
                c.move_by(-1, len, height);
            }
        }
        KeyCode::Char('d') if ctrl(&key) => {
            if let Some(c) = state.cursor() {
                c.move_by(page / 2, len, height);
            }
        }
        KeyCode::Char('u') if ctrl(&key) => {
            if let Some(c) = state.cursor() {
                c.move_by(-page / 2, len, height);
            }
        }
        KeyCode::PageDown => {
            if let Some(c) = state.cursor() {
                c.move_by(page, len, height);
            }
        }
        KeyCode::PageUp => {
            if let Some(c) = state.cursor() {
                c.move_by(-page, len, height);
            }
        }
        KeyCode::Char('g') | KeyCode::Home => {
            if let Some(c) = state.cursor() {
                c.to_start();
            }
        }
        KeyCode::Char('G') | KeyCode::End => {
            if let Some(c) = state.cursor() {
                c.to_end(len, height);
            }
        }

        KeyCode::Char('/') => state.mode = Mode::Search(state.filter.query.clone()),
        KeyCode::Char(':') => state.mode = Mode::Command(String::new()),
        KeyCode::Char('?') => state.overlay = Overlay::Help,

        // Sorting moves rows out from under the cursor, so the view returns
        // to the top rather than landing somewhere arbitrary.
        KeyCode::Char('<') => {
            state.sort = step_sort(state.sort, -1);
            state.selection.to_start();
        }
        KeyCode::Char('>') => {
            state.sort = step_sort(state.sort, 1);
            state.selection.to_start();
        }
        KeyCode::Char('I') => {
            state.descending = !state.descending;
            state.selection.to_start();
        }

        KeyCode::Char('t') => {
            state.tree_view = !state.tree_view;
            state.selection.to_start();
        }
        KeyCode::Char('H') => {
            state.filter.hide_kernel_threads = !state.filter.hide_kernel_threads;
            state.selection.to_start();
        }
        // Clearing an active filter needs no visible row, so it works from
        // any tab.
        KeyCode::Char('u') if state.filter.user.is_some() => {
            state.filter.user = None;
            state.selection.to_start();
        }
        // Setting one names the selected process, so it needs a row the
        // user can actually see.
        KeyCode::Char('u') if state.on_processes() => {
            state.filter.user = state.selected(rows).map(|r| r.user.to_string());
            state.selection.to_start();
        }

        KeyCode::Tab | KeyCode::Char('\t') => state.tab = step_tab(state.tab, 1),
        KeyCode::BackTab => state.tab = step_tab(state.tab, -1),
        KeyCode::Char(c @ '1'..='4') => {
            state.tab = Tab::ALL[c as usize - '1' as usize];
        }

        KeyCode::Enter => {
            if state.on_processes()
                && let Some(row) = state.selected(rows)
            {
                state.overlay = Overlay::Detail {
                    key: row.proc.key(),
                };
            }
        }
        // `dd`, vim's delete: this arms the prefix, and the second `d`
        // (handled above) opens the confirmation. Two keystrokes plus a
        // confirmation, and deliberately not `k`, which is navigation in a
        // list being actively scrolled.
        KeyCode::Char('d') if !ctrl(&key) && state.on_processes() => state.pending = Some('d'),
        KeyCode::Char('n') => {
            if state.on_processes()
                && let Some(row) = state.selected(rows)
            {
                state.overlay = Overlay::Renice {
                    key: row.proc.key(),
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
    state.selection.to_start();
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
            state.selection.to_start();
        }
        "sort" => match argument.and_then(SortKey::from_word) {
            Some(key) => {
                state.sort = key;
                state.selection.to_start();
            }
            None => {
                state.notice = Some(format!(
                    "sort: expected one of {} (got {})",
                    SortKey::all_spellings(),
                    argument.unwrap_or("nothing")
                ));
            }
        },
        "filter" => {
            state.filter.query = argument.unwrap_or_default().to_string();
            state.selection.to_start();
        }
        "user" => {
            state.filter.user = argument.map(str::to_string);
            state.selection.to_start();
        }
        "help" => state.overlay = Overlay::Help,
        other => state.notice = Some(format!("unknown command: {other}")),
    }
    Effect::None
}

fn handle_overlay_key(state: &mut UiState, key: KeyEvent) -> Effect {
    match &mut state.overlay {
        Overlay::None => Effect::None,

        Overlay::Help | Overlay::Detail { .. } => {
            // Any key dismisses, since neither takes input of its own.
            state.overlay = Overlay::None;
            Effect::None
        }

        Overlay::Kill {
            key: proc, index, ..
        } => match key.code {
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
                    pid: proc.pid,
                    starttime: proc.starttime,
                    signal: SIGNALS[*index],
                };
                state.overlay = Overlay::None;
                effect
            }
            _ => Effect::None,
        },

        Overlay::Renice {
            key: proc, input, ..
        } => match key.code {
            // `q` closes this the way it closes the kill dialog; it is not a
            // character a nice value can contain, so there is no conflict
            // with the text field.
            KeyCode::Esc | KeyCode::Char('q') => {
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
                    let effect = Effect::Renice {
                        pid: proc.pid,
                        starttime: proc.starttime,
                        nice,
                    };
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

/// Whether this is `c` pressed with no modifier holding it down.
///
/// Multi-key sequences must match on the whole event, not just the code:
/// several `KeyCode::Char(_)` values are shared with their control-modified
/// forms, so a code-only comparison silently accepts the wrong key.
fn is_plain(key: &KeyEvent, c: char) -> bool {
    key.code == KeyCode::Char(c)
        && !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}
