//! Verified, versioned installer for BtbN static GPL FFmpeg builds.
//!
//! Only the selected ffmpeg executable is extracted. Archive paths are never
//! written to disk, publisher SHA-256 is checked before extraction, and the
//! active pointer changes only after version and libmp3lame probes succeed.

use crate::ffmpeg_updates::LatestFfmpegRelease;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ARCHIVE_BYTES: u64 = 320 * 1024 * 1024;
const MAX_EXECUTABLE_BYTES: u64 = 180 * 1024 * 1024;
const MAX_EXPANDED_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveFfmpeg {
    pub version: String,
    pub asset_name: String,
    pub executable_path: String,
    pub sha256: String,
    pub encoder: String,
}

fn expected_asset(target: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    match target {
        "windows-x86_64" => Ok(("ffmpeg-master-latest-win64-gpl.zip", "zip", "ffmpeg.exe")),
        "windows-aarch64" => Ok(("ffmpeg-master-latest-winarm64-gpl.zip", "zip", "ffmpeg.exe")),
        "linux-x86_64" => Ok(("ffmpeg-master-latest-linux64-gpl.tar.xz", "tar.xz", "ffmpeg")),
        "linux-aarch64" => Ok(("ffmpeg-master-latest-linuxarm64-gpl.tar.xz", "tar.xz", "ffmpeg")),
        _ => Err(format!("FFmpeg managed installation is not supported for target {target}")),
    }
}

fn safe_version_component(version: &str) -> Result<String, String> {
    if version.trim().is_empty() || version.len() > 160 {
        return Err("The FFmpeg release version is empty or too long".to_string());
    }
    let mut result = String::with_capacity(version.len());
    let mut previous_dash = false;
    for byte in version.bytes() {
        let ch = if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-') {
            byte as char
        } else {
            '-'
        };
        if ch == '-' {
            if !previous_dash {
                result.push(ch);
            }
            previous_dash = true;
        } else {
            result.push(ch);
            previous_dash = false;
        }
    }
    let result = result.trim_matches(['.', '-', '_']).to_string();
    if result.is_empty() || result == "." || result == ".." {
        return Err("The FFmpeg release version cannot be used as a directory name".to_string());
    }
    Ok(result)
}

fn validate_candidate(candidate: &LatestFfmpegRelease, target: &str) -> Result<&'static str, String> {
    let (asset, format, executable) = expected_asset(target)?;
    if candidate.asset_name != asset {
        return Err("The FFmpeg archive does not match this platform".to_string());
    }
    if !candidate.sha256.bytes().all(|b| b.is_ascii_hexdigit()) || candidate.sha256.len() != 64 {
        return Err("The publisher SHA-256 value is malformed".to_string());
    }
    let url = reqwest::Url::parse(&candidate.download_url)
        .map_err(|_| "The FFmpeg archive URL is invalid".to_string())?;
    let expected_path = format!("/BtbN/FFmpeg-Builds/releases/download/{}/{}", release_tag(&candidate.download_url)?, asset);
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.path() != expected_path
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("The FFmpeg archive is not the exact selected BtbN release asset".to_string());
    }
    if !candidate.release_page.starts_with("https://github.com/BtbN/FFmpeg-Builds/releases/") {
        return Err("The FFmpeg release page is not an official BtbN GitHub page".to_string());
    }
    let path_format = if asset.ends_with(".zip") { "zip" } else { "tar.xz" };
    if path_format != format || executable.is_empty() {
        return Err("The FFmpeg archive format is inconsistent".to_string());
    }
    safe_version_component(&candidate.version)?;
    Ok(format)
}

fn release_tag(download_url: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(download_url)
        .map_err(|_| "The FFmpeg archive URL is invalid".to_string())?;
    let parts: Vec<_> = url.path_segments()
        .ok_or_else(|| "The FFmpeg archive URL has no path".to_string())?
        .collect();
    if parts.len() != 6 || parts[0] != "BtbN" || parts[1] != "FFmpeg-Builds"
        || parts[2] != "releases" || parts[3] != "download" || parts[4].is_empty()
    {
        return Err("The FFmpeg release URL path is not canonical".to_string());
    }
    Ok(parts[4].to_string())
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .user_agent("Easy-Download/ffmpeg-installer")
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url = attempt.url();
            let trusted = url.scheme() == "https"
                && matches!(url.host_str(),
                    Some("github.com")
                    | Some("release-assets.githubusercontent.com")
                    | Some("objects.githubusercontent.com"));
            if trusted && attempt.previous().len() < 5 { attempt.follow() } else { attempt.stop() }
        }))
        .build()
        .map_err(|error| format!("Could not create FFmpeg download client: {error}"))
}

