//! The only operations that change the system.
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
