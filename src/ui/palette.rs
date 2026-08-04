//! rtop's colors.
//!
//! ntui's `Theme` carries eight general tokens (accent, surface, border,
//! muted, foreground, danger, success, border_style), which is the right
//! size for a widget set but not for a monitor: a CPU meter needs
//! distinguishable colors for user, system, nice, irq and iowait time all
//! at once, and those have meanings no general token expresses.

use std::fmt;

use ntui::Color;
use serde::Deserialize;

/// Every color rtop can be told about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub cpu_user: Color,
    pub cpu_system: Color,
    pub cpu_nice: Color,
    pub cpu_irq: Color,
    pub cpu_iowait: Color,
    pub mem_used: Color,
    pub swap: Color,
    pub label: Color,
    pub header_fg: Color,
    pub header_bg: Color,
    pub muted: Color,
    pub text: Color,
    pub selected_fg: Color,
    pub selected_bg: Color,
    pub status_bg: Color,
    /// Background for modal panels. Must be opaque: `Color::Reset` means
    /// "do not paint", which lets the table show through the dialog.
    pub panel_bg: Color,
    /// Applied to a figure that has crossed into "worth looking at".
    pub warn: Color,
    /// Applied to a figure that has crossed into "something is wrong".
    pub alert: Color,
}

/// Themes compiled into the binary.
pub const BUNDLED: &[(&str, &str)] = &[
    ("default", include_str!("../../themes/default.toml")),
    ("gruvbox", include_str!("../../themes/gruvbox.toml")),
    ("mono", include_str!("../../themes/mono.toml")),
];

impl Default for Palette {
    fn default() -> Self {
        Palette {
            cpu_user: Color::Green,
            cpu_system: Color::Red,
            cpu_nice: Color::Blue,
            cpu_irq: Color::Magenta,
            cpu_iowait: Color::Cyan,
            mem_used: Color::Green,
            swap: Color::Red,
            label: Color::Cyan,
            header_fg: Color::Black,
            header_bg: Color::Green,
            muted: Color::DarkGrey,
            text: Color::Reset,
            selected_fg: Color::Black,
            selected_bg: Color::Cyan,
            status_bg: Color::Blue,
            panel_bg: Color::Black,
            warn: Color::Yellow,
            alert: Color::Red,
        }
    }
}

impl Palette {
    /// Parse a theme document. Unmentioned colors keep their defaults;
    /// unknown keys are rejected so a typo is reported rather than ignored.
    pub fn parse(text: &str) -> Result<Palette, Error> {
        let file: PaletteFile = toml::from_str(text).map_err(|e| Error(e.to_string()))?;
        Ok(file.into_palette())
    }

    /// Look up a bundled theme.
    ///
    /// An unknown name is an error rather than a silent fall back to the
    /// default, which would leave the user staring at the wrong colors with
    /// nothing saying their theme name was a typo.
    pub fn named(name: &str) -> Result<Palette, Error> {
        match BUNDLED.iter().find(|(n, _)| *n == name) {
            Some((_, toml)) => Palette::parse(toml),
            None => {
                let available: Vec<&str> = BUNDLED.iter().map(|(n, _)| *n).collect();
                Err(Error(format!(
                    "unknown theme {name:?}; available: {}",
                    available.join(", ")
                )))
            }
        }
    }

    /// The color for a process's CPU figure, so a busy process is visible
    /// without reading the number.
    pub fn cpu_load(&self, fraction: f32) -> Color {
        // No NaN arm: every comparison below is false for NaN, so it lands
        // on the last arm. `mem_load` omits one for the same reason.
        match fraction {
            f if f >= 0.5 => self.alert,
            f if f >= 0.1 => self.warn,
            f if f > 0.0 => self.text,
            _ => self.muted,
        }
    }

