//! `/etc/passwd` — uid to username resolution.

use std::collections::HashMap;

/// Maps uids to usernames.
#[derive(Clone, Debug, Default)]
pub struct UserTable {
    names: HashMap<u32, String>,
}

impl UserTable {
    /// The username for `uid`, or `None` if this machine has no entry for it
    /// — routine under containers and directory services.
    pub fn name(&self, uid: u32) -> Option<&str> {
        self.names.get(&uid).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// Parse the contents of `/etc/passwd`.
///
/// Entries are `name:passwd:uid:gid:gecos:home:shell`. Comments, blank
/// lines, and anything that does not have a parseable uid in the third field
/// are skipped — a malformed line should cost one username, not the whole
/// table.
pub fn parse_passwd(text: &str) -> UserTable {
    let mut names = HashMap::new();

    for line in text.lines() {
        // A commented-out entry is still a well-formed record: without this
        // `#nobody:x:65534:...` parses as a user literally named "#nobody"
        // and, because the map is last-wins, can displace the real one.
        if line.trim_start().starts_with('#') {
            continue;
        }
        let mut fields = line.split(':');
        let (Some(name), Some(_passwd), Some(uid)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(uid) = uid.parse() else {
            continue;
        };
        names.insert(uid, name.to_string());
    }

    UserTable { names }
}
