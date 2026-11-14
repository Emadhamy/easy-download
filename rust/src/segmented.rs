//! Pure planning primitives for safe parallel HTTP byte-range downloads.
//!
//! This module deliberately contains no networking or filesystem code. Keeping
//! range arithmetic isolated makes boundary conditions easy to test before
//! introducing concurrent transfer workers.

/// Maximum number of byte-range segments allowed for one file.
pub const MAX_SEGMENTS_PER_FILE: usize = 8;

/// An inclusive HTTP byte range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    /// Number of bytes covered by this inclusive range.
    pub fn len(self) -> u64 {
        self.end - self.start + 1
    }

    /// Returns true when this range contains no bytes.
    pub fn is_empty(self) -> bool {
        self.end < self.start
    }

    /// Formats the range for an HTTP Range request.
    pub fn to_header_value(self) -> String {
        format!("bytes={}-{}", self.start, self.end)
    }
}

/// Divides a known positive file length into contiguous, non-overlapping
/// inclusive ranges. Earlier ranges receive one extra byte when division is
/// uneven. The segment count is intentionally capped to keep worker creation
/// bounded by policy.
pub fn plan_ranges(total_bytes: u64, segment_count: usize) -> Result<Vec<ByteRange>, String> {
    if total_bytes == 0 {
        return Err("Cannot segment an empty file".to_string());
    }
    if segment_count == 0 {
        return Err("Segment count must be at least one".to_string());
    }
    if segment_count > MAX_SEGMENTS_PER_FILE {
        return Err(format!(
            "Segment count exceeds the per-file limit of {MAX_SEGMENTS_PER_FILE}"
        ));
    }

    let count = u64::try_from(segment_count)
        .map_err(|_| "Segment count is too large".to_string())?;
    if count > total_bytes {
        return Err("Segment count cannot exceed the file length in bytes".to_string());
    }

    let base_length = total_bytes / count;
    let remainder = total_bytes % count;
    let mut ranges = Vec::with_capacity(segment_count);
    let mut next_start = 0_u64;

    for index in 0..count {
        let extra_byte = if index < remainder { 1_u64 } else { 0_u64 };
        let length = base_length + extra_byte;
        let end = next_start
            .checked_add(length - 1)
            .ok_or_else(|| "Byte-range arithmetic overflowed".to_string())?;
        ranges.push(ByteRange {
            start: next_start,
            end,
        });
        next_start = end
            .checked_add(1)
            .ok_or_else(|| "Byte-range arithmetic overflowed".to_string())?;
    }

    if next_start != total_bytes {
        return Err("Internal error: byte ranges do not cover the requested length".to_string());
    }

    Ok(ranges)
}

#[cfg(test)]
mod tests {
    use super::{plan_ranges, ByteRange, MAX_SEGMENTS_PER_FILE};

    #[test]
    fn plans_even_ranges_without_gaps_or_overlaps() {
        assert_eq!(
            plan_ranges(16, 4).unwrap(),
            vec![
                ByteRange { start: 0, end: 3 },
                ByteRange { start: 4, end: 7 },
                ByteRange { start: 8, end: 11 },
                ByteRange { start: 12, end: 15 },
            ]
        );
    }

    #[test]
    fn distributes_remainder_across_earlier_ranges() {
        assert_eq!(
            plan_ranges(11, 4).unwrap(),
            vec![
                ByteRange { start: 0, end: 2 },
                ByteRange { start: 3, end: 5 },
                ByteRange { start: 6, end: 8 },
                ByteRange { start: 9, end: 10 },
            ]
        );
    }

    #[test]
    fn handles_single_segment_and_one_byte_ranges() {
        assert_eq!(
            plan_ranges(7, 1).unwrap(),
            vec![ByteRange { start: 0, end: 6 }]
        );
        assert_eq!(
            plan_ranges(4, 4).unwrap(),
            vec![
                ByteRange { start: 0, end: 0 },
                ByteRange { start: 1, end: 1 },
                ByteRange { start: 2, end: 2 },
                ByteRange { start: 3, end: 3 },
            ]
        );
    }

    #[test]
    fn handles_large_lengths_without_overflow() {
        let ranges = plan_ranges(u64::MAX, MAX_SEGMENTS_PER_FILE).unwrap();
        assert_eq!(ranges.first().unwrap().start, 0);
        assert_eq!(ranges.last().unwrap().end, u64::MAX - 1);
        assert_eq!(ranges.iter().map(|range| range.len()).sum::<u64>(), u64::MAX);
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].end.checked_add(1), Some(pair[1].start));
        }
    }

    #[test]
    fn rejects_invalid_or_unbounded_segment_counts() {
        assert!(plan_ranges(0, 1).is_err());
        assert!(plan_ranges(10, 0).is_err());
        assert!(plan_ranges(10, MAX_SEGMENTS_PER_FILE + 1).is_err());
        assert!(plan_ranges(3, 4).is_err());
    }

    #[test]
    fn formats_inclusive_http_range_header() {
        let range = ByteRange { start: 12, end: 34 };
        assert_eq!(range.to_header_value(), "bytes=12-34");
        assert!(!range.is_empty());
    }
}
