//! A03's isolated stale-series LAN handoff diagnostic.
//!
//! Only the dedicated A03 guest checkpoint owns a temporary map mutation.
//! It waits for the production Ready request to gain a reliable admitted
//! generation, changes one unselected later map in its private fixture, then
//! restores the exact bytes before acknowledging the existing blocking notice.

use super::*;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MUTATION_COMMENT: &[u8] = b"\r\n; VERA20k A03 controlled later-map change\r\n";
const RENDEZVOUS_ENV: &str = "VERA20K_A03_RENDEZVOUS_MARKER";
const RENDEZVOUS_SCHEMA: &str = "vera20k.a03-later-map-ready.v1";
const RENDEZVOUS_WAIT_FRAMES: u32 = 1_800;
const HOST_NO_MAJORITY_DETAIL: &str =
    "initial active roster is not a majority of the original admission";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Host,
    Guest,
}

pub(super) struct A03Capture {
    role: Role,
    ready_generation_before: Option<u64>,
    ready_generation_after: Option<u64>,
    host_start_frame: Option<u32>,
    loading_observed: bool,
    gameplay_entered: bool,
    startup_failure_observed: bool,
    gameplay_active_at_notice: Option<bool>,
    native_frame_at_notice: Option<u32>,
    mutation: Option<OwnedMapMutation>,
    selected_map: Option<String>,
    campaign_path: Option<PathBuf>,
    selected_path: Option<PathBuf>,
    campaign_sha256_before: Option<String>,
    campaign_sha256_after: Option<String>,
    selected_sha256_before: Option<String>,
    selected_sha256_after: Option<String>,
    unselected_later_map: Option<String>,
    unselected_sha256_before: Option<String>,
    unselected_sha256_mutated: Option<String>,
    unselected_sha256_restored: Option<String>,
    blocking_notice: Option<String>,
    notice_frame: Option<u32>,
    notice_locked: bool,
    notice_acknowledged: bool,
    rendezvous_path: Option<PathBuf>,
    rendezvous_wait_started: Option<u32>,
    rendezvous_wait_frames: Option<u32>,
    rendezvous_marker_sha256: Option<String>,
    rendezvous_written: bool,
    rendezvous_observed: bool,
    handoff_refused: bool,
    room_exit_observed: bool,
    complete: bool,
}

impl A03Capture {
    pub(super) fn new(role: Role) -> Self {
        Self {
            role,
            ready_generation_before: None,
            ready_generation_after: None,
            host_start_frame: None,
            loading_observed: false,
            gameplay_entered: false,
            startup_failure_observed: false,
            gameplay_active_at_notice: None,
            native_frame_at_notice: None,
            mutation: None,
            selected_map: None,
            campaign_path: None,
            selected_path: None,
            campaign_sha256_before: None,
            campaign_sha256_after: None,
            selected_sha256_before: None,
            selected_sha256_after: None,
            unselected_later_map: None,
            unselected_sha256_before: None,
            unselected_sha256_mutated: None,
            unselected_sha256_restored: None,
            blocking_notice: None,
            notice_frame: None,
            notice_locked: false,
            notice_acknowledged: false,
            rendezvous_path: std::env::var_os(RENDEZVOUS_ENV).map(PathBuf::from),
            rendezvous_wait_started: None,
            rendezvous_wait_frames: None,
            rendezvous_marker_sha256: None,
            rendezvous_written: false,
            rendezvous_observed: false,
            handoff_refused: false,
            room_exit_observed: false,
            complete: false,
        }
    }

    pub(super) fn ready_request_issued(&mut self, generation: u64) -> Result<()> {
        ensure!(
            self.role == Role::Guest,
            "A03 Ready request was issued by the host"
        );
        ensure!(
            self.ready_generation_before.is_none(),
            "A03 guest issued more than one initial Ready request"
        );
        self.ready_generation_before = Some(generation);
        Ok(())
    }

    pub(super) fn host_start_clicked(&mut self, frame: u32) {
        self.host_start_frame = Some(frame);
    }