fn download_and_verify<F>(
    url: &str,
    destination: &Path,
    expected_sha256: &str,
    on_progress: F,
) -> Result<u64, String>
where
    F: FnMut(u64, Option<u64>),
{
    let client = http_client()?;
    let mut response = client.get(url).send()
        .map_err(|error| format!("Could not download FFmpeg archive: {error}"))?
        .error_for_status()
        .map_err(|error| format!("FFmpeg archive server returned an error: {error}"))?;
    let total = response.content_length();
    if total.is_some_and(|size| size > MAX_ARCHIVE_BYTES) {
        return Err("The FFmpeg archive exceeds the permitted download size".to_string());
    }
    let mut output = OpenOptions::new().write(true).create_new(true).open(destination)
        .map_err(|error| format!("Could not create FFmpeg archive staging file: {error}"))?;
    let result = (|| {
        let mut hasher = Sha256::new();
        let mut received = 0_u64;
        let mut buffer = [0_u8; 128 * 1024];
        loop {
            let count = response.read(&mut buffer)
                .map_err(|error| format!("Could not read FFmpeg archive: {error}"))?;
            if count == 0 { break; }
            received = received.saturating_add(count as u64);
            if received > MAX_ARCHIVE_BYTES {
                return Err("The FFmpeg archive exceeds the permitted download size".to_string());
            }
            output.write_all(&buffer[..count])
                .map_err(|error| format!("Could not write FFmpeg archive: {error}"))?;
            hasher.update(&buffer[..count]);
            on_progress(received, total);
        }
        output.flush().and_then(|_| output.sync_all())
            .map_err(|error| format!("Could not sync FFmpeg archive: {error}"))?;
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected_sha256) {
            return Err(format!("FFmpeg SHA-256 mismatch: expected {}, received {}", expected_sha256, actual));
        }
        Ok(received)
    })();
    drop(output);
    if result.is_err() { let _ = fs::remove_file(destination); }
    result
}

fn is_safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|part| matches!(part, Component::Normal(_)))
}

fn is_expected_binary(path: &Path, executable_name: &str) -> bool {
    if !is_safe_relative(path) { return false; }
    let components: Vec<_> = path.components().collect();
    components.len() >= 2
        && components[components.len() - 2].as_os_str() == "bin"
        && components[components.len() - 1].as_os_str() == executable_name
}

fn extract_zip(archive_path: &Path, destination: &Path, executable_name: &str) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("Could not open FFmpeg ZIP archive: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("FFmpeg ZIP archive is invalid: {error}"))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("The FFmpeg ZIP contains too many entries".to_string());
    }
    let mut expanded = 0_u64;
    let mut found = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)
            .map_err(|error| format!("Could not inspect FFmpeg ZIP entry: {error}"))?;
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_EXPANDED_ARCHIVE_BYTES {
            return Err("The FFmpeg ZIP expands beyond the permitted size".to_string());
        }
        let Some(path) = entry.enclosed_name().map(Path::to_path_buf) else { continue; };
        if !is_expected_binary(&path, executable_name) { continue; }
        if found { return Err("The FFmpeg ZIP contains multiple FFmpeg executables".to_string()); }
        if entry.size() == 0 || entry.size() > MAX_EXECUTABLE_BYTES {
            return Err("The FFmpeg executable has an invalid size".to_string());
        }
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err("The FFmpeg ZIP executable entry is a symbolic link".to_string());
        }
        let mut output = OpenOptions::new().write(true).create_new(true).open(destination)
            .map_err(|error| format!("Could not create staged FFmpeg executable: {error}"))?;
        std::io::copy(&mut entry, &mut output)
            .and_then(|_| output.sync_all())
            .map_err(|error| format!("Could not extract FFmpeg executable: {error}"))?;
        found = true;
    }
    if !found { return Err("The FFmpeg ZIP does not contain bin/ffmpeg.exe".to_string()); }
    Ok(())
}

