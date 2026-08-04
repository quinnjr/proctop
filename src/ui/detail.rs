//! Per-process detail, and the kill and renice dialogs.

use ntui::style::{Color, Weight};
use ntui::{Component, Element, Hooks};

use crate::actions::{NICE_MAX, NICE_MIN, SIGNALS};
use crate::format;
use crate::model::ProcRow;
use crate::ui::overlay;
use crate::ui::palette::Palette;

#[derive(Clone, PartialEq, Default)]
pub struct DetailProps {
    /// `None` when the process exited between opening the pane and this
    /// frame, which is entirely normal for a short-lived one.
    pub row: Option<ProcRow>,
    pub pid: i32,
    pub palette: Palette,
}

pub struct Detail;

impl Component for Detail {
    type Props = DetailProps;

    fn render(props: &DetailProps, _hooks: &mut Hooks) -> Element {
        Detail::render_tree(props)
    }
}

impl Detail {
    fn render_tree(props: &DetailProps) -> Element {
        let palette = &props.palette;

        let Some(row) = &props.row else {
            return overlay::panel(
                &format!("Process {}", props.pid),
                palette,
                vec![
                    overlay::row(
                        "This process has exited.",
                        palette.muted,
                        Weight::Normal,
                        palette,
                    ),
                    overlay::row("", palette.text, Weight::Normal, palette),
                    overlay::row("any key to close", palette.muted, Weight::Normal, palette),
                ],
            );
        };

        let p = &row.proc;
        let body = vec![
            overlay::field("Command", &p.name, palette),
            overlay::field("PID", p.pid.to_string(), palette),
            overlay::field("Parent", p.ppid.to_string(), palette),
            overlay::field("User", row.user.to_string(), palette),
            overlay::field(
                "State",
                format!("{} ({})", p.state.as_char(), state_name(p)),
                palette,
            ),
            overlay::field("Threads", p.threads.to_string(), palette),
            overlay::field(
                "Priority",
                format!("{} (nice {})", p.priority, p.nice),
                palette,
            ),
            overlay::field("Virtual", format::bytes(p.vsize), palette),
            overlay::field("Resident", format::bytes(p.rss), palette),
            overlay::field(
                "Memory",
                format!("{:.2}% of physical", row.mem * 100.0),
                palette,
            ),
            overlay::field(
                "CPU",
                format!("{:.1}% of one core", row.cpu * 100.0),
                palette,
            ),
            overlay::field("CPU time", format::cpu_time(p.cpu_time()), palette),
            overlay::field(
                "User / sys",
                format!(
                    "{} / {}",
                    format::cpu_time(p.utime),
                    format::cpu_time(p.stime)
                ),
                palette,
            ),
            overlay::row("", palette.text, Weight::Normal, palette),
            overlay::row("any key to close", palette.muted, Weight::Normal, palette),
        ];

        overlay::panel(&format!("Process {}", p.pid), palette, body)
    }
}

fn state_name(p: &crate::model::Proc) -> &'static str {
    use crate::model::ProcState::*;
    match p.state {
        Running => "running",
        Sleeping => "sleeping",
        UninterruptibleSleep => "uninterruptible",
        Zombie => "zombie",
        Stopped => "stopped",
        TracingStop => "traced",
        Idle => "idle",
        Dead => "dead",
        Unknown => "unknown",
    }
}

#[derive(Clone, PartialEq, Default)]
pub struct KillProps {
    pub pid: i32,
    pub name: String,
    /// Index into [`SIGNALS`].
    pub index: usize,
    /// Whether the process is still in the latest sample. A dialog can sit
    /// open while its process exits, and pressing Enter on a dead pid is
    /// worth warning about before it happens.
    pub alive: bool,
    pub palette: Palette,
}

pub struct Kill;

impl Component for Kill {
    type Props = KillProps;

    fn render(props: &KillProps, _hooks: &mut Hooks) -> Element {
        Kill::render_tree(props)
    }
}

impl Kill {
    fn render_tree(props: &KillProps) -> Element {
        let palette = &props.palette;
        let mut body = vec![
            overlay::row(
                format!("{} (pid {})", props.name, props.pid),
                palette.text,
                Weight::Bold,
                palette,
            ),
            overlay::row("", palette.text, Weight::Normal, palette),
        ];

        if !props.alive {
            body.push(overlay::row(
                "This process has exited.",
                palette.alert,
                Weight::Bold,
                palette,
            ));
            body.push(overlay::row("", palette.text, Weight::Normal, palette));
            body.push(overlay::row(
                "Esc to close",
                palette.muted,
                Weight::Normal,
                palette,
            ));
            return overlay::panel("Send signal", palette, body);
        }

        for (i, signal) in SIGNALS.iter().enumerate() {
            let selected = i == props.index;
            body.push(overlay::row(
                format!(
                    "{} {:<8} {}",
                    if selected { "▸" } else { " " },
                    signal.label(),
                    signal.number()
                ),
                // Against the panel background, not the selection bar, so
                // the marked signal takes a foreground accent rather than
                // the inverted pair used for table rows.
                if selected { palette.warn } else { palette.text },
                if selected {
                    Weight::Bold
                } else {
                    Weight::Normal
                },
                palette,
            ));
        }

        body.push(overlay::row("", palette.text, Weight::Normal, palette));
        body.push(overlay::row(
            "j/k choose · Enter send · Esc cancel",
            palette.muted,
            Weight::Normal,
            palette,
        ));

        overlay::panel("Send signal", palette, body)
    }
}

#[derive(Clone, PartialEq, Default)]
pub struct ReniceProps {
    pub pid: i32,
    pub name: String,
    pub input: String,
    /// Whether the process is still in the latest sample.
    pub alive: bool,
    pub palette: Palette,
}

pub struct Renice;

impl Component for Renice {
    type Props = ReniceProps;

    fn render(props: &ReniceProps, _hooks: &mut Hooks) -> Element {
        Renice::render_tree(props)
    }
}

impl Renice {
    fn render_tree(props: &ReniceProps) -> Element {
        let palette = &props.palette;

        if !props.alive {
            return overlay::panel(
                "Renice",
                palette,
                vec![
                    overlay::row(
                        format!("{} (pid {})", props.name, props.pid),
                        palette.text,
                        Weight::Bold,
                        palette,
                    ),
                    overlay::row("", palette.text, Weight::Normal, palette),
                    overlay::row(
                        "This process has exited.",
                        palette.alert,
                        Weight::Bold,
                        palette,
                    ),
                    overlay::row("", palette.text, Weight::Normal, palette),
                    overlay::row("Esc to close", palette.muted, Weight::Normal, palette),
                ],
            );
        }

        overlay::panel(
            "Renice",
            palette,
            vec![
                overlay::row(
                    format!("{} (pid {})", props.name, props.pid),
                    palette.text,
                    Weight::Bold,
                    palette,
                ),
                overlay::row("", palette.text, Weight::Normal, palette),
                overlay::row(
                    format!("nice: {}_", props.input),
                    palette.text,
                    Weight::Normal,
                    palette,
                ),
                overlay::row(
                    format!("{NICE_MIN} to {NICE_MAX}; lowering needs privileges"),
                    palette.muted,
                    Weight::Normal,
                    palette,
                ),
                overlay::row("", Color::Reset, Weight::Normal, palette),
                overlay::row(
                    "Enter apply · Esc cancel",
                    palette.muted,
                    Weight::Normal,
                    palette,
                ),
            ],
        )
    }
}
