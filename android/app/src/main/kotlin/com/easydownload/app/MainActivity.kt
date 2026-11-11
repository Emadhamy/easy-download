package com.easydownload.app

import android.content.Intent
import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.documentfile.provider.DocumentFile
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File

class MainActivity : FlutterActivity() {
    private val storageChannel = "com.easydownload/storage"
    private val pickDirectoryRequestCode = 9401
    private var pendingDirectoryResult: MethodChannel.Result? = null

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
}
