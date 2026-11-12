//! HTTP Range based download continuation.
//!
//! Partial data is kept beside the destination in a .easy-download.part file.
//! A sidecar records the URL and an HTTP validator so a later attempt can
//! continue only when the remote representation is still the same.

use crate::{DownloadControl, DownloadUrl};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_RANGE, ETAG, IF_RANGE, LAST_MODIFIED, RANGE};
use sha2::{Digest, Sha256};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct ResumeMetadata {
    request_fingerprint: String,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    last_modified: Option<String>,
}

impl ResumeMetadata {
    fn validator(&self) -> Option<&str> {
        self.etag.as_deref().or(self.last_modified.as_deref())
    }
}

fn sidecar_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".easy-download.meta.json");
    PathBuf::from(name)
}

fn part_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".easy-download.part");
    PathBuf::from(name)
}

fn content_range_start_and_total(response: &Response) -> Result<(u64, Option<u64>), String> {
    let value = response.headers().get(CONTENT_RANGE)
        .ok_or_else(|| "Server returned partial content without Content-Range".to_string())?
        .to_str()
        .map_err(|_| "Server returned an invalid Content-Range header".to_string())?;
    let range = value.strip_prefix("bytes ")
        .ok_or_else(|| "Server returned an unsupported Content-Range unit".to_string())?;
    let (span, total) = range.split_once('/')
        .ok_or_else(|| "Server returned an invalid Content-Range header".to_string())?;
    let (start, _) = span.split_once('-')
        .ok_or_else(|| "Server returned an invalid Content-Range span".to_string())?;
    let start = start.parse::<u64>()
        .map_err(|_| "Server returned an invalid Content-Range start".to_string())?;
    let total = if total == "*" { None } else {
        Some(total.parse::<u64>()
            .map_err(|_| "Server returned an invalid Content-Range total".to_string())?)
    };
    Ok((start, total))
}

fn header_string(response: &Response, name: reqwest::header::HeaderName) -> Option<String> {
    response.headers().get(name).and_then(|value| value.to_str().ok()).map(str::to_owned)
}

fn save_metadata(path: &Path, metadata: &ResumeMetadata) -> Result<(), String> {
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    let encoded = serde_json::to_vec(metadata)
        .map_err(|error| format!("Could not encode resume metadata: {error}"))?;
    let mut file = OpenOptions::new().write(true).create(true).truncate(true)
        .open(&temporary)
        .map_err(|error| format!("Could not create resume metadata: {error}"))?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("Could not save resume metadata: {error}"))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("Could not finalize resume metadata: {error}"))
}

/// Downloads a URL while preserving a valid partial file after network errors or
/// process interruption. Explicit cancellation removes partial data.
pub fn download_to_path_resumable_with_control<F>(
    input_url: &str,
    destination: impl AsRef<Path>,
    control: DownloadControl,
    on_progress: F,
) -> Result<u64, String>
where
    F: FnMut(u64, Option<u64>),
{
    download_to_path_resumable_with_control_and_headers(
        input_url, "{}", destination, control, on_progress,
    )
}

