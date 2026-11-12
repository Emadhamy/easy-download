//! Safe process adapter for yt-dlp source inspection.
//! This step inspects a source only; it does not download media or choose formats.

use std::path::Path;
use std::process::Command;
use url::Url;

fn validate_source_url(input: &str) -> Result<String, String> {
    let parsed = Url::parse(input.trim())
        .map_err(|error| format!("Invalid video URL: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Unsupported video URL scheme: {scheme}")),
    }
    if parsed.host_str().is_none() {
        return Err("Video URL must include a host".to_string());
    }
    Ok(parsed.to_string())
}

fn validate_executable(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("yt-dlp executable path must be absolute".to_string());
    }
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Could not access yt-dlp executable: {error}"))?;
    if !metadata.is_file() {
        return Err("yt-dlp executable path is not a file".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("yt-dlp file is not executable".to_string());
        }
    }
    Ok(())
}

/// Probe the version of the verified executable supplied by the managed-tools layer.
pub fn get_version(executable_path: impl AsRef<Path>) -> Result<String, String> {
    let executable_path = executable_path.as_ref();
    validate_executable(executable_path)?;
    let output = Command::new(executable_path)
        .arg("--version")
        .output()
        .map_err(|error| format!("Could not start yt-dlp: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "yt-dlp version probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        return Err("yt-dlp returned an empty version".to_string());
    }
    Ok(version)
}

/// Inspect one HTTP(S) source and return yt-dlp JSON metadata. No media is downloaded.
pub fn inspect_source(
    executable_path: impl AsRef<Path>,
    source_url: &str,
) -> Result<String, String> {
    let source_url = validate_source_url(source_url)?;
    let executable_path = executable_path.as_ref();
    validate_executable(executable_path)?;
    let output = Command::new(executable_path)
        .args([
            "--ignore-config",
            "--no-warnings",
            "--no-playlist",
            "--skip-download",
            "--dump-single-json",
            "--",
            source_url.as_str(),
        ])
        .output()
        .map_err(|error| format!("Could not start yt-dlp: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        let detail = if detail.len() > 1024 { &detail[..1024] } else { detail };
        return Err(format!("yt-dlp source inspection failed: {detail}"));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("yt-dlp returned invalid JSON metadata: {error}"))?;
    if !metadata.is_object() {
        return Err("yt-dlp returned unexpected metadata (expected a JSON object)".to_string());
    }
    serde_json::to_string(&metadata)
        .map_err(|error| format!("Could not encode yt-dlp metadata: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{get_version, inspect_source};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    fn fake_executable(script: &str, label: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("easy-download-yt-dlp-{label}-{unique}"));
        fs::write(&path, script).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn version_probe_reads_output_from_the_executable() {
        let path = fake_executable("#!/bin/sh\nprintf '2026.10.01\\n'\n", "version");
        assert_eq!(get_version(&path).unwrap(), "2026.10.01");
        fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn inspection_returns_json_metadata_without_downloading() {
        let path = fake_executable(
            "#!/bin/sh\n[ \"$1\" = \"--ignore-config\" ] || exit 8\n[ \"$6\" = \"--\" ] || exit 9\nprintf '{\"id\":\"fixture\",\"title\":\"Test video\",\"formats\":[]}'\n",
            "inspect",
        );
        let metadata = inspect_source(&path, "https://example.com/watch?v=fixture").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&metadata).unwrap();
        assert_eq!(parsed["id"], "fixture");
        assert_eq!(parsed["title"], "Test video");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn inspection_rejects_non_http_urls_before_launching_a_process() {
        let result = inspect_source("/missing/yt-dlp", "file:///tmp/video.mp4");
        assert!(result.unwrap_err().contains("Unsupported video URL scheme"));
    }
}
