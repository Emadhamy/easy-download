//! Stable, UI-independent entry points intended for the Flutter–Rust bridge.
//!
//! Keep bridge-facing arguments and return values simple. The generated bridge
//! is not wired into Flutter yet; these functions are the first API boundary.

use crate::{download_to_path, download_to_path_with_control, download_to_path_with_progress, DownloadControl, DownloadUrl};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use flutter_rust_bridge::frb;
use crate::frb_generated::StreamSink;
use crate::managed_tools::{Manifest, Target, ToolKind};
use crate::yt_dlp_updates::fetch_latest_release;
use crate::yt_dlp_installer::install_release;
use serde_json;

// Controls are retained by task ID so Flutter can send pause/resume/cancel
// commands while a download runs on a worker thread.
static DOWNLOAD_CONTROLS: OnceLock<Mutex<HashMap<String, DownloadControl>>> = OnceLock::new();

fn controls() -> &'static Mutex<HashMap<String, DownloadControl>> {
    DOWNLOAD_CONTROLS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Starts a controlled download. Keep task_id and use the command functions below.
#[frb]
pub fn download_file_controlled(
    task_id: String,
    input_url: String,
    destination_path: String,
    sink: StreamSink<String>,
) -> Result<u64, String> {
    let control = DownloadControl::new();
    {
        let mut registry = controls().lock().map_err(|_| "Download control registry lock poisoned".to_string())?;
        if registry.contains_key(&task_id) {
            return Err("A download with this task ID is already registered".to_string());
        }
        registry.insert(task_id.clone(), control.clone());
    }

    let result = download_to_path_with_control(&input_url, destination_path, control, |received, total| {
        let event = serde_json::json!({
            "taskId": task_id,
            "received": received,
            "total": total,
        });
        let _ = sink.add(event.to_string());
    });

    if let Ok(mut registry) = controls().lock() {
        registry.remove(&task_id);
    }
    result
}

fn control_for(task_id: &str) -> Result<DownloadControl, String> {
    controls().lock()
        .map_err(|_| "Download control registry lock poisoned".to_string())?
        .get(task_id)
        .cloned()
        .ok_or_else(|| "No active download found for this task ID".to_string())
}

#[frb]
pub fn pause_download(task_id: String) -> Result<(), String> {
    control_for(&task_id)?.pause();
    Ok(())
}

#[frb]
pub fn resume_download(task_id: String) -> Result<(), String> {
    control_for(&task_id)?.resume();
    Ok(())
}

#[frb]
pub fn cancel_download(task_id: String) -> Result<(), String> {
    control_for(&task_id)?.cancel();
    Ok(())
}

/// Starts or continues a download using HTTP Range and a validated remote entity.
#[frb]
pub fn download_file_resumable_controlled(
    task_id: String,
    input_url: String,
    destination_path: String,
    sink: StreamSink<String>,
) -> Result<u64, String> {
    let control = DownloadControl::new();
    {
        let mut registry = controls().lock().map_err(|_| "Download control registry lock poisoned".to_string())?;
        if registry.contains_key(&task_id) {
            return Err("A download with this task ID is already registered".to_string());
        }
        registry.insert(task_id.clone(), control.clone());
    }

    let result = crate::resumable::download_to_path_resumable_with_control(
        &input_url, destination_path, control, |received, total| {
            let event = serde_json::json!({
                "taskId": task_id,
                "received": received,
                "total": total,
            });
            let _ = sink.add(event.to_string());
        },
    );

    if let Ok(mut registry) = controls().lock() {
        registry.remove(&task_id);
    }
    result
}

/// Starts a resumable download with validated per-request HTTP headers.
/// Headers are supplied as a JSON object and are not stored in resume metadata.
#[frb]
pub fn download_file_resumable_controlled_with_headers(
    task_id: String,
    input_url: String,
    headers_json: String,
    destination_path: String,
    sink: StreamSink<String>,
) -> Result<u64, String> {
    let control = DownloadControl::new();
    {
        let mut registry = controls().lock().map_err(|_| "Download control registry lock poisoned".to_string())?;
        if registry.contains_key(&task_id) {
            return Err("A download with this task ID is already registered".to_string());
        }
        registry.insert(task_id.clone(), control.clone());
    }

    let result = crate::resumable::download_to_path_resumable_with_control_and_headers(
        &input_url, &headers_json, destination_path, control, |received, total| {
            let event = serde_json::json!({
                "taskId": task_id,
                "received": received,
                "total": total,
            });
            let _ = sink.add(event.to_string());
        },
    );

    if let Ok(mut registry) = controls().lock() {
        registry.remove(&task_id);
    }
    result
}

/// Returns the canonical OS/CPU key for this application build.
#[frb]
pub fn get_current_platform_target() -> Result<String, String> {
    Target::current()
        .map(|target| target.key())
        .map_err(|error| format!("Unsupported application platform: {error:?}"))
}

/// Returns trusted release metadata as JSON for the current platform.
/// None means the tool is disabled or no release is configured for this target.
/// This function never downloads or executes a binary.
#[frb]
pub fn get_managed_tool_release(tool_name: String) -> Result<Option<String>, String> {
    let kind = match tool_name.as_str() {
        "yt-dlp" => ToolKind::YtDlp,
        "ffmpeg" => ToolKind::Ffmpeg,
        _ => return Err("Unknown managed tool. Use 'yt-dlp' or 'ffmpeg'.".to_string()),
    };
    let manifest = Manifest::parse_and_validate(include_str!("../resources/managed-tools/manifest.json"))
        .map_err(|error| format!("Managed tools manifest is invalid: {error:?}"))?;
    let release = manifest.release_for_current_platform(kind)
        .map_err(|error| format!("Could not resolve current platform: {error:?}"))?;
    release
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| format!("Could not encode release metadata: {error}"))
}


