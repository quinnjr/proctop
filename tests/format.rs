use std::time::Duration;

use rtop::format::{bar, bytes, cpu_time, uptime};

#[test]
fn fills_a_bar_in_proportion_to_the_value() {
    assert_eq!(bar(0.0, 10), "          ");
    assert_eq!(bar(0.5, 10), "|||||     ");
    assert_eq!(bar(1.0, 10), "||||||||||");
}

#[test]
fn keeps_the_bar_exactly_as_wide_as_requested() {
    // The bar sits in a fixed-width column; a bar that is one cell too long
    // reflows the whole meter row.
    for percent in 0..=100 {
        assert_eq!(bar(percent as f32 / 100.0, 17).chars().count(), 17);
    }
}

#[test]
fn clamps_a_value_outside_zero_to_one() {
    // A process can exceed one core; a meter cannot exceed its box.
    assert_eq!(bar(2.5, 4), "||||");
    assert_eq!(bar(-1.0, 4), "    ");
}

#[test]
fn renders_an_empty_bar_for_nan_rather_than_panicking() {
    assert_eq!(bar(f32::NAN, 4), "    ");
}

#[test]
fn renders_nothing_in_a_zero_width_column() {
    assert_eq!(bar(0.5, 0), "");
}

#[test]
fn shows_small_sizes_in_bytes() {
    assert_eq!(bytes(0), "0B");
    assert_eq!(bytes(512), "512B");
}

#[test]
fn scales_to_the_largest_unit_that_fits_the_column() {
    assert_eq!(bytes(1024), "1.0K");
    assert_eq!(bytes(1536), "1.5K");
    assert_eq!(bytes(1024 * 1024), "1.0M");
    assert_eq!(bytes(1024 * 1024 * 1024), "1.0G");
}

#[test]
fn advances_a_unit_before_rounding_would_overflow_the_column() {
    // The loop advances at >= 1024, but `{:.0}` rounds — so a value in
    // 1023.5..1024.0 printed four digits in the smaller unit.
    assert_eq!(bytes(1_048_575), "1.0M");
    assert_eq!(bytes(1_073_741_823), "1.0G");
}

#[test]
fn keeps_scaling_past_terabytes() {
    // The unit table stopped at T, so a petabyte-scale figure printed eight
    // digits. Out of reach of a process's RSS, but the claim is about the
    // function, not about /proc.
    assert_eq!(bytes(1024u64.pow(5)), "1.0P");
    assert_eq!(bytes(1024u64.pow(6)), "1.0E");
}

#[test]
fn never_prints_more_than_four_characters_of_value() {
    for shift in 0..64 {
        let n = 1u64 << shift;
        for candidate in [n, n.saturating_sub(1), n.saturating_add(1), u64::MAX] {
            let rendered = bytes(candidate);
            let digits = rendered.chars().filter(char::is_ascii_digit).count();
            assert!(
                digits <= 4,
                "bytes({candidate}) = {rendered:?} has {digits} digits"
            );
        }
    }
}

#[test]
fn drops_the_decimal_once_it_stops_adding_information() {
    // Column width is scarce. Below 10 the decimal distinguishes 1.2 from
    // 1.9; above it, "128M" says as much as "128.4M" in one fewer cell.
    assert_eq!(bytes(10 * 1024), "10K");
    assert_eq!(bytes(128 * 1024 * 1024), "128M");
    assert_eq!(bytes(5_439_725_568), "5.1G");
}

#[test]
fn formats_cpu_time_the_way_htop_does() {
    // Jiffies at the standard 100 Hz tick: minutes:seconds.hundredths.
    assert_eq!(cpu_time(0), "0:00.00");
    assert_eq!(cpu_time(12_345), "2:03.45");
    assert_eq!(cpu_time(29_786), "4:57.86");
}

#[test]
fn switches_to_hours_when_minutes_would_overflow_the_column() {
    assert_eq!(cpu_time(360_000), "1h00:00");
    assert_eq!(cpu_time(366_000), "1h01:00");
}

#[test]
fn formats_uptime_in_days_and_clock_time() {
    assert_eq!(uptime(Duration::from_secs(0)), "00:00:00");
    assert_eq!(uptime(Duration::from_secs(3_661)), "01:01:01");
    assert_eq!(uptime(Duration::from_secs(90_061)), "1 day, 01:01:01");
    assert_eq!(uptime(Duration::from_secs(176_461)), "2 days, 01:01:01");
}
