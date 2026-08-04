use proctop::filter::{Filter, apply};
use proctop::model::{Proc, ProcRow, ProcState};

fn row(pid: i32, name: &str, user: &str, state: ProcState) -> ProcRow {
    ProcRow {
        proc: Proc {
            pid,
            name: name.to_string(),
            state,
            ..Proc::default()
        },
        user: user.into(),
        uid: Some(if user == "root" { 0 } else { 1000 }),
        ..ProcRow::default()
    }
}

fn sample() -> Vec<ProcRow> {
    vec![
        row(1, "systemd", "root", ProcState::Sleeping),
        row(2, "kthreadd", "root", ProcState::Sleeping),
        row(3, "zsh", "joseph", ProcState::Sleeping),
        row(4, "Firefox", "joseph", ProcState::Running),
        row(5, "postgres", "postgres", ProcState::Zombie),
    ]
}

fn names(rows: &[ProcRow]) -> Vec<&str> {
    rows.iter().map(|r| r.proc.name.as_str()).collect()
}

#[test]
fn keeps_everything_when_no_filter_is_set() {
    let rows = apply(sample(), &Filter::default());

    assert_eq!(rows.len(), 5);
}

#[test]
fn matches_a_substring_of_the_command() {
    let filter = Filter {
        query: "sys".to_string(),
        ..Filter::default()
    };

    assert_eq!(names(&apply(sample(), &filter)), vec!["systemd"]);
}

#[test]
fn matches_the_command_case_insensitively() {
    // Typing `firefox` and getting nothing because the process is named
    // `Firefox` is the kind of thing that makes a search feel broken.
    let filter = Filter {
        query: "firefox".to_string(),
        ..Filter::default()
    };

    assert_eq!(names(&apply(sample(), &filter)), vec!["Firefox"]);
}

#[test]
fn matches_a_pid_typed_in_full() {
    let filter = Filter {
        query: "4".to_string(),
        ..Filter::default()
    };

    assert_eq!(names(&apply(sample(), &filter)), vec!["Firefox"]);
}

#[test]
fn filters_by_user_independently_of_the_query() {
    let filter = Filter {
        user: Some("joseph".to_string()),
        ..Filter::default()
    };

    assert_eq!(names(&apply(sample(), &filter)), vec!["zsh", "Firefox"]);
}

#[test]
fn combines_the_user_and_query_filters() {
    let filter = Filter {
        query: "z".to_string(),
        user: Some("joseph".to_string()),
        ..Filter::default()
    };

    assert_eq!(names(&apply(sample(), &filter)), vec!["zsh"]);
}

#[test]
fn hides_kernel_threads_when_asked() {
    // Kernel threads have no address space, which is how htop tells them
    // apart from userspace processes.
    let mut rows = sample();
    rows[0].proc.vsize = 4096; // systemd is userspace
    rows[3].proc.vsize = 4096; // so is Firefox

    let filter = Filter {
        hide_kernel_threads: true,
        ..Filter::default()
    };

    assert_eq!(names(&apply(rows, &filter)), vec!["systemd", "Firefox"]);
}

#[test]
fn matches_nothing_rather_than_everything_when_the_query_matches_nothing() {
    let filter = Filter {
        query: "nosuchprocess".to_string(),
        ..Filter::default()
    };

    assert!(apply(sample(), &filter).is_empty());
}

#[test]
fn treats_an_empty_query_as_no_filter_rather_than_as_a_match_on_nothing() {
    let filter = Filter {
        query: String::new(),
        user: None,
        ..Filter::default()
    };

    assert_eq!(apply(sample(), &filter).len(), 5);
}

#[test]
fn reports_whether_it_would_filter_anything() {
    assert!(!Filter::default().is_active());
    assert!(
        Filter {
            query: "x".into(),
            ..Filter::default()
        }
        .is_active()
    );
    assert!(
        Filter {
            user: Some("root".into()),
            ..Filter::default()
        }
        .is_active()
    );
}

#[test]
fn matches_a_pid_written_with_leading_zeros_or_a_sign() {
    // The comparison is numeric, not textual — documented on matches_query.
    for query in ["4", "004", "+4"] {
        let filter = Filter {
            query: query.to_string(),
            ..Filter::default()
        };
        assert_eq!(
            names(&apply(sample(), &filter)),
            vec!["Firefox"],
            "query {query:?}"
        );
    }
}

#[test]
fn a_non_ascii_query_still_matches_case_insensitively() {
    // The ASCII fast path must not be reached for either side here.
    let mut rows = sample();
    rows[0].proc.name = "ÜBERPROC".to_string();
    let filter = Filter {
        query: "über".to_string(),
        ..Filter::default()
    };

    assert_eq!(names(&apply(rows, &filter)), vec!["ÜBERPROC"]);
}

#[test]
fn an_ascii_name_does_not_match_a_non_ascii_query() {
    let filter = Filter {
        query: "zß".to_string(),
        ..Filter::default()
    };

    assert!(apply(sample(), &filter).is_empty());
}

#[test]
fn a_query_longer_than_the_name_matches_nothing() {
    let filter = Filter {
        query: "systemd-and-then-some".to_string(),
        ..Filter::default()
    };

    assert!(apply(sample(), &filter).is_empty());
}
