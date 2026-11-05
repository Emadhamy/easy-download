//! Stable, UI-independent entry points intended for the Flutter–Rust bridge.
//!
//! Keep bridge-facing arguments and return values simple. The generated bridge
//! is not wired into Flutter yet; these functions are the first API boundary.

use crate::{download_to_path, download_to_path_with_control, download_to_path_with_progress, DownloadControl, DownloadUrl};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use flutter_rust_bridge::frb;
use crate::frb_generated::StreamSink;
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
