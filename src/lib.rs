//! rtop — an htop-inspired system monitor built on ntui.
//!
//! The crate is split into a sampling half and a UI half. Everything under
//! [`sample`], [`delta`], [`sort`], and [`model`] parses and reasons about
//! `/proc` without knowing a terminal exists, and is tested against captured
//! fixture text rather than the live system.

pub mod delta;
pub mod model;
pub mod sample;
pub mod sampler;
pub mod sort;
