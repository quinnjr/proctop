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

// ---------- identity-checked actions ----------

use rtop::sample::process::parse_pid_stat;

/// This process's own start time, as `/proc` reports it.
fn my_starttime() -> u64 {
    let me = std::process::id();
    let text = std::fs::read_to_string(format!("/proc/{me}/stat")).unwrap();
    parse_pid_stat(&text, 4096).unwrap().starttime
}

#[test]
fn signals_a_process_whose_identity_still_matches() {
    let me = std::process::id() as i32;

    // Signal 0 delivers nothing; this is the permission-and-existence path.
    assert!(actions::kill_if_unchanged(me, my_starttime(), Signal::Cont).is_ok());
}

#[test]
fn refuses_to_signal_a_pid_whose_start_time_has_changed() {
    // Linux recycles PIDs. A kill dialog can sit open while its process
    // exits and its number is reused, and signalling the stranger that
    // inherited it is the worst possible outcome of pressing Enter.
    let me = std::process::id() as i32;

    let err = actions::kill_if_unchanged(me, my_starttime() + 1, Signal::Kill)
        .expect_err("must not signal a different process");

    assert!(err.to_string().contains("exited"), "unhelpful: {err}");
}

#[test]
fn refuses_to_renice_a_pid_whose_start_time_has_changed() {
    let me = std::process::id() as i32;

    let err = actions::renice_if_unchanged(me, my_starttime() + 1, 5)
        .expect_err("must not renice a different process");

    assert!(err.to_string().contains("exited"), "unhelpful: {err}");
}

#[test]
fn reports_an_exited_process_rather_than_a_raw_errno() {
    let result = actions::kill_if_unchanged(0x3FFF_FFFF, 1, Signal::Term);

    assert!(result.is_err());
}

#[test]
fn renices_a_process_whose_identity_still_matches() {
    let me = std::process::id() as i32;
    let before = actions::nice_of(me).unwrap();

    actions::renice_if_unchanged(me, my_starttime(), before + 1).expect("raising is unprivileged");

    assert_eq!(actions::nice_of(me).unwrap(), before + 1);
}

#[test]
fn distinguishes_a_process_it_cannot_read_from_one_that_exited() {
    // pid 1 exists but its stat is readable, so this checks the other side:
    // an identity mismatch on a live process must say "exited", and that is
    // the only case allowed to.
    let me = std::process::id() as i32;

    let err = actions::kill_if_unchanged(me, my_starttime() + 1, Signal::Cont)
        .expect_err("identity mismatch");

    assert!(err.to_string().contains("exited"), "{err}");
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}