/// Header-aware variant used by yt-dlp media URLs. Request identity is hashed,
/// so signed URLs and sensitive query strings are not written to the sidecar.
pub fn download_to_path_resumable_with_control_and_headers<F>(
    input_url: &str,
    headers_json: &str,
    destination: impl AsRef<Path>,
    control: DownloadControl,
    mut on_progress: F,
) -> Result<u64, String>
where
    F: FnMut(u64, Option<u64>),
{
    let url = DownloadUrl::parse(input_url)?;
    let headers = parse_request_headers(headers_json)?;
    let fingerprint = request_fingerprint(url.as_str(), &headers)?;
    let destination = destination.as_ref();
    let partial = part_path(destination);
    let metadata_file = sidecar_path(destination);
    let client = Client::builder().build()
        .map_err(|error| format!("Could not create HTTP client: {error}"))?;

    let mut offset = fs::metadata(&partial).map(|metadata| metadata.len()).unwrap_or(0);
    let mut previous_metadata = if offset > 0 {
        fs::read(&metadata_file).ok()
            .and_then(|bytes| serde_json::from_slice::<ResumeMetadata>(&bytes).ok())
    } else {
        None
    };

    if offset > 0 && !previous_metadata.as_ref().is_some_and(|metadata| {
        metadata.request_fingerprint == fingerprint && metadata.validator().is_some()
    }) {
        let _ = fs::remove_file(&partial);
        let _ = fs::remove_file(&metadata_file);
        offset = 0;
        previous_metadata = None;
    }

    let mut request = client.get(url.as_str()).headers(headers.clone());
    if offset > 0 {
        let validator = previous_metadata.as_ref().and_then(ResumeMetadata::validator)
            .ok_or_else(|| "No safe HTTP validator is available for resuming".to_string())?;
        request = request.header(RANGE, format!("bytes={offset}-"))
            .header(IF_RANGE, validator);
    }
    let mut response = request.send()
        .map_err(|error| format!("Download request failed: {error}"))?;

    let mut resumed = false;
    let total_bytes;
    if offset > 0 && response.status() == StatusCode::PARTIAL_CONTENT {
        let (range_start, total) = content_range_start_and_total(&response)?;
        if range_start != offset {
            return Err(format!("Server resumed at byte {range_start}, expected {offset}"));
        }
        let old = previous_metadata.as_ref().expect("validated metadata exists");
        let response_etag = header_string(&response, ETAG);
        let response_modified = header_string(&response, LAST_MODIFIED);
        if old.etag.as_ref().zip(response_etag.as_ref()).is_some_and(|(a, b)| a != b)
            || old.last_modified.as_ref().zip(response_modified.as_ref()).is_some_and(|(a, b)| a != b)
        {
            response = client.get(url.as_str()).headers(headers.clone()).send()
                .map_err(|error| format!("Restart request failed: {error}"))?;
            if response.status() != StatusCode::OK {
                return Err(format!("Server did not provide a full response after validator change: {}", response.status()));
            }
            offset = 0;
            resumed = false;
            let _ = fs::remove_file(&partial);
            let _ = fs::remove_file(&metadata_file);
            total_bytes = response.content_length();
        } else {
            resumed = true;
            total_bytes = total;
        }
    } else if offset > 0 && response.status() == StatusCode::OK {
        offset = 0;
        let _ = fs::remove_file(&partial);
        let _ = fs::remove_file(&metadata_file);
        total_bytes = response.content_length();
    } else {
        if response.status() != StatusCode::OK {
            return Err(format!("Download request failed: {}", response.status()));
        }
        total_bytes = response.content_length();
    }

    let metadata = ResumeMetadata {
        request_fingerprint: fingerprint,
        etag: header_string(&response, ETAG)
            .or_else(|| previous_metadata.as_ref().and_then(|old| old.etag.clone())),
        last_modified: header_string(&response, LAST_MODIFIED)
            .or_else(|| previous_metadata.as_ref().and_then(|old| old.last_modified.clone())),
    };
    save_metadata(&metadata_file, &metadata)?;

    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&partial)
            .map_err(|error| format!("Could not open partial download file: {error}"))?;

        let mut received = offset;
        on_progress(received, total_bytes);
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            control.wait_until_running()?;
            let bytes_read = response_read(&mut response, &mut buffer)?;
            if bytes_read == 0 { break; }
            control.wait_until_running()?;
            output.write_all(&buffer[..bytes_read])
                .map_err(|error| format!("Could not write downloaded data: {error}"))?;
            received += bytes_read as u64;
            on_progress(received, total_bytes);
        }

        output.flush()
            .and_then(|_| output.sync_all())
            .map_err(|error| format!("Could not sync partial download: {error}"))?;
        drop(output);

        if total_bytes.is_some_and(|total| total != received) {
            return Err(format!("Incomplete response: received {received} of {} bytes", total_bytes.unwrap()));
        }
        fs::rename(&partial, destination)
            .map_err(|error| format!("Could not finalize downloaded file: {error}"))?;
        let _ = fs::remove_file(&metadata_file);
        Ok(received)
    })();

    if result.as_ref().is_err_and(|error| error.contains("Download cancelled")) {
        let _ = fs::remove_file(&partial);
        let _ = fs::remove_file(&metadata_file);
    }
    result
}

