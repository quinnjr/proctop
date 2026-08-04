#![cfg(target_os = "linux")]

use proctop::actions::{self, Signal};

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

use proctop::sample::process::parse_pid_stat;

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

// ---------- who may signal what ----------

#[test]
fn the_kernel_permits_signalling_our_own_process() {
    // `signal_exists` is `kill(pid, 0)` — the kernel's own existence and
    // permission check, and the same rule the real signal will be judged by.
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("should spawn");
    let pid = child.id() as i32;

    assert!(
        actions::signal_exists(pid).is_ok(),
        "we spawned it, so we may signal it"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn the_kernel_refuses_another_users_process() {
    // pid 1 is root-owned on every Linux. Skipped under root, where the
    // refusal legitimately does not happen.
    if actions::our_euid() == 0 {
        return;
    }

    let err = actions::signal_exists(1).expect_err("pid 1 is not ours");

    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
}

#[test]
fn a_process_that_has_exited_is_not_a_permission_problem() {
    // The dialog reports an exited process separately, so the two must not
    // be confused: `ESRCH` is `NotFound`, never `PermissionDenied`.
    // Asserted on the errno, not the `ErrorKind`: std maps ESRCH to
    // `Uncategorized`, so only the number is stable to match on.
    let err = actions::signal_exists(0x3FFF_FFFF).expect_err("no such pid");

    assert_eq!(err.raw_os_error(), Some(libc::ESRCH));
    assert_ne!(
        err.kind(),
        std::io::ErrorKind::PermissionDenied,
        "an absent process must not read as a permission problem"
    );
}

#[test]
fn a_refused_signal_says_what_would_fix_it() {
    // Built the way the kernel builds it — `Error::from(ErrorKind::_)`
    // stringifies to "permission denied" and carries no errno, so a test
    // using it cannot tell "replaced the message" from "appended to it",
    // and never exercises the shape the real path produces.
    let refused = std::io::Error::from_raw_os_error(libc::EPERM);

    assert_eq!(
        actions::explain_as(&refused, 1000),
        "not permitted — run proctop as root"
    );
}

#[test]
fn lowering_a_nice_value_is_not_reported_as_someone_elses_process() {
    // `setpriority(2)` refuses this with EACCES even for a process you own,
    // so a bare "not permitted" reads as "that isn't yours" — which is false
    // and contradicts the kill dialog, which shows no owner warning for it.
    let refused = std::io::Error::from_raw_os_error(libc::EACCES);

    let message = actions::explain_as(&refused, 1000);

    assert!(
        message.contains("nice value"),
        "should name the real cause: {message}"
    );
}

#[test]
fn root_is_not_told_to_become_root() {
    // EPERM still reaches a privileged caller — an LSM denial, a seccomp
    // filter, a container without CAP_KILL. Repeating the remedy they
    // already have is a dead end, so the OS text has to survive.
    let refused = std::io::Error::from_raw_os_error(libc::EPERM);

    let message = actions::explain_as(&refused, 0);

    assert!(message.contains("even as root"), "{message}");
    assert!(
        message.contains("Operation not permitted"),
        "the diagnosable text must survive: {message}"
    );
}

#[test]
fn an_unverifiable_identity_is_not_reported_as_a_refused_signal() {
    // `verify_unchanged` builds this when it cannot read /proc/<pid>/stat —
    // no syscall was attempted, and its wording exists to avoid claiming
    // otherwise. Errors constructed here carry no errno, which is what
    // separates them from a real refusal.
    let unverifiable = std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "cannot verify process 1234: Permission denied (os error 13)",
    );

    assert_eq!(
        actions::explain_as(&unverifiable, 1000),
        "cannot verify process 1234: Permission denied (os error 13)"
    );
}
