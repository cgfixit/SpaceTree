//! Display sizes follow the macOS convention: decimal SI units.

use spacetree::format_bytes;

#[test]
fn bytes_under_one_kilobyte_stay_exact() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1), "1 B");
    assert_eq!(format_bytes(999), "999 B");
}

#[test]
fn units_are_powers_of_one_thousand() {
    assert_eq!(format_bytes(1_000), "1.00 KB");
    assert_eq!(format_bytes(4_096), "4.10 KB");
    assert_eq!(format_bytes(1_000_000), "1.00 MB");
    assert_eq!(format_bytes(1_000_000_000), "1.00 GB");
    assert_eq!(format_bytes(1_000_000_000_000), "1.00 TB");
    assert_eq!(format_bytes(2_000_000_000_000_000), "2.00 PB");
}

#[test]
fn a_one_terabyte_ssd_reads_like_finder() {
    // 994,662,584,320 bytes is the APFS container on a "1 TB" Mac. Finder shows
    // 994.66 GB; binary units would show 926.4 and mislabel it GB.
    assert_eq!(format_bytes(994_662_584_320), "994.7 GB");
    assert_eq!(format_bytes(8_001_563_222_016), "8.00 TB");
}

#[test]
fn precision_is_two_decimals_below_ten_and_one_above() {
    assert_eq!(format_bytes(1_234_567), "1.23 MB");
    assert_eq!(format_bytes(12_345_678), "12.3 MB");
    assert_eq!(format_bytes(123_456_789), "123.5 MB");
    assert_eq!(format_bytes(9_996_000), "10.0 MB");
}

#[test]
fn rounding_carries_into_the_next_unit() {
    assert_eq!(format_bytes(999_949), "999.9 KB");
    assert_eq!(format_bytes(999_950), "1.00 MB");
    assert_eq!(format_bytes(999_999_999), "1.00 GB");
    assert_eq!(format_bytes(u64::MAX), "18446.7 PB");
}
