//! The operations that change the system, and the rules about who may.
//!
//! Everything here is guarded against the PID values that mean something
//! other than "one process": `kill(0, ..)` signals the caller's entire
//! process group and `kill(-1, ..)` signals every process the user can
//! reach. Neither is ever what someone selecting a table row meant, so both
//! are rejected before they reach the kernel.

use std::io;

/// The signals htop offers in its kill menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Signal {
    /// The default, and htop's: asks the process to exit and lets it clean
    /// up. Reaching for `Kill` first takes that chance away.
    #[default]
    Term,
    Kill,
    Hup,
    Int,
    Stop,
    Cont,
}

/// Offered in the kill dialog, in this order.
pub const SIGNALS: [Signal; 6] = [
    Signal::Term,
    Signal::Kill,
    Signal::Hup,
    Signal::Int,
    Signal::Stop,
    Signal::Cont,
];

impl Signal {
    pub fn number(self) -> i32 {
        match self {
            Signal::Hup => libc::SIGHUP,
            Signal::Int => libc::SIGINT,
            Signal::Kill => libc::SIGKILL,
            Signal::Term => libc::SIGTERM,
            Signal::Cont => libc::SIGCONT,
            Signal::Stop => libc::SIGSTOP,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Signal::Hup => "SIGHUP",
            Signal::Int => "SIGINT",
            Signal::Kill => "SIGKILL",
            Signal::Term => "SIGTERM",
            Signal::Cont => "SIGCONT",
            Signal::Stop => "SIGSTOP",
        }
    }
}

/// The lowest and highest nice values the kernel accepts.
pub const NICE_MIN: i32 = -20;
pub const NICE_MAX: i32 = 19;

/// This process's effective uid.
///
/// The permission questions all reduce to comparing this against a target's
/// owner, so it is read here rather than threaded through the UI.
pub fn our_euid() -> u32 {
    // SAFETY: `geteuid` takes no arguments, cannot fail, and returns a
    // plain integer.
    unsafe { libc::geteuid() }
}

/// Whether `ours` would be permitted to signal a process owned by `owner`.
///
/// A **hint for the interface, never a gate.** The kernel's rule is richer
/// than this: it compares the sender's real or effective uid against the
/// target's real *or saved-set* uid, and the saved-set uid is not visible
/// in `/proc/<pid>` ownership, which reports the effective one. A process
/// that dropped privileges from your account is signalable by you and looks
/// foreign here.
///
/// So this is used to warn before a confirmation, and the syscall stays the
/// authority. An unknown owner — `None`, from a `/proc` we could not stat —
/// is treated as permitted, because discouraging someone from trying is
/// worse than letting the kernel answer.
pub fn may_signal(owner: Option<u32>, ours: u32) -> bool {
    // uid 0 carries CAP_KILL, so ownership does not enter into it.
    ours == 0 || owner.is_none_or(|uid| uid == ours)
}

/// Restate an action failure in terms of what the user can do about it.
///
/// `EPERM` reaches here as "Operation not permitted", which is accurate and
/// useless: it does not say that the process belongs to someone else, nor
/// that root is the remedy.
pub fn explain(err: &io::Error) -> String {
    if err.kind() == io::ErrorKind::PermissionDenied {
        return "not permitted — run proctop as root".to_string();
    }
    err.to_string()
}

/// Check that a process exists and that we would be permitted to signal it,
/// without delivering anything.
///
/// Signal 0 performs exactly those checks and nothing else.
pub fn signal_exists(pid: i32) -> io::Result<()> {
    checked_pid(pid)?;
    syscall(unsafe { libc::kill(pid, 0) })
}

/// Send `signal` to `pid`.
pub fn kill(pid: i32, signal: Signal) -> io::Result<()> {
    checked_pid(pid)?;
    syscall(unsafe { libc::kill(pid, signal.number()) })
}

/// Set the nice value of `pid`.
///
/// Raising niceness is always permitted; lowering it needs privileges, and
/// fails with `EPERM` without them.
pub fn renice(pid: i32, nice: i32) -> io::Result<()> {
    checked_pid(pid)?;
    if !(NICE_MIN..=NICE_MAX).contains(&nice) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("nice must be between {NICE_MIN} and {NICE_MAX}"),
        ));
    }
    syscall(unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice) })
}

/// Read the current nice value of `pid`.
pub fn nice_of(pid: i32) -> io::Result<i32> {
    checked_pid(pid)?;
    // getpriority legitimately returns -1, so errno must be cleared first
    // to tell a real value from a failure.
    unsafe { *libc::__errno_location() = 0 };
    let value = unsafe { libc::getpriority(libc::PRIO_PROCESS, pid as libc::id_t) };
    if value == -1 {
        let err = io::Error::last_os_error();
        if err.raw_os_error() != Some(0) {
            return Err(err);
        }
    }
    Ok(value)
}

/// Send `signal` to `pid`, but only if it is still the process that started
/// at `starttime`.
///
/// Linux recycles PIDs. Anything that captures a pid and acts on it later —
/// a confirmation dialog the user leaves open, a queued action — can find
/// the number reattached to something else by the time it fires, and
/// signalling that stranger is the worst outcome available. Identity here
/// means the same `(pid, starttime)` pair the sampler keys on.
pub fn kill_if_unchanged(pid: i32, starttime: u64, signal: Signal) -> io::Result<()> {
    verify_unchanged(pid, starttime)?;
    kill(pid, signal)
}

/// Set the nice value of `pid`, subject to the same identity check as
/// [`kill_if_unchanged`].
pub fn renice_if_unchanged(pid: i32, starttime: u64, nice: i32) -> io::Result<()> {
    verify_unchanged(pid, starttime)?;
    renice(pid, nice)
}

/// Confirm `pid` is still the process that started at `starttime`.
///
/// There is an unavoidable race between this check and the syscall that
/// follows it — the kernel offers no way to signal a process by identity —
/// but it closes the window from "however long the dialog was open" to
/// "microseconds", which is the difference that matters.
fn verify_unchanged(pid: i32, starttime: u64) -> io::Result<()> {
    checked_pid(pid)?;

    let exited = || {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("process {pid} exited before the action could be applied"),
        )
    };

    let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(exited()),
        // Anything else — EACCES under a `hidepid` mount, EMFILE, ENOMEM —
        // means the process may well be alive and we simply cannot tell.
        // Reporting it as exited would be a confident statement of
        // something we do not know.
        Err(e) => {
            return Err(io::Error::new(
                e.kind(),
                format!("cannot verify process {pid}: {e}"),
            ));
        }
    };

    // The page size is irrelevant here: only `starttime` is read, and it is
    // not scaled by it.
    let Some(proc) = crate::sample::process::parse_pid_stat(&stat, 1) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not identify process {pid}"),
        ));
    };

    if proc.starttime != starttime {
        return Err(exited());
    }
    Ok(())
}

/// Reject the PID values that address something other than one process.
fn checked_pid(pid: i32) -> io::Result<()> {
    if pid > 0 {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{pid} does not identify a single process"),
    ))
}

fn syscall(result: i32) -> io::Result<()> {
    if result == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
