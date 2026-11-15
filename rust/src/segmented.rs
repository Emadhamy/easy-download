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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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


// A persisted record contains only safe transfer metadata: never the URL,
// cookies, authorization headers, or other request credentials.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SegmentState {
    pub index: usize,
    pub range: ByteRange,
    pub received_bytes: u64,
}

/// Versioned sidecar data used to recover independently downloaded segments.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SegmentManifest {
    pub schema_version: u32,
    pub total_bytes: u64,
    pub validator: String,
    pub validator_is_etag: bool,
    pub segments: Vec<SegmentState>,
}

impl SegmentManifest {
    pub const SCHEMA_VERSION: u32 = 1;

    /// Creates a fresh manifest from a verified range probe and range plan.
    pub fn new(probe: &RangeProbe, segment_count: usize) -> Result<Self, String> {
        let ranges = plan_ranges(probe.total_bytes, segment_count)?;
        let manifest = Self {
            schema_version: Self::SCHEMA_VERSION,
            total_bytes: probe.total_bytes,
            validator: probe.validator.clone(),
            validator_is_etag: probe.validator_is_etag,
            segments: ranges.into_iter().enumerate().map(|(index, range)| SegmentState {
                index,
                range,
                received_bytes: 0,
            }).collect(),
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Rejects stale schemas, altered layouts, invalid validators, and
    /// progress counters that exceed the exact byte range assigned to a part.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != Self::SCHEMA_VERSION {
            return Err("Unsupported segmented-download manifest version".to_string());
        }
        if self.validator.trim().is_empty() {
            return Err("Segmented-download manifest has no remote validator".to_string());
        }
        if self.validator_is_etag && self.validator.trim_start().starts_with("W/") {
            return Err("Weak ETag is not valid for segmented-download recovery".to_string());
        }
        if self.segments.is_empty() || self.segments.len() > MAX_SEGMENTS_PER_FILE {
            return Err("Segmented-download manifest has an invalid segment count".to_string());
        }
        let expected = plan_ranges(self.total_bytes, self.segments.len())?;
        if self.segments.len() != expected.len() {
            return Err("Segmented-download manifest layout is incomplete".to_string());
        }
        for (index, (segment, expected_range)) in self.segments.iter().zip(expected.iter()).enumerate() {
            if segment.index != index || segment.range != *expected_range {
                return Err("Segmented-download manifest ranges are not canonical".to_string());
            }
            if segment.received_bytes > segment.range.len() {
                return Err("Segment progress exceeds its assigned byte range".to_string());
            }
        }
        Ok(())
    }
}


#[cfg(test)]
mod manifest_tests {
    use super::{ByteRange, RangeProbe, SegmentManifest};

    fn sample_manifest() -> SegmentManifest {
        SegmentManifest::new(&RangeProbe {
            total_bytes: 100,
            validator: "\"stable-v1\"".to_string(),
            validator_is_etag: true,
        }, 4).unwrap()
    }

