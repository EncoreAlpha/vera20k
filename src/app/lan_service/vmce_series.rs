//! VMCE LAN whole-series verification: one background scan and its cache.
//!
//! A VMCE LAN options revision advertises one SHA-256 over the campaign's
//! entire series directory: `campaign.ini` bytes, manifest order, and every
//! manifest map's relative logical name, byte length and raw bytes. A guest
//! may not report its map admission as available (`K` status 1) or accept
//! Ready until its own copy of that whole series produced the advertised
//! digest; otherwise a peer with a different later mission could be admitted.
//!
//! The scan streams up to 512 maps, so it runs on a worker. This owner holds
//! only the worker, its cancellation flag, the generation and the cached
//! result, and exposes queries. The authoritative admission flag stays in
//! `MatchSession`'s participant admission; the `K`/`A1` sends stay in
//! `NetworkSetupProtocol`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

/// One local series identity the scanner was asked about. The advertised
/// digest is `None` only on the host, which computes the digest it publishes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SeriesKey {
    directory: String,
    manifest_sha256: String,
    advertised: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ScanOutcome {
    Digest(String),
    Failed(String),
}

struct ScanResult {
    generation: u64,
    key: SeriesKey,
    outcome: ScanOutcome,
}

/// What the caller may do with the current whole-series identity.
///
/// `Verified` on a guest means the local series produced the advertised
/// digest; on the host it means the digest it advertises was computed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VmceSeriesState {
    /// No series identity has been requested for the active setup.
    Idle,
    /// A scan for the active identity is running.
    Pending,
    Verified {
        digest: String,
    },
    /// The local series is readable but its digest differs from the
    /// advertised one.
    Mismatch {
        local: String,
    },
    /// The local series cannot be read or validated.
    Failed(String),
}

impl VmceSeriesState {
    pub(crate) fn verified(&self) -> bool {
        matches!(self, Self::Verified { .. })
    }
}

pub(crate) struct VmceSeriesVerifier {
    generation: u64,
    cancel: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    results: Option<Receiver<ScanResult>>,
    /// The identity the cached outcome (or the running worker) belongs to.
    current: Option<SeriesKey>,
    outcome: Option<ScanOutcome>,
    /// A Ready/Go boundary revalidation produced the cached outcome, so that
    /// boundary may pass. Admission-time scans do not set this.
    revalidated: bool,
    /// Host boundary scans remember the digest the host published; a fresh
    /// scan that produces a different digest means the series changed after
    /// the room opened and must refuse the start.
    boundary_expected: Option<String>,
    /// Whether the current key's finished verdict has reached the admission
    /// owner. A verdict is applied at most once per scan; reconciliation
    /// retries until it succeeds, so a completion that lands while the setup
    /// identity is temporarily unavailable is never lost.
    applied: bool,
    #[cfg(test)]
    scans_started: usize,
}

impl Default for VmceSeriesVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl VmceSeriesVerifier {
    pub(crate) fn new() -> Self {
        Self {
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            worker: None,
            results: None,
            current: None,
            outcome: None,
            revalidated: false,
            boundary_expected: None,
            applied: false,
            #[cfg(test)]
            scans_started: 0,
        }
    }

    /// The active identity's state. Cheap: never touches the filesystem.
    pub(crate) fn state(&self) -> VmceSeriesState {
        if self.worker.is_some() {
            return VmceSeriesState::Pending;
        }
        let Some(key) = self.current.as_ref() else {
            return VmceSeriesState::Idle;
        };
        match self.outcome.as_ref() {
            Some(ScanOutcome::Failed(error)) => VmceSeriesState::Failed(error.clone()),
            Some(ScanOutcome::Digest(local)) => match key.advertised.as_deref() {
                Some(advertised) if advertised != local => VmceSeriesState::Mismatch {
                    local: local.clone(),
                },
                _ => VmceSeriesState::Verified {
                    digest: local.clone(),
                },
            },
            None => VmceSeriesState::Idle,
        }
    }

    /// Host query: the whole-series digest for this local series, starting a
    /// scan when the identity is new. `None` while a scan is running; inspect
    /// [`Self::state`] for a failed scan's reason.
    pub(crate) fn host_digest(&mut self, series: &LocalSeries) -> Option<String> {
        let key = Self::key(series, None);
        if self.current.as_ref() != Some(&key) {
            self.start(key, series, false);
            return None;
        }
        if self.worker.is_some() {
            return None;
        }
        match self.outcome.as_ref() {
            Some(ScanOutcome::Digest(digest)) => Some(digest.clone()),
            _ => None,
        }
    }