    pub(super) fn after_present(
        &mut self,
        state: &mut AppState,
        frame: u32,
        route: &mut Vec<Value>,
        screenshots: &mut Vec<String>,
    ) -> Result<bool> {
        if self.complete {
            return Ok(true);
        }
        if self.role == Role::Guest && self.mutation.is_none() {
            self.observe_ready_admission_and_mutate(state, frame, route)?;
        }
        if self.role == Role::Host && state.frontend.screen == GameScreen::Loading {
            self.loading_observed = true;
        }
        if self.role == Role::Host && self.host_start_frame.is_some_and(|start| frame > start) {
            self.gameplay_entered |= state.frontend.screen == GameScreen::InGame;
            ensure!(
                !self.gameplay_entered,
                "A03 host entered gameplay after Start"
            );
        }

        let notice = state
            .frontend
            .multiplayer_setup
            .as_ref()
            .and_then(|ui| ui.notice.as_ref())
            .map(|notice| notice.message.clone());
        if let Some(notice) = notice {
            if self.notice_frame.is_none() {
                match self.role {
                    Role::Host => {
                        ensure!(
                            self.host_start_frame.is_some() && self.rendezvous_observed,
                            "A03 host saw a startup notice before its controlled Go boundary"
                        );
                        ensure!(
                            self.loading_observed,
                            "A03 host startup notice did not follow the Loading screen"
                        );
                        ensure!(
                            state.frontend.screen == GameScreen::MainMenu,
                            "A03 host loading refusal did not return through the MainMenu shell"
                        );
                        self.native_frame_at_notice = state
                            .match_state
                            .sim_runtime
                            .runtime()
                            .map(|runtime| runtime.simulation.session().binary_frame());
                        ensure!(
                            self.native_frame_at_notice.is_none_or(|frame| frame == 0),
                            "A03 host startup refusal advanced gameplay to native frame {:?}",
                            self.native_frame_at_notice
                        );
                        self.gameplay_active_at_notice = Some(
                            self.gameplay_entered
                                || self.native_frame_at_notice.is_some_and(|frame| frame > 0),
                        );
                        ensure!(
                            !self.gameplay_entered && self.gameplay_active_at_notice == Some(false),
                            "A03 host entered gameplay after Start"
                        );
                        let expected = crate::app::loading::pump::vmce_load_failure_message(
                            crate::net::lan_messages::ingame::LoadingFailureReason::LoadFailed,
                        );
                        let mut notice_lines = notice.lines();
                        ensure!(
                            notice_lines.next() == Some(expected.as_str())
                                && notice_lines.any(|line| line == HOST_NO_MAJORITY_DETAIL),
                            "A03 host startup refusal showed an unexpected notice: {notice:?}"
                        );
                        self.notice_locked = false;
                        self.startup_failure_observed = true;
                    }
                    Role::Guest => {
                        ensure!(
                            self.mutation.is_some() && self.rendezvous_written,
                            "A03 guest saw a blocking notice before its controlled Go boundary"
                        );
                        let expected = crate::app::App::csf_label(
                            state,
                            "TXT_UNABLE_PLAY_WAAUGH",
                            "TXT_UNABLE_PLAY_WAAUGH",
                        );
                        ensure!(
                            notice == expected,
                            "A03 guest Go refusal showed an unexpected notice: {notice:?}"
                        );
                        self.notice_locked = setup::view(state).is_some_and(|view| view.locked);
                        ensure!(
                            self.notice_locked,
                            "A03 guest blocking notice did not follow a locked Go/handoff"
                        );
                        self.restore_later_map()?;
                    }
                }
                self.handoff_refused = true;
                self.blocking_notice = Some(notice.clone());
                self.notice_frame = Some(frame);
                route.push(json!({
                    "render_frame": frame,
                    "action": match self.role {
                        Role::Host => "A03 captured the host startup LoadFailed notice after the loading refusal",
                        Role::Guest => "A03 captured the production blocking notice after VMCE Go refusal",
                    },
                    "role": format!("{:?}", self.role),
                    "message": notice,
                    "locked": self.notice_locked,
                    "loading_observed": self.loading_observed,
                    "gameplay_entered": self.gameplay_entered,
                    "native_frame_at_notice": self.native_frame_at_notice,
                    "gameplay_active": self.gameplay_active_at_notice,
                }));
                screenshots.push("vera-a03-handoff-refused".to_owned());
                return Ok(false);
            }
            if !self.notice_acknowledged && frame > self.notice_frame.context("A03 notice frame")? {
                // This enters the same input owner as a player pressing Enter
                // on the visible CE dialog; it preserves the normal room exit.
                setup_input::key_press(state, Some(KeyCode::Enter), None);
                self.notice_acknowledged = state
                    .frontend
                    .multiplayer_setup
                    .as_ref()
                    .is_none_or(|ui| ui.notice.is_none());
                ensure!(
                    self.notice_acknowledged,
                    "A03 Enter did not dismiss the production blocking notice"
                );
                route.push(json!({
                    "render_frame": frame,
                    "action": "A03 dismissed the production blocking notice through setup input",
                }));
                return Ok(false);
            }
        }

        let notice_dismissed = state
            .frontend
            .multiplayer_setup
            .as_ref()
            .is_none_or(|ui| ui.notice.is_none());
        if self.notice_acknowledged
            && notice_dismissed
            && !state.frontend.shell_route.network_setup()
        {
            if self.role == Role::Guest {
                self.restore_later_map()?;
                self.verify_unchanged_inputs_after_restore()?;
            }
            self.room_exit_observed = true;
            self.complete = true;
            route.push(json!({
                "render_frame": frame,
                "action": "A03 observed the existing blocking-notice room exit",
                "role": format!("{:?}", self.role),
            }));
            return Ok(true);
        }
        if let Some(notice_frame) = self.notice_frame {
            ensure!(
                frame.saturating_sub(notice_frame) < 1_800,
                "A03 blocking notice did not return the process to the shell"
            );
        }
        Ok(false)
    }