fn parse_request_headers(encoded: &str) -> Result<HeaderMap, String> {
    let values: serde_json::Value = serde_json::from_str(encoded)
        .map_err(|error| format!("Invalid request headers JSON: {error}"))?;
    let object = values.as_object()
        .ok_or_else(|| "Request headers must be a JSON object".to_string())?;
    if object.len() > 32 {
        return Err("Too many request headers".to_string());
    }
    let mut headers = HeaderMap::new();
    for (name, value) in object {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| "Invalid HTTP request header name".to_string())?;
        if matches!(name.as_str(), "host" | "content-length" | "range" | "if-range" | "connection" | "transfer-encoding") {
            return Err(format!("The request header '{}' is managed by the downloader", name));
        }
        let value = value.as_str()
            .ok_or_else(|| format!("Request header '{}' must have a string value", name))?;
        if value.len() > 8192 {
            return Err(format!("Request header '{}' is too large", name));
        }
        let value = HeaderValue::from_str(value)
            .map_err(|_| format!("Invalid value for request header '{}'", name))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn request_fingerprint(url: &str, headers: &HeaderMap) -> Result<String, String> {
    let mut canonical = format!("{url}\\n");
    let mut entries: Vec<_> = headers.iter().collect();
    entries.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    for (name, value) in entries {
        canonical.push_str(name.as_str());
        canonical.push(':');
        canonical.push_str(value.to_str().map_err(|_| "Invalid request header encoding".to_string())?);
        canonical.push('\\n');
    }
    let digest = Sha256::digest(canonical.as_bytes());
    Ok(format!("{:x}", digest))
}

fn response_read(response: &mut Response, buffer: &mut [u8]) -> Result<usize, String> {
    response.read(buffer).map_err(|error| format!("Could not read download response: {error}"))
}

#[cfg(test)]
mod tests {
    use super::download_to_path_resumable_with_control;
    use crate::DownloadControl;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn destination() -> std::path::PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("easy-download-resume-{unique}.bin"))
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 512];
        while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = stream.read(&mut chunk).unwrap();
            if count == 0 { break; }
            bytes.extend_from_slice(&chunk[..count]);
        }
        String::from_utf8_lossy(&bytes).to_string()
    }

    #[test]
    fn resumes_interrupted_download_with_range_and_if_range() {
        let body = b"easy-download-resume-payload";
        let split = 9;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_body = body.to_vec();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            let _ = read_request(&mut first);
            write!(first,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n",
                server_body.len()).unwrap();
            first.write_all(&server_body[..split]).unwrap();
            drop(first);

            let (mut second, _) = listener.accept().unwrap();
            let request = read_request(&mut second);
            assert!(request.to_ascii_lowercase().contains(&format!("range: bytes={split}-")));
            assert!(request.to_ascii_lowercase().contains("if-range: \"v1\""));
            write!(second,
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n",
                server_body.len() - split, split, server_body.len() - 1, server_body.len()).unwrap();
            second.write_all(&server_body[split..]).unwrap();
        });

        let url = format!("http://{address}/file.bin");
        let target = destination();
        let first = download_to_path_resumable_with_control(&url, &target, DownloadControl::new(), |_, _| {});
        assert!(first.is_err(), "the deliberately interrupted first transfer must fail");
        let partial = std::path::PathBuf::from(format!("{}.easy-download.part", target.display()));
        assert_eq!(std::fs::metadata(&partial).unwrap().len(), split as u64);

        let mut last_progress = 0;
        let received = download_to_path_resumable_with_control(&url, &target, DownloadControl::new(), |bytes, _| {
            last_progress = bytes;
        }).unwrap();
        server.join().unwrap();

        assert_eq!(received, body.len() as u64);
        assert_eq!(last_progress, body.len() as u64);
        assert_eq!(std::fs::read(&target).unwrap(), body);
        assert!(!partial.exists());
        assert!(!std::path::PathBuf::from(format!("{}.easy-download.meta.json", target.display())).exists());
        std::fs::remove_file(target).unwrap();
    }

    #[test]
    fn restarts_cleanly_when_server_ignores_range_request() {
        let body = b"fresh-complete-download-payload";
        let split = 8;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_body = body.to_vec();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            let _ = read_request(&mut first);
            write!(first,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n",
                server_body.len()).unwrap();
            first.write_all(&server_body[..split]).unwrap();
            drop(first);

            let (mut second, _) = listener.accept().unwrap();
            let request = read_request(&mut second);
            assert!(request.to_ascii_lowercase().contains(&format!("range: bytes={split}-")));
            // Some servers ignore Range and return the complete representation.
            write!(second,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n",
                server_body.len()).unwrap();
            second.write_all(&server_body).unwrap();
        });

        let url = format!("http://{address}/file.bin");
        let target = destination();
        let first = download_to_path_resumable_with_control(
            &url, &target, DownloadControl::new(), |_, _| {},
        );
        assert!(first.is_err(), "the deliberately interrupted first transfer must fail");

        let partial = std::path::PathBuf::from(format!("{}.easy-download.part", target.display()));
        assert_eq!(std::fs::metadata(&partial).unwrap().len(), split as u64);

        let received = download_to_path_resumable_with_control(
            &url, &target, DownloadControl::new(), |_, _| {},
        ).unwrap();
        server.join().unwrap();

        assert_eq!(received, body.len() as u64);
        assert_eq!(std::fs::read(&target).unwrap(), body);
        assert!(!partial.exists());
        assert!(!std::path::PathBuf::from(format!("{}.easy-download.meta.json", target.display())).exists());
        std::fs::remove_file(target).unwrap();
    }

}