    /// Guest query: verify the local series against the digest the host
    /// advertised. Repeated options revisions of the same identity reuse the
    /// cached verdict instead of rescanning.
    pub(crate) fn verify_advertised(
        &mut self,
        series: &LocalSeries,
        advertised: &str,
    ) -> VmceSeriesState {
        let key = Self::key(series, Some(advertised));
        if self.current.as_ref() != Some(&key) {
            self.start(key, series, false);
            return VmceSeriesState::Pending;
        }
        if self.worker.is_some() {
            return VmceSeriesState::Pending;
        }
        if self.outcome.is_none() {
            self.start(key, series, false);
            return VmceSeriesState::Pending;
        }
        self.state()
    }

    /// Ready/Go boundary: the guest needs a scan that completed for the
    /// current identity after admission, because the series files may have
    /// changed since the options revision was admitted. Starts that
    /// revalidation when it is missing and reports `Pending` until it ends.
    pub(crate) fn boundary(&mut self, series: &LocalSeries, advertised: &str) -> VmceSeriesState {
        let key = Self::key(series, Some(advertised));
        if self.current.as_ref() != Some(&key) {
            self.start(key, series, true);
            return VmceSeriesState::Pending;
        }
        if self.worker.is_some() {
            return VmceSeriesState::Pending;
        }
        match self.state() {
            state @ (VmceSeriesState::Mismatch { .. } | VmceSeriesState::Failed(_)) => state,
            VmceSeriesState::Verified { .. } if !self.revalidated => {
                self.start(key, series, true);
                VmceSeriesState::Pending
            }
            state => state,
        }
    }

    /// Host Ready/Go boundary: revalidate the series the host advertises, so a
    /// change to any later mission after the room opened is caught before GO
    /// (a guest already verified the digest the host published). Reports
    /// `Pending` until that fresh scan ends.
    pub(crate) fn host_boundary(&mut self, series: &LocalSeries) -> VmceSeriesState {
        let key = Self::key(series, None);
        if self.current.as_ref() != Some(&key) {
            self.start(key, series, true);
            return VmceSeriesState::Pending;
        }
        if self.worker.is_some() {
            return VmceSeriesState::Pending;
        }
        match self.state() {
            state @ VmceSeriesState::Failed(_) => state,
            VmceSeriesState::Verified { digest } if !self.revalidated => {
                self.start(key, series, true);
                self.boundary_expected = Some(digest);
                VmceSeriesState::Pending
            }
            state => state,
        }
    }

    /// Collect a finished worker scan. Returns true when the active identity's
    /// outcome changed; a result from a previous identity/generation is
    /// discarded, never applied.
    pub(crate) fn poll(&mut self) -> bool {
        let result = match self.results.as_ref() {
            Some(results) => match results.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(ScanResult {
                    generation: self.generation,
                    key: self.current.clone().unwrap_or(SeriesKey {
                        directory: String::new(),
                        manifest_sha256: String::new(),
                        advertised: None,
                    }),
                    outcome: ScanOutcome::Failed("VMCE series scan stopped".to_owned()),
                }),
            },
            None => None,
        };
        let Some(result) = result else {
            return false;
        };
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.results = None;
        if result.generation != self.generation || self.current.as_ref() != Some(&result.key) {
            return false;
        }
        let mut outcome = result.outcome;
        if let (Some(expected), ScanOutcome::Digest(digest)) =
            (self.boundary_expected.take(), &outcome)
            && expected != *digest
        {
            outcome =
                ScanOutcome::Failed("VMCE series changed after the host advertised it".to_owned());
        }
        self.outcome = Some(outcome);
        self.applied = false;
        true
    }

    /// Whether the finished verdict for the current identity still needs to
    /// reach the admission owner. Callers reconcile every poll pass, so a
    /// completion that lands while the setup identity is momentarily
    /// unavailable is applied on a later frame instead of being dropped.
    pub(crate) fn admission_needed(&self) -> bool {
        !self.applied && self.worker.is_none() && self.outcome.is_some()
    }

    /// The admission owner applied (or deliberately skipped) the current
    /// verdict; a following scan becomes unapplied again on completion.
    pub(crate) fn mark_admission_applied(&mut self) {
        self.applied = true;
    }

    /// Cancel any running scan, forget the cached identity and discard its
    /// result. Called on options-identity changes, room close/leave and when
    /// entering or leaving the intermission.
    pub(crate) fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        // Every scan checks cancellation before each 64 KiB chunk and each
        // map, so this only waits for the in-flight read.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.results = None;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation = self.generation.wrapping_add(1);
        self.current = None;
        self.outcome = None;
        self.revalidated = false;
        self.boundary_expected = None;
        self.applied = false;
    }

    fn start(&mut self, key: SeriesKey, series: &LocalSeries, revalidation: bool) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.results = None;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation = self.generation.wrapping_add(1);
        self.current = Some(key.clone());
        self.outcome = None;
        self.revalidated = revalidation;
        self.boundary_expected = None;
        self.applied = false;
        #[cfg(test)]
        {
            self.scans_started += 1;
        }
        let generation = self.generation;
        let cancel = self.cancel.clone();
        let directory = series.directory_path.clone();
        let manifest = series.manifest.clone();
        let (sender, receiver) = channel();
        self.results = Some(receiver);
        let spawned =
            std::thread::Builder::new()
                .name("vmce-series-scan".to_owned())
                .spawn(move || {
                    let outcome =
                        match crate::app::shell_network_setup::vmce_map::series_content_sha256(
                            &directory, &manifest, &cancel,
                        ) {
                            Ok(digest) => ScanOutcome::Digest(digest),
                            Err(error) => ScanOutcome::Failed(format!("{error:#}")),
                        };
                    let _ = sender.send(ScanResult {
                        generation,
                        key,
                        outcome,
                    });
                });
        match spawned {
            Ok(worker) => self.worker = Some(worker),
            Err(_) => {
                self.results = None;
                self.outcome = Some(ScanOutcome::Failed(
                    "VMCE series scan could not start a worker".to_owned(),
                ));
            }
        }
    }

    fn key(series: &LocalSeries, advertised: Option<&str>) -> SeriesKey {
        SeriesKey {
            directory: series.directory.clone(),
            manifest_sha256: series.manifest_sha256(),
            advertised: advertised.map(str::to_owned),
        }
    }

    #[cfg(test)]
    pub(crate) fn scanning(&self) -> bool {
        self.worker.is_some()
    }

    #[cfg(test)]
    pub(crate) fn scans_started(&self) -> usize {
        self.scans_started
    }
}

