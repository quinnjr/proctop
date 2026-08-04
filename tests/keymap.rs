use ntui::{KeyCode, KeyEvent, KeyModifiers};
use rtop::actions::Signal;
use rtop::model::{Proc, ProcRow};
use rtop::sort::SortKey;
use rtop::ui::state::{Effect, Mode, Overlay, Tab, UiState, handle_key};

const HEIGHT: usize = 10;

fn rows(n: usize) -> Vec<ProcRow> {
    (0..n)
        .map(|i| ProcRow {
            proc: Proc {
                pid: 100 + i as i32,
                name: format!("proc-{i}"),
                nice: 0,
                ..Proc::default()
            },
            user: if i % 2 == 0 { "root" } else { "joseph" }.to_string(),
            ..ProcRow::default()
        })
        .collect()
}

fn press(state: &mut UiState, code: KeyCode) -> Effect {
    handle_key(
        state,
        KeyEvent::new(code, KeyModifiers::NONE),
        &rows(50),
        HEIGHT,
    )
}

fn press_ctrl(state: &mut UiState, code: KeyCode) -> Effect {
    handle_key(
        state,
        KeyEvent::new(code, KeyModifiers::CONTROL),
        &rows(50),
        HEIGHT,
    )
}

fn type_str(state: &mut UiState, text: &str) {
    for c in text.chars() {
        press(state, KeyCode::Char(c));
    }
}

// ---------- navigation ----------

#[test]
fn moves_the_cursor_with_vim_keys() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::Char('j'));
    press(&mut state, KeyCode::Char('j'));

    assert_eq!(state.selection.index, 2);

    press(&mut state, KeyCode::Char('k'));

    assert_eq!(state.selection.index, 1);
}

#[test]
fn jumps_to_the_ends() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::Char('G'));
    assert_eq!(state.selection.index, 49);

    press(&mut state, KeyCode::Char('g'));
    assert_eq!(state.selection.index, 0);
}

#[test]
fn moves_a_half_page_only_with_control_held() {
    let mut state = UiState::default();

    press_ctrl(&mut state, KeyCode::Char('d'));

    assert_eq!(state.selection.index, HEIGHT as isize as usize / 2);
}

// ---------- quitting ----------

#[test]
fn quits_on_q() {
    let mut state = UiState::default();

    assert_eq!(press(&mut state, KeyCode::Char('q')), Effect::Quit);
}

#[test]
fn escape_clears_an_active_filter_before_it_quits() {
    // Otherwise an incremental search becomes a way to accidentally exit.
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('/'));
    type_str(&mut state, "proc");
    press(&mut state, KeyCode::Enter);
    assert!(state.filter.is_active());

    assert_eq!(press(&mut state, KeyCode::Esc), Effect::None);
    assert!(!state.filter.is_active());

    assert_eq!(press(&mut state, KeyCode::Esc), Effect::Quit);
}

// ---------- search ----------

#[test]
fn search_narrows_the_filter_as_each_character_is_typed() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::Char('/'));
    assert_eq!(state.mode, Mode::Search(String::new()));

    type_str(&mut state, "fire");

    assert_eq!(state.filter.query, "fire");
}

#[test]
fn backspace_widens_the_search_again() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('/'));
    type_str(&mut state, "fire");

    press(&mut state, KeyCode::Backspace);

    assert_eq!(state.filter.query, "fir");
}

#[test]
fn enter_accepts_a_search_and_keeps_the_filter() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('/'));
    type_str(&mut state, "zsh");

    press(&mut state, KeyCode::Enter);

    assert_eq!(state.mode, Mode::Normal);
    assert_eq!(state.filter.query, "zsh");
}

#[test]
fn escape_abandons_a_search_and_restores_the_full_list() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('/'));
    type_str(&mut state, "zsh");

    press(&mut state, KeyCode::Esc);

    assert_eq!(state.mode, Mode::Normal);
    assert_eq!(state.filter.query, "");
}

#[test]
fn typing_j_while_searching_types_a_j_rather_than_moving() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('/'));

    type_str(&mut state, "j");

    assert_eq!(state.filter.query, "j");
    assert_eq!(state.selection.index, 0);
}

// ---------- command line ----------

#[test]
fn the_command_line_quits() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char(':'));
    type_str(&mut state, "q");

    assert_eq!(press(&mut state, KeyCode::Enter), Effect::Quit);
}

#[test]
fn the_command_line_sets_the_sort_column() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char(':'));
    type_str(&mut state, "sort mem");
    press(&mut state, KeyCode::Enter);

    assert_eq!(state.sort, SortKey::Memory);
    assert_eq!(state.mode, Mode::Normal);
}

#[test]
fn an_unknown_sort_column_explains_itself_rather_than_failing_silently() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char(':'));
    type_str(&mut state, "sort nonsense");
    press(&mut state, KeyCode::Enter);

    let notice = state.notice.expect("should explain");
    assert!(notice.contains("nonsense"), "{notice}");
    assert!(notice.contains("cpu"), "should list the options: {notice}");
}

#[test]
fn an_unknown_command_says_so() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char(':'));
    type_str(&mut state, "frobnicate");
    press(&mut state, KeyCode::Enter);

    assert!(state.notice.unwrap().contains("frobnicate"));
}

#[test]
fn backspacing_past_the_start_leaves_command_mode() {
    // Otherwise the prompt gets stuck open and empty with no way out but Esc.
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char(':'));

    press(&mut state, KeyCode::Backspace);

    assert_eq!(state.mode, Mode::Normal);
}

