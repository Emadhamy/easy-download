//! Stable, UI-independent entry points intended for the Flutter–Rust bridge.
//!
//! Keep bridge-facing arguments and return values simple. The generated bridge
//! is not wired into Flutter yet; these functions are the first API boundary.

use crate::{download_to_path, DownloadUrl};

/// Validates and normalizes a direct HTTP(S) download URL.
pub fn validate_download_url(input: String) -> Result<String, String> {
    DownloadUrl::parse(&input).map(|url| url.as_str().to_owned())
}

/// Downloads a URL to the exact destination path supplied by the caller.
///
/// This is a blocking Rust operation. The bridge integration must expose it
/// asynchronously to Dart so it does not block Flutter's UI isolate.
pub fn download_file(input_url: String, destination_path: String) -> Result<u64, String> {
    download_to_path(&input_url, destination_path)
}

#[cfg(test)]
mod tests {
    use super::validate_download_url;

    #[test]
    fn bridge_api_normalizes_valid_http_url() {
        assert_eq!(
            validate_download_url("https://example.com/file.zip".to_string()).unwrap(),
            "https://example.com/file.zip"
        );
    }

    #[test]
    fn bridge_api_rejects_unsupported_scheme() {
        assert!(validate_download_url("file:///tmp/file.zip".to_string()).is_err());
    }
}
