//! Verified, versioned installer for official yt-dlp desktop releases.
//!
//! Release metadata is validated before network access. Downloads are streamed
//! into a private staging directory, hashed from the received bytes, executed
//! for a version probe, and only then activated through an atomic pointer file.

use crate::yt_dlp_updates::LatestYtDlpRelease;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 80
        && version.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_' || b == b'+')
}

fn validate_digest(digest: &str) -> bool {
    digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit())
}

fn expected_asset(target: &str) -> Result<&'static str, String> {
    crate::yt_dlp_updates::asset_name_for_target(target)
}

fn validate_candidate(candidate: &LatestYtDlpRelease, target: &str) -> Result<(), String> {
    if !valid_version(&candidate.version) {
        return Err("The yt-dlp release version is invalid".to_string());
    }
    if candidate.asset_name != expected_asset(target)? {
        return Err("The yt-dlp asset does not match the current platform".to_string());
    }
    if !validate_digest(&candidate.sha256) {
        return Err("The official yt-dlp SHA-256 digest is invalid".to_string());
    }

    let expected_path = format!(
        "/yt-dlp/yt-dlp/releases/download/{}/{}",
        candidate.version, candidate.asset_name
    );
    let url = reqwest::Url::parse(&candidate.download_url)
        .map_err(|_| "The yt-dlp download URL is invalid".to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.path() != expected_path
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("The download URL is not the exact official yt-dlp release asset".to_string());
    }

    let release_page = reqwest::Url::parse(&candidate.release_page)
        .map_err(|_| "The yt-dlp release page URL is invalid".to_string())?;
    if release_page.scheme() != "https"
        || release_page.host_str() != Some("github.com")
        || !release_page.path().starts_with("/yt-dlp/yt-dlp/releases/")
    {
        return Err("The release page is not an official yt-dlp GitHub page".to_string());
    }
    Ok(())
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent("Easy-Download/yt-dlp-updater")
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url = attempt.url();
            let trusted = url.scheme() == "https"
                && matches!(
                    url.host_str(),
                    Some("github.com")
                        | Some("release-assets.githubusercontent.com")
                        | Some("objects.githubusercontent.com")
                );
            if trusted && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|error| format!("Could not create yt-dlp download client: {error}"))
}

fn download_and_verify<F>(
    client: &reqwest::blocking::Client,
    url: &str,
    destination: &Path,
    expected_sha256: &str,
    mut on_progress: F,
) -> Result<u64, String>
where
    F: FnMut(u64, Option<u64>),
{
    let mut response = client
        .get(url)
        .send()
        .map_err(|error| format!("Could not download the official yt-dlp asset: {error}"))?
        .error_for_status()
        .map_err(|error| format!("The yt-dlp asset server returned an error: {error}"))?;

    let total = response.content_length();
    if total.is_some_and(|size| size > MAX_ASSET_BYTES) {
        return Err("The yt-dlp asset exceeds the permitted download size".to_string());
    }

    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("Could not create the yt-dlp staging file: {error}"))?;

    let result = (|| {
        let mut hasher = Sha256::new();
        let mut received = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];

        loop {
            let count = response
                .read(&mut buffer)
                .map_err(|error| format!("Could not read the yt-dlp download: {error}"))?;
            if count == 0 {
                break;
            }
            received = received.saturating_add(count as u64);
            if received > MAX_ASSET_BYTES {
                return Err("The yt-dlp asset exceeds the permitted download size".to_string());
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| format!("Could not write the yt-dlp staging file: {error}"))?;
            hasher.update(&buffer[..count]);
            on_progress(received, total);
        }

        output
            .flush()
            .and_then(|_| output.sync_all())
            .map_err(|error| format!("Could not sync the yt-dlp staging file: {error}"))?;

        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected_sha256) {
            return Err(format!(
                "yt-dlp SHA-256 mismatch: expected {}, received {}",
                expected_sha256, actual
            ));
        }
        Ok(received)
    })();

    drop(output);
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn set_executable_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| format!("Could not read yt-dlp file permissions: {error}"))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("Could not mark yt-dlp executable: {error}"))?;
    }
    Ok(())
}

