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

/// A strictly parsed Content-Range response for a single byte-range request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

impl ContentRange {
    /// Parses the canonical form: `bytes START-END/TOTAL`.
    /// Unsatisfied ranges and unknown totals are deliberately rejected.
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        let span = value.strip_prefix("bytes ")
            .ok_or_else(|| "Unsupported Content-Range unit".to_string())?;
        let (range, total) = span.split_once('/')
            .ok_or_else(|| "Malformed Content-Range total".to_string())?;
        if total.is_empty() || total == "*" || total.contains('/') {
            return Err("Content-Range must include a known total length".to_string());
        }
        let (start, end) = range.split_once('-')
            .ok_or_else(|| "Malformed Content-Range span".to_string())?;
        if start.is_empty() || end.is_empty() || end.contains('-') {
            return Err("Malformed Content-Range span".to_string());
        }
        let start = start.parse::<u64>()
            .map_err(|_| "Invalid Content-Range start".to_string())?;
        let end = end.parse::<u64>()
            .map_err(|_| "Invalid Content-Range end".to_string())?;
        let total = total.parse::<u64>()
            .map_err(|_| "Invalid Content-Range total".to_string())?;
        if total == 0 || start > end || end >= total {
            return Err("Content-Range bounds are outside the representation".to_string());
        }
        Ok(Self { start, end, total })
    }

    /// Confirms that the response exactly matches the requested range and file size.
    pub fn validate(self, expected: ByteRange, expected_total: u64) -> Result<(), String> {
        if self.start != expected.start || self.end != expected.end || self.total != expected_total {
            return Err(format!(
                "Content-Range mismatch: received bytes {}-{}/{}; expected bytes {}-{}/{}",
                self.start, self.end, self.total,
                expected.start, expected.end, expected_total
            ));
        }
        Ok(())
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
    fn parses_and_validates_exact_content_range() {
        let parsed = super::ContentRange::parse("bytes 10-19/100").unwrap();
        assert_eq!(parsed, super::ContentRange { start: 10, end: 19, total: 100 });
        assert!(parsed.validate(ByteRange { start: 10, end: 19 }, 100).is_ok());
    }

    #[test]
    fn rejects_malformed_or_unknown_content_ranges() {
        for value in [
            "items 0-1/2",
            "bytes 0-1/*",
            "bytes 0-1/",
            "bytes -1/2",
            "bytes 2-1/3",
            "bytes 0-3/3",
            "bytes 0-1/0",
            "bytes 0-1/2/3",
            "bytes 0-x/2",
        ] {
            assert!(super::ContentRange::parse(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn rejects_content_range_that_does_not_match_requested_segment() {
        let parsed = super::ContentRange::parse("bytes 10-19/100").unwrap();
        assert!(parsed.validate(ByteRange { start: 10, end: 18 }, 100).is_err());
        assert!(parsed.validate(ByteRange { start: 10, end: 19 }, 101).is_err());
        assert!(parsed.validate(ByteRange { start: 11, end: 19 }, 100).is_err());
    }

    #[test]
    fn formats_inclusive_http_range_header() {
        let range = ByteRange { start: 12, end: 34 };
        assert_eq!(range.to_header_value(), "bytes=12-34");
        assert!(!range.is_empty());
    }
}