    pub(super) fn receipt(&self) -> Result<Value> {
        ensure!(
            self.complete
                && self.handoff_refused
                && self.blocking_notice.is_some()
                && match self.role {
                    Role::Host => {
                        self.loading_observed
                            && self.startup_failure_observed
                            && !self.gameplay_entered
                            && !self.notice_locked
                            && self.gameplay_active_at_notice == Some(false)
                            && self.native_frame_at_notice.is_none_or(|frame| frame == 0)
                    }
                    Role::Guest => self.notice_locked,
                }
                && self.notice_acknowledged
                && self.room_exit_observed,
            "A03 did not observe the role-specific production refusal and room exit"
        );
        let mut receipt = json!({
            "role": match self.role { Role::Host => "host", Role::Guest => "guest" },
            "host_start_clicked": self.host_start_frame.is_some(),
            "handoff_refused": self.handoff_refused,
            "failure_phase": match self.role { Role::Host => "loading", Role::Guest => "setup" },
            "loading_observed": self.loading_observed,
            "gameplay_entered": self.gameplay_entered,
            "startup_failure_observed": self.startup_failure_observed,
            "gameplay_active_at_notice": self.gameplay_active_at_notice,
            "native_frame_at_notice": self.native_frame_at_notice,
            "blocking_notice": self.blocking_notice,
            "locked_when_notice": self.notice_locked,
            "notice_acknowledged": self.notice_acknowledged,
            "room_exit_observed": self.room_exit_observed,
            "rendezvous_marker_path": self.rendezvous_path,
            "rendezvous_marker_sha256": self.rendezvous_marker_sha256,
            "rendezvous_marker_written": self.rendezvous_written,
            "rendezvous_marker_observed": self.rendezvous_observed,
            "rendezvous_wait_frames": self.rendezvous_wait_frames,
            "rendezvous_timeout_frames": RENDEZVOUS_WAIT_FRAMES,
        });
        if self.role == Role::Guest {
            receipt["ready_generation_before"] = json!(self.ready_generation_before);
            receipt["ready_generation_after"] = json!(self.ready_generation_after);
            receipt["mutation_after_admitted_ready"] = json!(
                self.ready_generation_before
                    .zip(self.ready_generation_after)
                    .is_some_and(|(before, after)| after > before)
                    && self.unselected_sha256_mutated != self.unselected_sha256_before
            );
            receipt["selected_map"] = json!(self.selected_map);
            receipt["campaign_sha256_before"] = json!(self.campaign_sha256_before);
            receipt["campaign_sha256_after"] = json!(self.campaign_sha256_after);
            receipt["selected_sha256_before"] = json!(self.selected_sha256_before);
            receipt["selected_sha256_after"] = json!(self.selected_sha256_after);
            receipt["unselected_later_map"] = json!(self.unselected_later_map);
            receipt["unselected_sha256_before"] = json!(self.unselected_sha256_before);
            receipt["unselected_sha256_mutated"] = json!(self.unselected_sha256_mutated);
            receipt["unselected_sha256_restored"] = json!(self.unselected_sha256_restored);
        }
        Ok(receipt)
    }

