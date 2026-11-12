//! Safe FFmpeg process adapter for audio extraction and MP3 conversion.
//!
//! The adapter never invokes a shell. It accepts only a verified executable,
//! a regular local input file, and a new .mp3 destination inside an existing
//! directory. Progress is reported from FFmpeg's machine-readable progress pipe.

use crate::DownloadControl;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const ALLOWED_BITRATES: &[&str] = &["128k", "160k", "192k", "256k", "320k"];

fn validate_executable(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("FFmpeg executable path must be absolute".to_string());
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Could not access FFmpeg executable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("FFmpeg executable must be a regular file, not a symbolic link".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("FFmpeg file is not executable".to_string());
        }
    }
    Ok(())
}

fn validate_input(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Input video path must be absolute".to_string());
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Could not access input video: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Input video must be a regular file, not a symbolic link".to_string());
    }
    std::fs::canonicalize(path)
        .map_err(|error| format!("Could not resolve input video path: {error}"))
}

fn validate_output(path: &Path, input: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("MP3 output path must be absolute".to_string());
    }
    if path.extension().and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("mp3")) != Some(true)
    {
        return Err("MP3 output file must use the .mp3 extension".to_string());
    }
    if path.exists() {
        return Err("MP3 output already exists; refusing to overwrite it".to_string());
    }
    let parent = path.parent()
        .ok_or_else(|| "MP3 output directory is missing".to_string())?;
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|error| format!("Could not resolve MP3 output directory: {error}"))?;
    if !canonical_parent.is_dir() {
        return Err("MP3 output parent is not a directory".to_string());
    }
    let file_name = path.file_name()
        .ok_or_else(|| "MP3 output file name is invalid".to_string())?;
    let output = canonical_parent.join(file_name);
    if output == input {
        return Err("Input and output paths must be different".to_string());
    }
    Ok(output)
}

fn parse_duration_millis(stderr: &str) -> Option<u64> {
    let marker = "Duration: ";
    let start = stderr.find(marker)? + marker.len();
    let value = stderr[start..].split(',').next()?.trim();
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<u64>().ok()?;
    let minutes = parts.next()?.parse::<u64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    if minutes >= 60 || !(0.0..60.0).contains(&seconds) {
        return None;
    }
    let total = (hours as f64 * 3600.0 + minutes as f64 * 60.0 + seconds) * 1000.0;
    if !total.is_finite() || total <= 0.0 || total > u64::MAX as f64 {
        return None;
    }
    Some(total.round() as u64)
}

fn validate_bitrate(bitrate: &str) -> Result<(), String> {
    if ALLOWED_BITRATES.contains(&bitrate) {
        Ok(())
    } else {
        Err("Unsupported MP3 bitrate. Choose 128k, 160k, 192k, 256k, or 320k.".to_string())
    }
}

