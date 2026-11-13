package com.easydownload.app

import android.content.Intent
import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.documentfile.provider.DocumentFile
import com.yausername.youtubedl_android.YoutubeDL
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import org.json.JSONObject
import java.util.concurrent.Executors

class MainActivity : FlutterActivity() {
    private val storageChannel = "com.easydownload/storage"
    private val ytDlpAndroidChannel = "com.easydownload/ytdlp_android"
    private val pickDirectoryRequestCode = 9401
    private var pendingDirectoryResult: MethodChannel.Result? = null
    private val ytDlpExecutor = Executors.newSingleThreadExecutor()

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, storageChannel)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "pickDownloadDirectory" -> {
                        if (pendingDirectoryResult != null) {
                            result.error("PICK_IN_PROGRESS", "A folder picker is already open.", null)
                            return@setMethodCallHandler
                        }
                        pendingDirectoryResult = result
                        try {
                            val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
                                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
                                addFlags(Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
                            }
                            startActivityForResult(intent, pickDirectoryRequestCode)
                        } catch (error: Exception) {
                            pendingDirectoryResult = null
                            result.error("PICK_FAILED", error.message, null)
                        }
                    }
                    "copyFileToDownloadDirectory" -> {
                        val treeUri = call.argument<String>("treeUri")
                        val sourcePath = call.argument<String>("sourcePath")
                        val fileName = call.argument<String>("fileName")
                        if (treeUri.isNullOrBlank() || sourcePath.isNullOrBlank() || fileName.isNullOrBlank()) {
                            result.error("INVALID_ARGUMENT", "Folder, source path, and file name are required.", null)
                            return@setMethodCallHandler
                        }
                        try {
                            val source = File(sourcePath)
                            if (!source.isFile) throw IllegalStateException("The completed temporary file was not found.")
                            val directory = DocumentFile.fromTreeUri(this, Uri.parse(treeUri))
                                ?: throw IllegalStateException("The selected folder is no longer available.")
                            if (!directory.canWrite()) throw IllegalStateException("The selected folder is not writable.")
                            val extension = fileName.substringAfterLast('.', "")
                            val mimeType = MimeTypeMap.getSingleton()
                                .getMimeTypeFromExtension(extension.lowercase()) ?: "application/octet-stream"
                            val base = fileName.substringBeforeLast('.', fileName)
                            val suffix = if (extension.isEmpty()) "" else ".$extension"
                            var candidate = fileName
                            var index = 1
                            while (directory.findFile(candidate) != null) {
                                candidate = "$base ($index)$suffix"
                                index++
                            }
                            val destination = directory.createFile(mimeType, candidate)
                                ?: throw IllegalStateException("Android could not create the file in the selected folder.")
                            try {
                                val output = contentResolver.openOutputStream(destination.uri, "w")
                                    ?: throw IllegalStateException("Android could not open the destination file.")
                                source.inputStream().use { input ->
                                    output.use { input.copyTo(it) }
                                }
                            } catch (error: Exception) {
                                destination.delete()
                                throw error
                            }
                            result.success(destination.uri.toString())
                        } catch (error: Exception) {
                            result.error("SAVE_FAILED", error.message, null)
                        }
                    }
                    else -> result.notImplemented()
                }
            }

        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, ytDlpAndroidChannel)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "initialize" -> ytDlpExecutor.execute {
                        try {
                            YoutubeDL.getInstance().init(applicationContext)
                            val version = YoutubeDL.getInstance().versionName(applicationContext)
                                ?: YoutubeDL.getInstance().version(applicationContext)
                            runOnUiThread {
                                result.success(mapOf("initialized" to true, "version" to version))
                            }
                        } catch (error: Exception) {
                            runOnUiThread {
                                result.error("YTDLP_ANDROID_INIT_FAILED", error.message, null)
                            }
                        }
                    }
                    "version" -> ytDlpExecutor.execute {
                        try {
                            YoutubeDL.getInstance().init(applicationContext)
                            val version = YoutubeDL.getInstance().versionName(applicationContext)
                                ?: YoutubeDL.getInstance().version(applicationContext)
                            runOnUiThread { result.success(version) }
                        } catch (error: Exception) {
                            runOnUiThread {
                                result.error("YTDLP_ANDROID_VERSION_FAILED", error.message, null)
                            }
                        }
                    }
                    "checkLatest" -> ytDlpExecutor.execute {
                        var connection: HttpURLConnection? = null
                        try {
                            YoutubeDL.getInstance().init(applicationContext)
                            connection = (URL("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
                                .openConnection() as HttpURLConnection).apply {
                                requestMethod = "GET"
                                connectTimeout = 15000
                                readTimeout = 15000
                                setRequestProperty("Accept", "application/vnd.github+json")
                                setRequestProperty("User-Agent", "Easy-Download-Android")
                            }
                            val payload = connection.inputStream.bufferedReader().use { it.readText() }
                            val latest = JSONObject(payload).optString("tag_name")
                            if (latest.isBlank()) throw IllegalStateException("The official release did not include a version tag.")
                            val installed = YoutubeDL.getInstance().version(applicationContext)
                            runOnUiThread {
                                result.success(mapOf("latestVersion" to latest, "installedVersion" to installed))
                            }
                        } catch (error: Exception) {
                            runOnUiThread {
                                result.error("YTDLP_ANDROID_CHECK_FAILED", error.message, null)
                            }
                        } finally {
                            connection?.disconnect()
                        }
                    }
                    "updateStable" -> ytDlpExecutor.execute {
                        try {
                            YoutubeDL.getInstance().init(applicationContext)
                            val status = YoutubeDL.getInstance().updateYoutubeDL(
                                applicationContext,
                                YoutubeDL.UpdateChannel.STABLE
                            )
                            val version = YoutubeDL.getInstance().versionName(applicationContext)
                                ?: YoutubeDL.getInstance().version(applicationContext)
                            runOnUiThread {
                                result.success(
                                    mapOf(
                                        "status" to status?.name,
                                        "version" to version
                                    )
                                )
                            }
                        } catch (error: Exception) {
                            runOnUiThread {
                                result.error("YTDLP_ANDROID_UPDATE_FAILED", error.message, null)
                            }
                        }
                    }
                    else -> result.notImplemented()
                }
            }
    }

    @Deprecated("Deprecated in Android")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != pickDirectoryRequestCode) return
        val callback = pendingDirectoryResult ?: return
        pendingDirectoryResult = null
        if (resultCode != RESULT_OK || data?.data == null) {
            callback.success(null)
            return
        }
        val uri = data.data!!
        try {
            val flags = data.flags and
                (Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
            contentResolver.takePersistableUriPermission(uri, flags)
            callback.success(uri.toString())
        } catch (error: Exception) {
            callback.error("PERMISSION_FAILED", error.message, null)
        }
    }

    override fun onDestroy() {
        ytDlpExecutor.shutdown()
        super.onDestroy()
    }
}
