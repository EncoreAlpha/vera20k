//! Local VMCE stage unlocks. Campaign content digests are deliberately not
//! part of these identities: updating a series must not erase its progress.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

const FILENAME: &str = "VmceProgress.json";
#[cfg(not(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown")))]
const LOCK_FILENAME: &str = ".vmce-progress.lock";
const VERSION: u32 = 1;

/// Directory names and map paths follow VMCE's case-insensitive logical map
/// matching; map paths use forward slashes on every platform.
#[derive(Debug, Default)]
pub(crate) struct VmceProgress {
    series: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgressFile {
    version: u32,
    series: BTreeMap<String, BTreeSet<String>>,
}

impl VmceProgress {
    /// Whether a series-relative map was unlocked on this computer.
    pub(crate) fn is_unlocked(&self, series: &str, map: &str) -> io::Result<bool> {
        let (series, map) = identity(series, map)?;
        Ok(self
            .series
            .get(&series)
            .is_some_and(|maps| maps.contains(&map)))
    }
}

/// Read the latest local record. An absent file starts empty; invalid or
/// unsupported records are errors so a subsequent unlock cannot erase them.
pub(crate) fn load(settings_dir: &Path) -> io::Result<VmceProgress> {
    let bytes = match crate::util::file_system::read(settings_dir.join(FILENAME)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(VmceProgress::default());
        }
        Err(error) => return Err(error),
    };
    let file: ProgressFile = serde_json::from_slice(&bytes).map_err(invalid_data)?;
    if file.version != VERSION {
        return Err(invalid_data("unsupported VMCE progress version"));
    }
    for (series, maps) in &file.series {
        for map in maps {
            let normalized = identity(series, map).map_err(invalid_data)?;
            if normalized != (series.clone(), map.clone()) {
                return Err(invalid_data("noncanonical VMCE progress identity"));
            }
        }
        // Validate even an empty series entry.
        if normalize_series(series).map_err(invalid_data)? != *series {
            return Err(invalid_data("noncanonical VMCE series identity"));
        }
    }
    Ok(VmceProgress {
        series: file.series,
    })
}

/// Record one stage and return whether it was newly unlocked. Callers may log
/// an error and continue the campaign; errors never turn a corrupt record into
/// an empty one. Repeated calls for the same logical identity do not write.
pub(crate) fn unlock(settings_dir: &Path, series: &str, map: &str) -> io::Result<bool> {
    let (series, map) = identity(series, map)?;
    crate::util::file_system::create_dir_all(settings_dir)?;
    // Lock a separate, permanent sidecar: renaming the progress file must not
    // let another process acquire a lock on a different file identity.
    #[cfg(not(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown")))]
    let _lock = lock_progress(settings_dir)?;
    let mut progress = load(settings_dir)?;
    if !progress.series.entry(series).or_default().insert(map) {
        return Ok(false);
    }
    let bytes = serde_json::to_vec_pretty(&ProgressFile {
        version: VERSION,
        series: progress.series,
    })
    .map_err(io::Error::other)?;
    super::account::write_private(&settings_dir.join(FILENAME), &bytes)?;
    Ok(true)
}

#[cfg(not(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown")))]
fn lock_progress(settings_dir: &Path) -> io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(settings_dir.join(LOCK_FILENAME))?;
    file.lock()?;
    Ok(file)
}

fn identity(series: &str, map: &str) -> io::Result<(String, String)> {
    let series = normalize_series(series)?;
    crate::map::source::validate_network_map_name(map).map_err(invalid_input)?;
    Ok((series, map.replace('\\', "/").to_ascii_lowercase()))
}

fn normalize_series(series: &str) -> io::Result<String> {
    crate::map::source::validate_network_map_name(series).map_err(invalid_input)?;
    if series.contains(['/', '\\']) {
        return Err(invalid_input("VMCE series must be one directory name"));
    }
    Ok(series.to_ascii_lowercase())
}

fn invalid_input(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::source::test_support::TestDirectory;

    #[test]
    fn unlocks_persist_by_series_and_logical_map_path() {
        let directory = TestDirectory::new("vmce-progress");
        assert!(
            !load(directory.path())
                .unwrap()
                .is_unlocked("Series", "01.map")
                .unwrap()
        );
        assert!(unlock(directory.path(), "Series", "mission\\02.map").unwrap());
        let original = std::fs::read(directory.path().join(FILENAME)).unwrap();
        assert!(!unlock(directory.path(), "series", "MISSION/02.MAP").unwrap());
        assert_eq!(
            std::fs::read(directory.path().join(FILENAME)).unwrap(),
            original
        );
        assert!(unlock(directory.path(), "Series", "mission/03.map").unwrap());
        let loaded = load(directory.path()).unwrap();
        assert!(loaded.is_unlocked("SERIES", "mission/02.map").unwrap());
        assert!(loaded.is_unlocked("Series", "mission/03.map").unwrap());
        assert!(!loaded.is_unlocked("Other", "mission/02.map").unwrap());
        assert!(!loaded.is_unlocked("Series", "mission/renamed.map").unwrap());
    }

    #[test]
    fn corrupt_or_future_records_are_never_replaced() {
        let directory = TestDirectory::new("vmce-progress-invalid");
        for bytes in [b"{bad json".as_slice(), br#"{"version":2,"series":{}}"#] {
            directory.write(FILENAME, bytes);
            assert_eq!(
                load(directory.path()).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            assert!(unlock(directory.path(), "Series", "01.map").is_err());
            assert_eq!(
                std::fs::read(directory.path().join(FILENAME)).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn invalid_identity_or_write_failure_does_not_unlock() {
        let directory = TestDirectory::new("vmce-progress-fail");
        assert!(unlock(directory.path(), "../Series", "01.map").is_err());
        assert!(unlock(directory.path(), "Series", "../01.map").is_err());
        let file = directory.path().join("not-a-directory");
        std::fs::write(&file, b"unchanged").unwrap();
        assert!(unlock(&file, "Series", "01.map").is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"unchanged");
        assert!(!directory.path().join(FILENAME).exists());
    }

    #[test]
    #[cfg(not(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown")))]
    fn simultaneous_unlocks_wait_for_the_shared_lock_and_keep_both_stages() {
        use std::sync::{Arc, Barrier, mpsc};
        use std::time::Duration;

        let directory = TestDirectory::new("vmce-progress-lock");
        let lock = lock_progress(directory.path()).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let settings = directory.path().to_owned();
        let waiting = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            done_tx.send(unlock(&settings, "Series", "01.map")).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        drop(lock);
        assert!(
            done_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap()
        );
        waiting.join().unwrap();

        let start = Arc::new(Barrier::new(3));
        let workers: Vec<_> = ["02.map", "03.map"]
            .into_iter()
            .map(|map| {
                let start = Arc::clone(&start);
                let settings = directory.path().to_owned();
                std::thread::spawn(move || {
                    start.wait();
                    unlock(&settings, "Series", map)
                })
            })
            .collect();
        start.wait();
        for worker in workers {
            assert!(worker.join().unwrap().unwrap());
        }
        let progress = load(directory.path()).unwrap();
        assert!(progress.is_unlocked("Series", "01.map").unwrap());
        assert!(progress.is_unlocked("Series", "02.map").unwrap());
        assert!(progress.is_unlocked("Series", "03.map").unwrap());
    }
}
