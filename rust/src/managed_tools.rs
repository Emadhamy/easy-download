//! Read and validate trusted metadata for managed external tools.
//!
//! This module intentionally does not download or execute binaries. It provides
//! a fail-closed foundation for a later updater implementation.

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub archive_format: ArchiveFormat,
    pub sha256: String,
    pub executable_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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

/// A canonical platform identifier used as a manifest release key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    os: String,
    arch: String,
}

impl Target {
    /// Build a target from explicit, canonical OS and architecture names.
    /// Unsupported combinations are rejected instead of guessed.
    pub fn from_parts(os: &str, arch: &str) -> Result<Self, ManifestError> {
        let key = format!("{os}-{arch}");
        if !valid_target(&key) {
            return Err(ManifestError::InvalidReleaseTarget(key));
        }
        Ok(Self { os: os.to_owned(), arch: arch.to_owned() })
    }

    /// Detect the current Rust compilation target.
    pub fn current() -> Result<Self, ManifestError> {
        let arch = match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            // Do not assume every 32-bit ARM target is ARMv7.
            other => return Err(ManifestError::InvalidReleaseTarget(
                format!("{}-{other}", std::env::consts::OS)
            )),
        };
        Self::from_parts(std::env::consts::OS, arch)
    }

    /// Return the exact key used by the managed-tools manifest.
    pub fn key(&self) -> String {
        format!("{}-{}", self.os, self.arch)
    }
}

impl Manifest {
    /// Find an enabled tool release for the current operating system and CPU.
    pub fn release_for_current_platform(
        &self,
        kind: ToolKind,
    ) -> Result<Option<&Release>, ManifestError> {
        let target = Target::current()?;
        Ok(self.release(kind, &target.key()))
    }
}

fn valid_target(target: &str) -> bool {
    let mut parts = target.split('-');
    let os = parts.next();
    let arch = parts.next();

    if parts.next().is_some() {
        return false;
    }

    match (os, arch) {
        // Windows ARMv7 is not a supported release target.
        (Some("windows"), Some("x86_64" | "aarch64")) => true,
        (Some("linux"), Some("x86_64" | "aarch64" | "armv7")) => true,
        (Some("android"), Some("aarch64" | "armv7")) => true,
        _ => false,
    }
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
    fn builds_supported_platform_targets() {
        assert_eq!(Target::from_parts("windows", "x86_64").unwrap().key(), "windows-x86_64");
        assert_eq!(Target::from_parts("linux", "aarch64").unwrap().key(), "linux-aarch64");
        assert_eq!(Target::from_parts("android", "aarch64").unwrap().key(), "android-aarch64");
    }

    #[test]
    fn accepts_supported_os_architecture_combinations() {
        for (os, arch) in [
            ("windows", "x86_64"),
            ("windows", "aarch64"),
            ("linux", "x86_64"),
            ("linux", "aarch64"),
            ("linux", "armv7"),
            ("android", "aarch64"),
            ("android", "armv7"),
        ] {
            assert!(
                Target::from_parts(os, arch).is_ok(),
                "expected {os}-{arch} to be supported"
            );
        }
    }

    #[test]
    fn rejects_unsupported_platform_or_architecture() {
        assert!(Target::from_parts("macos", "aarch64").is_err());
        assert!(Target::from_parts("linux", "mips").is_err());
        assert!(Target::from_parts("windows", "armv7").is_err());
    }

    #[test]
    fn disabled_or_missing_tool_release_is_never_selected() {
        let manifest = Manifest::parse_and_validate(EMPTY_MANIFEST).unwrap();
        assert!(manifest.release(ToolKind::YtDlp, "linux-x86_64").is_none());
        assert!(manifest.release(ToolKind::Ffmpeg, "android-aarch64").is_none());
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