fn verify_executable(path: &Path, expected_version: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect staged yt-dlp executable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("The staged yt-dlp executable is not a regular file".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("The staged yt-dlp file is not executable".to_string());
        }
    }

    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|error| format!("Could not run the staged yt-dlp version check: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "The staged yt-dlp version check failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if actual != expected_version {
        return Err(format!(
            "The staged yt-dlp version is {}, expected {}",
            actual, expected_version
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveYtDlp {
    pub version: String,
    pub asset_name: String,
    pub executable_path: String,
    pub sha256: String,
}

fn reject_symlink_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect {label} directory: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("The {label} path must be a real directory, not a symbolic link"));
    }
    Ok(())
}

fn write_json_atomically(path: &Path, value: &ActiveYtDlp) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "Active yt-dlp pointer has no parent directory".to_string())?;
    let file_name = path.file_name().ok_or_else(|| "Active yt-dlp pointer has no file name".to_string())?;
    let temporary = parent.join(format!(
        "{}.tmp-{}-{}",
        file_name.to_string_lossy(),
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()
    ));
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Could not encode active yt-dlp metadata: {error}"))?;

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not create active yt-dlp pointer: {error}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Could not sync active yt-dlp pointer: {error}"))?;
        replace_file_atomically(&temporary, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(unix)]
fn replace_file_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination)
        .map_err(|error| format!("Could not atomically activate yt-dlp: {error}"))
}