    /// The color for a process's memory figure.
    pub fn mem_load(&self, fraction: f32) -> Color {
        match fraction {
            f if f >= 0.10 => self.alert,
            f if f >= 0.02 => self.warn,
            _ => self.text,
        }
    }

    /// The color for a temperature reading, on the usual silicon thresholds.
    pub fn temperature(&self, celsius: f32) -> Color {
        match celsius {
            c if c >= 85.0 => self.alert,
            c if c >= 70.0 => self.warn,
            _ => self.text,
        }
    }
}

/// The on-disk shape: every field optional, so a theme can set only what it
/// wants to change.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct PaletteFile {
    cpu_user: Option<Spec>,
    cpu_system: Option<Spec>,
    cpu_nice: Option<Spec>,
    cpu_irq: Option<Spec>,
    cpu_iowait: Option<Spec>,
    mem_used: Option<Spec>,
    swap: Option<Spec>,
    label: Option<Spec>,
    header_fg: Option<Spec>,
    header_bg: Option<Spec>,
    muted: Option<Spec>,
    text: Option<Spec>,
    selected_fg: Option<Spec>,
    selected_bg: Option<Spec>,
    status_bg: Option<Spec>,
    panel_bg: Option<Spec>,
    warn: Option<Spec>,
    alert: Option<Spec>,
}

impl PaletteFile {
    fn into_palette(self) -> Palette {
        let d = Palette::default();
        Palette {
            cpu_user: self.cpu_user.map_or(d.cpu_user, |s| s.0),
            cpu_system: self.cpu_system.map_or(d.cpu_system, |s| s.0),
            cpu_nice: self.cpu_nice.map_or(d.cpu_nice, |s| s.0),
            cpu_irq: self.cpu_irq.map_or(d.cpu_irq, |s| s.0),
            cpu_iowait: self.cpu_iowait.map_or(d.cpu_iowait, |s| s.0),
            mem_used: self.mem_used.map_or(d.mem_used, |s| s.0),
            swap: self.swap.map_or(d.swap, |s| s.0),
            label: self.label.map_or(d.label, |s| s.0),
            header_fg: self.header_fg.map_or(d.header_fg, |s| s.0),
            header_bg: self.header_bg.map_or(d.header_bg, |s| s.0),
            muted: self.muted.map_or(d.muted, |s| s.0),
            text: self.text.map_or(d.text, |s| s.0),
            selected_fg: self.selected_fg.map_or(d.selected_fg, |s| s.0),
            selected_bg: self.selected_bg.map_or(d.selected_bg, |s| s.0),
            status_bg: self.status_bg.map_or(d.status_bg, |s| s.0),
            panel_bg: self.panel_bg.map_or(d.panel_bg, |s| s.0),
            warn: self.warn.map_or(d.warn, |s| s.0),
            alert: self.alert.map_or(d.alert, |s| s.0),
        }
    }
}

/// A color written as an ANSI name or as `#rrggbb`.
struct Spec(Color);

impl<'de> Deserialize<'de> for Spec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        parse_color(&text)
            .map(Spec)
            .ok_or_else(|| serde::de::Error::custom(format!("not a color: {text:?}")))
    }
}

fn parse_color(text: &str) -> Option<Color> {
    if let Some(hex) = text.strip_prefix('#') {
        // Both checks matter: `len` counts bytes, and the slices below are
        // at byte offsets, so a six-byte string of multibyte characters
        // ("#aébcd") would pass the length guard and then split mid-char.
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        // `from_str_radix` accepts a leading `+`, so "#+1+2+3" would parse
        // as a colour rather than being reported as the typo it is — and a
        // setting that silently does something else is what this project
        // refuses to ship.
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
        return Some(Color::Rgb(byte(0..2)?, byte(2..4)?, byte(4..6)?));
    }
    Some(match text.to_lowercase().as_str() {
        "reset" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "darkgrey" | "darkgray" | "grey" | "gray" => Color::DarkGrey,
        _ => return None,
    })
}

#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}