    fn observe_ready_admission_and_mutate(
        &mut self,
        state: &AppState,
        frame: u32,
        route: &mut Vec<Value>,
    ) -> Result<()> {
        let Some(before) = self.ready_generation_before else {
            return Ok(());
        };
        let after = crate::app::lan_service::vmce_ready_admission_generation(state);
        if after <= before {
            return Ok(());
        }
        self.ready_generation_after = Some(after);
        let mutation = OwnedMapMutation::create(state)?;
        self.selected_map = Some(mutation.selected_map.clone());
        self.campaign_path = Some(mutation.campaign_path.clone());
        self.selected_path = Some(mutation.selected_path.clone());
        self.campaign_sha256_before = Some(mutation.campaign_sha256.clone());
        self.selected_sha256_before = Some(mutation.selected_sha256.clone());
        self.unselected_later_map = Some(mutation.relative_map.clone());
        self.unselected_sha256_before = Some(mutation.original_sha256.clone());
        self.unselected_sha256_mutated = Some(mutation.mutated_sha256.clone());
        self.campaign_sha256_after = Some(mutation.campaign_sha256_after.clone());
        self.selected_sha256_after = Some(mutation.selected_sha256_after.clone());
        route.push(json!({
            "render_frame": frame,
            "action": "A03 observed successful production Ready queue admission, then changed only an unselected later map",
            "ready_generation_before": before,
            "ready_generation_after": after,
            "selected_map": mutation.selected_map,
            "unselected_later_map": mutation.relative_map,
            "unselected_sha256_before": mutation.original_sha256,
            "unselected_sha256_mutated": mutation.mutated_sha256,
            "campaign_sha256_before": mutation.campaign_sha256,
            "campaign_sha256_after": mutation.campaign_sha256_after,
            "selected_sha256_before": mutation.selected_sha256,
            "selected_sha256_after": mutation.selected_sha256_after,
        }));
        self.mutation = Some(mutation);
        self.write_rendezvous_marker(frame, route)?;
        Ok(())
    }