impl Drop for VmceSeriesVerifier {
    fn drop(&mut self) {
        // The scan checks cancellation before every chunk; do not block the
        // event loop on a join while the visit is being torn down.
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The files one series scan reads. `directory` is the catalog series
/// component; `directory_path` is its canonical location.
pub(crate) struct LocalSeries {
    pub(crate) directory: String,
    pub(crate) directory_path: PathBuf,
    pub(crate) manifest: Vec<u8>,
}

impl LocalSeries {
    /// The exact `campaign.ini` bytes' SHA-256, the options identity's
    /// `sha256` field.
    pub(crate) fn manifest_sha256(&self) -> String {
        crate::util::sha256::sha256_hex(&self.manifest)
    }
}

/// Read one local series' `campaign.ini` for the scan without loading a map.
pub(crate) fn read_local_series(
    state: &crate::app::AppState,
    map_file: &str,
) -> anyhow::Result<LocalSeries> {
    let series =
        crate::app::shell_network_setup::vmce_map::local_vmce_series_manifest(state, map_file)?;
    Ok(LocalSeries {
        directory: series.directory,
        directory_path: series.directory_path,
        manifest: series.manifest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::source::test_support::TestDirectory;

    fn write_series(root: &TestDirectory, maps: &[(&str, &[u8])]) {
        for (map, bytes) in maps {
            root.write(map, bytes);
        }
        root.write(
            "campaign.ini",
            format!(
                "[Campaign]\nEntry={}\nScenarioCount={}\n{}",
                maps[0].0,
                maps.len(),
                maps.iter()
                    .enumerate()
                    .map(|(index, (map, _))| format!("Scenario{}={map}\n", index + 1))
                    .collect::<String>()
            )
            .as_bytes(),
        );
    }

    fn local_series(root: &TestDirectory) -> LocalSeries {
        let directory_path = root.path().canonicalize().unwrap();
        let manifest = std::fs::read(directory_path.join("campaign.ini")).unwrap();
        LocalSeries {
            directory: root
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            directory_path,
            manifest,
        }
    }

    fn wait_settled(verifier: &mut VmceSeriesVerifier) -> VmceSeriesState {
        for _ in 0..600 {
            if verifier.poll() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        verifier.state()
    }

    fn host_digest(series: &LocalSeries) -> String {
        let mut host = VmceSeriesVerifier::new();
        assert!(host.host_digest(series).is_none());
        match wait_settled(&mut host) {
            VmceSeriesState::Verified { digest } => digest,
            other => panic!("host scan did not verify: {other:?}"),
        }
    }

    #[test]
    fn identical_series_verifies_and_reuses_the_cached_result() {
        let root = TestDirectory::new("vmce-series-verifier-cache");
        write_series(
            &root,
            &[("first.map", b"selected"), ("second.map", b"later")],
        );
        let source = local_series(&root);
        let digest = host_digest(&source);

        let mut guest = VmceSeriesVerifier::new();
        assert_eq!(
            guest.state(),
            VmceSeriesState::Idle,
            "a fresh verifier has no identity"
        );
        assert_eq!(
            guest.verify_advertised(&source, &digest),
            VmceSeriesState::Pending
        );
        assert_eq!(
            wait_settled(&mut guest),
            VmceSeriesState::Verified {
                digest: digest.clone()
            }
        );
        let started = guest.scans_started();
        // A repeated options revision of the same identity must reuse it.
        assert!(guest.verify_advertised(&source, &digest).verified());
        assert_eq!(guest.scans_started(), started);
    }

    #[test]
    fn a_differing_unselected_map_is_a_mismatch_even_when_the_selected_map_matches() {
        let host_root = TestDirectory::new("vmce-series-verifier-host");
        let guest_root = TestDirectory::new("vmce-series-verifier-guest");
        write_series(
            &host_root,
            &[("first.map", b"selected"), ("second.map", b"later")],
        );
        write_series(
            &guest_root,
            &[("first.map", b"selected"), ("second.map", b"other!")],
        );
        let host_source = local_series(&host_root);
        let digest = host_digest(&host_source);
        let guest_source = local_series(&guest_root);
        assert_eq!(
            std::fs::read(guest_source.directory_path.join("first.map")).unwrap(),
            std::fs::read(host_source.directory_path.join("first.map")).unwrap(),
            "the selected map still matches"
        );

        let mut guest = VmceSeriesVerifier::new();
        assert_eq!(
            guest.verify_advertised(&guest_source, &digest),
            VmceSeriesState::Pending
        );
        let state = wait_settled(&mut guest);
        assert!(
            matches!(state, VmceSeriesState::Mismatch { .. }),
            "an unselected map's bytes are part of the advertised identity: {state:?}"
        );
        assert!(!state.verified());
    }

    #[test]
    fn a_result_from_a_previous_series_is_discarded_after_the_identity_changes() {
        let first_root = TestDirectory::new("vmce-series-verifier-first");
        let second_root = TestDirectory::new("vmce-series-verifier-second");
        write_series(&first_root, &[("first.map", b"one")]);
        write_series(&second_root, &[("first.map", b"two")]);
        let first_source = local_series(&first_root);
        let second_source = local_series(&second_root);
        let digest = host_digest(&first_source);

        let mut guest = VmceSeriesVerifier::new();
        assert_eq!(
            guest.verify_advertised(&first_source, &digest),
            VmceSeriesState::Pending
        );
        // The room moves to another series before the first scan finishes.
        assert_eq!(
            guest.verify_advertised(&second_source, &digest),
            VmceSeriesState::Pending
        );
        let state = wait_settled(&mut guest);
        assert!(
            matches!(state, VmceSeriesState::Mismatch { .. }),
            "the current identity is the second series, whose digest differs: {state:?}"
        );
        assert_ne!(state, VmceSeriesState::Verified { digest });
    }

    #[test]
    fn cancel_stops_a_running_scan_and_discards_its_result() {
        let root = TestDirectory::new("vmce-series-verifier-cancel");
        let names: Vec<String> = (0..32).map(|index| format!("map{index}.map")).collect();
        write_series(
            &root,
            &names
                .iter()
                .map(|name| (name.as_str(), b"m".as_slice()))
                .collect::<Vec<_>>(),
        );
        // Sparse 8 MiB files keep the worker hashing long enough to cancel.
        for name in &names {
            let file = std::fs::File::create(root.path().join(name)).unwrap();
            file.set_len(crate::map::source::MAX_TRANSFER_MAP_BYTES as u64)
                .unwrap();
        }
        let source = local_series(&root);
        let mut verifier = VmceSeriesVerifier::new();
        assert!(verifier.host_digest(&source).is_none());
        assert!(verifier.scanning());
        verifier.cancel();
        assert!(
            !verifier.scanning(),
            "cancel joins the worker, so the scan cannot still be reading"
        );
        assert_eq!(verifier.state(), VmceSeriesState::Idle);
        assert!(!verifier.poll(), "a cancelled scan applies no result");
        assert!(verifier.host_digest(&source).is_none());
        assert!(verifier.scanning(), "the identity scans again after cancel");
        verifier.cancel();
    }

    #[test]
    fn pending_blocks_the_ready_boundary_until_a_revalidation_completes() {
        let root = TestDirectory::new("vmce-series-verifier-boundary");
        write_series(
            &root,
            &[("first.map", b"selected"), ("later.map", b"later")],
        );
        let source = local_series(&root);
        let digest = host_digest(&source);
        let mut guest = VmceSeriesVerifier::new();
        assert_eq!(
            guest.verify_advertised(&source, &digest),
            VmceSeriesState::Pending
        );
        assert!(wait_settled(&mut guest).verified());
        // Admission only proves the files at options time; the first Ready
        // forces a fresh scan and reports pending until it ends.
        assert_eq!(guest.boundary(&source, &digest), VmceSeriesState::Pending);
        assert!(guest.scanning());
        assert!(wait_settled(&mut guest).verified());
        assert!(guest.boundary(&source, &digest).verified());
    }

    #[test]
    fn a_changed_series_file_fails_the_ready_revalidation() {
        let root = TestDirectory::new("vmce-series-verifier-revalidate");
        write_series(
            &root,
            &[("first.map", b"selected"), ("later.map", b"later")],
        );
        let source = local_series(&root);
        let digest = host_digest(&source);
        let mut guest = VmceSeriesVerifier::new();
        assert_eq!(
            guest.verify_advertised(&source, &digest),
            VmceSeriesState::Pending
        );
        assert!(wait_settled(&mut guest).verified());
        // A later mission's bytes change after the options revision.
        root.write("later.map", b"changed after admission");
        assert_eq!(guest.boundary(&source, &digest), VmceSeriesState::Pending);
        let state = wait_settled(&mut guest);
        assert!(
            matches!(state, VmceSeriesState::Mismatch { .. }),
            "a later mission changed after admission must refuse Ready: {state:?}"
        );
        assert_eq!(guest.boundary(&source, &digest), state);
    }

    #[test]
    fn host_boundary_refuses_a_series_that_changed_after_it_was_advertised() {
        let root = TestDirectory::new("vmce-series-verifier-host-boundary");
        write_series(
            &root,
            &[("first.map", b"selected"), ("later.map", b"later")],
        );
        let source = local_series(&root);
        let mut unchanged = VmceSeriesVerifier::new();
        assert!(unchanged.host_digest(&source).is_none());
        assert!(wait_settled(&mut unchanged).verified());
        // The first Ready/Go revalidates unchanged official files and passes.
        assert_eq!(unchanged.host_boundary(&source), VmceSeriesState::Pending);
        assert!(wait_settled(&mut unchanged).verified());
        assert!(unchanged.host_boundary(&source).verified());

        // A later mission's bytes change after the digest was published; the
        // host must not start on the identity its guests verified.
        let mut host = VmceSeriesVerifier::new();
        assert!(host.host_digest(&source).is_none());
        assert!(wait_settled(&mut host).verified());
        root.write("later.map", b"changed after publish");
        assert_eq!(host.host_boundary(&source), VmceSeriesState::Pending);
        let state = wait_settled(&mut host);
        assert!(
            matches!(state, VmceSeriesState::Failed(_)),
            "the host's advertised series changed before GO: {state:?}"
        );
    }

    /// A finished scan stays unapplied until the admission owner consumes it.
    /// The LAN service reconciles every pass, so a completion that lands while
    /// the setup identity is momentarily unavailable is applied on a later
    /// frame instead of leaving the guest without a `K` answer.
    #[test]
    fn a_finished_scan_waits_for_the_admission_owner_to_apply_it() {
        let root = TestDirectory::new("vmce-series-verifier-applied");
        write_series(
            &root,
            &[("first.map", b"selected"), ("later.map", b"later")],
        );
        let source = local_series(&root);
        let digest = host_digest(&source);
        let mut guest = VmceSeriesVerifier::new();
        assert_eq!(
            guest.verify_advertised(&source, &digest),
            VmceSeriesState::Pending
        );
        assert!(
            !guest.admission_needed(),
            "a running scan has no verdict to apply"
        );
        assert!(wait_settled(&mut guest).verified());
        assert!(
            guest.admission_needed(),
            "a finished verdict must reach the admission owner"
        );
        guest.mark_admission_applied();
        assert!(!guest.admission_needed());
        // A later boundary scan becomes unapplied again when it completes.
        assert_eq!(guest.boundary(&source, &digest), VmceSeriesState::Pending);
        assert!(!guest.admission_needed());
        assert!(wait_settled(&mut guest).verified());
        assert!(guest.admission_needed());
        guest.mark_admission_applied();
        assert!(!guest.admission_needed());
        // Cancelling forgets the identity and its verdict entirely.
        guest.cancel();
        assert!(!guest.admission_needed());
    }
}
