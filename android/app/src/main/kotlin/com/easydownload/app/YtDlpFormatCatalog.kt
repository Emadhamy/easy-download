package com.easydownload.app

import org.json.JSONArray
import org.json.JSONObject

/**
 * Converts yt-dlp's raw metadata into the same safe, ordered catalogue used by
 * the desktop Rust inspector. Media URLs and downloader-only fields are never
 * copied into the Flutter response.
 */
internal object YtDlpFormatCatalog {
    private data class RankedFormat(
        val json: JSONObject,
        val hasVideo: Boolean,
        val height: Int,
        val fps: Double,
        val formatId: String,
    )

    fun fromMetadata(metadataJson: String): String {
        val metadata = JSONObject(metadataJson)
        val sourceFormats = metadata.optJSONArray("formats")
            ?: throw IllegalArgumentException("yt-dlp metadata does not contain a formats list.")
        val formats = mutableListOf<RankedFormat>()

        for (index in 0 until sourceFormats.length()) {
            val item = sourceFormats.optJSONObject(index) ?: continue
            if (item.optBoolean("has_drm", false)) continue

            val formatId = nonEmptyString(item, "format_id") ?: continue
            val extension = nonEmptyString(item, "ext") ?: continue
            val videoCodec = nonEmptyString(item, "vcodec")
            val audioCodec = nonEmptyString(item, "acodec")
            val hasVideo = videoCodec != null && !videoCodec.equals("none", ignoreCase = true)
            val hasAudio = audioCodec != null && !audioCodec.equals("none", ignoreCase = true)
            if (!hasVideo && !hasAudio) continue

            val width = item.optInt("width", 0).takeIf { it > 0 }
            val height = item.optInt("height", 0).takeIf { it > 0 }
            val resolution = nonEmptyString(item, "resolution")
                ?: when {
                    width != null && height != null -> "${width}x${height}"
                    height != null -> "${height}p"
                    else -> null
                }
            val fps = item.optDouble("fps", Double.NaN)
                .takeIf { it.isFinite() && it > 0 }
            val fileSize = item.optLong("filesize", -1L).takeIf { it > 0 }
            val approximateSize = item.optLong("filesize_approx", -1L).takeIf { it > 0 }
            val audioBitrate = item.optDouble("abr", Double.NaN)
                .takeIf { it.isFinite() && it > 0 }

            val normalized = JSONObject()
                .put("formatId", formatId)
                .put("ext", extension)
                .put("resolution", resolution ?: JSONObject.NULL)
                .put("width", width ?: JSONObject.NULL)
                .put("height", height ?: JSONObject.NULL)
                .put("fps", fps ?: JSONObject.NULL)
                .put("filesize", fileSize ?: JSONObject.NULL)
                .put("filesizeApprox", approximateSize ?: JSONObject.NULL)
                .put("hasVideo", hasVideo)
                .put("hasAudio", hasAudio)
                .put("audioBitrate", audioBitrate ?: JSONObject.NULL)
                .put("formatNote", nonEmptyString(item, "format_note") ?: JSONObject.NULL)
                .put("vcodec", videoCodec ?: JSONObject.NULL)
                .put("acodec", audioCodec ?: JSONObject.NULL)

            formats += RankedFormat(
                json = normalized,
                hasVideo = hasVideo,
                height = height ?: 0,
                fps = fps ?: 0.0,
                formatId = formatId,
            )
        }

        val orderedFormats = formats.sortedWith(
            compareByDescending<RankedFormat> { it.hasVideo }
                .thenByDescending { it.height }
                .thenByDescending { it.fps }
                .thenBy { it.formatId },
        )
        val formatArray = JSONArray()
        orderedFormats.forEach { formatArray.put(it.json) }

        val response = JSONObject()
            .put("id", nonEmptyString(metadata, "id") ?: JSONObject.NULL)
            .put("title", nonEmptyString(metadata, "title") ?: "Video")
            .put(
                "duration",
                metadata.optDouble("duration", Double.NaN)
                    .takeIf { it.isFinite() && it > 0 } ?: JSONObject.NULL,
            )
            .put("formats", formatArray)
        return response.toString()
    }

    private fun nonEmptyString(source: JSONObject, key: String): String? {
        if (!source.has(key) || source.isNull(key)) return null
        return source.optString(key).trim().takeIf { it.isNotEmpty() && it != "NA" }
    }
}
