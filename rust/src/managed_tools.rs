//! Read and validate trusted metadata for managed external tools.
//!
//! This module intentionally does not download or execute binaries. It provides
//! a fail-closed foundation for a later updater implementation.

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub manifest_version: u32,
    pub status: ManifestStatus,
    pub tools: Tools,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManifestStatus {
    Unconfigured,
    Ready,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tools {
    pub yt_dlp: ManagedTool,
    pub ffmpeg: ManagedTool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedTool {
    pub enabled: bool,
    pub releases: BTreeMap<String, Release>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub archive_format: ArchiveFormat,
    pub sha256: String,
    pub executable_path: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ArchiveFormat {
    Zip,
    TarGz,
    TarXz,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    YtDlp,
    Ffmpeg,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    UnsupportedVersion(u32),
    EnabledToolWithoutRelease(&'static str),
    InvalidReleaseTarget(String),
    InvalidReleaseMetadata(String),
}

impl Manifest {
    /// Parse JSON and enforce the runtime safety checks.
    pub fn parse_and_validate(json: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_str(json)
            .map_err(|e| ManifestError::InvalidReleaseMetadata(e.to_string()))?;

        if manifest.manifest_version != 1 {
            return Err(ManifestError::UnsupportedVersion(manifest.manifest_version));
        }

        for (name, tool) in [
            ("ytDlp", &manifest.tools.yt_dlp),
            ("ffmpeg", &manifest.tools.ffmpeg),
        ] {
            if tool.enabled && tool.releases.is_empty() {
                return Err(ManifestError::EnabledToolWithoutRelease(name));
            }

            for (target, release) in &tool.releases {
                if !valid_target(target) {
                    return Err(ManifestError::InvalidReleaseTarget(target.clone()));
                }
                if release.version.trim().is_empty()
                    || !release.url.starts_with("https://")
                    || release.sha256.len() != 64
                    || !release.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                    || release.executable_path.trim().is_empty()
                {
                    return Err(ManifestError::InvalidReleaseMetadata(target.clone()));
                }
            }
        }

        Ok(manifest)
    }

    pub fn release(&self, kind: ToolKind, target: &str) -> Option<&Release> {
        let tool = match kind {
            ToolKind::YtDlp => &self.tools.yt_dlp,
            ToolKind::Ffmpeg => &self.tools.ffmpeg,
        };
        if !tool.enabled {
            return None;
        }
        tool.releases.get(target)
    }
}

fn valid_target(target: &str) -> bool {
    let mut parts = target.split('-');
    let os = parts.next();
    let arch = parts.next();
    parts.next().is_none()
        && matches!(os, Some("windows" | "linux" | "android"))
        && matches!(arch, Some("x86_64" | "aarch64" | "armv7"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY_MANIFEST: &str = r#"{
      "manifestVersion": 1,
      "status": "unconfigured",
      "tools": {
        "ytDlp": { "enabled": false, "releases": {} },
        "ffmpeg": { "enabled": false, "releases": {} }
      }
    }"#;

    #[test]
    fn parses_repository_manifest_file() {
        let json = include_str!("../resources/managed-tools/manifest.json");
        let manifest = Manifest::parse_and_validate(json).expect("repository manifest must be valid");
        assert_eq!(manifest.manifest_version, 1);
        assert_eq!(manifest.status, ManifestStatus::Unconfigured);
        assert!(!manifest.tools.yt_dlp.enabled);
        assert!(!manifest.tools.ffmpeg.enabled);
    }

    #[test]
    fn accepts_unconfigured_manifest() {
        assert!(Manifest::parse_and_validate(EMPTY_MANIFEST).is_ok());
    }

    #[test]
    fn rejects_enabled_tool_without_release() {
        let json = EMPTY_MANIFEST.replace(
            r#""enabled": false, "releases": {}"#,
            r#""enabled": true, "releases": {}"#,
        );
        assert!(matches!(
            Manifest::parse_and_validate(&json),
            Err(ManifestError::EnabledToolWithoutRelease(_))
        ));
    }

    #[test]
    fn rejects_unsupported_manifest_version() {
        let json = EMPTY_MANIFEST.replace(r#""manifestVersion": 1"#, r#""manifestVersion": 9"#);
        assert_eq!(
            Manifest::parse_and_validate(&json).unwrap_err(),
            ManifestError::UnsupportedVersion(9)
        );
    }

    #[test]
    fn rejects_unknown_target_architecture() {
        let json = EMPTY_MANIFEST.replace(
            r#""releases": {}"#,
            r#""releases": {"linux-mips": {
              "version":"1", "url":"https://example.invalid/tool",
              "archiveFormat":"binary", "sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              "executablePath":"tool"
            }}"#,
        );
        assert!(matches!(
            Manifest::parse_and_validate(&json),
            Err(ManifestError::InvalidReleaseTarget(_))
        ));
    }
}
