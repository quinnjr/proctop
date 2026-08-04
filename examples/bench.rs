//! Times a full sample against the live machine.
//!
//! The number that matters is the duty cycle: what fraction of one core
//! rtop spends on itself at the default refresh. A monitor that is visible
//! in its own process list has failed at its job.
//!
//! Both gated subsystems are measured as well as excluded. Timing only the
//! cheap configuration is how a 6x sensors regression reached a release: the
//! number stayed flat because the expensive path was never on the clock.
//!
//! Run with `cargo run --release --example bench` — a debug build measures
//! the wrong thing by a wide margin.

use rtop::sampler::Wanted;
use std::hint::black_box;
use std::time::{Duration, Instant};

/// Matches `Config::default().refresh_ms`.
const REFRESH: Duration = Duration::from_millis(1500);
const ITERATIONS: u32 = 20;

/// Each gated subsystem serves one tab, so these are the configurations a
/// running rtop actually cycles through.
const CASES: [(&str, Wanted); 4] = [
    (
        "processes only",
        Wanted {
            sensors: false,
            sockets: false,
        },
    ),
    (
        "sensors tab",
        Wanted {
            sensors: true,
            sockets: false,
        },
    ),
    (
        "network tab",
        Wanted {
            sensors: false,
            sockets: true,
        },
    ),
    (
        "everything",
        Wanted {
            sensors: true,
            sockets: true,
        },
    ),
];

fn main() {
    let mut sampler = rtop::sampler::Sampler::new();

    // The first sample has no previous reading to diff against and does not
    // represent steady-state cost.
    let warmup = sampler.sample(Wanted {
        sensors: true,
        sockets: true,
    });
    println!("processes:   {}", warmup.procs.len());
    println!();

    for (name, wanted) in CASES {
        // A fresh sampler per iteration, timed on its *first* sample. The
        // gated subsystems are rate-limited to their own interval, so
        // reusing one sampler would time a single real read followed by
        // nineteen cache hits and report the expensive path as free. The
        // cost of this is one `/etc/passwd` read per iteration, which is
        // constant across the four cases and so does not distort the
        // comparison between them.
        let mut total = Duration::ZERO;
        for _ in 0..ITERATIONS {
            let mut fresh = rtop::sampler::Sampler::new();
            let started = Instant::now();
            black_box(fresh.sample(wanted));
            total += started.elapsed();
        }

        let mean = total / ITERATIONS;
        println!(
            "{name:<15} {mean:>10.2?}   {:.3}% of one core at a {}ms refresh",
            mean.as_secs_f64() / REFRESH.as_secs_f64() * 100.0,
            REFRESH.as_millis(),
        );
    }
}
