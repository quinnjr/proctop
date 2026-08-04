use proctop::model::SensorKind;
use proctop::sample::sensors::{parse_battery, parse_hwmon};

fn files(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(n, c)| (n.to_string(), c.to_string()))
        .collect()
}

#[test]
fn converts_millidegrees_to_celsius() {
    let sensors = parse_hwmon("coretemp", &files(&[("temp1_input", "47000\n")]));

    assert_eq!(sensors.len(), 1);
    assert_eq!(sensors[0].kind, SensorKind::Temperature);
    assert!((sensors[0].value - 47.0).abs() < 0.001, "{:?}", sensors[0]);
}

#[test]
fn prefers_the_sensors_own_label() {
    let sensors = parse_hwmon(
        "coretemp",
        &files(&[("temp1_input", "47000"), ("temp1_label", "Package id 0\n")]),
    );

    assert_eq!(sensors[0].label, "Package id 0");
}

#[test]
fn falls_back_to_the_file_name_when_there_is_no_label() {
    // Plenty of chips ship inputs with no label at all.
    let sensors = parse_hwmon("nvme", &files(&[("temp3_input", "38000")]));

    assert_eq!(sensors[0].label, "temp3");
}

#[test]
fn records_the_chip_each_reading_came_from() {
    // "Core 0" is meaningless without knowing whether it came from coretemp
    // or from a motherboard chip inventing its own names.
    let sensors = parse_hwmon("coretemp", &files(&[("temp1_input", "47000")]));

    assert_eq!(sensors[0].chip, "coretemp");
}

#[test]
fn orders_sensors_numerically_rather_than_lexicographically() {
    // Real indices are sparse: 1, 2, 6, 10, 14. Sorted as text, temp10 lands
    // between temp1 and temp2 and the core list reads as scrambled.
    let sensors = parse_hwmon(
        "coretemp",
        &files(&[
            ("temp14_input", "14000"),
            ("temp2_input", "2000"),
            ("temp10_input", "10000"),
            ("temp1_input", "1000"),
        ]),
    );

    let order: Vec<String> = sensors.iter().map(|s| s.label.clone()).collect();
    assert_eq!(order, vec!["temp1", "temp2", "temp10", "temp14"]);
}

#[test]
fn reads_fan_speeds_in_rpm_without_scaling_them() {
    let sensors = parse_hwmon("nct6798", &files(&[("fan1_input", "1250")]));

    assert_eq!(sensors[0].kind, SensorKind::Fan);
    assert!((sensors[0].value - 1250.0).abs() < 0.001);
}

#[test]
fn ignores_the_threshold_files_that_sit_alongside_the_readings() {
    // Every temp input is surrounded by _max, _crit and _crit_alarm; taking
    // them for readings triples the list with fictional sensors.
    let sensors = parse_hwmon(
        "coretemp",
        &files(&[
            ("temp1_input", "47000"),
            ("temp1_max", "100000"),
            ("temp1_crit", "100000"),
            ("temp1_crit_alarm", "0"),
            ("name", "coretemp"),
        ]),
    );

    assert_eq!(sensors.len(), 1);
}

#[test]
fn drops_a_reading_that_is_not_a_number() {
    // A sensor can return ENODATA and read back empty.
    let sensors = parse_hwmon(
        "nvme",
        &files(&[("temp1_input", ""), ("temp2_input", "38000")]),
    );

    assert_eq!(sensors.len(), 1);
    assert_eq!(sensors[0].label, "temp2");
}

#[test]
fn returns_nothing_for_a_chip_with_no_readings() {
    // spd5118 memory-module chips expose a name and nothing else.
    assert!(parse_hwmon("spd5118", &files(&[("name", "spd5118")])).is_empty());
}

#[test]
fn reads_battery_charge_and_status() {
    let battery = parse_battery("BAT0", "87\n", "Discharging\n").expect("should parse");

    assert_eq!(battery.kind, SensorKind::Battery);
    assert_eq!(battery.chip, "BAT0");
    assert!((battery.value - 87.0).abs() < 0.001);
    assert_eq!(battery.label, "Discharging");
}

#[test]
fn rejects_a_battery_with_an_unreadable_capacity() {
    assert!(parse_battery("BAT0", "", "Full").is_none());
}
