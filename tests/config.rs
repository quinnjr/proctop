use rtop::config::Config;
use rtop::sort::SortKey;

#[test]
fn an_absent_config_is_not_an_error() {
    // A machine with no config file is the normal case, not a failure.
    let config = Config::parse("").expect("empty config should be valid");

    assert_eq!(config, Config::default());
}

#[test]
fn defaults_match_htops_familiar_behaviour() {
    let config = Config::default();

    assert_eq!(config.refresh_ms, 1500);
    assert_eq!(config.processes.sort_by, SortKey::Cpu);
    assert!(config.processes.sort_desc);
    assert!(!config.tree_view);
}

#[test]
fn reads_every_documented_setting() {
    let config = Config::parse(
        r#"
        refresh_ms = 500
        theme = "gruvbox"
        tree_view = true
        hide_kernel_threads = true

        [processes]
        sort_by = "mem"
        sort_desc = false
        "#,
    )
    .expect("should parse");

    assert_eq!(config.refresh_ms, 500);
    assert_eq!(config.theme, "gruvbox");
    assert!(config.tree_view);
    assert!(config.hide_kernel_threads);
    assert_eq!(config.processes.sort_by, SortKey::Memory);
    assert!(!config.processes.sort_desc);
}

#[test]
fn leaves_unmentioned_settings_at_their_defaults() {
    let config = Config::parse("refresh_ms = 250").expect("should parse");

    assert_eq!(config.refresh_ms, 250);
    assert_eq!(config.processes.sort_by, SortKey::Cpu);
}

#[test]
fn rejects_a_misspelled_key_rather_than_ignoring_it() {
    // Silently ignoring `refresh_msec` means the setting appears not to
    // work, with nothing anywhere saying why.
    let err = Config::parse("refresh_msec = 500").expect_err("should reject");

    assert!(err.to_string().contains("refresh_msec"), "{err}");
}

#[test]
fn rejects_a_sort_column_that_does_not_exist() {
    let err = Config::parse("[processes]\nsort_by = \"nonsense\"").expect_err("should reject");

    assert!(err.to_string().contains("nonsense"), "{err}");
}

#[test]
fn accepts_every_sort_column_the_ui_offers() {
    for (name, expected) in [
        ("pid", SortKey::Pid),
        ("name", SortKey::Name),
        ("cpu", SortKey::Cpu),
        ("mem", SortKey::Memory),
        ("time", SortKey::Time),
    ] {
        let config = Config::parse(&format!("[processes]\nsort_by = \"{name}\""))
            .unwrap_or_else(|e| panic!("{name} should parse: {e}"));
        assert_eq!(config.processes.sort_by, expected);
    }
}

#[test]
fn refuses_a_refresh_fast_enough_to_peg_a_core() {
    // Sampling costs several milliseconds; a 1 ms refresh would spend the
    // whole machine watching itself.
    let err = Config::parse("refresh_ms = 1").expect_err("should reject");

    assert!(err.to_string().contains("refresh_ms"), "{err}");
}

#[test]
fn accepts_the_fastest_refresh_that_is_still_sane() {
    assert!(Config::parse("refresh_ms = 100").is_ok());
}

#[test]
fn names_the_file_and_the_problem_when_it_cannot_be_read() {
    let err = Config::load_from("/nonexistent/rtop/config.toml").expect_err("should fail");

    assert!(
        err.to_string().contains("/nonexistent/rtop/config.toml"),
        "{err}"
    );
}

#[test]
fn an_absent_config_file_loads_defaults_rather_than_failing() {
    // Distinct from an unreadable one: nothing there is fine, something
    // there that is broken is not.
    let config = Config::load_from_optional("/nonexistent/rtop/config.toml")
        .expect("a missing file is not an error");

    assert_eq!(config, Config::default());
}

#[test]
fn a_malformed_config_file_is_an_error_rather_than_silently_defaulted() {
    // The whole argument for rejecting unknown keys: a setting that appears
    // not to work, with nothing anywhere saying why, is worse than a refusal
    // to start. That has to hold for a file on disk, not just for a string.
    let dir = temp_dir("malformed");
    let path = dir.join("config.toml");
    std::fs::write(&path, "refresh_msec = 500\n").unwrap();

    let err = Config::load_from_optional(&path).expect_err("should reject");

    assert!(err.to_string().contains("refresh_msec"), "{err}");
    assert!(
        err.to_string().contains(&path.display().to_string()),
        "should name the file: {err}"
    );
}

#[test]
fn a_present_but_unreadable_config_file_is_an_error_not_an_absent_one() {
    // `Path::exists()` reports false for a permission error, which would
    // quietly take the "missing, use defaults" branch.
    let dir = temp_dir("unreadable");
    let path = dir.join("config.toml");
    std::fs::write(&path, "refresh_ms = 500\n").unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o000);
    std::fs::set_permissions(&path, perms).unwrap();

    let result = Config::load_from_optional(&path);

    // Running as root defeats the permission bit; skip rather than fail.
    if result.is_ok() && unsafe { libc::geteuid() } == 0 {
        return;
    }
    let err = result.expect_err("should surface the read error");
    assert!(
        err.to_string().contains(&path.display().to_string()),
        "should name the file: {err}"
    );
}

/// A fresh directory under the system temp dir, removed if it already exists.
fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rtop-config-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_config_file_accepts_every_spelling_the_cli_and_command_line_do() {
    // One vocabulary, three entry points. A user who learns an alias from
    // `:sort memory` and writes it into the config must not get a program
    // that refuses to start.
    for word in [
        "pid", "name", "command", "cpu", "mem", "memory", "res", "time",
    ] {
        let from_word =
            SortKey::from_word(word).unwrap_or_else(|| panic!("{word} should be a known column"));
        let config = Config::parse(&format!("[processes]\nsort_by = \"{word}\""))
            .unwrap_or_else(|e| panic!("config should accept {word}: {e}"));

        assert_eq!(config.processes.sort_by, from_word, "{word}");
    }
}

#[test]
fn rejects_a_word_that_is_not_a_column_at_every_entry_point() {
    assert_eq!(SortKey::from_word("nonsense"), None);
    assert!(Config::parse("[processes]\nsort_by = \"nonsense\"").is_err());
}
