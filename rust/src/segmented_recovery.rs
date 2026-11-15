//! Durable metadata primitives for crash-safe segmented downloads.
//!
//! The sidecar stores only a SHA-256 request fingerprint and validated range
//! metadata. It never stores URLs, cookies, authorization headers, or tokens.

use crate::segmented::SegmentManifest;
use crate::DownloadUrl;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const RECOVERY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryManifest {
    pub schema_version: u32,
    pub request_fingerprint: String,
    pub transfer: SegmentManifest,
}

impl RecoveryManifest {
    pub fn new(input_url: &str, request_headers_json: &str, transfer: SegmentManifest) -> Result<Self, String> {
        transfer.validate()?;
        Ok(Self {
            schema_version: RECOVERY_SCHEMA_VERSION,
            request_fingerprint: request_fingerprint(input_url, request_headers_json)?,
            transfer,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != RECOVERY_SCHEMA_VERSION {
            return Err("Unsupported segmented recovery sidecar version".to_string());
        }
        if self.request_fingerprint.len() != 64
            || !self.request_fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Segmented recovery sidecar has an invalid request fingerprint".to_string());
        }
        self.transfer.validate()
    }
}

/// Hashes the normalized URL and request headers without persisting either.
pub fn request_fingerprint(input_url: &str, request_headers_json: &str) -> Result<String, String> {
    let url = DownloadUrl::parse(input_url)?;
    let mut digest = Sha256::new();
    digest.update(b"easy-download-segment-request-v1\\0");
    digest.update(url.as_str().as_bytes());
    digest.update(b"\\0");
    digest.update(request_headers_json.as_bytes());
    Ok(format!("{:x}", digest.finalize()))
}

pub fn sidecar_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".easy-download.segments.json");
    PathBuf::from(name)
}

pub fn segment_part_path(destination: &Path, index: usize) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(format!(".segment-{index}.part"));
    PathBuf::from(name)
}

/// Writes JSON to a sibling staging file, syncs it, then atomically replaces
/// the sidecar. The temporary file is removed on every error path.
pub fn save_atomic(path: &Path, manifest: &RecoveryManifest) -> Result<(), String> {
    manifest.validate()?;
    let parent = path.parent().filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err("Recovery sidecar parent directory does not exist".to_string());
    }
    let mut temporary_name = path.as_os_str().to_os_string();
    temporary_name.push(format!(".tmp-{}", std::process::id()));
    let temporary = PathBuf::from(temporary_name);
    let result = (|| {
        let encoded = serde_json::to_vec(manifest)
            .map_err(|error| format!("Could not encode recovery sidecar: {error}"))?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)
            .map_err(|error| format!("Could not create recovery sidecar staging file: {error}"))?;
        file.write_all(&encoded)
            .and_then(|_| file.flush())
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Could not sync recovery sidecar: {error}"))?;
        drop(file);
        atomic_replace(&temporary, path)
            .map_err(|error| format!("Could not atomically replace recovery sidecar: {error}"))?;
        sync_parent(parent).map_err(|error| format!("Could not sync recovery sidecar directory: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn load(path: &Path) -> Result<RecoveryManifest, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Could not open segmented recovery sidecar: {error}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read segmented recovery sidecar: {error}"))?;
    let manifest: RecoveryManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Segmented recovery sidecar is invalid JSON: {error}"))?;
    manifest.validate()?;
    Ok(manifest)
}

/// Reconciles sidecar progress with durable part files. Only regular files with
/// the exact expected length are considered complete; symlinks and truncated
/// or oversized files are discarded.
pub fn reconcile_parts(destination: &Path, manifest: &mut RecoveryManifest) -> Result<usize, String> {
    manifest.validate()?;
    let mut completed = 0;
    for segment in &mut manifest.transfer.segments {
        let part = segment_part_path(destination, segment.index);
        match fs::symlink_metadata(&part) {
            Ok(metadata) if metadata.file_type().is_file() && metadata.len() == segment.range.len() => {
                segment.received_bytes = segment.range.len();
                completed += 1;
            }
            Ok(_) => {
                fs::remove_file(&part)
                    .map_err(|error| format!("Could not discard invalid segment part {}: {error}", segment.index))?;
                segment.received_bytes = 0;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                segment.received_bytes = 0;
            }
            Err(error) => return Err(format!("Could not inspect segment part {}: {error}", segment.index)),
        }
    }
    Ok(completed)
}

