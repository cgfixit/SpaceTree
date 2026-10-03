//! Physical byte ranges already counted by the scan, per device.
//!
//! APFS clones share extents. Once a clone is rewritten in part it gets its
//! own clone id, so ids alone cannot tell that the two files still share
//! most of their blocks. The scanner claims the physical ranges of every
//! file that shares data; bytes another file claimed first count as zero.

use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
pub(crate) struct ExtentLedger {
    /// Disjoint, non-adjacent `[start, end)` ranges keyed by start, per device.
    by_dev: HashMap<u64, BTreeMap<u64, u64>>,
}

impl ExtentLedger {
    /// Record `ranges` as counted on `dev` and return how many of their bytes
    /// were already counted. Ranges within one call may overlap each other;
    /// overlap among them counts once.
    pub(crate) fn claim(&mut self, dev: u64, ranges: &[(u64, u64)]) -> u64 {
        let map = self.by_dev.entry(dev).or_default();
        let mut asked = 0u64;
        let mut fresh = 0u64;
        for &(start, len) in ranges {
            let end = start.saturating_add(len);
            if end > start {
                asked = asked.saturating_add(end - start);
                fresh = fresh.saturating_add(insert(map, start, end));
            }
        }
        asked - fresh
    }
}

/// Bytes of `[start, end)` not yet in `map`.
fn uncovered(map: &BTreeMap<u64, u64>, start: u64, end: u64) -> u64 {
    let mut hit = 0u64;
    let first = map
        .range(..start)
        .next_back()
        .map(|(&s, _)| s)
        .unwrap_or(start);
    for (&s, &e) in map.range(first..end) {
        let lo = s.max(start);
        let hi = e.min(end);
        if hi > lo {
            hit += hi - lo;
        }
    }
    (end - start) - hit
}

/// Merge `[start, end)` into `map`; return the bytes that were not already there.
fn insert(map: &mut BTreeMap<u64, u64>, start: u64, end: u64) -> u64 {
    let gap = uncovered(map, start, end);
    let mut lo = start;
    let mut hi = end;
    let touching: Vec<u64> = {
        let first = map
            .range(..=start)
            .next_back()
            .map(|(&s, _)| s)
            .unwrap_or(start);
        map.range(first..=end)
            .filter(|(&s, &e)| e >= start && s <= end)
            .map(|(&s, _)| s)
            .collect()
    };
    for s in touching {
        if let Some(e) = map.remove(&s) {
            lo = lo.min(s);
            hi = hi.max(e);
        }
    }
    map.insert(lo, hi);
    gap
}

#[cfg(test)]
mod tests {
    use super::ExtentLedger;

    #[test]
    fn first_claim_is_all_new_and_a_repeat_is_all_counted() {
        let mut l = ExtentLedger::default();
        assert_eq!(l.claim(1, &[(0, 8192)]), 0);
        assert_eq!(l.claim(1, &[(0, 8192)]), 8192);
    }

    #[test]
    fn partial_overlap_counts_only_the_shared_bytes() {
        let mut l = ExtentLedger::default();
        // An 8 MiB file and a clone with its first 16 KiB rewritten elsewhere.
        let mib = 1 << 20;
        assert_eq!(l.claim(1, &[(0, 8 * mib)]), 0);
        let counted = l.claim(1, &[(100 * mib, 16384), (16384, 8 * mib - 16384)]);
        assert_eq!(counted, 8 * mib - 16384);
    }

    #[test]
    fn devices_are_separate_address_spaces() {
        let mut l = ExtentLedger::default();
        assert_eq!(l.claim(1, &[(0, 4096)]), 0);
        assert_eq!(l.claim(2, &[(0, 4096)]), 0);
    }

    #[test]
    fn ranges_merge_across_gaps_and_neighbors() {
        let mut l = ExtentLedger::default();
        assert_eq!(l.claim(1, &[(0, 10), (20, 10)]), 0);
        assert_eq!(l.claim(1, &[(10, 10)]), 0); // fills the gap exactly
        assert_eq!(l.claim(1, &[(5, 20)]), 20); // all inside the merged [0, 30)
        assert_eq!(l.claim(1, &[(25, 10)]), 5); // [25, 30) old, [30, 35) new
        assert_eq!(l.claim(1, &[(0, 35)]), 35);
    }

    #[test]
    fn overlap_within_one_claim_counts_once() {
        let mut l = ExtentLedger::default();
        assert_eq!(l.claim(1, &[(0, 100), (50, 100)]), 50);
        assert_eq!(l.claim(1, &[(0, 150)]), 150);
    }

    #[test]
    fn empty_and_zero_length_ranges_are_ignored() {
        let mut l = ExtentLedger::default();
        assert_eq!(l.claim(1, &[]), 0);
        assert_eq!(l.claim(1, &[(10, 0)]), 0);
    }
}
