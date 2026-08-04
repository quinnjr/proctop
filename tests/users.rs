use proctop::sample::users;

const PASSWD: &str = include_str!("fixtures/passwd.txt");

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
    // The fixture's comment is a commented-out *entry* — a well-formed
    // record behind a `#`. Without the guard it parses as a user named
    // "#joseph" and, the map being last-wins, displaces the real one.
    let table = users::parse_passwd(PASSWD);

    assert_eq!(table.name(0), Some("root"), "valid entries still resolve");
    assert_eq!(table.len(), 6);
}
