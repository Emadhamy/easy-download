package com.easydownload.app

import android.content.Intent
import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.documentfile.provider.DocumentFile
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
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
    private val shareIntentsChannel = "com.easydownload/share_intents"
    private var shareMethodChannel: MethodChannel? = null
    private val pendingSharedContent = PendingSharedContentQueue()
    private val pickDirectoryRequestCode = 9401
    private var pendingDirectoryResult: MethodChannel.Result? = null
    private val ytDlpExecutor = Executors.newSingleThreadExecutor()

    // The bundled yt-dlp binary is present immediately after init(), but the
    // library's versionName()/version() only read metadata written by its updater.
    // Run the embedded binary itself so a fresh install is correctly recognized.
    private fun installedYtDlpVersion(): String {
        val request = YoutubeDLRequest(emptyList()).apply {
            addCommands(listOf("--version"))
        }
        val version = YoutubeDL.getInstance().execute(request).out.trim()
        if (version.isBlank()) {
            throw IllegalStateException("The bundled yt-dlp executable did not return a version.")
        }
        return version.lineSequence().first().trim()
    }

    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        captureSharedIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        captureSharedIntent(intent)
    }

    private fun captureSharedIntent(intent: Intent?) {
        if (intent == null) return
        val payload = pendingSharedContent.capture(
            action = intent.action,
            text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString(),
        ) ?: return
        shareMethodChannel?.invokeMethod("sharedContentReceived", payload.toChannelMap())
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        shareMethodChannel = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, shareIntentsChannel).also { channel ->
            channel.setMethodCallHandler { call, result ->
                when (call.method) {
                    "getInitialSharedContent" -> {
                        result.success(pendingSharedContent.snapshot().map { it.toChannelMap() })
                    }
                    "acknowledgeSharedContent" -> {
                        val id = call.argument<Number>("id")?.toLong()
                        if (id == null) {
                            result.error("INVALID_ARGUMENT", "A shared-content ID is required.", null)
                        } else {
                            pendingSharedContent.acknowledge(id)
                            result.success(null)
                        }
                    }
                    else -> result.notImplemented()
                }
            }
        }
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
                        val safeFileName = requireNotNull(fileName)
                        try {
                            val source = File(sourcePath)
                            if (!source.isFile) throw IllegalStateException("The completed temporary file was not found.")
                            val directory = DocumentFile.fromTreeUri(this, Uri.parse(treeUri))
                                ?: throw IllegalStateException("The selected folder is no longer available.")
                            if (!directory.canWrite()) throw IllegalStateException("The selected folder is not writable.")
                            val extension = safeFileName.substringAfterLast('.', "")
                            val mimeType = MimeTypeMap.getSingleton()
                                .getMimeTypeFromExtension(extension.lowercase()) ?: "application/octet-stream"
                            val base = safeFileName.substringBeforeLast('.', fileName)
                            val suffix = if (extension.isEmpty()) "" else ".$extension"
                            var candidate = safeFileName
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

        val ytDlpMethodChannel = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, ytDlpAndroidChannel)
        ytDlpMethodChannel.setMethodCallHandler { call, result ->
                when (call.method) {
                    "cancelDownload" -> {
                        val taskId = call.argument<String>("taskId")
                        if (taskId.isNullOrBlank()) {
                            result.error("INVALID_ARGUMENT", "A download task ID is required.", null)
                        } else {
                            result.success(YoutubeDL.getInstance().destroyProcessById(taskId))
                        }
                    }
                    "inspectFormats" -> ytDlpExecutor.execute {
                        try {
                            val sourceUrl = call.argument<String>("sourceUrl")
                                ?: throw IllegalArgumentException("A source URL is required.")
                            val source = Uri.parse(sourceUrl)
                            if ((source.scheme != "http" && source.scheme != "https") || source.host.isNullOrBlank()) {
                                throw IllegalArgumentException("Only valid HTTP(S) video URLs are supported.")
                            }
                            YoutubeDL.getInstance().init(applicationContext)
                            val request = YoutubeDLRequest(sourceUrl).apply {
                                addOption("--dump-single-json")
                                addOption("--skip-download")
                                addOption("--no-warnings")
                                addOption("--no-playlist")
                            }
                            val metadata = YoutubeDL.getInstance().execute(request).out
                            val response = YtDlpFormatCatalog.fromMetadata(metadata)
                            runOnUiThread { result.success(response) }
                        } catch (error: Exception) {
                            runOnUiThread { result.error("YTDLP_ANDROID_INSPECT_FAILED", error.message, null) }
                        }
                    }
                    "downloadFormat" -> ytDlpExecutor.execute {
                        try {
                            val sourceUrl = call.argument<String>("sourceUrl")
                                ?: throw IllegalArgumentException("A source URL is required.")
                            val formatId = call.argument<String>("formatId")
                                ?: throw IllegalArgumentException("A format ID is required.")
                            if (!formatId.matches(Regex("[A-Za-z0-9_.+-]{1,80}"))) {
                                throw IllegalArgumentException("The selected format ID is invalid.")
                            }
                            val taskId = call.argument<String>("taskId")
                                ?: throw IllegalArgumentException("A download task ID is required.")
                            val outputPath = call.argument<String>("outputPath")
                                ?: throw IllegalArgumentException("An output path is required.")
                            val source = Uri.parse(sourceUrl)
                            if ((source.scheme != "http" && source.scheme != "https") || source.host.isNullOrBlank()) {
                                throw IllegalArgumentException("Only valid HTTP(S) video URLs are supported.")
                            }
                            val requestedOutput = File(outputPath).canonicalFile
                            // Flutter's Android documents directory (app_flutter) is a sibling of files/.
                            // Keep writes inside the app's private data directory while allowing that location.
                            val appRoot = requireNotNull(applicationContext.filesDir.parentFile).canonicalFile
                            if (!requestedOutput.path.startsWith(appRoot.path + File.separator)) {
                                throw IllegalArgumentException("The output path must stay inside Easy Download's private storage.")
                            }
                            val outputDirectory = requestedOutput.parentFile
                                ?: throw IllegalArgumentException("The output directory is missing.")
                            if (!outputDirectory.exists() && !outputDirectory.mkdirs()) {
                                throw IllegalStateException("Could not create the download directory.")
                            }
                            // Let yt-dlp choose the final extension from the selected format metadata.
                            // The actual post-processing path is returned through ED_FILE below.
                            val outputTemplate = File(
                                outputDirectory,
                                "${requestedOutput.nameWithoutExtension}.%(ext)s"
                            ).absolutePath
                            YoutubeDL.getInstance().init(applicationContext)
                            val request = YoutubeDLRequest(sourceUrl).apply {
                                addOption("-f", formatId)
                                addOption("-o", outputTemplate)
                                addOption("--no-playlist")
                                addOption("--no-part")
                                addOption("--no-mtime")
                                addOption("--newline")
                                addOption("--print", "after_move:ED_FILE:%(filepath)s")
                            }
                            val response = YoutubeDL.getInstance().execute(request, taskId, true) { percent, etaSeconds, line ->
                                runOnUiThread {
                                    ytDlpMethodChannel.invokeMethod(
                                            "downloadProgress",
                                            mapOf(
                                                "taskId" to taskId,
                                                "percent" to percent.toDouble().coerceIn(0.0, 100.0),
                                                "etaSeconds" to etaSeconds,
                                                "line" to line
                                            )
                                        )
                                }
                            }
                            val reportedPath = response.out.lineSequence()
                                .mapNotNull { line ->
                                    line.takeIf { it.startsWith("ED_FILE:") }
                                        ?.removePrefix("ED_FILE:")
                                        ?.trim()
                                        ?.takeIf { it.isNotEmpty() }
                                }
                                .lastOrNull()
                                ?: throw IllegalStateException("yt-dlp completed without reporting the output file.")
                            val output = File(reportedPath).canonicalFile
                            if (!output.path.startsWith(appRoot.path + File.separator)) {
                                throw IllegalStateException("yt-dlp reported a file outside Easy Download's private storage.")
                            }
                            if (!output.isFile || output.length() <= 0L) {
                                throw IllegalStateException("yt-dlp reported an output file that is missing or empty.")
                            }
                            runOnUiThread { result.success(output.absolutePath) }
                        } catch (error: Exception) {
                            runOnUiThread { result.error("YTDLP_ANDROID_DOWNLOAD_FAILED", error.message, null) }
                        }
                    }
                    "initialize" -> ytDlpExecutor.execute {
                        try {
                            YoutubeDL.getInstance().init(applicationContext)
                            val version = installedYtDlpVersion()
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
                            val version = installedYtDlpVersion()
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
                            val installed = installedYtDlpVersion()
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
                            val version = installedYtDlpVersion()
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
