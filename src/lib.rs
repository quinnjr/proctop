//! rtop — an htop-inspired system monitor built on ntui.
//!
//! The crate is split into a sampling half and a UI half. Everything under
//! [`sample`], [`delta`], [`sort`], [`filter`], [`tree`], [`format`],
//! [`config`], and [`model`] reasons about processes without knowing a
//! terminal exists, and is tested against captured fixture text rather than
//! the live system. [`actions`] is the one module that changes the system,
//! and [`sampler`] the one that reads it.

pub mod actions;
pub mod config;
pub mod delta;
pub mod filter;
pub mod format;
pub mod model;
pub mod sample;
pub mod sampler;
pub mod sort;
pub mod tree;
pub mod ui;
