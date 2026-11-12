//! Discovers versioned FFmpeg builds from the BtbN/FFmpeg-Builds GitHub releases.
//!
//! BtbN is an independent binary maintainer, not the FFmpeg project itself.
//! We select only static GPL archives and require the maintainer's published
//! checksums.sha256 file before a candidate can be installed.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const RELEASE_API: &str = "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest";
const CHECKSUM_ASSET: &str = "checksums.sha256";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LatestFfmpegRelease {
    pub version: String,
    pub asset_name: String,
    pub download_url: String,
    pub sha256: String,
    pub release_page: String,
    pub published_at: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    published_at: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn expected_asset_name(target: &str) -> Result<&'static str, String> {
    match target {
        "windows-x86_64" => Ok("ffmpeg-master-latest-win64-gpl.zip"),
        "windows-aarch64" => Ok("ffmpeg-master-latest-winarm64-gpl.zip"),
        "linux-x86_64" => Ok("ffmpeg-master-latest-linux64-gpl.tar.xz"),
        "linux-aarch64" => Ok("ffmpeg-master-latest-linuxarm64-gpl.tar.xz"),
        _ => Err(format!("FFmpeg managed updates are not supported for target {target}")),
    }
}

fn parse_checksum_file(contents: &str, expected_name: &str) -> Result<String, String> {
    let mut matches = contents.lines().filter_map(|line| {
        let mut fields = line.split_whitespace();
        let digest = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        if name == expected_name { Some(digest) } else { None }
    });
    let digest = matches.next()
        .ok_or_else(|| format!("The publisher checksum file has no entry for {expected_name}"))?;
    if matches.next().is_some() {
        return Err("The publisher checksum file contains duplicate entries for the selected archive".to_string());
    }
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("The publisher checksum for the selected FFmpeg archive is malformed".to_string());
    }
    Ok(digest.to_ascii_lowercase())
}

fn validate_release_url(url: &str, tag: &str, asset: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| "The FFmpeg release asset URL is invalid".to_string())?;
    let expected_path = format!("/BtbN/FFmpeg-Builds/releases/download/{tag}/{asset}");
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("github.com")
        || parsed.path() != expected_path
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("The FFmpeg archive is not the exact selected BtbN GitHub release asset".to_string());
    }
    Ok(())
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(45))
        .user_agent("Easy-Download/ffmpeg-updater")
        .build()
        .map_err(|error| format!("Could not create FFmpeg release client: {error}"))
}

/// Fetches release metadata and the publisher's checksum list; it never downloads
/// or executes the large FFmpeg archive.
pub fn fetch_latest_release(target: &str) -> Result<LatestFfmpegRelease, String> {
    let expected_name = expected_asset_name(target)?;
    let client = http_client()?;
    let release: GithubRelease = client.get(RELEASE_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Could not query the BtbN FFmpeg release API: {error}"))?
        .json()
        .map_err(|error| format!("The BtbN FFmpeg release response is invalid: {error}"))?;

    if release.tag_name.trim().is_empty() || release.published_at.trim().is_empty() {
        return Err("The BtbN FFmpeg release is missing its version or publication date".to_string());
    }

    let archive = release.assets.iter().find(|asset| asset.name == expected_name)
        .ok_or_else(|| format!("The latest BtbN release has no archive for {target}"))?;
    let checksum_asset = release.assets.iter().find(|asset| asset.name == CHECKSUM_ASSET)
        .ok_or_else(|| "The latest BtbN release does not publish checksums.sha256".to_string())?;

    validate_release_url(&archive.browser_download_url, &release.tag_name, expected_name)?;
    validate_release_url(&checksum_asset.browser_download_url, &release.tag_name, CHECKSUM_ASSET)?;

    let checksum_text = client.get(&checksum_asset.browser_download_url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Could not download the FFmpeg publisher checksum list: {error}"))?
        .text()
        .map_err(|error| format!("Could not read the FFmpeg publisher checksum list: {error}"))?;
    if checksum_text.len() > 128 * 1024 {
        return Err("The FFmpeg publisher checksum list is unexpectedly large".to_string());
    }
    let sha256 = parse_checksum_file(&checksum_text, expected_name)?;

    Ok(LatestFfmpegRelease {
        version: release.name.filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| release.tag_name.clone()),
        asset_name: expected_name.to_string(),
        download_url: archive.browser_download_url.clone(),
        sha256,
        release_page: release.html_url,
        published_at: release.published_at,
    })
}

#[cfg(test)]
mod tests {
    use super::{expected_asset_name, parse_checksum_file, validate_release_url};

    #[test]
    fn selects_exact_static_gpl_archive_for_supported_desktop_targets() {
        assert_eq!(expected_asset_name("windows-x86_64").unwrap(), "ffmpeg-master-latest-win64-gpl.zip");
        assert_eq!(expected_asset_name("windows-aarch64").unwrap(), "ffmpeg-master-latest-winarm64-gpl.zip");
        assert_eq!(expected_asset_name("linux-x86_64").unwrap(), "ffmpeg-master-latest-linux64-gpl.tar.xz");
        assert_eq!(expected_asset_name("linux-aarch64").unwrap(), "ffmpeg-master-latest-linuxarm64-gpl.tar.xz");
    }

    #[test]
    fn rejects_android_and_unknown_targets_instead_of_guessing() {
        assert!(expected_asset_name("android-aarch64").is_err());
        assert!(expected_asset_name("linux-armv7").is_err());
    }

    #[test]
    fn parses_only_the_exact_publisher_checksum_entry() {
        let digest = "a".repeat(64);
        let contents = format!("{digest}  ffmpeg-one.zip\n{}  ffmpeg-two.zip\n", "b".repeat(64));
        assert_eq!(parse_checksum_file(&contents, "ffmpeg-one.zip").unwrap(), digest);
        assert!(parse_checksum_file(&contents, "missing.zip").is_err());
    }

    #[test]
    fn rejects_duplicate_or_malformed_checksum_entries() {
        let digest = "a".repeat(64);
        let duplicate = format!("{digest}  archive.zip\n{digest} *archive.zip\n");
        assert!(parse_checksum_file(&duplicate, "archive.zip").is_err());
        assert!(parse_checksum_file("not-a-digest  archive.zip", "archive.zip").is_err());
    }

    #[test]
    fn requires_exact_https_release_asset_url() {
        let valid = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/archive.zip";
        assert!(validate_release_url(valid, "latest", "archive.zip").is_ok());
        assert!(validate_release_url("http://github.com/BtbN/FFmpeg-Builds/releases/download/latest/archive.zip", "latest", "archive.zip").is_err());
        assert!(validate_release_url("https://example.com/archive.zip", "latest", "archive.zip").is_err());
    }
}
