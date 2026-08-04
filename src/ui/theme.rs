//! rtop's palette.
//!
//! ntui's `Theme` carries eight general tokens (accent, surface, border,
//! muted, foreground, danger, success, border_style), which is the right
//! size for widgets but not enough for a monitor: a CPU meter needs
//! distinguishable colors for user, system, nice, irq and iowait time all at
//! once, and those have meanings no general token expresses.

use ntui::Color;

/// Meter segment colors, following htop's conventions closely enough that
/// muscle memory transfers.
pub const USER: Color = Color::Green;
pub const SYSTEM: Color = Color::Red;
pub const NICE: Color = Color::Blue;
pub const IRQ: Color = Color::Magenta;
pub const IOWAIT: Color = Color::Cyan;

/// Used memory.
pub const MEM_USED: Color = Color::Green;
/// Buffers and page cache — reclaimable, so drawn as less significant.
pub const MEM_CACHE: Color = Color::Yellow;
pub const SWAP: Color = Color::Red;

/// Chrome.
pub const LABEL: Color = Color::Cyan;
pub const HEADER: Color = Color::Black;
pub const HEADER_BG: Color = Color::Green;
pub const MUTED: Color = Color::DarkGrey;
pub const TEXT: Color = Color::Reset;
pub const SELECTED_BG: Color = Color::Cyan;
pub const SELECTED_FG: Color = Color::Black;

/// The column a process's CPU figure is drawn in, so a busy process is
/// visible without reading the number.
pub fn cpu_color(fraction: f32) -> Color {
    match fraction {
        f if f.is_nan() => MUTED,
        f if f >= 0.5 => Color::Red,
        f if f >= 0.1 => Color::Yellow,
        f if f > 0.0 => TEXT,
        _ => MUTED,
    }
}

/// The color of a process's memory figure.
pub fn mem_color(fraction: f32) -> Color {
    match fraction {
        f if f >= 0.10 => Color::Red,
        f if f >= 0.02 => Color::Yellow,
        _ => TEXT,
    }
}