    #[test]
    fn manifest_round_trips_and_contains_no_request_url_or_credentials() {
        let manifest = sample_manifest();
        let encoded = serde_json::to_string(&manifest).unwrap();
        let decoded: SegmentManifest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, manifest);
        assert!(!encoded.contains("https://"));
        assert!(!encoded.to_ascii_lowercase().contains("authorization"));
        assert!(!encoded.to_ascii_lowercase().contains("cookie"));
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn manifest_rejects_changed_or_noncanonical_segment_ranges() {
        let mut manifest = sample_manifest();
        manifest.segments[1].range = ByteRange { start: 24, end: 49 };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn manifest_rejects_progress_beyond_segment_length() {
        let mut manifest = sample_manifest();
        manifest.segments[0].received_bytes = manifest.segments[0].range.len() + 1;
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn manifest_rejects_unsupported_schema_and_weak_etag() {
        let mut manifest = sample_manifest();
        manifest.schema_version += 1;
        assert!(manifest.validate().is_err());

        let mut manifest = sample_manifest();
        manifest.validator = "W/\"weak\"".to_string();
        assert!(manifest.validate().is_err());
    }
}


// Downloads one complete, independently validated segment into a staging file.
// The caller may run several of these workers concurrently; each worker owns
// exactly one range and one part path.
pub fn download_segment_to_part(
    input_url: &str,
    manifest: &SegmentManifest,
    segment_index: usize,
    part_path: impl AsRef<std::path::Path>,
    control: crate::DownloadControl,
) -> Result<u64, String> {
    download_segment_to_part_with_progress(
        input_url, manifest, segment_index, part_path, control, |_| {},
    )
}

fn download_segment_to_part_with_progress<F>(
    input_url: &str,
    manifest: &SegmentManifest,
    segment_index: usize,
    part_path: impl AsRef<std::path::Path>,
    control: crate::DownloadControl,
    mut on_progress: F,
) -> Result<u64, String>
where
    F: FnMut(u64),
{
    use reqwest::header::{CONTENT_RANGE, ETAG, IF_RANGE, LAST_MODIFIED};
    use reqwest::StatusCode;
    use std::fs::{self, OpenOptions};
    use std::io::{Read, Write};

    manifest.validate()?;
    let segment = manifest.segments.get(segment_index)
        .ok_or_else(|| "Segment index is outside the manifest".to_string())?;
    let url = DownloadUrl::parse(input_url)?;
    let part_path = part_path.as_ref();
    let mut temporary_name = part_path.as_os_str().to_os_string();
    temporary_name.push(".easy-download.tmp");
    let temporary_path = std::path::PathBuf::from(temporary_name);

    let result = (|| {
        let client_builder = Client::builder();
        #[cfg(test)]
        let client_builder = client_builder.no_proxy();
        let client = client_builder.build()
            .map_err(|error| format!("Could not create HTTP client: {error}"))?;
        let mut request = client.get(url.as_str())
            .header(RANGE, segment.range.to_header_value())
            .header(ACCEPT_ENCODING, "identity")
            .header(IF_RANGE, manifest.validator.as_str());
        let mut response = request.send()
            .map_err(|error| format!("Segment request failed: {error}"))?;
        if response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(format!("Segment request expected HTTP 206, received {}", response.status()));
        }

        let content_range = response.headers().get(CONTENT_RANGE)
            .ok_or_else(|| "Segment response omitted Content-Range".to_string())?
            .to_str().map_err(|_| "Invalid segment Content-Range".to_string())?;
        ContentRange::parse(content_range)?.validate(segment.range, manifest.total_bytes)?;

        let response_validator = if manifest.validator_is_etag {
            response.headers().get(ETAG)
        } else {
            response.headers().get(LAST_MODIFIED)
        }.ok_or_else(|| "Segment response omitted its remote validator".to_string())?
            .to_str().map_err(|_| "Invalid segment validator header".to_string())?;
        if response_validator != manifest.validator {
            return Err("Remote representation changed while downloading segments".to_string());
        }
        if response.content_length().is_some_and(|length| length != segment.range.len()) {
            return Err("Segment Content-Length does not match its assigned range".to_string());
        }

        let mut output = OpenOptions::new().write(true).create_new(true)
            .open(&temporary_path)
            .map_err(|error| format!("Could not create segment staging file: {error}"))?;
        let mut received = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            control.wait_until_running()?;
            let count = response.read(&mut buffer)
                .map_err(|error| format!("Could not read segment response: {error}"))?;
            if count == 0 { break; }
            control.wait_until_running()?;
            let next = received.checked_add(count as u64)
                .ok_or_else(|| "Segment byte counter overflowed".to_string())?;
            if next > segment.range.len() {
                return Err("Segment response exceeded its assigned byte range".to_string());
            }
            output.write_all(&buffer[..count])
                .map_err(|error| format!("Could not write segment data: {error}"))?;
            received = next;
            on_progress(received);
        }
        if received != segment.range.len() {
            return Err(format!("Incomplete segment: received {received} of {} bytes", segment.range.len()));
        }
        output.flush().and_then(|_| output.sync_all())
            .map_err(|error| format!("Could not sync segment staging file: {error}"))?;
        drop(output);
        fs::rename(&temporary_path, part_path)
            .map_err(|error| format!("Could not finalize segment part file: {error}"))?;
        Ok(received)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}


#[cfg(test)]
mod worker_tests {
    use super::{download_segment_to_part, ByteRange, RangeProbe, SegmentManifest};
    use crate::DownloadControl;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn start_server(
        status: &str,
        content_range: Option<&str>,
        etag: Option<&str>,
        body: &'static [u8],
        content_length: Option<usize>,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_string();
        let content_range = content_range.map(str::to_string);
        let etag = etag.map(str::to_string);
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
            let mut response = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
            if let Some(length) = content_length {
                response.push_str(&format!("Content-Length: {length}\r\n"));
            }
            if let Some(value) = content_range {
                response.push_str(&format!("Content-Range: {value}\r\n"));
            }
            if let Some(value) = etag {
                response.push_str(&format!("ETag: {value}\r\n"));
            }
            response.push_str("\r\n");
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        (format!("http://{address}/segment.bin"), handle)
    }

    fn manifest() -> SegmentManifest {
        SegmentManifest::new(&RangeProbe {
            total_bytes: 4,
            validator: "\"stable-v1\"".to_string(),
            validator_is_etag: true,
        }, 1).unwrap()
    }

    fn destination(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("easy-download-segment-{label}-{unique}.part"))
    }

    fn run_worker(
        status: &str,
        content_range: Option<&str>,
        etag: Option<&str>,
        body: &'static [u8],
        content_length: Option<usize>,
        label: &str,
    ) -> (Result<u64, String>, std::path::PathBuf) {
        let (url, server) = start_server(status, content_range, etag, body, content_length);
        let path = destination(label);
        let result = download_segment_to_part(&url, &manifest(), 0, &path, DownloadControl::new());
        server.join().unwrap();
        (result, path)
    }

    #[test]
    fn worker_downloads_exact_range_and_atomically_creates_part_file() {
        let (result, path) = run_worker(
            "206 Partial Content", Some("bytes 0-3/4"), Some("\"stable-v1\""),
            b"data", Some(4), "success",
        );
        assert_eq!(result.unwrap(), 4);
        assert_eq!(std::fs::read(&path).unwrap(), b"data");
        let mut temporary = path.as_os_str().to_os_string();
        temporary.push(".easy-download.tmp");
        assert!(!std::path::PathBuf::from(temporary).exists());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn worker_rejects_server_that_ignores_range() {
        let (result, path) = run_worker("200 OK", None, Some("\"stable-v1\""), b"data", Some(4), "ignore-range");
        assert!(result.is_err());
        assert!(!path.exists());
    }

    #[test]
    fn worker_rejects_mismatched_content_range_and_changed_validator() {
        let (range_result, range_path) = run_worker(
            "206 Partial Content", Some("bytes 1-4/5"), Some("\"stable-v1\""),
            b"data", Some(4), "wrong-range",
        );
        assert!(range_result.is_err());
        assert!(!range_path.exists());

        let (validator_result, validator_path) = run_worker(
            "206 Partial Content", Some("bytes 0-3/4"), Some("\"stable-v2\""),
            b"data", Some(4), "changed-validator",
        );
        assert!(validator_result.is_err());
        assert!(!validator_path.exists());
    }

    #[test]
    fn worker_rejects_short_and_long_bodies_without_content_length() {
        let (short_result, short_path) = run_worker(
            "206 Partial Content", Some("bytes 0-3/4"), Some("\"stable-v1\""),
            b"abc", None, "short",
        );
        assert!(short_result.is_err());
        assert!(!short_path.exists());

        let (long_result, long_path) = run_worker(
            "206 Partial Content", Some("bytes 0-3/4"), Some("\"stable-v1\""),
            b"abcde", None, "long",
        );
        assert!(long_result.is_err());
        assert!(!long_path.exists());
    }

    #[test]
    fn worker_cleans_staging_file_when_cancelled() {
        let (url, server) = start_server(
            "206 Partial Content", Some("bytes 0-3/4"), Some("\"stable-v1\""),
            b"data", Some(4),
        );
        let path = destination("cancelled");
        let control = DownloadControl::new();
        control.cancel();
        let result = download_segment_to_part(&url, &manifest(), 0, &path, control);
        server.join().unwrap();
        assert!(result.unwrap_err().contains("cancelled"));
        assert!(!path.exists());
        let mut temporary = path.as_os_str().to_os_string();
        temporary.push(".easy-download.tmp");
        assert!(!std::path::PathBuf::from(temporary).exists());
    }

    #[test]
    fn worker_rejects_out_of_range_segment_index_before_network_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let path = destination("bad-index");
        let result = download_segment_to_part(
            &format!("http://{address}/unused"), &manifest(), 1, &path, DownloadControl::new(),
        );
        assert!(result.is_err());
        assert!(!path.exists());
        drop(listener);
    }
}


/// Default user-facing number of parallel segments per direct download.
pub const DEFAULT_SEGMENT_COUNT: usize = 4;
/// User-selectable upper bound for parallel segments per direct download.
pub const USER_MAX_SEGMENT_COUNT: usize = 8;
/// Number of attempts per segment, including the first request.
const SEGMENT_ATTEMPTS: usize = 3;

/// Downloads all segments concurrently, retries failed segments, and assembles
/// the verified parts in order into the destination only after every worker
/// succeeds. The caller may select 4 through 8 segments; four is the default.
///
/// Progress is reported as bytes arrive from every worker. The callback
/// receives completed segments, total segments, aggregate received bytes, and
/// total bytes. This first orchestration layer deliberately keeps recovery
/// metadata out of scope; durable per-segment restart recovery is a later step.
pub fn download_segmented_to_path<F>(
    input_url: &str,
    manifest: &SegmentManifest,
    destination: impl AsRef<std::path::Path>,
    segment_count: usize,
    control: crate::DownloadControl,
    mut on_progress: F,
) -> Result<u64, String>
where
    F: FnMut(usize, usize, u64, u64),
{
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::sync::mpsc;

    if !(DEFAULT_SEGMENT_COUNT..=USER_MAX_SEGMENT_COUNT).contains(&segment_count) {
        return Err(format!(
            "Parallel segment count must be between {DEFAULT_SEGMENT_COUNT} and {USER_MAX_SEGMENT_COUNT}"
        ));
    }
    manifest.validate()?;
    if manifest.segments.len() != segment_count {
        return Err("Segment count does not match the validated manifest".to_string());
    }
    let url = DownloadUrl::parse(input_url)?;
    let destination = destination.as_ref();
    if destination.exists() {
        return Err("Destination already exists; refusing to overwrite an existing file".to_string());
    }
    let parent = destination.parent().filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    if !parent.is_dir() {
        return Err("Destination directory does not exist".to_string());
    }

    let mut part_paths = Vec::with_capacity(segment_count);
    for index in 0..segment_count {
        let mut name = destination.as_os_str().to_os_string();
        name.push(format!(".segment-{index}.part"));
        part_paths.push(std::path::PathBuf::from(name));
    }
    let mut assembly_name = destination.as_os_str().to_os_string();
    assembly_name.push(".easy-download.assembling");
    let assembly_path = std::path::PathBuf::from(assembly_name);

    // Never reuse untrusted leftovers from a prior attempt. Durable recovery
    // will add a request fingerprint and verified sidecar before reusing parts.
    for path in &part_paths {
        if path.exists() {
            return Err("A segment part already exists; clean up or recover it before retrying".to_string());
        }
    }
    if assembly_path.exists() {
        return Err("An assembly staging file already exists".to_string());
    }

    let shared_control = control.clone();
    enum WorkerMessage {
        Progress(usize, u64),
        Finished(usize, Result<u64, String>),
    }

    let (sender, receiver) = mpsc::channel::<WorkerMessage>();
    let mut first_error: Option<String> = None;
    let mut completed_segments = 0_usize;
    let mut segment_progress = vec![0_u64; segment_count];

    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(segment_count);
        for (index, part_path) in part_paths.iter().cloned().enumerate() {
            let sender = sender.clone();
            let worker_url = url.as_str().to_string();
            let worker_manifest = manifest.clone();
            let worker_control = shared_control.clone();
            handles.push(scope.spawn(move || {
                let mut last_error = None;
                for attempt in 1..=SEGMENT_ATTEMPTS {
                    if worker_control.wait_until_running().is_err() {
                        last_error = Some("Download cancelled".to_string());
                        break;
                    }
                    // A retry starts a fresh response, so remove this attempt's
                    // previous contribution from the aggregate progress.
                    let _ = sender.send(WorkerMessage::Progress(index, 0));
                    if part_path.exists() {
                        let _ = fs::remove_file(&part_path);
                    }
                    match download_segment_to_part_with_progress(
                        &worker_url,
                        &worker_manifest,
                        index,
                        &part_path,
                        worker_control.clone(),
                        |received| {
                            let _ = sender.send(WorkerMessage::Progress(index, received));
                        },
                    ) {
                        Ok(bytes) => {
                            let _ = sender.send(WorkerMessage::Finished(index, Ok(bytes)));
                            return;
                        }
                        Err(error) => {
                            last_error = Some(error);
                            if attempt < SEGMENT_ATTEMPTS {
                                // A failed attempt must not leave a partial part.
                                let _ = fs::remove_file(&part_path);
                            }
                        }
                    }
                }
                let _ = sender.send(WorkerMessage::Finished(
                    index,
                    Err(last_error.unwrap_or_else(|| "Segment failed without an error detail".to_string())),
                ));
            }));
        }
        drop(sender);

        let mut finished_workers = 0_usize;
        while finished_workers < segment_count {
            match receiver.recv() {
                Ok(WorkerMessage::Progress(index, received)) => {
                    if let Some(progress) = segment_progress.get_mut(index) {
                        *progress = received;
                    }
                    let received_total = segment_progress.iter().copied().sum();
                    on_progress(
                        completed_segments,
                        segment_count,
                        received_total,
                        manifest.total_bytes,
                    );
                }
                Ok(WorkerMessage::Finished(index, Ok(bytes))) => {
                    finished_workers += 1;
                    completed_segments += 1;
                    if let Some(progress) = segment_progress.get_mut(index) {
                        *progress = bytes;
                    }
                    let received_total = segment_progress.iter().copied().sum();
                    on_progress(
                        completed_segments,
                        segment_count,
                        received_total,
                        manifest.total_bytes,
                    );
                }
                Ok(WorkerMessage::Finished(_index, Err(error))) => {
                    finished_workers += 1;
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                    shared_control.cancel();
                }
                Err(_) => {
                    if first_error.is_none() {
                        first_error = Some("A segment worker stopped unexpectedly".to_string());
                    }
                    shared_control.cancel();
                    break;
                }
            }
        }
        for handle in handles {
            if handle.join().is_err() && first_error.is_none() {
                first_error = Some("A segment worker panicked".to_string());
                shared_control.cancel();
            }
        }
    });

    let completed_bytes: u64 = segment_progress.iter().copied().sum();

    if let Some(error) = first_error {
        for path in &part_paths { let _ = fs::remove_file(path); }
        let _ = fs::remove_file(&assembly_path);
        return Err(error);
    }
    if completed_segments != segment_count || completed_bytes != manifest.total_bytes {
        for path in &part_paths { let _ = fs::remove_file(path); }
        return Err("Completed segment byte count does not match the original file".to_string());
    }

    let assembly_result = (|| {
        let mut output = OpenOptions::new().write(true).create_new(true)
            .open(&assembly_path)
            .map_err(|error| format!("Could not create assembly staging file: {error}"))?;
        let mut assembled_bytes = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        for (index, part_path) in part_paths.iter().enumerate() {
            control.wait_until_running()?;
            let expected = manifest.segments[index].range.len();
            let mut input = File::open(part_path)
                .map_err(|error| format!("Could not open completed segment {index}: {error}"))?;
            let mut part_bytes = 0_u64;
            loop {
                let count = input.read(&mut buffer)
                    .map_err(|error| format!("Could not read completed segment {index}: {error}"))?;
                if count == 0 { break; }
                part_bytes = part_bytes.checked_add(count as u64)
                    .ok_or_else(|| "Segment assembly byte counter overflowed".to_string())?;
                if part_bytes > expected {
                    return Err(format!("Segment {index} exceeds its expected length"));
                }
                output.write_all(&buffer[..count])
                    .map_err(|error| format!("Could not assemble segment {index}: {error}"))?;
            }
            if part_bytes != expected {
                return Err(format!("Segment {index} has {part_bytes} bytes; expected {expected}"));
            }
            assembled_bytes = assembled_bytes.checked_add(part_bytes)
                .ok_or_else(|| "Final assembly byte counter overflowed".to_string())?;
        }
        output.flush().and_then(|_| output.sync_all())
            .map_err(|error| format!("Could not sync assembled download: {error}"))?;
        drop(output);
        if assembled_bytes != manifest.total_bytes {
            return Err(format!(
                "Assembled file has {assembled_bytes} bytes; expected {}",
                manifest.total_bytes
            ));
        }
        fs::rename(&assembly_path, destination)
            .map_err(|error| format!("Could not finalize assembled download: {error}"))?;
        Ok(assembled_bytes)
    })();

    if assembly_result.is_ok() {
        for path in &part_paths { let _ = fs::remove_file(path); }
    } else {
        let _ = fs::remove_file(&assembly_path);
        // Keep complete verified parts if assembly fails so a later recovery
        // implementation can preserve useful work; do not create a final file.
    }
    assembly_result
}


