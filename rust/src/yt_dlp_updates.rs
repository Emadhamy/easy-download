//! Official yt-dlp release discovery.
//!
//! This module only reads GitHub's official latest-release API. It does not
//! download, install, or execute release assets.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const RELEASE_API: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
const RELEASE_PAGE: &str = "https://github.com/yt-dlp/yt-dlp/releases";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LatestYtDlpRelease {
    pub version: String,
    pub asset_name: String,
    pub download_url: String,
    pub sha256: String,
    pub release_page: String,
}

/// Return the exact official asset name for a supported desktop target.
/// Android is intentionally unsupported until an Android-compatible official
/// distribution and runtime strategy are verified.
pub fn asset_name_for_target(target: &str) -> Result<&'static str, String> {
    match target {
        "windows-x86_64" => Ok("yt-dlp.exe"),
        "windows-aarch64" => Ok("yt-dlp_arm64.exe"),
        "linux-x86_64" => Ok("yt-dlp_linux"),
        "linux-aarch64" => Ok("yt-dlp_linux_aarch64"),
        _ => Err(format!("yt-dlp does not provide a configured official asset for {target}")),
    }
}

/// Query GitHub's official latest stable release and require its published
/// SHA-256 digest before returning an update candidate.
pub fn fetch_latest_release(target: &str) -> Result<LatestYtDlpRelease, String> {
    let expected_asset = asset_name_for_target(target)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Easy-Download/yt-dlp-updater")
        .build()
        .map_err(|error| format!("Could not create GitHub release client: {error}"))?;

    let response = client
        .get(RELEASE_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .map_err(|error| format!("Could not reach the official yt-dlp release service: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Official yt-dlp release service returned an error: {error}"))?;

    let release: GithubRelease = response
        .json()
        .map_err(|error| format!("Official yt-dlp release response was invalid: {error}"))?;

    if release.prerelease {
        return Err("The official latest-release endpoint returned a prerelease; refusing it".to_string());
    }

    let asset = release.assets.iter()
        .find(|asset| asset.name == expected_asset)
        .ok_or_else(|| format!("The official release has no asset named {expected_asset}"))?;

    let digest = asset.digest.as_deref()
        .and_then(|value| value.strip_prefix("sha256:"))
        .ok_or_else(|| format!("The official release did not publish a SHA-256 digest for {expected_asset}"))?;

    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("The official release returned an invalid SHA-256 digest".to_string());
    }

    let download_url = reqwest::Url::parse(&asset.browser_download_url)
        .map_err(|_| "The official release asset URL is invalid".to_string())?;
    if download_url.scheme() != "https"
        || download_url.host_str() != Some("github.com")
        || !download_url.path().starts_with("/yt-dlp/yt-dlp/releases/download/")
    {
        return Err("The release asset URL is not an official yt-dlp GitHub release URL".to_string());
    }

    let version = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);
    if version.trim().is_empty() {
        return Err("The official release has an empty version tag".to_string());
    }

    let release_page = reqwest::Url::parse(&release.html_url)
        .map_err(|_| "The official release page URL is invalid".to_string())?;
    if release_page.scheme() != "https"
        || release_page.host_str() != Some("github.com")
        || !release_page.path().starts_with("/yt-dlp/yt-dlp/releases/")
    {
        return Err("The release page is not an official yt-dlp GitHub page".to_string());
    }

    Ok(LatestYtDlpRelease {
        version: version.to_string(),
        asset_name: asset.name.clone(),
        download_url: download_url.to_string(),
        sha256: digest.to_ascii_lowercase(),
        release_page: release_page.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::asset_name_for_target;

    #[test]
    fn selects_official_asset_for_supported_desktop_targets() {
        assert_eq!(asset_name_for_target("windows-x86_64").unwrap(), "yt-dlp.exe");
        assert_eq!(asset_name_for_target("windows-aarch64").unwrap(), "yt-dlp_arm64.exe");
        assert_eq!(asset_name_for_target("linux-x86_64").unwrap(), "yt-dlp_linux");
        assert_eq!(asset_name_for_target("linux-aarch64").unwrap(), "yt-dlp_linux_aarch64");
    }

    #[test]
    fn refuses_android_and_unknown_targets() {
        assert!(asset_name_for_target("android-aarch64").is_err());
        assert!(asset_name_for_target("macos-aarch64").is_err());
        assert!(asset_name_for_target("linux-mips").is_err());
    }
}
