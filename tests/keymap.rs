use ntui::{KeyCode, KeyEvent, KeyModifiers};
use rtop::actions::Signal;
use rtop::model::{ListeningSocket, Proc, ProcRow, Protocol, Socket};
use rtop::sort::SortKey;
use rtop::ui::state::{Effect, Lists, Mode, Overlay, Tab, UiState, handle_key};

const HEIGHT: usize = 10;

/// The process list, with no sockets — what every test here drives unless
/// it says otherwise.
fn procs(rows: &[ProcRow]) -> Lists<'_> {
    Lists::processes(rows, HEIGHT)
}

fn rows(n: usize) -> Vec<ProcRow> {
    (0..n)
        .map(|i| ProcRow {
            proc: Proc {
                pid: 100 + i as i32,
                name: format!("proc-{i}"),
                nice: 0,
                ..Proc::default()
            },
            user: if i % 2 == 0 { "root" } else { "joseph" }.into(),
            ..ProcRow::default()
        })
        .collect()
}

fn press(state: &mut UiState, code: KeyCode) -> Effect {
    handle_key(
        state,
        KeyEvent::new(code, KeyModifiers::NONE),
        procs(&rows(50)),
    )
}

fn press_ctrl(state: &mut UiState, code: KeyCode) -> Effect {
    handle_key(
        state,
        KeyEvent::new(code, KeyModifiers::CONTROL),
        procs(&rows(50)),
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

    for expected in [Tab::Disk, Tab::Network, Tab::Sensors, Tab::Processes] {
        press(&mut state, KeyCode::Tab);
        assert_eq!(state.tab, expected, "Tab should advance to {expected:?}");
    }

    press(&mut state, KeyCode::Char('3'));
    assert_eq!(state.tab, Tab::Network);
    press(&mut state, KeyCode::Char('4'));
    assert_eq!(state.tab, Tab::Sensors);
}

#[test]
fn every_tab_has_a_number_key_and_they_agree_with_the_order() {
    // The digits are computed from the enum order; a tab added without a
    // digit, or a digit that lands on the wrong tab, is silent otherwise.
    for (i, expected) in Tab::ALL.iter().enumerate() {
        let mut state = UiState::default();
        let digit = char::from_digit(i as u32 + 1, 10).expect("one digit per tab");

        press(&mut state, KeyCode::Char(digit));

        assert_eq!(state.tab, *expected, "key {digit}");
    }
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

    assert!(matches!(&state.overlay, Overlay::Detail { key } if key.pid == 101));
}

// ---------- kill ----------

/// Open the kill dialog the way a user does: the `dd` sequence.
fn press_dd(state: &mut UiState) {
    press(state, KeyCode::Char('d'));
    press(state, KeyCode::Char('d'));
}

#[test]
fn a_single_d_opens_nothing() {
    // The binding is `dd`, vim's delete. One `d` is a prefix, not a command:
    // a destructive action reachable from a single keystroke in a list the
    // user is actively scrolling is exactly what the two-key sequence avoids.
    let mut state = UiState::default();

    let effect = press(&mut state, KeyCode::Char('d'));

    assert_eq!(effect, Effect::None);
    assert_eq!(state.overlay, Overlay::None);
}

#[test]
fn dd_opens_a_kill_confirmation_rather_than_killing_immediately() {
    let mut state = UiState::default();

    press_dd(&mut state);

    assert!(matches!(&state.overlay, Overlay::Kill { key, .. } if key.pid == 100));
}

#[test]
fn a_d_followed_by_anything_else_abandons_the_prefix() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::Char('d'));
    press(&mut state, KeyCode::Char('j'));

    assert_eq!(state.overlay, Overlay::None, "j is not the second d");
    assert_eq!(state.selection.index, 1, "and j still moves the cursor");

    // The prefix must not survive the interruption.
    press(&mut state, KeyCode::Char('d'));
    assert_eq!(state.overlay, Overlay::None, "this d starts a fresh prefix");
}