#[cfg(test)]
mod orchestrator_tests {
    use super::{
        download_segmented_to_path, RangeProbe, SegmentManifest,
        DEFAULT_SEGMENT_COUNT, USER_MAX_SEGMENT_COUNT,
    };
    use crate::DownloadControl;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> Vec<u8> {
        (0..=255).cycle().take(128 * 1024).map(|value| value as u8).collect()
    }

    fn range_server(body: Vec<u8>, expected_requests: usize) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            for _ in 0..expected_requests {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 2048];
                loop {
                    let count = stream.read(&mut buffer).unwrap();
                    if count == 0 { break; }
                    request.extend_from_slice(&buffer[..count]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") { break; }
                }
                let request = String::from_utf8_lossy(&request);
                let range_line = request.lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                    .expect("worker must send Range header");
                let value = range_line.split_once('=').unwrap().1.trim();
                let (start, end) = value.split_once('-').unwrap();
                let start = start.parse::<usize>().unwrap();
                let end = end.parse::<usize>().unwrap();
                assert!(start <= end && end < body.len());
                let chunk = &body[start..=end];
                let headers = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nETag: \"orchestrator-v1\"\r\nConnection: close\r\n\r\n",
                    chunk.len(), start, end, body.len()
                );
                stream.write_all(headers.as_bytes()).unwrap();
                stream.write_all(chunk).unwrap();
            }
        });
        (format!("http://{address}/fixture.bin"), handle)
    }

    fn destination(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("easy-download-orchestrator-{label}-{unique}.bin"))
    }

    fn run_success(segment_count: usize) {
        let body = fixture();
        let (url, server) = range_server(body.clone(), segment_count);
        let probe = RangeProbe {
            total_bytes: body.len() as u64,
            validator: "\"orchestrator-v1\"".to_string(),
            validator_is_etag: true,
        };
        let manifest = SegmentManifest::new(&probe, segment_count).unwrap();
        let path = destination(&format!("success-{segment_count}"));
        let mut progress_events = Vec::new();
        let received = download_segmented_to_path(
            &url,
            &manifest,
            &path,
            segment_count,
            DownloadControl::new(),
            |completed, total, bytes, expected| {
                progress_events.push((completed, total, bytes, expected));
            },
        ).unwrap();
        server.join().unwrap();
        assert_eq!(received, body.len() as u64);
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert!(progress_events.len() >= segment_count);
        assert!(progress_events.iter().any(|event| {
            event.2 > 0 && event.2 < body.len() as u64
        }), "progress should report bytes before the whole file completes");
        assert_eq!(progress_events.last().unwrap().0, segment_count);
        assert_eq!(progress_events.last().unwrap().1, segment_count);
        assert_eq!(progress_events.last().unwrap().2, body.len() as u64);
        assert_eq!(progress_events.last().unwrap().3, body.len() as u64);
        for index in 0..segment_count {
            let mut part = path.as_os_str().to_os_string();
            part.push(format!(".segment-{index}.part"));
            assert!(!std::path::PathBuf::from(part).exists());
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn default_policy_uses_four_segments() {
        assert_eq!(DEFAULT_SEGMENT_COUNT, 4);
        run_success(DEFAULT_SEGMENT_COUNT);
    }

    #[test]
    fn user_can_select_eight_segments() {
        assert_eq!(USER_MAX_SEGMENT_COUNT, 8);
        run_success(USER_MAX_SEGMENT_COUNT);
    }

    #[test]
    fn paused_segmented_download_waits_until_resumed() {
        let body = fixture();
        let (url, server) = range_server(body.clone(), DEFAULT_SEGMENT_COUNT);
        let probe = RangeProbe {
            total_bytes: body.len() as u64,
            validator: "\"orchestrator-v1\"".to_string(),
            validator_is_etag: true,
        };
        let manifest = SegmentManifest::new(&probe, DEFAULT_SEGMENT_COUNT).unwrap();
        let path = destination("pause-resume");
        let control = DownloadControl::new();
        control.pause();

        let resume_control = control.clone();
        let resume = thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(40));
            resume_control.resume();
        });

        let received = download_segmented_to_path(
            &url,
            &manifest,
            &path,
            DEFAULT_SEGMENT_COUNT,
            control,
            |_, _, _, _| {},
        ).unwrap();

        resume.join().unwrap();
        server.join().unwrap();
        assert_eq!(received, body.len() as u64);
        assert_eq!(std::fs::read(&path).unwrap(), body);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_segment_count_outside_user_settings() {
        let probe = RangeProbe {
            total_bytes: 64,
            validator: "\"v1\"".to_string(),
            validator_is_etag: true,
        };
        let manifest = SegmentManifest::new(&probe, 3).unwrap();
        let path = destination("invalid-count");
        let result = download_segmented_to_path(
            "http://127.0.0.1:1/file",
            &manifest,
            &path,
            3,
            DownloadControl::new(),
            |_, _, _, _| {},
        );
        assert!(result.unwrap_err().contains("between 4 and 8"));
        assert!(!path.exists());
    }

    #[test]
    fn refuses_to_overwrite_an_existing_destination() {
        let path = destination("existing");
        std::fs::write(&path, b"keep-existing").unwrap();
        let probe = RangeProbe {
            total_bytes: 64,
            validator: "\"v1\"".to_string(),
            validator_is_etag: true,
        };
        let manifest = SegmentManifest::new(&probe, DEFAULT_SEGMENT_COUNT).unwrap();
        let result = download_segmented_to_path(
            "http://127.0.0.1:1/file",
            &manifest,
            &path,
            DEFAULT_SEGMENT_COUNT,
            DownloadControl::new(),
            |_, _, _, _| {},
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"keep-existing");
        std::fs::remove_file(path).unwrap();
    }
}