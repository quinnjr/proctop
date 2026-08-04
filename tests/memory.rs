use rtop::sample::memory;

const MEMINFO: &str = include_str!("fixtures/meminfo.txt");

#[test]
fn reads_totals_in_bytes_not_kilobytes() {
    // `/proc/meminfo` reports kB. Everything downstream works in bytes so
    // there is exactly one place that knows about the unit.
    let mem = memory::parse_meminfo(MEMINFO).expect("fixture should parse");

    assert_eq!(mem.total, 65_507_936 * 1024);
    assert_eq!(mem.free, 30_982_844 * 1024);
    assert_eq!(mem.available, 36_722_468 * 1024);
}

#[test]
fn reads_swap_totals() {
    let mem = memory::parse_meminfo(MEMINFO).expect("fixture should parse");

    assert_eq!(mem.swap_total, 166_970_360 * 1024);
    assert_eq!(mem.swap_free, 164_557_552 * 1024);
    assert_eq!(mem.swap_used(), (166_970_360 - 164_557_552) * 1024);
}

#[test]
fn excludes_reclaimable_memory_from_used() {
    // Buffers, page cache and reclaimable slab are all available under
    // pressure. Counting them as used is the classic "Linux ate my RAM"
    // misreport; htop shows them as their own meter segments.
    let mem = memory::parse_meminfo(MEMINFO).expect("fixture should parse");

    let cache = (13_092_832 + 3_080_172 - 382_520) * 1024;
    assert_eq!(mem.buffers, 825_316 * 1024);
    assert_eq!(mem.cached, cache);
    assert_eq!(
        mem.used(),
        mem.total - mem.free - mem.buffers - mem.cached,
        "used must be what is left after the reclaimable segments"
    );
}

#[test]
fn used_plus_the_reclaimable_segments_never_exceeds_total() {
    let mem = memory::parse_meminfo(MEMINFO).expect("fixture should parse");

    assert!(mem.used() + mem.buffers + mem.cached + mem.free <= mem.total);
}

#[test]
fn returns_none_when_the_total_is_missing() {
    assert!(memory::parse_meminfo("MemFree: 100 kB\n").is_none());
}

#[test]
fn treats_a_machine_without_swap_as_zero_rather_than_missing() {
    let mem =
        memory::parse_meminfo("MemTotal: 100 kB\nMemFree: 50 kB\n").expect("swap is optional");

    assert_eq!(mem.swap_total, 0);
    assert_eq!(mem.swap_used(), 0);
}
