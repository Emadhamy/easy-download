//! Core utilities and direct HTTP(S) download support for Easy Download.
//!
//! The Rust download function is not connected to the Flutter interface yet.

use std::fs::File;
use std::io;
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
/// This first version streams the response into a file and returns the number
/// of bytes written. It does not yet support pause/resume, progress events,
/// retries, or integration with the Flutter UI.
pub fn download_to_path(
    input_url: &str,
    destination: impl AsRef<Path>,
) -> Result<u64, String> {
    let url = DownloadUrl::parse(input_url)?;
    let client = Client::builder()
        .build()
        .map_err(|error| format!("Could not create HTTP client: {error}"))?;

    let mut response = client
        .get(url.as_str())
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Download request failed: {error}"))?;

    let mut output = File::create(destination.as_ref())
        .map_err(|error| format!("Could not create destination file: {error}"))?;

    io::copy(&mut response, &mut output)
        .map_err(|error| format!("Could not write downloaded data: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{download_to_path, DownloadUrl};

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
}
