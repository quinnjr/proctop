//! Readers that turn `/proc` and `/sys` text into typed snapshots.
//!
//! Every parser in here takes a `&str` rather than a path, so it can be
//! tested against captured fixtures instead of the live system. The thin
//! layer that reads the files lives in `sampler`, which is the only module
//! in the crate that performs I/O.

pub mod cpu;
pub mod disk;
pub mod memory;
pub mod net;
pub mod process;
pub mod sensors;
pub mod sockets;
pub mod system;
pub mod users;
