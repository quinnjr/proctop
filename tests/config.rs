use proctop::config::Config;
use proctop::sort::SortKey;

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

#[test]
fn every_columns_canonical_name_round_trips() {
    // `canonical` is the name a config would be written back as, so it has
    // to be one `from_word` accepts — and the two tables are maintained
    // separately.
    for (key, spellings) in SortKey::SPELLINGS {
        assert_eq!(
            SortKey::from_word(key.canonical()),
            Some(key),
            "{:?} canonicalises to {:?}, which does not parse",
            key,
            key.canonical()
        );
        assert_eq!(
            key.canonical(),
            spellings[0],
            "canonical should be the first spelling listed"
        );
    }
}

/// The design spec, read at compile time so the examples in it are checked
/// rather than copied.
const DESIGN_SPEC: &str = include_str!("../docs/superpowers/specs/2026-08-04-rtop-design.md");

#[test]
fn every_config_example_in_the_design_spec_parses() {
    // Copying the spec's TOML into this file could only ever prove the copy
    // still parsed — and the finding this test exists for was the spec
    // drifting from the code. `deny_unknown_fields` means a stale example
    // is not a harmless illustration: pasting it is a fatal startup error.
    let blocks: Vec<&str> = DESIGN_SPEC
        .split("```toml")
        .skip(1)
        .filter_map(|rest| rest.split_once("```").map(|(block, _)| block))
        .collect();

    assert!(
        !blocks.is_empty(),
        "the spec should still document a config"
    );

    for block in blocks {
        let config = Config::parse(block)
            .unwrap_or_else(|e| panic!("a documented config must load: {e}\n{block}"));

        // Not just "does not error": the values the example states are the
        // claim a reader acts on.
        assert_eq!(config.refresh_ms, 1500);
        assert_eq!(config.theme, "gruvbox");
        assert!(!config.tree_view);
        assert!(!config.hide_kernel_threads);
        assert_eq!(config.processes.sort_by, SortKey::Cpu);
        assert!(config.processes.sort_desc);
    }
}

const CONFIG_DOC: &str = include_str!("../docs/configuration.md");

#[test]
fn every_config_example_in_the_documentation_parses() {
    // The reference doc is the one a user copies from, so a stale key in it
    // is not a harmless illustration: `deny_unknown_fields` makes pasting it
    // a fatal startup error. Parsed from the file rather than copied here,
    // because a copy can only ever prove the copy still works.
    let blocks: Vec<&str> = CONFIG_DOC
        .split("```toml")
        .skip(1)
        .filter_map(|rest| rest.split_once("```").map(|(block, _)| block))
        .collect();

    assert!(!blocks.is_empty(), "the doc should show a config");

    for block in &blocks {
        Config::parse(block)
            .unwrap_or_else(|e| panic!("a documented config must load: {e}\n{block}"));
    }

    // The worked example claims to set every key at a non-default value, so
    // check it actually does — a doc that silently drops a key as the struct
    // grows is the drift this test exists to catch.
    let complete = blocks
        .iter()
        .find(|b| b.contains("[processes]"))
        .expect("the doc should show a complete example");
    let config = Config::parse(complete).expect("should load");

    assert_eq!(config.refresh_ms, 2000);
    assert_eq!(config.theme, "gruvbox");
    assert!(config.tree_view);
    assert!(config.hide_kernel_threads);
    assert_eq!(config.processes.sort_by, SortKey::Memory);
    assert!(config.processes.sort_desc);
    assert_ne!(config, Config::default(), "every key should differ");
}

#[test]
fn the_documentation_names_every_theme_that_ships() {
    // The doc lists them by name, and there is no user theme file — so a
    // theme added without a doc edit is undiscoverable.
    for (name, _) in proctop::ui::palette::BUNDLED {
        assert!(
            CONFIG_DOC.contains(&format!("`{name}`")),
            "theme {name} is not in docs/configuration.md"
        );
    }
}

#[test]
fn the_documentation_names_every_sort_spelling() {
    // Three tables drifted once and `sort_by = "memory"` became a fatal
    // startup error while `--sort memory` worked.
    for (_, spellings) in SortKey::SPELLINGS {
        for spelling in spellings {
            assert!(
                CONFIG_DOC.contains(&format!("`{spelling}`")),
                "sort spelling {spelling} is not in docs/configuration.md"
            );
        }
    }
}