fn extract_tar_xz(archive_path: &Path, destination: &Path, executable_name: &str) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("Could not open FFmpeg TAR.XZ archive: {error}"))?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive.entries()
        .map_err(|error| format!("FFmpeg TAR.XZ archive is invalid: {error}"))?;
    let mut expanded = 0_u64;
    let mut count = 0_usize;
    let mut found = false;
    for item in entries {
        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            return Err("The FFmpeg TAR.XZ contains too many entries".to_string());
        }
        let mut entry = item.map_err(|error| format!("Could not inspect FFmpeg TAR entry: {error}"))?;
        let size = entry.size();
        expanded = expanded.saturating_add(size);
        if expanded > MAX_EXPANDED_ARCHIVE_BYTES {
            return Err("The FFmpeg TAR.XZ expands beyond the permitted size".to_string());
        }
        let path = entry.path()
            .map_err(|error| format!("FFmpeg TAR entry has an invalid path: {error}"))?
            .into_owned();
        if !is_expected_binary(&path, executable_name) { continue; }
        if found { return Err("The FFmpeg TAR.XZ contains multiple FFmpeg executables".to_string()); }
        if !entry.header().entry_type().is_file() || size == 0 || size > MAX_EXECUTABLE_BYTES {
            return Err("The FFmpeg TAR executable entry is not a safe regular file".to_string());
        }
        let mut output = OpenOptions::new().write(true).create_new(true).open(destination)
            .map_err(|error| format!("Could not create staged FFmpeg executable: {error}"))?;
        std::io::copy(&mut entry, &mut output)
            .and_then(|_| output.sync_all())
            .map_err(|error| format!("Could not extract FFmpeg executable: {error}"))?;
        found = true;
    }
    if !found { return Err("The FFmpeg TAR.XZ does not contain bin/ffmpeg".to_string()); }
    Ok(())
}

fn set_executable_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("Could not inspect FFmpeg permissions: {error}"))?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("Could not mark FFmpeg executable: {error}"))?;
    }
    Ok(())
}

fn verify_ffmpeg(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect staged FFmpeg: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
        return Err("The staged FFmpeg executable is not a regular non-empty file".to_string());
    }
    let version = crate::ffmpeg::get_version(path)?;
    let output = Command::new(path).args(["-hide_banner", "-encoders"]).output()
        .map_err(|error| format!("Could not inspect FFmpeg encoders: {error}"))?;
    if !output.status.success() {
        return Err(format!("FFmpeg encoder check failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    let encoder_list = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if !encoder_list.contains("libmp3lame") {
        return Err("This FFmpeg build does not include the required libmp3lame encoder".to_string());
    }
    Ok(version)
}

fn reject_symlink_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect {label} directory: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("The {label} path must be a real directory, not a symbolic link"));
    }
    Ok(())
}

fn write_json_atomically(path: &Path, value: &ActiveFfmpeg) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "FFmpeg pointer has no parent directory".to_string())?;
    let name = path.file_name().ok_or_else(|| "FFmpeg pointer has no file name".to_string())?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let temporary = parent.join(format!("{}.tmp-{}-{nonce}", name.to_string_lossy(), std::process::id()));
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Could not encode FFmpeg pointer: {error}"))?;
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)
            .map_err(|error| format!("Could not create FFmpeg pointer: {error}"))?;
        file.write_all(&bytes).and_then(|_| file.sync_all())
            .map_err(|error| format!("Could not sync FFmpeg pointer: {error}"))?;
        replace_file_atomically(&temporary, path)
    })();
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}

#[cfg(unix)]
fn replace_file_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination)
        .map_err(|error| format!("Could not atomically activate FFmpeg: {error}"))
}

#[cfg(windows)]
fn replace_file_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe { MoveFileExW(source_wide.as_ptr(), destination_wide.as_ptr(),
        MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) };
    if result == 0 {
        Err(format!("Could not atomically activate FFmpeg: {}", std::io::Error::last_os_error()))
    } else { Ok(()) }
}

#[cfg(not(any(unix, windows)))]
fn replace_file_atomically(_source: &Path, _destination: &Path) -> Result<(), String> {
    Err("Atomic FFmpeg activation is not supported on this platform".to_string())
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("Could not hash FFmpeg executable: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| format!("Could not hash FFmpeg executable: {error}"))?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_pointer(path: &Path) -> Result<ActiveFfmpeg, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect FFmpeg active pointer: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("FFmpeg active pointer must be a regular file".to_string());
    }
    serde_json::from_slice(&fs::read(path)
        .map_err(|error| format!("Could not read FFmpeg active pointer: {error}"))?)
        .map_err(|error| format!("FFmpeg active pointer is invalid: {error}"))
}

