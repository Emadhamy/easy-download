//! Core utilities and direct HTTP(S) download support for Easy Download.
//!
//! The Rust download functions are not connected to the Flutter interface yet.

pub mod api;

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use reqwest::blocking::Client;
use url::Url;

/// A normalized HTTP(S) download address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadUrl(String);

impl DownloadUrl {
    /// Parse and validate a URL that can be used for a direct HTTP(S) download.
    pub fn parse(input: &str) -> Result<Self, String> {
        let parsed = Url::parse(input.trim())
            .map_err(|error| format!("Invalid URL: {error}"))?;
        match parsed.scheme() {
            "http" | "https" => {}
            scheme => return Err(format!("Unsupported URL scheme: {scheme}")),
        }

        if parsed.host_str().is_none() {
            return Err("URL must include a host".to_string());
        }

        Ok(Self(parsed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Downloads a direct HTTP(S) URL to the destination file.
///
/// The callback receives (bytes_received, total_bytes). The total is None if
/// the server does not provide a Content-Length header. This function streams
/// data in chunks rather than loading the entire response into memory.
pub fn download_to_path_with_progress<F>(
    input_url: &str,
    destination: impl AsRef<Path>,
    mut on_progress: F,
) -> Result<u64, String>
where
    F: FnMut(u64, Option<u64>),
{
    let url = DownloadUrl::parse(input_url)?;
    let client = Client::builder()
        .build()
        .map_err(|error| format!("Could not create HTTP client: {error}"))?;

    let mut response = client
        .get(url.as_str())
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Download request failed: {error}"))?;

    let total_bytes = response.content_length();
    let mut output = File::create(destination.as_ref())
        .map_err(|error| format!("Could not create destination file: {error}"))?;

    let mut received_bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let bytes_read = response
            .read(&mut buffer)
            .map_err(|error| format!("Could not read download response: {error}"))?;
        if bytes_read == 0 {
            break;
        }

        output
            .write_all(&buffer[..bytes_read])
            .map_err(|error| format!("Could not write downloaded data: {error}"))?;

        received_bytes += bytes_read as u64;
        on_progress(received_bytes, total_bytes);
    }

    output
        .flush()
        .map_err(|error| format!("Could not flush destination file: {error}"))?;

    Ok(received_bytes)
}

/// Downloads a direct HTTP(S) URL to a file and returns the number of bytes
/// written. Use `download_to_path_with_progress` to observe progress.
pub fn download_to_path(
    input_url: &str,
    destination: impl AsRef<Path>,
) -> Result<u64, String> {
    download_to_path_with_progress(input_url, destination, |_, _| {})
}

#[cfg(test)]
mod tests {
    use super::{download_to_path, download_to_path_with_progress, DownloadUrl};

    #[test]
    fn accepts_https_url() {
        let url = DownloadUrl::parse("https://example.com/file.zip").unwrap();
        assert_eq!(url.as_str(), "https://example.com/file.zip");
    }

    #[test]
    fn rejects_non_http_scheme() {
        assert!(DownloadUrl::parse("file:///tmp/example.zip").is_err());
    }

    #[test]
    fn downloader_rejects_non_http_scheme_before_requesting() {
        let result = download_to_path(
            "file:///tmp/example.zip",
            "/tmp/easy-download-invalid-scheme-test.zip",
        );
        assert!(result.is_err());
    }

    #[test]
    fn progress_downloader_rejects_invalid_scheme_before_callback() {
        let mut callback_called = false;
        let result = download_to_path_with_progress(
            "file:///tmp/example.zip",
            "/tmp/easy-download-invalid-scheme-progress-test.zip",
            |_, _| callback_called = true,
        );
        assert!(result.is_err());
        assert!(!callback_called);
    }
}