#[cfg(windows)]
fn replace_file_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(format!("Could not atomically activate yt-dlp (Windows error {})", std::io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn replace_file_atomically(_source: &Path, _destination: &Path) -> Result<(), String> {
    Err("Atomic yt-dlp activation is not supported on this platform".to_string())
}

/// Downloads, verifies, stages, version-checks, and activates one official release.
/// The existing active pointer is not changed unless every verification succeeds.
pub fn install_release<F>(
    candidate: &LatestYtDlpRelease,
    target: &str,
    install_root: impl AsRef<Path>,
    on_progress: F,
) -> Result<ActiveYtDlp, String>
where
    F: FnMut(u64, Option<u64>),
{
    validate_candidate(candidate, target)?;

    let root = install_root.as_ref();
    if !root.is_absolute() {
        return Err("The yt-dlp install directory must be an absolute path".to_string());
    }
    fs::create_dir_all(root)
        .map_err(|error| format!("Could not create yt-dlp install directory: {error}"))?;
    reject_symlink_directory(root, "yt-dlp install root")?;

    let versions_dir = root.join("versions");
    let staging_dir = root.join(".staging");
    fs::create_dir_all(&versions_dir)
        .map_err(|error| format!("Could not create yt-dlp versions directory: {error}"))?;
    fs::create_dir_all(&staging_dir)
        .map_err(|error| format!("Could not create yt-dlp staging directory: {error}"))?;
    reject_symlink_directory(&versions_dir, "yt-dlp versions")?;
    reject_symlink_directory(&staging_dir, "yt-dlp staging")?;

    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let stage = staging_dir.join(format!("{}-{}-{unique}", candidate.version, std::process::id()));
    fs::create_dir(&stage)
        .map_err(|error| format!("Could not create private yt-dlp staging directory: {error}"))?;

    let result = (|| {
        let staged_executable = stage.join(&candidate.asset_name);
        let client = http_client()?;
        download_and_verify(
            &client,
            &candidate.download_url,
            &staged_executable,
            &candidate.sha256,
            on_progress,
        )?;
        set_executable_permissions(&staged_executable)?;
        verify_executable(&staged_executable, &candidate.version)?;

        let version_dir = versions_dir.join(&candidate.version);
        let final_executable = version_dir.join(&candidate.asset_name);
        if version_dir.exists() {
            verify_executable(&final_executable, &candidate.version)?;
            let existing_hash = hash_file(&final_executable)?;
            if !existing_hash.eq_ignore_ascii_case(&candidate.sha256) {
                return Err("An existing yt-dlp version directory has different binary contents".to_string());
            }
            fs::remove_dir_all(&stage).ok();
        } else {
            fs::rename(&stage, &version_dir)
                .map_err(|error| format!("Could not publish verified yt-dlp version: {error}"))?;
        }

        let relative = PathBuf::from("versions").join(&candidate.version).join(&candidate.asset_name);
        let active = ActiveYtDlp {
            version: candidate.version.clone(),
            asset_name: candidate.asset_name.clone(),
            executable_path: relative.to_string_lossy().replace('\\', "/"),
            sha256: candidate.sha256.to_ascii_lowercase(),
        };
        let active_path = root.join("active.json");
        if active_path.exists() {
            let metadata = fs::symlink_metadata(&active_path)
                .map_err(|error| format!("Could not inspect current yt-dlp pointer: {error}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("The current yt-dlp pointer must be a regular file".to_string());
            }
            let old: ActiveYtDlp = serde_json::from_slice(
                &fs::read(&active_path).map_err(|error| format!("Could not read current yt-dlp pointer: {error}"))?
            ).map_err(|error| format!("Current yt-dlp pointer is invalid: {error}"))?;
            write_json_atomically(&root.join("previous.json"), &old)?;
        }
        write_json_atomically(&active_path, &active)?;
        Ok(active)
    })();

    if stage.exists() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Could not open installed yt-dlp binary for hashing: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)
            .map_err(|error| format!("Could not hash installed yt-dlp binary: {error}"))?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::{download_and_verify, validate_candidate, write_json_atomically, ActiveYtDlp};
    use reqwest::blocking::Client;
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> std::path::PathBuf {
        let id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("easy-download-yt-dlp-{label}-{id}"))
    }

    fn fixture(body: &'static [u8]) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            ).unwrap();
            stream.write_all(body).unwrap();
        });
        (format!("http://{address}/asset"), handle)
    }

    #[test]
    fn streams_asset_and_verifies_actual_sha256() {
        let body = b"verified yt-dlp fixture";
        let digest = format!("{:x}", Sha256::digest(body));
        let (url, server) = fixture(body);
        let path = temp_path("verified");
        let client = Client::builder().build().unwrap();
        let received = download_and_verify(&client, &url, &path, &digest, |_, _| {}).unwrap();
        server.join().unwrap();
        assert_eq!(received, body.len() as u64);
        assert_eq!(std::fs::read(&path).unwrap(), body);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_checksum_mismatch_and_removes_partial_file() {
        let (url, server) = fixture(b"untrusted payload");
        let path = temp_path("bad-hash");
        let client = Client::builder().build().unwrap();
        let result = download_and_verify(&client, &url, &path, &"0".repeat(64), |_, _| {});
        server.join().unwrap();
        assert!(result.unwrap_err().contains("SHA-256 mismatch"));
        assert!(!path.exists());
    }

    #[test]
    fn rejects_non_official_release_url_and_wrong_asset() {
        let candidate = crate::yt_dlp_updates::LatestYtDlpRelease {
            version: "2026.10.26".to_string(),
            asset_name: "yt-dlp.exe".to_string(),
            download_url: "https://example.com/yt-dlp.exe".to_string(),
            sha256: "a".repeat(64),
            release_page: "https://github.com/yt-dlp/yt-dlp/releases/tag/2026.10.26".to_string(),
        };
        assert!(validate_candidate(&candidate, "windows-x86_64").is_err());
        assert!(validate_candidate(&candidate, "linux-x86_64").is_err());
    }

    #[test]
    fn active_pointer_is_written_as_valid_json() {
        let root = temp_path("pointer");
        std::fs::create_dir_all(&root).unwrap();
        let pointer = root.join("active.json");
        let active = ActiveYtDlp {
            version: "2026.10.26".to_string(),
            asset_name: "yt-dlp_linux".to_string(),
            executable_path: "versions/2026.10.26/yt-dlp_linux".to_string(),
            sha256: "b".repeat(64),
        };
        write_json_atomically(&pointer, &active).unwrap();
        let parsed: ActiveYtDlp = serde_json::from_slice(&std::fs::read(&pointer).unwrap()).unwrap();
        assert_eq!(parsed, active);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn verifies_executable_version_before_activation() {
        use super::{set_executable_permissions, verify_executable};
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("version-probe");
        std::fs::write(&path, "#!/bin/sh\nprintf '2026.10.26\\n'\n").unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&path, permissions).unwrap();
        set_executable_permissions(&path).unwrap();
        verify_executable(&path, "2026.10.26").unwrap();
        assert!(verify_executable(&path, "2026.10.27").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
