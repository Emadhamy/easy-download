//! Pure planning primitives for safe parallel HTTP byte-range downloads.
//!
//! This module deliberately contains no networking or filesystem code. Keeping
//! range arithmetic isolated makes boundary conditions easy to test before
//! introducing concurrent transfer workers.

use crate::DownloadUrl;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT_ENCODING, ETAG, LAST_MODIFIED, RANGE};
use reqwest::StatusCode;
use std::io::Read;

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

/// Stable metadata required before enabling parallel range downloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeProbe {
    pub total_bytes: u64,
    pub validator: String,
    pub validator_is_etag: bool,
}

/// Probes a server with GET Range: bytes=0-0. Unsupported or unverifiable
/// servers return an error so callers can safely fall back to single-stream
/// downloads instead of guessing.
pub fn probe_range_support(input_url: &str) -> Result<RangeProbe, String> {
    let url = DownloadUrl::parse(input_url)?;
    #[cfg(test)]
    let builder = Client::builder().no_proxy();
    #[cfg(not(test))]
    let builder = Client::builder();
    let client = builder.build()
        .map_err(|error| format!("Could not create HTTP client: {error}"))?;
    let mut response = client.get(url.as_str())
        .header(RANGE, "bytes=0-0")
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|error| format!("Range capability probe failed: {error}"))?;

    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(format!("Server does not support byte ranges (HTTP {})", response.status()));
    }
    let header = response.headers().get(reqwest::header::CONTENT_RANGE)
        .ok_or_else(|| "Range probe response omitted Content-Range".to_string())?
        .to_str()
        .map_err(|_| "Range probe returned an invalid Content-Range".to_string())?;
    let parsed = ContentRange::parse(header)?;
    parsed.validate(ByteRange { start: 0, end: 0 }, parsed.total)?;

    let (validator, validator_is_etag) = if let Some(etag) = response.headers().get(ETAG) {
        let value = etag.to_str().map_err(|_| "Invalid ETag header".to_string())?;
        if value.trim_start().starts_with("W/") {
            return Err("Weak ETag cannot safely validate parallel segments".to_string());
        }
        (value.to_string(), true)
    } else if let Some(modified) = response.headers().get(LAST_MODIFIED) {
        (modified.to_str().map_err(|_| "Invalid Last-Modified header".to_string())?.to_string(), false)
    } else {
        return Err("Server provided no stable ETag or Last-Modified validator".to_string());
    };

    let mut probe_byte = [0_u8; 1];
    let received = response.read(&mut probe_byte)
        .map_err(|error| format!("Could not read range probe body: {error}"))?;
    if received != 1 {
        return Err("Range probe did not return exactly one byte".to_string());
    }
    let mut extra_byte = [0_u8; 1];
    if response.read(&mut extra_byte)
        .map_err(|error| format!("Could not validate range probe body length: {error}"))? != 0
    {
        return Err("Range probe returned more than one byte".to_string());
    }

    Ok(RangeProbe {
        total_bytes: parsed.total,
        validator,
        validator_is_etag,
    })
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
    use super::{plan_ranges, probe_range_support, ByteRange, MAX_SEGMENTS_PER_FILE};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};

    fn start_http_server(status: &str, headers: &[(&str, &str)], body: &[u8]) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_string();
        let headers = headers.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect::<Vec<_>>();
        let body = body.to_vec();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 { break; }
                request.extend_from_slice(&buffer[..count]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") { break; }
            }
            let mut response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
            for (name, value) in headers {
                response.push_str(&format!("{name}: {value}\r\n"));
            }
            response.push_str("\r\n");
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
            String::from_utf8_lossy(&request).into_owned()
        });
        (format!("http://{address}/file.bin"), handle)
    }

    fn probe_with_response(status: &str, headers: &[(&str, &str)], body: &[u8]) -> (Result<super::RangeProbe, String>, String) {
        let (url, server) = start_http_server(status, headers, body);
        let result = probe_range_support(&url);
        let request = server.join().unwrap();
        (result, request)
    }

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

    #[test]
    fn probes_real_http_range_response_and_sends_exact_request_header() {
        let (result, request) = probe_with_response(
            "206 Partial Content",
            &[("Content-Range", "bytes 0-0/1234"), ("ETag", "\"stable-v1\"")],
            b"x",
        );
        let probe = result.unwrap();
        assert_eq!(probe.total_bytes, 1234);
        assert_eq!(probe.validator, "\"stable-v1\"");
        assert!(probe.validator_is_etag);
        assert!(request.starts_with("GET /file.bin HTTP/1.1"));
        assert!(request.to_ascii_lowercase().contains("range: bytes=0-0"));
        assert!(request.to_ascii_lowercase().contains("accept-encoding: identity"));
    }

    #[test]
    fn rejects_http_server_that_ignores_range_request() {
        let (result, _) = probe_with_response("200 OK", &[("ETag", "\"v1\"")], b"x");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_malformed_and_mismatched_content_range_responses() {
        for content_range in ["nonsense", "bytes 0-1/10", "bytes 1-1/10"] {
            let (result, _) = probe_with_response(
                "206 Partial Content",
                &[("Content-Range", content_range), ("ETag", "\"v1\"")],
                b"x",
            );
            assert!(result.is_err(), "accepted Content-Range {content_range}");
        }
    }

    #[test]
    fn rejects_missing_or_weak_range_validators() {
        let (missing, _) = probe_with_response("206 Partial Content", &[("Content-Range", "bytes 0-0/10")], b"x");
        assert!(missing.is_err());

        let (weak, _) = probe_with_response(
            "206 Partial Content",
            &[("Content-Range", "bytes 0-0/10"), ("ETag", "W/\"weak\"")],
            b"x",
        );
        assert!(weak.is_err());
    }

    #[test]
    fn accepts_last_modified_when_strong_etag_is_absent() {
        let (result, _) = probe_with_response(
            "206 Partial Content",
            &[("Content-Range", "bytes 0-0/10"), ("Last-Modified", "Wed, 21 Oct 2015 07:28:00 GMT")],
            b"x",
        );
        let probe = result.unwrap();
        assert_eq!(probe.total_bytes, 10);
        assert!(!probe.validator_is_etag);
    }

    #[test]
    fn rejects_probe_bodies_that_are_short_or_longer_than_one_byte() {
        let (short, _) = probe_with_response(
            "206 Partial Content",
            &[("Content-Range", "bytes 0-0/10"), ("ETag", "\"v1\"")],
            b"",
        );
        assert!(short.is_err());

        let (long, _) = probe_with_response(
            "206 Partial Content",
            &[("Content-Range", "bytes 0-0/10"), ("ETag", "\"v1\"")],
            b"xy",
        );
        assert!(long.is_err());
    }

}