    /// Host gate called from its ordinary setup loop before the production
    /// Start button. The target reads only the run's explicit rendezvous file.
    pub(super) fn wait_for_guest_mutation(&mut self, frame: u32) -> Result<bool> {
        ensure!(self.role == Role::Host, "A03 rendezvous wait is host-only");
        if self.rendezvous_observed {
            return Ok(true);
        }
        let started = *self.rendezvous_wait_started.get_or_insert(frame);
        let marker_path = self
            .rendezvous_path
            .as_ref()
            .context("A03 runner did not provide its per-scenario rendezvous path")?;
        ensure!(
            marker_path.is_absolute(),
            "A03 rendezvous marker path must be absolute"
        );
        if !marker_path.is_file() {
            ensure!(
                frame.saturating_sub(started) < RENDEZVOUS_WAIT_FRAMES,
                "A03 host timed out waiting for the guest's admitted-Ready mutation marker"
            );
            return Ok(false);
        }
        let bytes = fs::read(marker_path).context("read A03 guest mutation marker")?;
        let marker: Value = serde_json::from_slice(&bytes).context("parse A03 mutation marker")?;
        ensure!(
            marker.get("schema").and_then(Value::as_str) == Some(RENDEZVOUS_SCHEMA),
            "A03 mutation marker has the wrong schema"
        );
        let before = marker
            .get("ready_generation_before")
            .and_then(Value::as_u64)
            .context("A03 marker Ready generation before")?;
        let after = marker
            .get("ready_generation_after")
            .and_then(Value::as_u64)
            .context("A03 marker Ready generation after")?;
        ensure!(after > before, "A03 marker precedes admitted guest Ready");
        let selected = marker
            .get("selected_map")
            .and_then(Value::as_str)
            .context("A03 marker selected map")?;
        let unselected = marker
            .get("unselected_later_map")
            .and_then(Value::as_str)
            .context("A03 marker unselected later map")?;
        ensure!(
            normalized_map_name(selected) != normalized_map_name(unselected),
            "A03 mutation marker names the selected map as its later map"
        );
        for (before_key, after_key) in [
            ("campaign_sha256_before", "campaign_sha256_after"),
            ("selected_sha256_before", "selected_sha256_after"),
        ] {
            ensure!(
                marker.get(before_key) == marker.get(after_key),
                "A03 marker reports changed selected or manifest bytes"
            );
        }
        ensure!(
            marker.get("unselected_sha256_before") != marker.get("unselected_sha256_mutated"),
            "A03 marker did not change the unselected later map"
        );
        self.rendezvous_wait_frames = Some(frame.saturating_sub(started));
        self.rendezvous_marker_sha256 = Some(sha256_bytes(&bytes));
        self.rendezvous_observed = true;
        Ok(true)
    }

    fn write_rendezvous_marker(&mut self, frame: u32, route: &mut Vec<Value>) -> Result<()> {
        ensure!(
            self.role == Role::Guest,
            "A03 rendezvous marker may be written only by the guest"
        );
        ensure!(
            !self.rendezvous_written,
            "A03 guest tried to publish the mutation marker twice"
        );
        let path = self
            .rendezvous_path
            .as_ref()
            .context("A03 runner did not provide its per-scenario rendezvous path")?;
        ensure!(
            path.is_absolute(),
            "A03 rendezvous marker path must be absolute"
        );
        let parent = path.parent().context("A03 marker parent directory")?;
        ensure!(parent.is_dir(), "A03 marker parent directory is missing");
        ensure!(!path.exists(), "A03 rendezvous marker already exists");
        let bytes = serde_json::to_vec(&json!({
            "schema": RENDEZVOUS_SCHEMA,
            "ready_generation_before": self.ready_generation_before,
            "ready_generation_after": self.ready_generation_after,
            "selected_map": self.selected_map,
            "unselected_later_map": self.unselected_later_map,
            "campaign_sha256_before": self.campaign_sha256_before,
            "campaign_sha256_after": self.campaign_sha256_after,
            "selected_sha256_before": self.selected_sha256_before,
            "selected_sha256_after": self.selected_sha256_after,
            "unselected_sha256_before": self.unselected_sha256_before,
            "unselected_sha256_mutated": self.unselected_sha256_mutated,
            "mutation_after_admitted_ready": true,
            "render_frame": frame,
        }))?;
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        ensure!(
            !temporary.exists(),
            "A03 temporary rendezvous marker already exists"
        );
        let write_result = (|| -> Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, path).context("publish A03 mutation marker")?;
            ensure!(
                fs::read(path)? == bytes,
                "A03 mutation marker did not read back exactly"
            );
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result?;
        self.rendezvous_marker_sha256 = Some(sha256_bytes(&bytes));
        self.rendezvous_written = true;
        route.push(json!({
            "render_frame": frame,
            "action": "A03 atomically published the guest mutation marker for the host Start gate",
            "marker": path,
            "marker_sha256": self.rendezvous_marker_sha256,
        }));
        Ok(())
    }