/// Loads or safely initializes recovery state for one direct HTTP transfer.
/// Existing parts are reused only when the request fingerprint, remote
/// validator, total length, and segment layout all match the current probe.
pub fn prepare_recovery(
    input_url: &str,
    request_headers_json: &str,
    destination: &Path,
    probe: &crate::segmented::RangeProbe,
    segment_count: usize,
) -> Result<(RecoveryManifest, usize), String> {
    use crate::segmented::SegmentManifest;

    if !(crate::segmented::DEFAULT_SEGMENT_COUNT..=crate::segmented::USER_MAX_SEGMENT_COUNT)
        .contains(&segment_count)
    {
        return Err(format!(
            "Recovery segment count must be between {} and {}",
            crate::segmented::DEFAULT_SEGMENT_COUNT,
            crate::segmented::USER_MAX_SEGMENT_COUNT
        ));
    }
    let expected_fingerprint = request_fingerprint(input_url, request_headers_json)?;
    let sidecar = sidecar_path(destination);
    let mut reusable = None;

    match fs::symlink_metadata(&sidecar) {
        Ok(metadata) if metadata.file_type().is_file() => {
            if let Ok(existing) = load(&sidecar) {
                let transfer = &existing.transfer;
                if existing.request_fingerprint == expected_fingerprint
                    && transfer.total_bytes == probe.total_bytes
                    && transfer.validator == probe.validator
                    && transfer.validator_is_etag == probe.validator_is_etag
                    && transfer.segments.len() == segment_count
                {
                    reusable = Some(existing);
                }
            }
        }
        Ok(_) => {
            fs::remove_file(&sidecar)
                .map_err(|error| format!("Could not remove unsafe recovery sidecar: {error}"))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not inspect recovery sidecar: {error}")),
    }

    let mut manifest = if let Some(existing) = reusable {
        existing
    } else {
        for index in 0..crate::segmented::USER_MAX_SEGMENT_COUNT {
            let part = segment_part_path(destination, index);
            match fs::symlink_metadata(&part) {
                Ok(_) => fs::remove_file(&part)
                    .map_err(|error| format!("Could not discard stale segment part {index}: {error}"))?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("Could not inspect stale segment part {index}: {error}")),
            }
        }
        match fs::remove_file(&sidecar) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not replace stale recovery sidecar: {error}")),
        }
        let transfer = SegmentManifest::new(probe, segment_count)?;
        RecoveryManifest::new(input_url, request_headers_json, transfer)?
    };

    let mut assembly_name = destination.as_os_str().to_os_string();
    assembly_name.push(".easy-download.assembling");
    let assembly = PathBuf::from(assembly_name);
    match fs::remove_file(&assembly) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not discard interrupted assembly staging file: {error}")),
    }

    let completed = reconcile_parts(destination, &mut manifest)?;
    save_atomic(&sidecar, &manifest)?;
    Ok((manifest, completed))
}

#[cfg(unix)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(source.as_ptr(), destination.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
    };
    if result == 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
}

