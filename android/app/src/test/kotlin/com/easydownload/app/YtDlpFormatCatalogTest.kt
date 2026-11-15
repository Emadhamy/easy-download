package com.easydownload.app

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class YtDlpFormatCatalogTest {
    @Test
    fun filtersDrmAndInvalidFormatsAndSortsVideoByQuality() {
        val metadata = """
            {
              "id": "sample-id",
              "title": "Sample video",
              "duration": 30.5,
              "formats": [
                {"format_id":"18","ext":"mp4","height":360,"vcodec":"avc1","acodec":"mp4a"},
                {"format_id":"140","ext":"m4a","vcodec":"none","acodec":"mp4a","abr":128},
                {"format_id":"137","ext":"mp4","width":1920,"height":1080,"fps":30,"vcodec":"avc1","acodec":"none"},
                {"format_id":"22","ext":"mp4","height":720,"fps":30,"vcodec":"avc1","acodec":"mp4a"},
                {"format_id":"drm","ext":"mp4","height":2160,"vcodec":"avc1","acodec":"mp4a","has_drm":true},
                {"format_id":"","ext":"mp4","vcodec":"avc1","acodec":"mp4a"},
                {"format_id":"storyboard","ext":"mhtml","vcodec":"none","acodec":"none"}
              ]
            }
        """.trimIndent()

        val response = JSONObject(YtDlpFormatCatalog.fromMetadata(metadata))
        val formats = response.getJSONArray("formats")

        assertEquals("sample-id", response.getString("id"))
        assertEquals("Sample video", response.getString("title"))
        assertEquals(4, formats.length())
        assertEquals("137", formats.getJSONObject(0).getString("formatId"))
        assertEquals("22", formats.getJSONObject(1).getString("formatId"))
        assertEquals("18", formats.getJSONObject(2).getString("formatId"))
        assertEquals("140", formats.getJSONObject(3).getString("formatId"))
        assertFalse(formats.getJSONObject(0).getBoolean("hasAudio"))
        assertTrue(formats.getJSONObject(3).getBoolean("hasAudio"))
    }

    @Test
    fun neverExposesSignedMediaUrlsOrRequestHeaders() {
        val metadata = """
            {
              "title": "Private media",
              "formats": [{
                "format_id": "18",
                "ext": "mp4",
                "vcodec": "avc1",
                "acodec": "mp4a",
                "url": "https://cdn.example/private?token=secret",
                "http_headers": {"Authorization": "Bearer secret"}
              }]
            }
        """.trimIndent()

        val output = YtDlpFormatCatalog.fromMetadata(metadata)

        assertFalse(output.contains("cdn.example"))
        assertFalse(output.contains("token=secret"))
        assertFalse(output.contains("Authorization"))
        assertFalse(output.contains("Bearer secret"))
    }

    @Test
    fun rejectsMetadataWithoutFormatsArray() {
        try {
            YtDlpFormatCatalog.fromMetadata("""{"title":"No formats"}""")
            throw AssertionError("Expected invalid metadata to be rejected")
        } catch (error: IllegalArgumentException) {
            assertTrue(error.message.orEmpty().contains("formats list"))
        }
    }
}
