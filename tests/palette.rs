use ntui::Color;
use rtop::ui::palette::{BUNDLED, Palette};

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