fn resolve_installed(root: &Path, active: &ActiveFfmpeg) -> Result<PathBuf, String> {
    let relative = Path::new(&active.executable_path);
    if !is_safe_relative(relative) {
        return Err("FFmpeg active executable path is unsafe".to_string());
    }
    let executable = root.join(relative);
    let metadata = fs::symlink_metadata(&executable)
        .map_err(|error| format!("Could not access installed FFmpeg executable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Installed FFmpeg executable is not a regular file".to_string());
    }
    let actual_hash = hash_file(&executable)?;
    if !actual_hash.eq_ignore_ascii_case(&active.sha256) {
        return Err("Installed FFmpeg executable does not match its active SHA-256".to_string());
    }
    verify_ffmpeg(&executable)?;
    Ok(executable)
}

/// Downloads, hashes, extracts, probes, and atomically activates a BtbN FFmpeg build.
pub fn install_release<F>(
    candidate: &LatestFfmpegRelease,
    target: &str,
    install_root: impl AsRef<Path>,
    mut on_progress: F,
) -> Result<ActiveFfmpeg, String>
where F: FnMut(&str, u64, Option<u64>) {
    let format = validate_candidate(candidate, target)?;
    let (_, _, executable_name) = expected_asset(target)?;
    let root = install_root.as_ref();
    if !root.is_absolute() { return Err("The FFmpeg install directory must be absolute".to_string()); }
    fs::create_dir_all(root).map_err(|error| format!("Could not create FFmpeg install directory: {error}"))?;
    reject_symlink_directory(root, "FFmpeg install root")?;
    let versions = root.join("versions");
    let staging = root.join(".staging");
    fs::create_dir_all(&versions).map_err(|error| format!("Could not create FFmpeg versions directory: {error}"))?;
    fs::create_dir_all(&staging).map_err(|error| format!("Could not create FFmpeg staging directory: {error}"))?;
    reject_symlink_directory(&versions, "FFmpeg versions")?;
    reject_symlink_directory(&staging, "FFmpeg staging")?;

    let version_component = safe_version_component(&candidate.version)?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let stage = staging.join(format!("{}-{}-{nonce}", version_component, std::process::id()));
    fs::create_dir(&stage).map_err(|error| format!("Could not create private FFmpeg staging directory: {error}"))?;
    let result = (|| {
        let archive_path = stage.join(&candidate.asset_name);
        on_progress("downloading", 0, None);
        download_and_verify(&candidate.download_url, &archive_path, &candidate.sha256,
            |received, total| on_progress("downloading", received, total))?;
        on_progress("verifying", 1, Some(1));
        let staged_executable = stage.join(executable_name);
        on_progress("extracting", 0, None);
        match format.as_ref() {
            "zip" => extract_zip(&archive_path, &staged_executable, executable_name)?,
            "tar.xz" => extract_tar_xz(&archive_path, &staged_executable, executable_name)?,
            _ => return Err("Unsupported FFmpeg archive format".to_string()),
        }
        fs::remove_file(&archive_path).map_err(|error| format!("Could not remove FFmpeg archive after extraction: {error}"))?;
        set_executable_permissions(&staged_executable)?;
        let version_output = verify_ffmpeg(&staged_executable)?;
        let executable_hash = hash_file(&staged_executable)?;
        let version_dir = versions.join(&version_component);
        let final_executable = version_dir.join(executable_name);
        if version_dir.exists() {
            reject_symlink_directory(&version_dir, "existing FFmpeg version")?;
            let existing = read_installed_binary(&final_executable)?;
            if !hash_file(&existing)?.eq_ignore_ascii_case(&executable_hash) {
                return Err("An existing FFmpeg version directory contains different executable bytes".to_string());
            }
            fs::remove_dir_all(&stage).map_err(|error| format!("Could not clean FFmpeg staging directory: {error}"))?;
        } else {
            fs::rename(&stage, &version_dir)
                .map_err(|error| format!("Could not publish verified FFmpeg version: {error}"))?;
        }
        let active = ActiveFfmpeg {
            version: candidate.version.clone(),
            asset_name: candidate.asset_name.clone(),
            executable_path: PathBuf::from("versions").join(&version_component).join(executable_name)
                .to_string_lossy().replace('\\', "/"),
            sha256: executable_hash,
            encoder: "libmp3lame".to_string(),
        };
        let active_path = root.join("active.json");
        if active_path.exists() {
            let previous = read_pointer(&active_path)?;
            write_json_atomically(&root.join("previous.json"), &previous)?;
        }
        write_json_atomically(&active_path, &active)?;
        on_progress("installed", 1, Some(1));
        let _ = version_output;
        Ok(active)
    })();
    if stage.exists() { let _ = fs::remove_dir_all(&stage); }
    result
}

