//! Times a full sample against the live machine.
//!
//! The number that matters is the duty cycle: what fraction of one core
//! rtop spends on itself at the default refresh. A monitor that is visible
//! in its own process list has failed at its job.
//!
//! Run with `cargo run --release --example bench` — a debug build measures
//! the wrong thing by a wide margin.

use std::hint::black_box;
use std::time::{Duration, Instant};

/// Matches `ui::app::REFRESH`.
const REFRESH: Duration = Duration::from_millis(1500);
const ITERATIONS: u32 = 20;

fn main() {
    let mut sampler = rtop::sampler::Sampler::new();

    // The first sample has no previous reading to diff against and does not
    // represent steady-state cost.
    let warmup = sampler.sample(false);
    let processes = warmup.procs.len();

    let mut total = Duration::ZERO;
    for _ in 0..ITERATIONS {
        let started = Instant::now();
        black_box(sampler.sample(false));
        total += started.elapsed();
    }

    let mean = total / ITERATIONS;
    println!("processes:  {processes}");
    println!("mean sample: {mean:?}");
    println!(
        "duty cycle:  {:.3}% of one core at a {}ms refresh",
        mean.as_secs_f64() / REFRESH.as_secs_f64() * 100.0,
        REFRESH.as_millis(),
    );
}
