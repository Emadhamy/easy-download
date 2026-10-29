//! Initial core utilities for Easy Download.
//! This crate does not perform downloads yet.

use url::Url;

/// A normalized HTTP(S) download address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadUrl(String);

impl DownloadUrl {
    /// Parse and validate a URL that can be used for a direct HTTP(S) download.
    pub fn parse(input: &str) -> Result<Self, String> {
        let parsed = Url::parse(input.trim()).map_err(|error| format!("Invalid URL: {error}"))?;
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

#[cfg(test)]
mod tests {
    use super::DownloadUrl;

    #[test]
    fn accepts_https_url() {
        let url = DownloadUrl::parse("https://example.com/file.zip").unwrap();
        assert_eq!(url.as_str(), "https://example.com/file.zip");
    }

    #[test]
    fn rejects_non_http_scheme() {
        assert!(DownloadUrl::parse("file:///tmp/example.zip").is_err());
    }
}