/// Runs the verified FFmpeg executable and returns its first version line.
pub fn get_version(executable_path: impl AsRef<Path>) -> Result<String, String> {
    let executable = executable_path.as_ref();
    validate_executable(executable)?;
    let output = Command::new(executable)
        .args(["-hide_banner", "-version"])
        .output()
        .map_err(|error| format!("Could not start FFmpeg: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "FFmpeg version probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let first_line = String::from_utf8_lossy(&output.stdout)
        .lines().next().unwrap_or("").trim().to_string();
    if !first_line.starts_with("ffmpeg version ") {
        return Err("The executable did not identify itself as FFmpeg".to_string());
    }
    Ok(first_line)
}

/// Extracts audio to a staged MP3 file, supports pause/resume/cancel, and atomically finalizes it.
pub fn convert_to_mp3<F>(
    executable_path: impl AsRef<Path>,
    input_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    bitrate: &str,
    on_progress: F,
) -> Result<String, String>
where F: FnMut(u64, Option<u64>) {
    convert_to_mp3_controlled(executable_path, input_path, output_path, bitrate, DownloadControl::new(), on_progress)
}

/// Controlled conversion. Cancellation kills FFmpeg and removes the staged output.
pub fn convert_to_mp3_controlled<F>(
    executable_path: impl AsRef<Path>,
    input_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    bitrate: &str,
    control: DownloadControl,
    mut on_progress: F,
) -> Result<String, String>
where F: FnMut(u64, Option<u64>) {
    let executable = executable_path.as_ref();
    validate_executable(executable)?;
    validate_bitrate(bitrate)?;
    let input = validate_input(input_path.as_ref())?;
    let output = validate_output(output_path.as_ref(), &input)?;

    let probe = Command::new(executable).args(["-hide_banner", "-i"]).arg(&input).output()
        .map_err(|error| format!("Could not probe input video: {error}"))?;
    let probe_text = format!("{}\\n{}", String::from_utf8_lossy(&probe.stdout), String::from_utf8_lossy(&probe.stderr));
    let duration = parse_duration_millis(&probe_text);

    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "System clock is before Unix epoch".to_string())?.as_nanos();
    let stem = output.file_stem().and_then(|v| v.to_str()).ok_or_else(|| "MP3 output file name is invalid".to_string())?;
    let staged = output.with_file_name(format!("{stem}.part-{nonce}.mp3"));

    let mut child = Command::new(executable)
        .args(["-n", "-nostdin", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(&input)
        .args(["-vn", "-map", "0:a:0", "-codec:a", "libmp3lame", "-b:a", bitrate,
            "-id3v2_version", "3", "-progress", "pipe:1", "-nostats"])
        .arg(&staged).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .map_err(|error| format!("Could not start FFmpeg conversion: {error}"))?;

    let stderr = child.stderr.take().ok_or_else(|| "Could not capture FFmpeg diagnostics".to_string())?;
    let stderr_reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut text = String::new();
        let _ = std::io::BufReader::new(stderr).read_to_string(&mut text);
        text
    });
    let stdout = child.stdout.take().ok_or_else(|| "Could not capture FFmpeg progress".to_string())?;
    let mut task_error = None;
    {
        use std::io::{BufRead, BufReader};
        for line in BufReader::new(stdout).lines() {
            if let Err(error) = control.wait_until_running() {
                task_error = Some(error);
                let _ = child.kill();
                break;
            }
            let line = match line {
                Ok(line) => line,
                Err(error) => {
                    task_error = Some(format!("Could not read FFmpeg progress: {error}"));
                    let _ = child.kill();
                    break;
                }
            };
            if let Some(value) = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms=")) {
                if let Ok(micros) = value.parse::<u64>() {
                    on_progress(micros / 1000, duration);
                }
            }
        }
    }
    let status = child.wait().map_err(|error| format!("Could not wait for FFmpeg: {error}"))?;
    let diagnostics = stderr_reader.join().unwrap_or_default();
    if let Some(error) = task_error {
        let _ = std::fs::remove_file(&staged);
        return Err(error);
    }
    if !status.success() {
        let _ = std::fs::remove_file(&staged);
        let detail = diagnostics.trim();
        let detail = if detail.len() > 2048 { &detail[..2048] } else { detail };
        return Err(if detail.is_empty() { format!("FFmpeg conversion failed with status {status}") }
            else { format!("FFmpeg conversion failed: {detail}") });
    }
    let metadata = std::fs::symlink_metadata(&staged)
        .map_err(|error| format!("FFmpeg did not create the staged MP3 output: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
        let _ = std::fs::remove_file(&staged);
        return Err("FFmpeg output is not a valid non-empty regular file".to_string());
    }
    if output.exists() {
        let _ = std::fs::remove_file(&staged);
        return Err("MP3 output appeared during conversion; refusing to overwrite it".to_string());
    }
    std::fs::rename(&staged, &output).map_err(|error| {
        let _ = std::fs::remove_file(&staged);
        format!("Could not finalize MP3 output: {error}")
    })?;
    Ok(output.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::{parse_duration_millis, validate_bitrate, validate_output};
    use std::path::Path;

    #[test]
    fn parses_ffmpeg_duration() {
        assert_eq!(
            parse_duration_millis("Input #0, mp4, from 'x':\n  Duration: 00:01:02.500, start: 0"),
            Some(62_500)
        );
        assert_eq!(parse_duration_millis("Duration: N/A"), None);
    }

    #[test]
    fn accepts_only_supported_mp3_bitrates() {
        for bitrate in ["128k", "160k", "192k", "256k", "320k"] {
            assert!(validate_bitrate(bitrate).is_ok());
        }
        assert!(validate_bitrate("192k; touch bad").is_err());
        assert!(validate_bitrate("999k").is_err());
    }

    #[test]
    fn refuses_existing_output_and_non_mp3_extension() {
        let input = std::env::current_dir().unwrap().join("input.mp4");
        assert!(validate_output(Path::new("/tmp/not-audio.wav"), &input).is_err());
    }
}