    fn restore_later_map(&mut self) -> Result<()> {
        if let Some(mutation) = self.mutation.as_mut()
            && self.unselected_sha256_restored.is_none()
        {
            self.unselected_sha256_restored = Some(mutation.restore()?);
        }
        Ok(())
    }

    fn verify_unchanged_inputs_after_restore(&mut self) -> Result<()> {
        let campaign = self.campaign_path.as_ref().context("A03 campaign path")?;
        let selected = self
            .selected_path
            .as_ref()
            .context("A03 selected map path")?;
        let campaign_after = sha256_file(campaign)?;
        let selected_after = sha256_file(selected)?;
        ensure!(
            self.campaign_sha256_before.as_deref() == Some(campaign_after.as_str()),
            "A03 campaign.ini changed during the controlled mutation"
        );
        ensure!(
            self.selected_sha256_before.as_deref() == Some(selected_after.as_str()),
            "A03 selected map changed during the controlled mutation"
        );
        self.campaign_sha256_after = Some(campaign_after);
        self.selected_sha256_after = Some(selected_after);
        Ok(())
    }
}

struct OwnedMapMutation {
    path: PathBuf,
    series_root: PathBuf,
    original_bytes: Vec<u8>,
    mutated_bytes: Vec<u8>,
    selected_map: String,
    campaign_path: PathBuf,
    selected_path: PathBuf,
    relative_map: String,
    campaign_sha256: String,
    campaign_sha256_after: String,
    selected_sha256: String,
    selected_sha256_after: String,
    original_sha256: String,
    mutated_sha256: String,
    restored_sha256: Option<String>,
}

impl OwnedMapMutation {
    fn create(state: &AppState) -> Result<Self> {
        let launch = state
            .match_state
            .session
            .network_setup()
            .context("A03 VMCE guest setup")?;
        ensure!(
            !state.match_state.session.host(),
            "A03 file mutation is guest-only"
        );
        let selected_map = launch
            .selected_map_file
            .as_deref()
            .context("A03 selected VMCE map")?
            .to_owned();
        let series = crate::app::shell_network_setup::vmce_map::local_vmce_series_manifest(
            state,
            &selected_map,
        )?;
        let manifest = crate::rules::ini_parser::IniFile::from_bytes_preserving_empty_values(
            &series.manifest,
        )?;
        let campaign = manifest
            .section("Campaign")
            .context("A03 campaign.ini [Campaign]")?;
        let count = campaign.read_int("ScenarioCount", 0);
        ensure!(
            (2..=512).contains(&count),
            "A03 requires at least two manifest maps"
        );
        let normalized_selected = relative_map_from_catalog(&selected_map, &series.directory)?;
        let mut maps = Vec::with_capacity(count as usize);
        for index in 1..=count {
            let key = format!("Scenario{index}");
            let map = campaign.read_string(&key, "", 1024).trim().to_owned();
            crate::map::source::validate_network_map_name(&map)
                .with_context(|| format!("A03 invalid {key}"))?;
            maps.push(map);
        }
        let selected_index = maps
            .iter()
            .position(|map| normalized_map_name(map) == normalized_map_name(&normalized_selected))
            .context("A03 selected map is absent from its campaign manifest")?;
        let relative_map = maps
            .get(selected_index + 1)
            .context("A03 selected map has no unselected later stage")?
            .clone();
        ensure!(
            normalized_map_name(&relative_map) != normalized_map_name(&normalized_selected),
            "A03 later map is the selected map"
        );

        let series_root = series.directory_path;
        let campaign_path = checked_series_file(&series_root, "campaign.ini")?;
        let selected_path = checked_series_file(&series_root, &normalized_selected)?;
        let path = checked_series_file(&series_root, &relative_map)?;
        let campaign_sha256 = sha256_bytes(&fs::read(&campaign_path)?);
        let selected_sha256 = sha256_bytes(&fs::read(&selected_path)?);
        let original_bytes = fs::read(&path)?;
        let original_sha256 = sha256_bytes(&original_bytes);
        let mut mutated_bytes = original_bytes.clone();
        mutated_bytes.extend_from_slice(MUTATION_COMMENT);
        let mutated_sha256 = sha256_bytes(&mutated_bytes);
        ensure!(
            mutated_sha256 != original_sha256,
            "A03 later-map comment did not change its bytes"
        );
        let mut guard = Self {
            path,
            series_root,
            original_bytes,
            mutated_bytes,
            selected_map,
            campaign_path,
            selected_path,
            relative_map,
            campaign_sha256: campaign_sha256.clone(),
            campaign_sha256_after: String::new(),
            selected_sha256: selected_sha256.clone(),
            selected_sha256_after: String::new(),
            original_sha256,
            mutated_sha256,
            restored_sha256: None,
        };
        fs::write(&guard.path, &guard.mutated_bytes)
            .context("A03 append controlled later-map comment")?;
        ensure!(
            fs::read(&guard.path)? == guard.mutated_bytes,
            "A03 later-map mutation did not read back exactly"
        );
        let campaign_sha256_after = sha256_file(&guard.campaign_path)?;
        let selected_sha256_after = sha256_file(&guard.selected_path)?;
        ensure!(
            campaign_sha256_after == campaign_sha256,
            "A03 mutation changed campaign.ini"
        );
        ensure!(
            selected_sha256_after == selected_sha256,
            "A03 mutation changed the selected stage map"
        );
        guard.campaign_sha256_after = campaign_sha256_after;
        guard.selected_sha256_after = selected_sha256_after;
        Ok(guard)
    }