fn read_installed_binary(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect existing FFmpeg executable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Existing FFmpeg executable is not a regular file".to_string());
    }
    verify_ffmpeg(path)?;
    Ok(path.to_path_buf())
}

/// Returns the active executable after checking its pointer, hash, version, and MP3 encoder.
pub fn active_installation(install_root: impl AsRef<Path>) -> Result<ActiveFfmpeg, String> {
    let root = install_root.as_ref();
    if !root.is_absolute() { return Err("The FFmpeg install directory must be absolute".to_string()); }
    reject_symlink_directory(root, "FFmpeg install root")?;
    let active = read_pointer(&root.join("active.json"))?;
    resolve_installed(root, &active)?;
    Ok(active)
}

/// Atomically swaps active and previous FFmpeg versions after validating both.
pub fn rollback(install_root: impl AsRef<Path>) -> Result<ActiveFfmpeg, String> {
    let root = install_root.as_ref();
    if !root.is_absolute() { return Err("The FFmpeg install directory must be absolute".to_string()); }
    reject_symlink_directory(root, "FFmpeg install root")?;
    let active_path = root.join("active.json");
    let previous_path = root.join("previous.json");
    let current = read_pointer(&active_path)?;
    let previous = read_pointer(&previous_path)?;
    resolve_installed(root, &previous)?;
    resolve_installed(root, &current)?;
    write_json_atomically(&active_path, &previous)?;
    write_json_atomically(&previous_path, &current)?;
    Ok(previous)
}

#[cfg(test)]
mod tests {
    use super::{expected_asset, is_expected_binary, safe_version_component, validate_candidate};
    use crate::ffmpeg_updates::LatestFfmpegRelease;
    use std::path::Path;

    #[test]
    fn selects_static_gpl_archives_for_supported_desktop_targets() {
        assert_eq!(expected_asset("windows-x86_64").unwrap().0, "ffmpeg-master-latest-win64-gpl.zip");
        assert_eq!(expected_asset("windows-aarch64").unwrap().0, "ffmpeg-master-latest-winarm64-gpl.zip");
        assert_eq!(expected_asset("linux-x86_64").unwrap().0, "ffmpeg-master-latest-linux64-gpl.tar.xz");
        assert_eq!(expected_asset("linux-aarch64").unwrap().0, "ffmpeg-master-latest-linuxarm64-gpl.tar.xz");
        assert!(expected_asset("android-aarch64").is_err());
    }

    #[test]
    fn only_accepts_expected_safe_bin_paths() {
        assert!(is_expected_binary(Path::new("ffmpeg/bin/ffmpeg"), "ffmpeg"));
        assert!(is_expected_binary(Path::new("ffmpeg/bin/ffmpeg.exe"), "ffmpeg.exe"));
        assert!(!is_expected_binary(Path::new("../bin/ffmpeg"), "ffmpeg"));
        assert!(!is_expected_binary(Path::new("/bin/ffmpeg"), "ffmpeg"));
        assert!(!is_expected_binary(Path::new("ffmpeg/../bin/ffmpeg"), "ffmpeg"));
        assert!(!is_expected_binary(Path::new("ffmpeg/bin/ffprobe"), "ffmpeg"));
    }

    #[test]
    fn version_directory_component_is_sanitized() {
        assert_eq!(safe_version_component("Latest Auto-Build (2026-11-11)").unwrap(), "Latest-Auto-Build-2026-11-11");
        assert!(safe_version_component("../").is_err());
        assert!(safe_version_component("").is_err());
    }

    #[test]
    fn validates_exact_candidate_asset_and_release_url() {
        let candidate = LatestFfmpegRelease {
            version: "build-2026-11-11".to_string(),
            asset_name: "ffmpeg-master-latest-linux64-gpl.tar.xz".to_string(),
            download_url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz".to_string(),
            sha256: "a".repeat(64),
            release_page: "https://github.com/BtbN/FFmpeg-Builds/releases/tag/latest".to_string(),
            published_at: "2026-11-11T12:39:00Z".to_string(),
        };
        assert_eq!(validate_candidate(&candidate, "linux-x86_64").unwrap(), "tar.xz");
        assert!(validate_candidate(&candidate, "windows-x86_64").is_err());
        let mut invalid = candidate.clone();
        invalid.download_url = "https://example.com/ffmpeg.tar.xz".to_string();
        assert!(validate_candidate(&invalid, "linux-x86_64").is_err());
    }
}