#[test]
fn the_kill_prefix_does_not_leak_into_search() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('d'));

    press(&mut state, KeyCode::Char('/'));
    type_str(&mut state, "d");

    assert_eq!(state.filter.query, "d", "the d is typed, not a prefix");
    assert_eq!(state.overlay, Overlay::None);
}

#[test]
fn the_kill_dialog_defaults_to_the_signal_that_asks_politely() {
    let mut state = UiState::default();
    press_dd(&mut state);

    let effect = press(&mut state, KeyCode::Enter);

    assert_eq!(
        effect,
        Effect::Kill {
            pid: 100,
            starttime: 0,
            signal: Signal::Term
        }
    );
}

#[test]
fn the_kill_dialog_picks_another_signal() {
    let mut state = UiState::default();
    press_dd(&mut state);

    press(&mut state, KeyCode::Char('j'));
    let effect = press(&mut state, KeyCode::Enter);

    assert_eq!(
        effect,
        Effect::Kill {
            pid: 100,
            starttime: 0,
            signal: Signal::Kill
        }
    );
}

#[test]
fn escape_cancels_a_kill() {
    let mut state = UiState::default();
    press_dd(&mut state);

    let effect = press(&mut state, KeyCode::Esc);

    assert_eq!(effect, Effect::None);
    assert_eq!(state.overlay, Overlay::None);
}

#[test]
fn dd_does_nothing_when_the_list_is_empty() {
    let mut state = UiState::default();

    for _ in 0..2 {
        let effect = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
            procs(&[]),
        );
        assert_eq!(effect, Effect::None);
    }

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

    assert_eq!(
        effect,
        Effect::Renice {
            pid: 100,
            starttime: 0,
            nice: 10
        }
    );
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

// ---------- documented aliases ----------

#[test]
fn every_arrow_and_named_key_matches_its_vim_counterpart() {
    // The README documents both spellings of each movement. Only the vim
    // half was ever tested, so the arrow and named keys were free to drift.
    for (vim, alias) in [
        (KeyCode::Char('j'), KeyCode::Down),
        (KeyCode::Char('k'), KeyCode::Up),
        (KeyCode::Char('g'), KeyCode::Home),
        (KeyCode::Char('G'), KeyCode::End),
    ] {
        let mut with_vim = UiState::default();
        let mut with_alias = UiState::default();
        // Start away from the ends so every key has somewhere to go.
        for state in [&mut with_vim, &mut with_alias] {
            press(state, KeyCode::Char('j'));
            press(state, KeyCode::Char('j'));
        }

        press(&mut with_vim, vim);
        press(&mut with_alias, alias);

        assert_eq!(
            with_vim.selection, with_alias.selection,
            "{alias:?} should do what {vim:?} does"
        );
    }
}

#[test]
fn page_keys_move_a_full_page_in_each_direction() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::PageDown);
    assert_eq!(state.selection.index, HEIGHT);

    press(&mut state, KeyCode::PageUp);
    assert_eq!(state.selection.index, 0);
}

#[test]
fn control_u_moves_back_a_half_page() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('G'));
    let bottom = state.selection.index;

    press_ctrl(&mut state, KeyCode::Char('u'));

    assert_eq!(state.selection.index, bottom - HEIGHT / 2);
}

#[test]
fn shift_tab_walks_the_tabs_backwards() {
    let mut state = UiState::default();

    press(&mut state, KeyCode::BackTab);

    assert_eq!(state.tab, Tab::Sensors, "wraps to the last tab");
}

#[test]
fn q_closes_the_renice_dialog_the_way_it_closes_the_kill_dialog() {
    let mut state = UiState::default();
    press(&mut state, KeyCode::Char('n'));

    let effect = press(&mut state, KeyCode::Char('q'));

    assert_eq!(effect, Effect::None, "and does not quit the app");
    assert_eq!(state.overlay, Overlay::None);
}