// ---------- sorting and views ----------

#[test]
fn cycles_the_sort_column_in_both_directions() {
    let mut state = UiState::default();
    assert_eq!(state.sort, SortKey::Cpu);

    press(&mut state, KeyCode::Char('>'));
    assert_eq!(state.sort, SortKey::Memory);

    press(&mut state, KeyCode::Char('<'));
    assert_eq!(state.sort, SortKey::Cpu);
}

#[test]
fn wraps_around_the_ends_of_the_sort_columns() {
    let mut state = UiState {
        sort: SortKey::Pid,
        ..UiState::default()
    };

    press(&mut state, KeyCode::Char('<'));

    assert_eq!(state.sort, SortKey::Time, "should wrap to the last column");
}

#[test]
fn returns_to_the_top_when_the_order_changes_underneath_the_cursor() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('G'));
    assert_ne!(state.selection.index, 0);

    press(&mut state, KeyCode::Char('>'));

    assert_eq!(state.selection.index, 0);
}

#[test]
fn toggles_the_tree_view() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::Char('t'));
    assert!(state.tree_view);

    press(&mut state, KeyCode::Char('t'));
    assert!(!state.tree_view);
}

#[test]
fn toggles_kernel_threads() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::Char('H'));

    assert!(state.filter.hide_kernel_threads);
}

#[test]
fn filters_to_the_selected_processs_owner_and_back() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::Char('u'));
    assert_eq!(state.filter.user.as_deref(), Some("root"));

    press(&mut state, KeyCode::Char('u'));
    assert_eq!(state.filter.user, None);
}

// ---------- tabs ----------

#[test]
fn cycles_tabs_with_tab_and_selects_them_by_number() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::Tab);
    assert_eq!(state.tab, Tab::Io);

    press(&mut state, KeyCode::Tab);
    assert_eq!(state.tab, Tab::Sensors);

    press(&mut state, KeyCode::Tab);
    assert_eq!(state.tab, Tab::Processes, "should wrap");

    press(&mut state, KeyCode::Char('3'));
    assert_eq!(state.tab, Tab::Sensors);
}

// ---------- overlays ----------

#[test]
fn opens_help_and_dismisses_it_with_any_key() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::Char('?'));
    assert_eq!(state.overlay, Overlay::Help);

    press(&mut state, KeyCode::Char('x'));
    assert_eq!(state.overlay, Overlay::None);
}

#[test]
fn a_key_pressed_over_the_help_overlay_does_not_also_reach_the_table() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('?'));

    press(&mut state, KeyCode::Char('j'));

    assert_eq!(
        state.selection.index, 0,
        "j dismissed help, it did not scroll"
    );
}

#[test]
fn opens_the_detail_pane_for_the_selected_process() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('j'));

    press(&mut state, KeyCode::Enter);

    assert_eq!(state.overlay, Overlay::Detail { pid: 101 });
}

// ---------- kill ----------

#[test]
fn d_opens_a_kill_confirmation_rather_than_killing_immediately() {
    let mut state = UiState::default();

    let effect = press(&mut state, KeyCode::Char('d'));

    assert_eq!(
        effect,
        Effect::None,
        "nothing should die from one keystroke"
    );
    assert!(matches!(state.overlay, Overlay::Kill { pid: 100, .. }));
}

#[test]
fn the_kill_dialog_defaults_to_the_signal_that_asks_politely() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('d'));

    let effect = press(&mut state, KeyCode::Enter);

    assert_eq!(
        effect,
        Effect::Kill {
            pid: 100,
            signal: Signal::Term
        }
    );
}

#[test]
fn the_kill_dialog_picks_another_signal() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('d'));

    press(&mut state, KeyCode::Char('j'));
    let effect = press(&mut state, KeyCode::Enter);

    assert_eq!(
        effect,
        Effect::Kill {
            pid: 100,
            signal: Signal::Kill
        }
    );
}

#[test]
fn escape_cancels_a_kill() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('d'));

    let effect = press(&mut state, KeyCode::Esc);

    assert_eq!(effect, Effect::None);
    assert_eq!(state.overlay, Overlay::None);
}

#[test]
fn d_does_nothing_when_the_list_is_empty() {
    let mut state = UiState::default();

    let effect = handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        &[],
        HEIGHT,
    );

    assert_eq!(effect, Effect::None);
    assert_eq!(state.overlay, Overlay::None);
}

// ---------- renice ----------

#[test]
fn renice_starts_from_the_processs_current_value() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::Char('n'));

    assert!(matches!(&state.overlay, Overlay::Renice { input, .. } if input == "0"));
}

#[test]
fn renice_accepts_a_value_in_range() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('n'));
    press(&mut state, KeyCode::Backspace);
    type_str(&mut state, "10");

    let effect = press(&mut state, KeyCode::Enter);

    assert_eq!(effect, Effect::Renice { pid: 100, nice: 10 });
}

#[test]
fn renice_refuses_a_value_outside_the_kernels_range() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('n'));
    press(&mut state, KeyCode::Backspace);
    type_str(&mut state, "99");

    let effect = press(&mut state, KeyCode::Enter);

    assert_eq!(effect, Effect::None);
    assert!(state.notice.expect("should explain").contains("19"));
}

#[test]
fn renice_ignores_characters_a_nice_value_could_never_contain() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('n'));
    press(&mut state, KeyCode::Backspace);
    type_str(&mut state, "1a2");

    assert!(matches!(&state.overlay, Overlay::Renice { input, .. } if input == "12"));
}
