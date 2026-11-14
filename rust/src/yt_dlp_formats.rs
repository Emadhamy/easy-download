//! Safe, normalized format options returned by yt-dlp source inspection.
//!
//! Only presentation metadata is exposed to Flutter. Signed media URLs and other
//! downloader-internal fields are deliberately excluded from this API.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoFormatOption {
    pub format_id: String,
    pub ext: String,
    pub resolution: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub format_note: Option<String>,
    pub audio_bitrate: Option<f64>,
    pub has_video: bool,
    pub has_audio: bool,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSourceFormats {
    pub id: Option<String>,
    pub title: Option<String>,
    pub duration: Option<f64>,
    pub formats: Vec<VideoFormatOption>,
}

fn non_empty_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn number_u32(value: Option<&Value>) -> Option<u32> {
    value.and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok())
}

fn number_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

/// Converts yt-dlp's source-inspection JSON into a small, safe format catalogue.
pub fn formats_from_metadata(metadata_json: &str) -> Result<String, String> {
    let metadata: Value = serde_json::from_str(metadata_json)
        .map_err(|error| format!("Invalid yt-dlp metadata JSON: {error}"))?;
    let source = metadata.as_object()
        .ok_or_else(|| "yt-dlp metadata must be a JSON object".to_string())?;
    let raw_formats = source.get("formats").and_then(Value::as_array)
        .ok_or_else(|| "yt-dlp metadata does not contain a formats list".to_string())?;

    let mut formats = Vec::new();
    for raw in raw_formats {
        if raw.get("has_drm").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        let Some(format_id) = non_empty_string(raw.get("format_id")) else { continue; };
        let Some(ext) = non_empty_string(raw.get("ext")) else { continue; };
        let vcodec = non_empty_string(raw.get("vcodec"));
        let acodec = non_empty_string(raw.get("acodec"));
        let has_video = vcodec.as_deref().is_some_and(|codec| codec != "none");
        let has_audio = acodec.as_deref().is_some_and(|codec| codec != "none");
        if !has_video && !has_audio {
            continue;
        }
        let width = number_u32(raw.get("width"));
        let height = number_u32(raw.get("height"));
        let resolution = non_empty_string(raw.get("resolution")).or_else(|| {
            match (width, height) {
                (Some(width), Some(height)) => Some(format!("{width}x{height}")),
                _ => None,
            }
        });
        formats.push(VideoFormatOption {
            format_id,
            ext,
            resolution,
            width,
            height,
            fps: raw.get("fps").and_then(Value::as_f64),
            filesize: number_u64(raw.get("filesize")),
            filesize_approx: number_u64(raw.get("filesize_approx")),
            format_note: non_empty_string(raw.get("format_note")),
            audio_bitrate: raw.get("abr").and_then(Value::as_f64),
            has_video,
            has_audio,
            vcodec,
            acodec,
        });
    }

    // Put complete video formats first, then higher resolutions and frame rates.
    formats.sort_by(|left, right| {
        right.has_video.cmp(&left.has_video)
            .then_with(|| right.height.unwrap_or(0).cmp(&left.height.unwrap_or(0)))
            .then_with(|| right.fps.unwrap_or(0.0).total_cmp(&left.fps.unwrap_or(0.0)))
            .then_with(|| left.format_id.cmp(&right.format_id))
    });

    let response = VideoSourceFormats {
        id: non_empty_string(source.get("id")),
        title: non_empty_string(source.get("title")),
        duration: source.get("duration").and_then(Value::as_f64),
        formats,
    };
    serde_json::to_string(&response)
        .map_err(|error| format!("Could not encode video formats: {error}"))
}

/// Inspects a URL with the verified yt-dlp executable and returns format choices.
pub fn inspect_formats(
    executable_path: impl AsRef<std::path::Path>,
    source_url: &str,
) -> Result<String, String> {
    let metadata = crate::yt_dlp::inspect_source(executable_path, source_url)?;
    formats_from_metadata(&metadata)
}

#[cfg(test)]
mod tests {
    use super::formats_from_metadata;
    use serde_json::Value;

    #[test]
    fn exposes_video_audio_and_combined_formats_without_signed_urls() {
        let input = r#"{
            "id":"abc","title":"Sample","duration":42.5,
            "formats":[
                {"format_id":"18","ext":"mp4","height":360,"vcodec":"avc1","acodec":"mp4a","url":"https://secret.invalid/media"},
                {"format_id":"137","ext":"mp4","height":1080,"vcodec":"avc1","acodec":"none","filesize":1200},
                {"format_id":"140","ext":"m4a","vcodec":"none","acodec":"mp4a","abr":128.0},
                {"format_id":"drm","ext":"mp4","vcodec":"avc1","acodec":"none","has_drm":true},
                {"format_id":"storyboard","ext":"mhtml","vcodec":"none","acodec":"none"}
            ]
        }"#;
        let output = formats_from_metadata(input).unwrap();
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["title"], "Sample");
        assert_eq!(parsed["formats"].as_array().unwrap().len(), 3);
        assert_eq!(parsed["formats"][0]["formatId"], "137");
        assert_eq!(parsed["formats"][1]["formatId"], "18");
        assert_eq!(parsed["formats"][2]["formatId"], "140");
        assert!(parsed["formats"][0]["hasVideo"].as_bool().unwrap());
        assert!(!parsed["formats"][0]["hasAudio"].as_bool().unwrap());
        assert!(parsed["formats"][2]["hasAudio"].as_bool().unwrap());
        assert_eq!(parsed["formats"][2]["audioBitrate"], 128.0);
        assert!(!output.contains("secret.invalid"));
        assert!(!output.contains("storyboard"));
        assert!(!output.contains("drm"));
    }

    #[test]
    fn rejects_missing_formats_array() {
        assert!(formats_from_metadata(r#"{"id":"abc"}"#).unwrap_err().contains("formats list"));
    }

    #[test]
    fn rejects_non_object_metadata() {
        assert!(formats_from_metadata("[]").unwrap_err().contains("JSON object"));
    }
}
