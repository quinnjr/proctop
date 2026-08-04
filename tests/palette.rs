use ntui::Color;
use proctop::ui::palette::{BUNDLED, Palette};

#[test]
fn ships_the_themes_the_config_can_name() {
    let names: Vec<&str> = BUNDLED.iter().map(|(name, _)| *name).collect();

    assert!(names.contains(&"default"));
    assert!(names.contains(&"gruvbox"));
    assert!(names.contains(&"mono"));
}

#[test]
fn every_bundled_theme_parses() {
    for (name, toml) in BUNDLED {
        Palette::parse(toml).unwrap_or_else(|e| panic!("bundled theme {name} is broken: {e}"));
    }
}

#[test]
fn looks_a_theme_up_by_name() {
    let palette = Palette::named("gruvbox").expect("gruvbox is bundled");

    assert_ne!(palette, Palette::default(), "gruvbox should differ");
}

#[test]
fn reports_an_unknown_theme_rather_than_falling_back_silently() {
    // Falling back leaves the user staring at the wrong colors with nothing
    // saying their theme name was a typo.
    let err = Palette::named("nosuchtheme").expect_err("should fail");

    assert!(err.to_string().contains("nosuchtheme"), "{err}");
    assert!(
        err.to_string().contains("default"),
        "should list what exists: {err}"
    );
}

#[test]
fn reads_named_ansi_colors() {
    let palette = Palette::parse(r#"cpu_user = "green""#).expect("should parse");

    assert_eq!(palette.cpu_user, Color::Green);
}

#[test]
fn reads_hex_colors() {
    let palette = Palette::parse(r##"cpu_user = "#ff8800""##).expect("should parse");

    assert_eq!(palette.cpu_user, Color::Rgb(0xff, 0x88, 0x00));
}

#[test]
fn rejects_a_color_that_is_neither() {
    let err = Palette::parse(r#"cpu_user = "burnt sienna""#).expect_err("should reject");

    assert!(err.to_string().contains("burnt sienna"), "{err}");
}

#[test]
fn rejects_malformed_hex() {
    assert!(Palette::parse(r##"cpu_user = "#ff88""##).is_err());
    assert!(Palette::parse(r##"cpu_user = "#gggggg""##).is_err());
}

#[test]
fn leaves_unmentioned_colors_at_their_defaults() {
    let palette = Palette::parse(r#"cpu_user = "red""#).expect("should parse");

    assert_eq!(palette.cpu_user, Color::Red);
    assert_eq!(palette.cpu_system, Palette::default().cpu_system);
}

#[test]
fn rejects_a_misspelled_color_key() {
    let err = Palette::parse(r#"cpu_users = "red""#).expect_err("should reject");

    assert!(err.to_string().contains("cpu_users"), "{err}");
}

#[test]
fn rejects_a_six_byte_hex_string_that_is_not_six_ascii_digits() {
    // The length guard counts bytes, but the slices are at byte offsets:
    // "#aébcd" is six bytes, and splitting it at offset 2 lands inside the
    // 'é'. A theme file is user input, so this must be an error, not a panic.
    let err = Palette::parse(r##"cpu_user = "#aébcd""##).expect_err("should reject");

    assert!(err.to_string().contains("aébcd"), "{err}");
}

#[test]
fn rejects_multibyte_hex_of_every_length_without_panicking() {
    for text in ["#éé", "#ééé", "#aaéé", "#ééééé", "#ααααα", "#\u{1F600}aa"] {
        let result = Palette::parse(&format!("cpu_user = {text:?}"));
        assert!(result.is_err(), "{text} should be rejected");
    }
}

#[test]
fn an_unmeasurable_load_is_muted_rather_than_alarming() {
    // NaN CPU is a real input — a process seen for the first time between
    // two samples has no delta. Every comparison is false for NaN, so it
    // must land on the last arm rather than on `alert`.
    let p = Palette::default();

    assert_eq!(p.cpu_load(f32::NAN), p.muted);
    assert_eq!(p.mem_load(f32::NAN), p.text);
}

#[test]
fn cpu_load_colours_each_band() {
    let p = Palette::default();

    assert_eq!(p.cpu_load(0.0), p.muted);
    assert_eq!(p.cpu_load(0.05), p.text);
    assert_eq!(p.cpu_load(0.1), p.warn);
    assert_eq!(p.cpu_load(0.5), p.alert);
}
