#![cfg(target_os = "linux")]

use rtop::actions::{self, Signal};

#[test]
fn names_the_signals_htop_offers() {
    assert_eq!(Signal::Term.number(), 15);
    assert_eq!(Signal::Kill.number(), 9);
    assert_eq!(Signal::Hup.number(), 1);
    assert_eq!(Signal::Int.number(), 2);
    assert_eq!(Signal::Stop.number(), 19);
    assert_eq!(Signal::Cont.number(), 18);
}

#[test]
fn labels_each_signal_for_the_picker() {
    assert_eq!(Signal::Term.label(), "SIGTERM");
    assert_eq!(Signal::Kill.label(), "SIGKILL");
}

#[test]
fn defaults_to_the_signal_that_asks_politely() {
    // htop's default too. Reaching for SIGKILL first loses the process's
    // chance to clean up.
    assert_eq!(Signal::default(), Signal::Term);
}

#[test]
fn reports_success_for_a_process_that_exists() {
    // Signal 0 performs the permission and existence checks without
    // actually delivering anything, which is exactly what a test wants.
    let me = std::process::id() as i32;

    assert!(actions::signal_exists(me).is_ok());
}

#[test]
fn reports_an_error_for_a_pid_that_does_not_exist() {
    // The kernel's maximum pid is well below this on any normal system.
    let result = actions::signal_exists(0x3FFF_FFFF);

    assert!(result.is_err());
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("No such process") || message.contains("no such"),
        "unhelpful message: {message}"
    );
}

#[test]
fn refuses_a_nonsensical_pid_rather_than_signalling_a_process_group() {
    // kill(0, sig) signals every process in the caller's group, and
    // kill(-1, sig) signals every process the user can reach. Neither is
    // ever what a user selecting a row meant.
    assert!(actions::kill(0, Signal::Term).is_err());
    assert!(actions::kill(-1, Signal::Kill).is_err());
    assert!(actions::renice(0, 5).is_err());
}

#[test]
fn rejects_a_nice_value_outside_the_kernels_range() {
    let me = std::process::id() as i32;

    assert!(actions::renice(me, 20).is_err());
    assert!(actions::renice(me, -21).is_err());
}

#[test]
fn reads_back_the_nice_value_it_set() {
    // Raising niceness never needs privileges; lowering it does. Going up
    // by one keeps this test runnable as an ordinary user.
    let me = std::process::id() as i32;
    let before = actions::nice_of(me).expect("should read own nice value");

    actions::renice(me, before + 1).expect("raising niceness needs no privileges");

    assert_eq!(actions::nice_of(me).unwrap(), before + 1);
}