    fn restore(&mut self) -> Result<String> {
        if let Some(restored) = &self.restored_sha256 {
            return Ok(restored.clone());
        }
        ensure_safe_regular_file(&self.path, &self.series_root)?;
        let current = fs::read(&self.path)?;
        ensure!(
            current == self.mutated_bytes || current == self.original_bytes,
            "A03 later-map bytes changed outside the owned mutation; preserving them"
        );
        if current != self.original_bytes {
            fs::write(&self.path, &self.original_bytes).context("restore A03 later-map bytes")?;
        }
        let restored = fs::read(&self.path)?;
        ensure!(
            restored == self.original_bytes,
            "A03 later-map byte restoration did not read back exactly"
        );
        let hash = sha256_bytes(&restored);
        ensure!(
            hash == self.original_sha256,
            "A03 later-map restored hash differs from its original bytes"
        );
        self.restored_sha256 = Some(hash.clone());
        Ok(hash)
    }
}

impl Drop for OwnedMapMutation {
    fn drop(&mut self) {
        if self.restored_sha256.is_none()
            && let Err(error) = self.restore()
        {
            log::warn!("A03 fixture map cleanup: {error:#}");
        }
    }
}

fn relative_map_from_catalog(map_file: &str, directory: &str) -> Result<String> {
    let normalized = map_file.replace('\\', "/");
    let prefix = format!("VMCE/{directory}/");
    ensure!(
        normalized
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(&prefix)),
        "A03 selected map does not name its VMCE series directory"
    );
    let relative = normalized[prefix.len()..].to_owned();
    crate::map::source::validate_network_map_name(&relative)?;
    Ok(relative)
}

fn normalized_map_name(map: &str) -> String {
    map.replace('\\', "/").to_ascii_lowercase()
}

fn checked_series_file(series_root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = relative.replace('\\', "/");
    crate::map::source::validate_network_map_name(&relative)?;
    let candidate = series_root.join(&relative);
    ensure_safe_regular_file(&candidate, series_root)?;
    Ok(candidate.canonicalize()?)
}

fn ensure_safe_regular_file(path: &Path, series_root: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("A03 fixture file is missing: {}", path.display()))?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "A03 fixture path is not a regular file: {}",
        path.display()
    );
    let canonical = path.canonicalize()?;
    let root = series_root.canonicalize()?;
    ensure!(
        canonical.starts_with(&root),
        "A03 fixture file escapes the private series directory"
    );
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    Ok(sha256_bytes(&fs::read(path)?))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    crate::util::sha256::sha256_hex(bytes)
}
