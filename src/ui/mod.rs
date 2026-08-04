//! The ntui half of rtop: components, keybindings, and layout.

pub mod app;
pub mod detail;
pub mod help;
pub mod io;
pub mod meters;
pub mod network;
pub mod overlay;
pub mod palette;
pub mod sensors;
pub mod state;
pub mod table;

/// Re-exported from ntui, where it now lives.
///
/// rtop needed this first — passing the process list down as a plain
/// `Arc<Vec<ProcRow>>` deep-compared several hundred processes on every
/// frame, because `Arc`'s own `PartialEq` compares pointees. It was a
/// general enough problem that it belongs in the library rather than here.
pub use ntui::Shared;

/// The cursor and viewport for the process table.
///
/// Re-exported from ntui: rtop wrote this first, it was general enough to
/// belong in the library, and keeping a second copy here would leave two
/// implementations of the same scroll arithmetic to drift apart.
pub use ntui::ListSelection as Selection;