/// Checks the latest stable yt-dlp release through the official GitHub API.
/// The returned candidate includes the official asset URL and published SHA-256.
/// This operation never downloads or installs the executable.
#[frb]
pub fn yt_dlp_check_latest_release() -> Result<String, String> {
    let target = Target::current()
        .map_err(|error| format!("Unsupported application platform: {error:?}"))?;
    let release = fetch_latest_release(&target.key())?;
    serde_json::to_string(&release)
        .map_err(|error| format!("Could not encode latest yt-dlp release: {error}"))
}

/// Downloads and installs the latest official yt-dlp release for this platform.
/// The installer verifies the published SHA-256 and executable version before activation.
#[frb]
pub fn yt_dlp_install_latest_release(
    install_root: String,
    sink: StreamSink<String>,
) -> Result<String, String> {
    let target = Target::current()
        .map_err(|error| format!("Unsupported application platform: {error:?}"))?;
    let target_key = target.key();
    let candidate = fetch_latest_release(&target_key)?;
    let installed = install_release(
        &candidate,
        &target_key,
        install_root,
        |received, total| {
            let event = serde_json::json!({
                "phase": "downloading",
                "received": received,
                "total": total,
            });
            let _ = sink.add(event.to_string());
        },
    )?;
    let event = serde_json::json!({
        "phase": "installed",
        "version": installed.version,
        "executablePath": installed.executable_path,
    });
    let _ = sink.add(event.to_string());
    serde_json::to_string(&installed)
        .map_err(|error| format!("Could not encode installed yt-dlp metadata: {error}"))
}

/// Inspects a source and returns safe, normalized video/audio format choices.
/// Signed media URLs and DRM-only formats are not exposed to Flutter.
#[frb]
pub fn yt_dlp_inspect_formats(
    executable_path: String,
    source_url: String,
) -> Result<String, String> {
    crate::yt_dlp_formats::inspect_formats(executable_path, &source_url)
}

/// Resolves a selected yt-dlp format to JSON containing its short-lived URL
/// and the HTTP headers required by the media host.
#[frb]
pub fn yt_dlp_resolve_format_request(
    executable_path: String,
    source_url: String,
    format_id: String,
) -> Result<String, String> {
    crate::yt_dlp::resolve_format_request(executable_path, &source_url, &format_id)
}

/// Downloads one selected yt-dlp format and streams byte progress events.
#[frb]
pub fn yt_dlp_download_format(
    executable_path: String,
    source_url: String,
    format_id: String,
    output_template: String,
    sink: StreamSink<String>,
) -> Result<String, String> {
    let output_path = crate::yt_dlp::download_format(
        executable_path,
        &source_url,
        &format_id,
        &output_template,
        |received, total| {
            let event = serde_json::json!({
                "phase": "downloading",
                "received": received,
                "total": total,
            });
            let _ = sink.add(event.to_string());
        },
    )?;
    let event = serde_json::json!({ "phase": "completed", "outputPath": output_path });
    let _ = sink.add(event.to_string());
    Ok(output_path)
}

/// Returns the version reported by the verified yt-dlp executable path.
#[frb]
pub fn yt_dlp_get_version(executable_path: String) -> Result<String, String> {
    crate::yt_dlp::get_version(executable_path)
}

/// Inspects a single HTTP(S) video source without downloading media.
/// Returns yt-dlp JSON metadata for the later format-selection stage.
#[frb]
pub fn yt_dlp_inspect_source(
    executable_path: String,
    source_url: String,
) -> Result<String, String> {
    crate::yt_dlp::inspect_source(executable_path, &source_url)
}

/// Validates and normalizes a direct HTTP(S) download URL.
pub fn validate_download_url(input: String) -> Result<String, String> {
    DownloadUrl::parse(&input).map(|url| url.as_str().to_owned())
}

/// Streams download progress events to Dart while writing the file.
#[frb]
pub fn download_file_with_progress(
    input_url: String,
    destination_path: String,
    sink: StreamSink<String>,
) -> Result<u64, String> {
    download_to_path_with_progress(&input_url, destination_path, |received, total| {
        let event = serde_json::json!({
            "received": received,
            "total": total,
        });
        let _ = sink.add(event.to_string());
    })
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


/// Returns the version reported by a verified FFmpeg executable.
#[frb]
pub fn ffmpeg_get_version(executable_path: String) -> Result<String, String> {
    crate::ffmpeg::get_version(executable_path)
}

/// Converts a local video to MP3 and streams elapsed-media-time progress.
#[frb]
pub fn ffmpeg_convert_to_mp3(
    executable_path: String,
    input_path: String,
    output_path: String,
    bitrate: String,
    sink: StreamSink<String>,
) -> Result<String, String> {
    crate::ffmpeg::convert_to_mp3(
        executable_path,
        input_path,
        output_path,
        &bitrate,
        |elapsed_millis, total_millis| {
            let event = serde_json::json!({
                "phase": "converting",
                "elapsedMillis": elapsed_millis,
                "totalMillis": total_millis,
            });
            let _ = sink.add(event.to_string());
        },
    )
}
