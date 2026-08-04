//! A process names itself; rtop displays that name. The two are not the
//! same trust level.
//!
//! `/proc/<pid>/comm` is writable by the process it describes, so every
//! name in every rtop view is attacker-controlled text for any local user.
//! The defence is in ntui's painter — this pins that rtop actually gets it,
//! since the dependency is a version range and the property is invisible in
//! `frame_text()` unless something is looking for it.

use ntui::element;
use ntui::testing::TestTerminal;
use rtop::model::{ListeningSocket, Protocol, Sample, Socket};
use rtop::ui::Selection;
use rtop::ui::Shared;
use rtop::ui::network::{NetworkView, NetworkViewProps};
use rtop::ui::palette::Palette;

#[test]
fn a_process_cannot_disguise_its_name_in_the_listening_table() {
    // U+202E reverses everything after it: this presents as "evilexe.png"
    // in any terminal that honours it, so a process could sit in the table
    // claiming to be something it is not.
    let hostile = ListeningSocket {
        socket: Socket {
            protocol: Protocol::Tcp,
            local: "0.0.0.0:80".parse().unwrap(),
            uid: 0,
            inode: 1,
            accept_queue: Some(0),
        },
        user: "root".into(),
        process: Some((1234, "evil\u{202E}gnp.exe".into())),
    };

    let t = TestTerminal::new(
        90,
        10,
        element!(NetworkView(
            sample: Shared::new(Sample {
                nets: Some(Vec::new()),
                sockets: Some(ntui::Shared::new(vec![hostile])),
                ..Sample::default()
            }),
            palette: Palette::default(),
            height: 10u16,
            selection: Selection::default(),
        )),
    )
    .expect("should render");

    let text = t.frame_text();
    assert!(
        !text.contains('\u{202E}'),
        "the override reached the terminal"
    );
    assert!(
        text.contains("evil"),
        "the real name should be readable:\n{text}"
    );
}