#[cfg(not(any(unix, windows)))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> std::io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> std::io::Result<()> { Ok(()) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmented::{RangeProbe, SegmentManifest};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_destination(label: &str) -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("easy-download-recovery-{label}-{nonce}.bin"))
    }

    fn sample() -> RecoveryManifest {
        let transfer = SegmentManifest::new(&RangeProbe {
            total_bytes: 12,
            validator: "\"stable-v1\"".to_string(),
            validator_is_etag: true,
        }, 4).unwrap();
        RecoveryManifest::new("https://example.com/file.bin?token=secret", "{}", transfer).unwrap()
    }

    #[test]
    fn fingerprint_is_stable_and_does_not_expose_url_or_credentials() {
        let first = request_fingerprint("https://example.com/file.bin?token=secret", "{}").unwrap();
        let second = request_fingerprint("https://example.com/file.bin?token=secret", "{}").unwrap();
        let changed = request_fingerprint("https://example.com/file.bin?token=other", "{}").unwrap();
        assert_eq!(first, second);
        assert_ne!(first, changed);
        let encoded = serde_json::to_string(&sample()).unwrap();
        assert!(!encoded.contains("example.com"));
        assert!(!encoded.contains("secret"));
    }

    #[test]
    fn sidecar_round_trips_atomically_and_replaces_previous_version() {
        let destination = temp_destination("roundtrip");
        let path = sidecar_path(&destination);
        let manifest = sample();
        save_atomic(&path, &manifest).unwrap();
        assert_eq!(load(&path).unwrap(), manifest);
        let mut changed = manifest.clone();
        changed.transfer.segments[0].received_bytes = changed.transfer.segments[0].range.len();
        save_atomic(&path, &changed).unwrap();
        assert_eq!(load(&path).unwrap(), changed);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_corrupt_or_unsupported_sidecars() {
        let destination = temp_destination("invalid");
        let path = sidecar_path(&destination);
        fs::write(&path, b"{not-json").unwrap();
        assert!(load(&path).is_err());
        fs::remove_file(path).unwrap();

        let mut manifest = sample();
        manifest.schema_version += 1;
        assert!(save_atomic(&sidecar_path(&destination), &manifest).is_err());
    }

    #[test]
    fn reconciliation_keeps_only_exact_complete_parts() {
        let destination = temp_destination("reconcile");
        let mut manifest = sample();
        let first = segment_part_path(&destination, 0);
        let second = segment_part_path(&destination, 1);
        let third = segment_part_path(&destination, 2);
        fs::write(&first, b"abc").unwrap();
        fs::write(&second, b"too-long").unwrap();
        assert_eq!(reconcile_parts(&destination, &mut manifest).unwrap(), 1);
        assert_eq!(manifest.transfer.segments[0].received_bytes, 3);
        assert_eq!(manifest.transfer.segments[1].received_bytes, 0);
        assert!(!second.exists());
        fs::remove_file(first).unwrap();
        let _ = fs::remove_file(third);
    }

    #[test]
    fn prepare_recovery_reuses_parts_only_for_same_request_and_validator() {
        let destination = temp_destination("prepare");
        let probe = RangeProbe { total_bytes: 12, validator: "\"stable-v1\"".to_string(), validator_is_etag: true };
        let first = prepare_recovery("https://example.com/file.bin", "{}", &destination, &probe, 4).unwrap();
        assert_eq!(first.1, 0);
        let complete_part = segment_part_path(&destination, 0);
        fs::write(&complete_part, b"abc").unwrap();
        let resumed = prepare_recovery("https://example.com/file.bin", "{}", &destination, &probe, 4).unwrap();
        assert_eq!(resumed.1, 1);
        assert_eq!(resumed.0.transfer.segments[0].received_bytes, 3);

        let changed_request = prepare_recovery("https://example.com/other.bin", "{}", &destination, &probe, 4).unwrap();
        assert_eq!(changed_request.1, 0);
        assert!(!complete_part.exists());

        fs::remove_file(sidecar_path(&destination)).unwrap();
    }

    #[test]
    fn prepare_recovery_discards_parts_when_remote_validator_changes() {
        let destination = temp_destination("validator");
        let original = RangeProbe { total_bytes: 12, validator: "\"stable-v1\"".to_string(), validator_is_etag: true };
        prepare_recovery("https://example.com/file.bin", "{}", &destination, &original, 4).unwrap();
        fs::write(segment_part_path(&destination, 0), b"abc").unwrap();

        let changed = RangeProbe { total_bytes: 12, validator: "\"stable-v2\"".to_string(), validator_is_etag: true };
        let prepared = prepare_recovery("https://example.com/file.bin", "{}", &destination, &changed, 4).unwrap();
        assert_eq!(prepared.1, 0);
        assert!(!segment_part_path(&destination, 0).exists());
        fs::remove_file(sidecar_path(&destination)).unwrap();
    }
}