#[test]
fn the_help_overlay_lists_only_keys_the_keymap_answers_to() {
    // The help text is a claim about behavior; nothing kept it honest.
    for (keys, description) in rtop::ui::help::BINDINGS {
        // Section headings carry no key, and `:command <arg>` entries are
        // command spellings rather than keys.
        if keys.is_empty() || keys.starts_with(':') {
            continue;
        }
        for key in keys.split(&[' ', '/', ','][..]).filter(|k| !k.is_empty()) {
            assert!(
                rtop::ui::help::is_documented_key(key),
                "help lists {key:?} ({description}) but the keymap has no such binding"
            );
        }
    }
}

#[test]
fn a_modified_d_does_not_complete_the_kill_sequence() {
    // Ctrl-D's KeyCode is Char('d') too. Comparing only the code let the
    // documented half-page-down key finish `dd` and open a destructive
    // dialog — the exact footgun the two-key sequence exists to prevent.
    let mut state = UiState::default();

    press(&mut state, KeyCode::Char('d'));
    press_ctrl(&mut state, KeyCode::Char('d'));

    assert_eq!(
        state.overlay,
        Overlay::None,
        "Ctrl-D must not open the dialog"
    );
    assert_eq!(
        state.selection.index,
        HEIGHT / 2,
        "and must still scroll half a page"
    );
}

#[test]
fn a_modified_d_does_not_arm_the_kill_sequence_either() {
    let mut state = UiState::default();

    press_ctrl(&mut state, KeyCode::Char('d'));
    press(&mut state, KeyCode::Char('d'));

    assert_eq!(
        state.overlay,
        Overlay::None,
        "the first d was Ctrl-D, so this is a fresh prefix"
    );
}

#[test]
fn every_help_key_fits_the_column_it_is_rendered_in() {
    // The key column is fixed-width and truncating, so an entry longer than
    // it is silently cut on screen — documentation that reads as complete
    // in the source and is not.
    for (keys, description) in rtop::ui::help::BINDINGS {
        assert!(
            keys.chars().count() <= rtop::ui::overlay::LABEL_WIDTH as usize,
            "{keys:?} ({description}) is {} chars, column is {}",
            keys.chars().count(),
            rtop::ui::overlay::LABEL_WIDTH
        );
    }
}

// ---------- the network tab drives its own list ----------

/// A Network-tab fixture that owns its socket count, so the number is
/// stated once. The previous shape took a count it ignored and made every
/// caller repeat it, where a mismatch read as intentional.
struct Net {
    state: UiState,
    rows: Vec<ProcRow>,
    sockets: Vec<ListeningSocket>,
}

fn network(sockets: usize) -> Net {
    Net {
        state: UiState {
            tab: Tab::Network,
            ..UiState::default()
        },
        rows: rows(50),
        sockets: (0..sockets).map(|i| socket(1000 + i as u16)).collect(),
    }
}

fn socket(port: u16) -> ListeningSocket {
    ListeningSocket {
        socket: Socket {
            protocol: Protocol::Tcp,
            local: format!("0.0.0.0:{port}").parse().unwrap(),
            uid: 0,
            inode: port as u64,
            accept_queue: Some(0),
        },
        user: "root".into(),
        process: None,
    }
}

impl Net {
    fn press(&mut self, code: KeyCode) {
        handle_key(
            &mut self.state,
            KeyEvent::new(code, KeyModifiers::NONE),
            Lists::processes(&self.rows, HEIGHT).with_sockets(&self.sockets, HEIGHT),
        );
    }
}

#[test]
fn movement_on_the_network_tab_moves_the_socket_cursor_only() {
    // One shared cursor meant scrolling the process table scrolled the
    // socket list off its own viewport, with no key that could bring it
    // back — the listening section rendered as an empty body under its
    // header.
    let mut net = network(20);
    net.state.selection.index = 40;
    net.state.selection.offset = 31;

    for _ in 0..3 {
        net.press(KeyCode::Char('j'));
    }

    assert_eq!(
        net.state.socket_selection.index, 3,
        "the socket cursor moved"
    );
    assert_eq!(
        net.state.selection.index, 40,
        "the process cursor stayed where it was"
    );
}

