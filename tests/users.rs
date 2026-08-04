use rtop::sample::{process, users};

const PASSWD: &str = include_str!("fixtures/passwd.txt");
const PID_STATUS: &str = include_str!("fixtures/pid_status.txt");

#[test]
fn resolves_uids_to_names() {
    let table = users::parse_passwd(PASSWD);

    assert_eq!(table.name(0), Some("root"));
    assert_eq!(table.name(1000), Some("joseph"));
    assert_eq!(table.name(65534), Some("nobody"));
}

#[test]
fn has_no_name_for_an_unknown_uid() {
    // Containers and LDAP setups routinely run processes as a uid with no
    // local passwd entry. htop shows the number; it does not hide the row.
    let table = users::parse_passwd(PASSWD);

    assert_eq!(table.name(4242), None);
}

#[test]
fn skips_comments_and_malformed_entries() {
    let table = users::parse_passwd(PASSWD);

    assert_eq!(table.name(0), Some("root"), "valid entries still resolve");
    assert_eq!(table.len(), 6);
}

#[test]
fn reads_the_real_uid_from_proc_status() {
    // The Uid line is `real  effective  saved  filesystem`. htop's USER
    // column shows the real uid — the effective one changes under setuid
    // and would make the column flicker.
    let uid = process::parse_status_uid(PID_STATUS).expect("fixture has a Uid line");

    assert_eq!(uid, 1000);
}

#[test]
fn returns_none_when_status_has_no_uid_line() {
    assert!(process::parse_status_uid("Name:\tcat\nState:\tR\n").is_none());
}
