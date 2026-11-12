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
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Could not access yt-dlp executable: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("yt-dlp executable must not be a symbolic link".to_string());
    }
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

fn validate_format_download(format_id: &str, output_template: &str) -> Result<(), String> {
    if format_id.is_empty()
        || !format_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err("Invalid yt-dlp format identifier".to_string());
    }
    let template = Path::new(output_template);
    if !template.is_absolute() {
        return Err("Video output template must be an absolute path".to_string());
    }
    let file_name = template.file_name().and_then(|name| name.to_str())
        .ok_or_else(|| "Video output template has an invalid file name".to_string())?;
    if !file_name.contains("%(ext)s") || file_name.matches("%(ext)s").count() != 1 {
        return Err("Video output template must contain exactly one %(ext)s placeholder".to_string());
    }
    let parent = template.parent().ok_or_else(|| "Video output directory is missing".to_string())?;
    if !parent.is_dir() {
        return Err("Video output directory does not exist".to_string());
    }
    Ok(())
}

/// Downloads one selected format with yt-dlp and reports structured progress.
/// The output template must be an absolute path ending in a %(ext)s placeholder.
pub fn download_format<F>(
    executable_path: impl AsRef<Path>,
    source_url: &str,
    format_id: &str,
    output_template: &str,
    mut on_progress: F,
) -> Result<String, String>
where
    F: FnMut(u64, Option<u64>),
{
    let source_url = validate_source_url(source_url)?;
    let executable_path = executable_path.as_ref();
    validate_executable(executable_path)?;
    validate_format_download(format_id, output_template)?;

    use std::io::{BufRead, BufReader, Read};
    use std::process::{Stdio, Command};
    let mut child = Command::new(executable_path)
        .args([
            "--ignore-config",
            "--no-warnings",
            "--no-playlist",
            "--newline",
            "--progress",
            "--progress-template",
            "download:ED_PROGRESS:%(progress.downloaded_bytes)s:%(progress.total_bytes_estimate)s",
            "--format",
            format_id,
            "--output",
            output_template,
            "--print",
            "after_move:ED_FILE:%(filepath)s",
            "--",
            source_url.as_str(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start yt-dlp download: {error}"))?;

    let stderr = child.stderr.take()
        .ok_or_else(|| "Could not capture yt-dlp error output".to_string())?;
    let stderr_reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut output = String::new();
        let _ = reader.read_to_string(&mut output);
        output
    });

    let stdout = child.stdout.take()
        .ok_or_else(|| "Could not capture yt-dlp progress output".to_string())?;
    let mut final_path: Option<String> = None;
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|error| format!("Could not read yt-dlp output: {error}"))?;
        if let Some(value) = line.strip_prefix("ED_FILE:") {
            if !value.trim().is_empty() {
                final_path = Some(value.trim().to_string());
            }
        } else if let Some(values) = line.strip_prefix("ED_PROGRESS:") {
            let mut parts = values.splitn(2, ':');
            let received = parts.next().and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
            let total = parts.next().and_then(|value| value.parse::<u64>().ok());
            on_progress(received, total);
        }
    }

    let status = child.wait()
        .map_err(|error| format!("Could not wait for yt-dlp: {error}"))?;
    let error_output = stderr_reader.join().unwrap_or_default();
    if !status.success() {
        let detail = error_output.trim();
        let detail = if detail.len() > 2048 { &detail[..2048] } else { detail };
        return Err(if detail.is_empty() {
            format!("yt-dlp download failed with status {status}")
        } else {
            format!("yt-dlp download failed: {detail}")
        });
    }

    let final_path = final_path.ok_or_else(|| "yt-dlp completed without reporting the output file".to_string())?;
    let output = Path::new(&final_path);
    let metadata = std::fs::symlink_metadata(output)
        .map_err(|error| format!("yt-dlp output file is missing: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("yt-dlp output is not a regular file".to_string());
    }
    Ok(final_path)
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
    use super::{get_version, inspect_source, validate_format_download};
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
    fn selected_format_requires_safe_id_and_absolute_output_template() {
        let output = std::env::temp_dir().join("easy-download-video-%(ext)s");
        assert!(validate_format_download("137", output.to_str().unwrap()).is_ok());
        assert!(validate_format_download("137+140", output.to_str().unwrap()).is_err());
        assert!(validate_format_download("137", "relative/video.%(ext)s").is_err());
        assert!(validate_format_download("137", "/tmp/video.mp4").is_err());
    }

    #[test]
    fn inspection_rejects_non_http_urls_before_launching_a_process() {
        let result = inspect_source("/missing/yt-dlp", "file:///tmp/video.mp4");
        assert!(result.unwrap_err().contains("Unsupported video URL scheme"));
    }
}
