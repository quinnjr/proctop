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

/// Restate an action failure in terms of what the user can do about it.
///
/// `EPERM` reaches here as "Operation not permitted", which is accurate and
/// useless: it does not say that root is the remedy. But three structurally
/// different failures share `ErrorKind::PermissionDenied`, and telling the
/// same story about all three is worse than telling none:
///
/// * `EPERM` from `kill(2)` — the kernel refused a signal that was actually
///   attempted. Root is the fix.
/// * `EACCES` from `setpriority(2)` — lowering a nice value, which needs
///   `CAP_SYS_NICE` **even for a process you own**. Saying "not permitted"
///   here reads as "that isn't yours", which is false.
/// * A wrapped error from [`verify_unchanged`] — the identity re-check could
///   not read `/proc/<pid>/stat`, so **no syscall was attempted at all**.
///   Its message was written precisely to avoid claiming what we do not
///   know, and must survive.
///
/// The discriminator is `raw_os_error`: the syscall wrappers build their
/// errors with [`io::Error::last_os_error`], so they carry an errno, while
/// every error constructed inside this module carries `None`.
pub fn explain(err: &io::Error) -> String {
    explain_as(err, our_euid())
}

/// [`explain`], with the caller's euid passed in so both branches are
/// reachable from a test that is not running as root.
pub fn explain_as(err: &io::Error, ours: u32) -> String {
    let remedy = if ours == 0 {
        // Already privileged: an LSM denial, a seccomp filter, a missing
        // capability in a container, or a PID-namespace boundary. Sending
        // this user to root is a dead end, so keep the OS text they will
        // need to diagnose it.
        return format!("not permitted even as root — {err}");
    } else {
        "run proctop as root"
    };

    match err.raw_os_error() {
        Some(libc::EPERM) => format!("not permitted — {remedy}"),
        // Lowering a nice value, which ownership does not govern.
        Some(libc::EACCES) => format!("not permitted — {remedy} to lower a nice value"),
        // Constructed here, not by a syscall: keep the context verbatim.
        _ => err.to_string(),
    }
}

/// Check that a process exists and that we would be permitted to signal it,
/// without delivering anything.
///
/// Signal 0 performs exactly those checks and nothing else, which makes this
/// the *exact* answer to "may I signal this?" — the kernel's own rule, not a
/// reconstruction of it. A uid comparison cannot be exact: `kill(2)` matches
/// the sender's real *or* effective uid against the target's real *or*
/// saved-set uid, `/proc/<pid>` ownership reports only the effective one,
/// and `CAP_KILL` can be held without being uid 0 at all.
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

    // Read bytes and decode lossily rather than `read_to_string`: `comm` is
    // raw kernel bytes that any process can set to invalid UTF-8 through
    // `prctl(PR_SET_NAME)`, and rejecting the line for that would make such
    // a process permanently unsignallable from the dialog. Only `starttime`
    // is read from it, which is ASCII whatever the name contains.
    let stat = match std::fs::read(format!("/proc/{pid}/stat")) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
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
