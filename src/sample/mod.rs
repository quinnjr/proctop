//! Readers that turn `/proc` and `/sys` text into typed snapshots.
//!
//! Every parser in here takes a `&str` rather than a path, so it can be
//! tested against captured fixtures instead of the live system. The thin
//! layer that reads the file lives alongside each parser.

pub mod cpu;
pub mod memory;
pub mod process;
pub mod system;
pub mod users;