#[test]
fn the_socket_cursor_stops_at_the_end_of_the_socket_list() {
    // Not at the end of the process list, which is longer.
    let mut net = network(4);

    for _ in 0..20 {
        net.press(KeyCode::Char('j'));
    }

    assert_eq!(net.state.socket_selection.index, 3);
}

#[test]
fn g_and_shift_g_move_the_socket_cursor() {
    let mut net = network(30);

    net.press(KeyCode::Char('G'));
    assert_eq!(net.state.socket_selection.index, 29);

    net.press(KeyCode::Char('g'));
    assert_eq!(net.state.socket_selection.index, 0);
    assert_eq!(
        net.state.selection.index, 0,
        "the process cursor never moved"
    );
}

#[test]
fn the_process_actions_do_nothing_on_the_network_tab() {
    // Every one of these names the selected process. Off the process tab
    // they were acting on a row the user could not see — `dd` offering to
    // signal an arbitrary process.
    let mut net = network(10);
    net.state.selection.index = 7;

    net.press(KeyCode::Enter);
    assert_eq!(net.state.overlay, Overlay::None, "no detail pane");

    net.press(KeyCode::Char('n'));
    assert_eq!(net.state.overlay, Overlay::None, "no renice dialog");

    for _ in 0..2 {
        net.press(KeyCode::Char('d'));
    }
    assert_eq!(net.state.overlay, Overlay::None, "no kill dialog");
}

#[test]
fn setting_a_user_filter_needs_a_visible_process_row() {
    // `u` off the process tab used to still run: it cleared nothing but
    // rewound the process cursor, which is the invisible-list scrolling the
    // whole fix exists to stop.
    let mut net = network(10);
    net.state.selection.index = 30;
    net.state.selection.offset = 21;

    net.press(KeyCode::Char('u'));

    assert_eq!(net.state.filter.user, None, "nothing to filter by off-tab");
    assert_eq!(
        net.state.selection.index, 30,
        "the process cursor must not move"
    );
    assert_eq!(net.state.selection.offset, 21);
}

#[test]
fn clearing_an_active_user_filter_works_from_any_tab() {
    // The useful half of `u` needs no visible row, so gating the whole key
    // would have cost something real.
    let mut net = network(10);
    net.state.filter.user = Some("postgres".into());

    net.press(KeyCode::Char('u'));

    assert_eq!(net.state.filter.user, None, "the filter is cleared");
}

#[test]
fn the_actions_still_work_after_coming_back_to_the_process_tab() {
    // The gate is the tab, not a latch that stays shut.
    let mut net = network(10);
    net.press(KeyCode::Char('1'));
    assert_eq!(net.state.tab, Tab::Processes);

    net.press(KeyCode::Enter);
    assert!(matches!(net.state.overlay, Overlay::Detail { .. }));
}

// ---------- tabs with no list of their own ----------

#[test]
fn movement_on_the_disk_and_sensors_tabs_scrolls_nothing() {
    // `j` on the Disk tab used to scroll the process table nobody was
    // looking at, so switching back left the cursor on a row the user never
    // chose — with `dd` two keystrokes away from it.
    for tab in [Tab::Disk, Tab::Sensors] {
        let mut state = UiState {
            tab,
            selection: rtop::ui::Selection {
                index: 12,
                offset: 5,
            },
            ..UiState::default()
        };
        let list = rows(50);

        for key in [
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('G'),
            KeyCode::Char('g'),
            KeyCode::PageDown,
        ] {
            handle_key(
                &mut state,
                KeyEvent::new(key, KeyModifiers::NONE),
                Lists::processes(&list, HEIGHT),
            );
        }

        assert_eq!(
            state.selection.index, 12,
            "{tab:?} moved the process cursor"
        );
        assert_eq!(
            state.selection.offset, 5,
            "{tab:?} scrolled the process list"
        );
    }
}
