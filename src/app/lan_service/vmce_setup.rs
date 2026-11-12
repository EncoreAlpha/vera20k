//! LAN VMCE setup verification and request-scoped start boundaries.
//!
//! The LAN service owns this setup state for the lifetime of its scope, even
//! while the shell protocol is replaced during handoff. MatchSession remains
//! authoritative for editable setup, readiness, and map admission.

use crate::app::match_runtime::session::SessionMode;
use crate::app::shell_network_setup::NetworkSetupRequest;
use crate::app::shell_network_setup::{HostOptions, VmceLobbyStatus};
use crate::net::lan_messages::SessionScope;
use crate::skirmish_launch::SkirmishLaunchSession;

use super::{AppState, vmce_series};

pub(super) mod options;

pub(crate) use super::vmce_series::VmceSeriesState;
pub(super) use options::{
    VmceRoleRow, VmceSeriesIdentity, intermission_unlock_earned, reconcile_finished_series_verdict,
    report_coop_campaign_unavailable, series_lobby_status, unlock_verified_intermission,
    validate_current_coop_campaign, validate_options_series_and_roles,
};

/// VMCE LAN map admission comes from its local series scan; the legacy map
/// transfer owner must retire transfers and ignore packets for that setup.
pub(super) fn uses_series_map_admission(mode: Option<crate::skirmish_modes::MpModeClass>) -> bool {
    mode.is_some_and(|class| class.is_vmce())
}

/// VMCE LAN assets come from each peer's local campaign series, not the
/// legacy preview/map transfer exchange.
pub(in crate::app) fn uses_local_series_map_assets(
    session_mode: SessionMode,
    mode: Option<crate::skirmish_modes::MpModeClass>,
) -> bool {
    session_mode == SessionMode::Lan && uses_series_map_admission(mode)
}

/// Consume old preview/file requests for VMCE LAN. The campaign manifest and
/// local series admission own those assets instead of the legacy transfer.
pub(super) fn consumes_legacy_map_asset_request(
    session_mode: SessionMode,
    mode: Option<crate::skirmish_modes::MpModeClass>,
    request: &NetworkSetupRequest,
) -> bool {
    uses_local_series_map_assets(session_mode, mode)
        && matches!(
            request,
            NetworkSetupRequest::RequestPreview
                | NetworkSetupRequest::RequestRandomMapImage
                | NetworkSetupRequest::OfferPreview
                | NetworkSetupRequest::SendPreviewFile
                | NetworkSetupRequest::AcknowledgePreview
                | NetworkSetupRequest::RequestPreviewFile
        )
}

pub(in crate::app) struct LocalMapAdmissionPolicy {
    pub(in crate::app) admission: crate::app::match_runtime::session::NetworkMapAdmission,
    pub(in crate::app) report_missing_vmce_map: bool,
}

/// VMCE LAN cannot turn a missing local campaign map into a transfer status;
/// the same decision also controls its lobby-specific missing-map message.
pub(in crate::app) fn local_map_admission_policy(
    session_mode: SessionMode,
    mode: Option<crate::skirmish_modes::MpModeClass>,
    admission: crate::app::match_runtime::session::NetworkMapAdmission,
) -> LocalMapAdmissionPolicy {
    use crate::app::match_runtime::session::NetworkMapAdmission;

    let vmce_lan = session_mode == SessionMode::Lan && uses_series_map_admission(mode);
    let admission = if vmce_lan && admission == NetworkMapAdmission::NeedsTransfer {
        NetworkMapAdmission::Unavailable
    } else {
        admission
    };
    LocalMapAdmissionPolicy {
        admission,
        report_missing_vmce_map: vmce_lan && admission == NetworkMapAdmission::Unavailable,
    }
}

pub(super) struct PublishedOptionsProjection {
    pub(super) roles: Vec<VmceRoleRow>,
    pub(super) series: VmceSeriesIdentity,
    pub(super) series_content: String,
    pub(super) identity: SeriesIdentity,
}

pub(super) enum OptionsPublication {
    NotVmce,
    Pending,
    Ready(PublishedOptionsProjection),
}

/// Bind all VMCE-only fields of one host snapshot to the current selected-map
/// identity. The shared serializer preserves field order and wire encoding.
pub(super) fn published_options_projection(
    state: &mut AppState,
) -> anyhow::Result<OptionsPublication> {
    let Some(launch) = state
        .match_state
        .session
        .network_setup()
        .filter(|launch| launch.mode.class.is_vmce())
        .cloned()
    else {
        return Ok(OptionsPublication::NotVmce);
    };
    let Some((series, series_content)) = options::published_series_identity(state)? else {
        return Ok(OptionsPublication::Pending);
    };
    let selected_map_file = launch
        .selected_map_file
        .ok_or_else(|| anyhow::anyhow!("VMCE has no selected map"))?;
    let metadata = state
        .match_state
        .session
        .network_map_metadata()
        .ok_or_else(|| anyhow::anyhow!("VMCE map metadata missing"))?;
    let identity = SeriesIdentity {
        selected_map_file: selected_map_file.clone(),
        source_sha256: metadata.source_sha256.clone(),
        effective_digest: metadata.effective_digest.clone(),
        directory: series.directory.clone(),
        manifest_sha256: series.sha256.clone(),
        content_sha256: series_content.clone(),
    };
    let roles = launch
        .humans
        .iter()
        .filter_map(|human| {
            Some(VmceRoleRow {
                row: human.raw_slot_index?,
                role: human.role,
            })
        })
        .collect();
    Ok(OptionsPublication::Ready(PublishedOptionsProjection {
        roles,
        series,
        series_content,
        identity,
    }))
}

pub(super) struct ReceivedOptionsEffect {
    local_status: Option<VmceLobbyStatus>,
    local_config_missing: bool,
    selected_map_file: String,
    series: Option<VmceSeriesIdentity>,
    series_content: Option<String>,
    intermission: Option<String>,
}

/// Resolve the guest's VMCE config and start its local series admission before
/// the generic HostSetup transaction commits. The returned effect carries
/// only presentation/admission work that must follow that commit.
pub(super) fn prepare_received_options(
    state: &mut AppState,
    options: &mut HostOptions,
    selected_map_file: String,
    source_sha256: &str,
    series: Option<VmceSeriesIdentity>,
    series_content: Option<String>,
    intermission: Option<String>,
) -> ReceivedOptionsEffect {
    let vmce = options.launch.mode.class.is_vmce();
    let (local_config, local_status) = if vmce {
        options::resolve_received_series_config(
            state,
            &selected_map_file,
            source_sha256,
            series.as_ref(),
        )
    } else {
        (None, None)
    };
    let local_config_missing = vmce && local_config.is_none();
    options.vmce_map_config = local_config;
    if vmce && !state.match_state.session.host() {
        observe_current_setup(state);
        prepare_received_series_admission(
            state,
            &selected_map_file,
            source_sha256,
            &options.map_metadata.effective_digest,
            options.vmce_map_config.as_ref(),
            series.as_ref(),
            series_content.as_deref(),
        );
    }
    let previous_vmce = state
        .match_state
        .session
        .network_setup()
        .is_some_and(|launch| launch.mode.class.is_vmce());
    if vmce || previous_vmce {
        super::discard_deferred_vmce_map_grants(state);
    }
    ReceivedOptionsEffect {
        local_status,
        local_config_missing,
        selected_map_file,
        series,
        series_content,
        intermission,
    }
}

/// Apply VMCE lobby/admission effects after the generic HostSetup transaction
/// has accepted the matching options snapshot.
pub(super) fn commit_received_options(
    state: &mut AppState,
    effect: ReceivedOptionsEffect,
) -> anyhow::Result<()> {
    if let Some(map) = effect.intermission.as_deref() {
        state
            .match_state
            .session
            .accept_vmce_intermission_map(map)?;
    }
    if let Some(protocol) = state.frontend.network_setup_protocol.as_mut() {
        protocol.reset_vmce_lobby_options(effect.local_status);
    }
    observe_current_setup(state);
    if effect.local_config_missing && !state.match_state.session.host() {
        options::report_coop_campaign_unavailable(state);
    }
    let verification = if let (Some(identity), Some(content)) =
        (effect.series.as_ref(), effect.series_content.as_deref())
    {
        options::admit_received_series(state, &effect.selected_map_file, identity, content)
    } else {
        None
    };
    if retry_allowed_after_options(!state.match_state.session.host(), verification.as_ref())
        && let Some(protocol) = state.frontend.network_setup_protocol.as_mut()
    {
        protocol.allow_vmce_retry();
    }
    Ok(())
}

fn retry_allowed_after_options(guest: bool, verification: Option<&super::VmceSeriesState>) -> bool {
    guest
        && matches!(
            verification,
            Some(super::VmceSeriesState::Mismatch { .. } | super::VmceSeriesState::Failed(_))
        )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SeriesIdentity {
    pub(super) selected_map_file: String,
    pub(super) source_sha256: String,
    pub(super) effective_digest: String,
    pub(super) directory: String,
    pub(super) manifest_sha256: String,
    pub(super) content_sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundaryKind {
    Ready,
    HostGo,
    GuestPrepare,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TicketBinding {
    Current,
    PendingPublication,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SetupMaterial {
    scope: Option<SessionScope>,
    selected_map_file: Option<String>,
    local_map_source: Option<crate::map::source::LoadedMapSource>,
    source_sha256: Option<String>,
    effective_digest: Option<String>,
    series: Option<SeriesIdentity>,
    setup: SkirmishLaunchSession,
    map_config: Option<crate::app::match_bootstrap::NetworkVmceMapConfig>,
    intermission_map: Option<String>,
    host: bool,
    preadmitted: bool,
}

impl SetupMaterial {
    /// The editable setup captured by a Start action. A Go may cross a scope
    /// only through its explicitly identified final Options publication; all
    /// player-editable material must remain identical across that boundary.
    fn same_editable_setup(&self, other: &Self) -> bool {
        self.selected_map_file == other.selected_map_file
            && self.local_map_source == other.local_map_source
            && self.source_sha256 == other.source_sha256
            && self.effective_digest == other.effective_digest
            && self.setup == other.setup
            && self.map_config == other.map_config
            && self.intermission_map == other.intermission_map
            && self.host == other.host
            && self.preadmitted == other.preadmitted
    }

    /// A completed map admission remains valid across Options revisions only
    /// while both the advertised whole-series identity and local typed map
    /// config are unchanged. Scope and role edits still invalidate boundaries.
    fn same_series_admission(&self, other: &Self) -> bool {
        self.series.is_some()
            && self.series == other.series
            && self.map_config.is_some()
            && self.map_config == other.map_config
            && self.selected_map_file == other.selected_map_file
            && self.local_map_source == other.local_map_source
            && self.source_sha256 == other.source_sha256
            && self.effective_digest == other.effective_digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SetupIdentity {
    revision: u64,
    material: SetupMaterial,
}

/// One user boundary request, captured when it first enters the LAN FIFO.
/// The envelope keeps it through `mem::take` and every defer/retry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BoundaryTicket {
    id: u64,
    kind: BoundaryKind,
    identity: SetupIdentity,
    publication_dependency: Option<u64>,
    validated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BoundaryDecision {
    NotApplicable,
    Pending,
    Verified,
    Refused(super::VmceSeriesState),
    Stale,
}

#[derive(Default)]
struct ActiveBoundary {
    ticket: Option<BoundaryTicket>,
}

struct AdmissionSnapshot {
    material: SetupMaterial,
    verdict: super::VmceSeriesState,
    applied: bool,
}

pub(super) struct VmceSetupState {
    verifier: vmce_series::VmceSeriesVerifier,
    admission: Option<AdmissionSnapshot>,
    next_request_id: u64,
    material_revision: u64,
    invalidated_through_id: u64,
    observed: Option<SetupMaterial>,
    active: ActiveBoundary,
    current_handoff_id: Option<u64>,
    published: Option<(SessionScope, SeriesIdentity)>,
    pending_publication_id: Option<u64>,
    last_published_request_id: Option<u64>,
    admitted_ready_generation: u64,
}

impl Default for VmceSetupState {
    fn default() -> Self {
        Self::new()
    }
}

impl VmceSetupState {
    pub(super) fn new() -> Self {
        Self {
            verifier: vmce_series::VmceSeriesVerifier::new(),
            admission: None,
            next_request_id: 1,
            material_revision: 0,
            invalidated_through_id: 0,
            observed: None,
            active: ActiveBoundary::default(),
            current_handoff_id: None,
            published: None,
            pending_publication_id: None,
            last_published_request_id: None,
            admitted_ready_generation: 0,
        }
    }

    pub(super) fn state(&self) -> super::VmceSeriesState {
        self.verifier.state()
    }

    pub(super) fn latest_request_id(&self) -> u64 {
        self.next_request_id.saturating_sub(1)
    }

    pub(super) fn poll(&mut self) -> bool {
        let boundary_request_id = self.verifier.boundary_request_id();
        let finished = self.verifier.poll();
        if finished {
            let boundary_is_current = boundary_request_id.is_some_and(|request_id| {
                self.active.ticket.as_ref().is_some_and(|ticket| {
                    ticket.id == request_id
                        && ticket.identity.revision == self.material_revision
                        && self
                            .observed
                            .as_ref()
                            .is_some_and(|material| ticket.identity.material == *material)
                })
            });
            if boundary_request_id.is_none() || boundary_is_current {
                let verdict = self.verifier.state();
                if let (Some(admission), Some(material)) =
                    (self.admission.as_mut(), self.observed.as_ref())
                    && admission.material.same_series_admission(material)
                {
                    admission.verdict = verdict;
                    admission.applied = false;
                }
            }
        }
        finished
    }

    pub(super) fn admission_needed(&self) -> bool {
        self.admission.as_ref().is_some_and(|admission| {
            !admission.applied
                && self
                    .observed
                    .as_ref()
                    .is_some_and(|material| admission.material.same_series_admission(material))
                && matches!(
                    admission.verdict,
                    super::VmceSeriesState::Verified { .. }
                        | super::VmceSeriesState::Mismatch { .. }
                        | super::VmceSeriesState::Failed(_)
                )
        })
    }

    pub(super) fn mark_admission_applied(&mut self) {
        if let Some(admission) = self.admission.as_mut() {
            admission.applied = true;
        }
    }

    pub(super) fn needs_admission_scan(&self) -> bool {
        self.admission.as_ref().is_some_and(|admission| {
            matches!(admission.verdict, super::VmceSeriesState::Pending)
                && self.verifier.state() == super::VmceSeriesState::Idle
        })
    }

    pub(super) fn record_admission_verdict(&mut self, verdict: &super::VmceSeriesState) {
        let Some(material) = self.observed.clone() else {
            return;
        };
        match self.admission.as_mut() {
            Some(admission)
                if admission.material.same_series_admission(&material)
                    && admission.verdict == *verdict => {}
            Some(admission) if admission.material.same_series_admission(&material) => {
                admission.verdict = verdict.clone();
                admission.applied = false;
            }
            _ => {
                self.admission = Some(AdmissionSnapshot {
                    material,
                    verdict: verdict.clone(),
                    applied: false,
                });
            }
        }
    }

    pub(super) fn admission_state_for(
        &self,
        session: &crate::app::match_runtime::session::MatchSession,
    ) -> super::VmceSeriesState {
        let Some(admission) = self.admission.as_ref() else {
            return super::VmceSeriesState::Idle;
        };
        let Some(launch) = session.network_setup() else {
            return super::VmceSeriesState::Idle;
        };
        let Some(metadata) = session.network_map_metadata() else {
            return super::VmceSeriesState::Idle;
        };
        let Some(series) = admission.material.series.as_ref() else {
            return super::VmceSeriesState::Idle;
        };
        let selected_map_file = launch.selected_map_file.as_deref();
        if session.host()
            || !launch.mode.class.is_vmce()
            || admission.material.map_config.as_ref() != session.network_vmce_map_config()
            || admission.material.selected_map_file.as_deref() != selected_map_file
            || admission.material.local_map_source.as_ref() != session.network_local_map_source()
            || admission.material.source_sha256.as_deref() != Some(metadata.source_sha256.as_str())
            || admission.material.effective_digest.as_deref()
                != Some(metadata.effective_digest.as_str())
            || selected_map_file != Some(series.selected_map_file.as_str())
            || series.source_sha256 != metadata.source_sha256
            || series.effective_digest != metadata.effective_digest
        {
            return super::VmceSeriesState::Idle;
        }
        admission.verdict.clone()
    }

    pub(super) fn map_admission_verified(
        &self,
        session: &crate::app::match_runtime::session::MatchSession,
    ) -> Option<bool> {
        let Some(launch) = session.network_setup() else {
            return None;
        };
        if !launch.mode.class.is_vmce() {
            return None;
        }
        if session.network_vmce_map_config().is_none() {
            return Some(false);
        }
        let verdict = if session.host() {
            self.verifier.state()
        } else {
            self.admission_state_for(session)
        };
        Some(verdict.verified())
    }

    #[cfg(test)]
    pub(super) fn scans_started(&self) -> usize {
        self.verifier.scans_started()
    }

    pub(super) fn host_digest(&mut self, series: &vmce_series::LocalSeries) -> Option<String> {
        self.verifier.host_digest(series)
    }

    pub(super) fn admitted_ready_generation(&self) -> u64 {
        self.admitted_ready_generation
    }

    pub(super) fn verify_advertised(
        &mut self,
        series: &vmce_series::LocalSeries,
        advertised: &str,
    ) -> super::VmceSeriesState {
        let requested = self.verifier.verify_advertised(series, advertised);
        match (requested, self.verifier.state()) {
            (super::VmceSeriesState::Pending, failed @ super::VmceSeriesState::Failed(_)) => failed,
            (requested, _) => requested,
        }
    }

    /// Bind the local scan result to the Options revision only after that
    /// complete payload has admission in the reliable transport queue.
    pub(super) fn record_published(
        &mut self,
        scope: SessionScope,
        identity: SeriesIdentity,
        publication_request_id: Option<u64>,
    ) {
        self.published = Some((scope, identity.clone()));
        self.last_published_request_id = publication_request_id;
        if self.pending_publication_id == publication_request_id {
            self.pending_publication_id = None;
        }
        let Some(mut material) = self.observed.clone() else {
            return;
        };
        if material.selected_map_file.as_deref() == Some(identity.selected_map_file.as_str())
            && material.source_sha256.as_deref() == Some(identity.source_sha256.as_str())
            && material.effective_digest.as_deref() == Some(identity.effective_digest.as_str())
        {
            // Host digest completion is the evidence used to construct this
            // admitted Options snapshot. Keep its completed scan; invalidate
            // only any request grant bound to the prior unpublished revision.
            self.material_revision = self.material_revision.wrapping_add(1);
            self.active.ticket = None;
            material.scope = Some(scope);
            material.series = Some(identity);
            self.observed = Some(material);
        }
    }

    pub(super) fn reset(&mut self) {
        self.verifier.cancel();
        self.admission = None;
        self.invalidated_through_id = self.next_request_id.saturating_sub(1);
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        self.material_revision = self.material_revision.wrapping_add(1);
        self.observed = None;
        self.active.ticket = None;
        self.current_handoff_id = None;
        self.published = None;
        self.pending_publication_id = None;
        self.last_published_request_id = None;
    }

    fn observe(&mut self, material: Option<SetupMaterial>) -> Option<SetupIdentity> {
        if self.observed != material {
            let preserves_admission = self
                .observed
                .as_ref()
                .zip(material.as_ref())
                .is_some_and(|(previous, current)| previous.same_series_admission(current));
            if !preserves_admission {
                self.verifier.cancel();
                self.admission = None;
            } else if self.verifier.boundary_request_id().is_some() {
                // A request-scoped scan belongs to its old material revision.
                // Keep only the independently cached map-admission verdict.
                self.verifier.cancel();
            }
            self.material_revision = self.material_revision.wrapping_add(1);
            self.observed = material.clone();
            self.active.ticket = None;
        }
        material.map(|material| SetupIdentity {
            revision: self.material_revision,
            material,
        })
    }

    fn enqueue_boundary(
        &mut self,
        id: u64,
        kind: BoundaryKind,
        identity: SetupIdentity,
        publication_dependency: Option<u64>,
    ) -> BoundaryTicket {
        self.current_handoff_id = match kind {
            BoundaryKind::HostGo | BoundaryKind::GuestPrepare => Some(id),
            BoundaryKind::Ready => self.current_handoff_id,
        };
        BoundaryTicket {
            id,
            kind,
            identity,
            publication_dependency,
            validated: false,
        }
    }

    fn cancel_through(&mut self, request_id: u64) {
        self.invalidated_through_id = self.invalidated_through_id.max(request_id);
        if self
            .active
            .ticket
            .as_ref()
            .is_some_and(|active| active.id <= request_id)
        {
            self.verifier.cancel();
            self.active.ticket = None;
        }
    }

    fn abandon_publication(&mut self, request_id: u64) {
        if self.pending_publication_id == Some(request_id) {
            self.pending_publication_id = None;
        }
    }

    fn bind_ticket(
        &self,
        ticket: &mut BoundaryTicket,
        current: &SetupIdentity,
        kind: BoundaryKind,
    ) -> TicketBinding {
        if ticket.id <= self.invalidated_through_id || ticket.kind != kind {
            return TicketBinding::Stale;
        }
        if matches!(kind, BoundaryKind::HostGo | BoundaryKind::GuestPrepare)
            && !self.current_handoff(ticket.id)
        {
            return TicketBinding::Stale;
        }
        if let Some(dependency) = ticket.publication_dependency {
            if self.last_published_request_id == Some(dependency) {
                let series_matches_material =
                    current.material.series.as_ref().is_some_and(|series| {
                        current.material.selected_map_file.as_deref()
                            == Some(series.selected_map_file.as_str())
                            && current.material.source_sha256.as_deref()
                                == Some(series.source_sha256.as_str())
                            && current.material.effective_digest.as_deref()
                                == Some(series.effective_digest.as_str())
                    });
                if !ticket
                    .identity
                    .material
                    .same_editable_setup(&current.material)
                    || !series_matches_material
                {
                    return TicketBinding::Stale;
                }
                ticket.identity = current.clone();
                // This is a one-time binding to the exact publication that
                // belonged to this already-enqueued Start request.
                ticket.publication_dependency = None;
            } else if self.pending_publication_id == Some(dependency) {
                return TicketBinding::PendingPublication;
            } else {
                return TicketBinding::Stale;
            }
        }
        if ticket.identity == *current {
            TicketBinding::Current
        } else {
            TicketBinding::Stale
        }
    }

    fn verify_series_for_boundary(
        &mut self,
        ticket: &mut BoundaryTicket,
        series: &vmce_series::LocalSeries,
    ) -> super::VmceSeriesState {
        let Some(identity) = ticket.identity.material.series.as_ref() else {
            return super::VmceSeriesState::Failed(
                "VMCE LAN Options omit the series identity".to_owned(),
            );
        };
        if !series.directory.eq_ignore_ascii_case(&identity.directory)
            || series.manifest_sha256() != identity.manifest_sha256
        {
            return super::VmceSeriesState::Failed(
                "local VMCE series identity changed after Options admission".to_owned(),
            );
        }
        if identity.selected_map_file
            != ticket
                .identity
                .material
                .selected_map_file
                .as_deref()
                .unwrap_or_default()
            || identity.source_sha256
                != ticket
                    .identity
                    .material
                    .source_sha256
                    .as_deref()
                    .unwrap_or_default()
            || identity.effective_digest
                != ticket
                    .identity
                    .material
                    .effective_digest
                    .as_deref()
                    .unwrap_or_default()
        {
            return super::VmceSeriesState::Failed(
                "VMCE Options identity does not match the current selected map".to_owned(),
            );
        }
        let verification = if ticket.identity.material.host {
            self.verifier
                .host_boundary_for_request(series, &identity.content_sha256, ticket.id)
        } else {
            self.verifier
                .guest_boundary_for_request(series, &identity.content_sha256, ticket.id)
        };
        if matches!(verification, super::VmceSeriesState::Verified { .. }) {
            ticket.validated = true;
        }
        verification
    }

    pub(super) fn consume_boundary(&mut self, ticket: &BoundaryTicket) {
        if ticket.kind == BoundaryKind::Ready && ticket.validated {
            self.admitted_ready_generation = self.admitted_ready_generation.wrapping_add(1);
        }
        if self
            .active
            .ticket
            .as_ref()
            .is_some_and(|active| active.id == ticket.id)
        {
            self.active.ticket = None;
        }
        if self.current_handoff_id == Some(ticket.id) {
            self.current_handoff_id = None;
        }
    }

    fn current_handoff(&self, request_id: u64) -> bool {
        self.current_handoff_id == Some(request_id)
    }
}

pub(super) fn next_request_id(state: &mut AppState) -> u64 {
    let Some(service) = state.match_state.lan_service.as_mut() else {
        return 0;
    };
    let request_id = service.vmce_setup.next_request_id;
    service.vmce_setup.next_request_id = request_id.wrapping_add(1).max(1);
    request_id
}

pub(crate) fn reset_vmce_setup(state: &mut AppState) {
    if let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_setup.reset();
    }
}

pub(super) fn note_publication_enqueued(state: &mut AppState, request_id: u64) {
    if let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_setup.pending_publication_id = Some(request_id);
    }
}

pub(super) fn abandon_publication(state: &mut AppState, request_id: u64) {
    if let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_setup.abandon_publication(request_id);
    }
}

pub(super) fn is_current_handoff(state: &AppState, request_id: u64) -> bool {
    state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(|service| service.vmce_setup.current_handoff(request_id))
}

fn boundary_kind(request: &NetworkSetupRequest) -> Option<BoundaryKind> {
    match request {
        NetworkSetupRequest::Ready(true) => Some(BoundaryKind::Ready),
        NetworkSetupRequest::Go { .. } => Some(BoundaryKind::HostGo),
        NetworkSetupRequest::PrepareMapForHandoff { .. } => Some(BoundaryKind::GuestPrepare),
        _ => None,
    }
}

fn current_material(state: &AppState) -> Option<SetupMaterial> {
    let session = &state.match_state.session;
    if !session
        .network_return_context()
        .is_some_and(|context| context.session_mode() == SessionMode::Lan)
    {
        return None;
    }
    let launch = session.network_setup()?;
    if !launch.mode.class.is_vmce() {
        return None;
    }
    let service = state.match_state.lan_service.as_ref()?;
    let scope = service.setup_scope();
    let series = if session.host() {
        service
            .vmce_setup
            .published
            .as_ref()
            .filter(|(published_scope, _)| Some(*published_scope) == scope)
            .map(|(_, identity)| identity.clone())
    } else {
        service
            .received_options
            .as_ref()
            .filter(|received| Some(received.scope()) == scope)
            .and_then(|received| received.vmce_series_identity())
    };
    let metadata = session.network_map_metadata();
    Some(SetupMaterial {
        scope,
        selected_map_file: launch.selected_map_file.clone(),
        local_map_source: session.network_local_map_source().cloned(),
        source_sha256: metadata.map(|metadata| metadata.source_sha256.clone()),
        effective_digest: metadata.map(|metadata| metadata.effective_digest.clone()),
        series,
        setup: launch.clone(),
        map_config: session.network_vmce_map_config().cloned(),
        intermission_map: session.vmce_intermission_map().map(str::to_owned),
        host: session.host(),
        preadmitted: service.preadmitted,
    })
}

/// Observe the current setup before polling a worker or retrying any request.
/// A changed scope/options identity invalidates boundary grants; the completed
/// map admission survives only while its series and local config remain exact.
pub(super) fn observe_current_setup(state: &mut AppState) -> Option<u64> {
    let material = current_material(state);
    let service = state.match_state.lan_service.as_mut()?;
    service
        .vmce_setup
        .observe(material)
        .map(|identity| identity.revision)
}

pub(super) fn admission_state(state: &AppState) -> super::VmceSeriesState {
    state
        .match_state
        .lan_service
        .as_ref()
        .map_or(super::VmceSeriesState::Idle, |service| {
            service
                .vmce_setup
                .admission_state_for(&state.match_state.session)
        })
}

pub(super) fn record_admission_verdict(state: &mut AppState, verdict: &super::VmceSeriesState) {
    if let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_setup.record_admission_verdict(verdict);
    }
}

/// HostSetup can apply map admission while it commits the received Options
/// revision. Clear an old verdict before that transaction unless the incoming
/// series and config exactly match the current guest admission identity.
pub(super) fn prepare_received_series_admission(
    state: &mut AppState,
    map_file: &str,
    source_sha256: &str,
    effective_digest: &str,
    map_config: Option<&crate::app::match_bootstrap::NetworkVmceMapConfig>,
    identity: Option<&VmceSeriesIdentity>,
    content_sha256: Option<&str>,
) {
    let matches_current = state
        .match_state
        .lan_service
        .as_ref()
        .and_then(|service| service.vmce_setup.observed.as_ref())
        .is_some_and(|material| {
            let Some((series, local_config)) =
                material.series.as_ref().zip(material.map_config.as_ref())
            else {
                return false;
            };
            let (Some(identity), Some(content_sha256), Some(incoming_config)) =
                (identity, content_sha256, map_config)
            else {
                return false;
            };
            material.selected_map_file.as_deref() == Some(map_file)
                && material.source_sha256.as_deref() == Some(source_sha256)
                && material.effective_digest.as_deref() == Some(effective_digest)
                && series.selected_map_file == map_file
                && series.source_sha256 == source_sha256
                && series.effective_digest == effective_digest
                && series.directory.eq_ignore_ascii_case(&identity.directory)
                && series.manifest_sha256 == identity.sha256
                && series.content_sha256 == content_sha256
                && local_config == incoming_config
        });
    if !matches_current && let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_setup.verifier.cancel();
        service.vmce_setup.admission = None;
        service.vmce_setup.active.ticket = None;
    }
}

/// Capture a boundary ticket when a request enters the LAN deferred FIFO.
pub(super) fn ticket_for_enqueue(
    state: &mut AppState,
    request: &NetworkSetupRequest,
    request_id: u64,
    publication_dependency: Option<u64>,
) -> Option<BoundaryTicket> {
    let kind = boundary_kind(request)?;
    let material = current_material(state);
    let service = state.match_state.lan_service.as_mut()?;
    let identity = service.vmce_setup.observe(material)?;
    Some(
        service
            .vmce_setup
            .enqueue_boundary(request_id, kind, identity, publication_dependency),
    )
}

/// Check the request's frozen identity, then reuse or start its one fresh
/// whole-series scan. A later ticket waits behind the active boundary rather
/// than canceling that scan.
pub(super) fn boundary(
    state: &mut AppState,
    request: &NetworkSetupRequest,
    ticket: &mut Option<BoundaryTicket>,
) -> anyhow::Result<BoundaryDecision> {
    let Some(kind) = boundary_kind(request) else {
        return Ok(BoundaryDecision::NotApplicable);
    };
    let material = current_material(state);
    let series_identity = {
        let Some(service) = state.match_state.lan_service.as_mut() else {
            if ticket.is_some() {
                *ticket = None;
                return Ok(BoundaryDecision::Stale);
            }
            return Ok(BoundaryDecision::NotApplicable);
        };
        let Some(current) = service.vmce_setup.observe(material) else {
            if ticket.is_some() {
                *ticket = None;
                return Ok(BoundaryDecision::Stale);
            }
            return Ok(BoundaryDecision::NotApplicable);
        };
        let Some(ticket_value) = ticket.as_mut() else {
            return Ok(BoundaryDecision::Stale);
        };
        let request_id = ticket_value.id;
        match service.vmce_setup.bind_ticket(ticket_value, &current, kind) {
            TicketBinding::Current => {}
            TicketBinding::PendingPublication => return Ok(BoundaryDecision::Pending),
            TicketBinding::Stale => {
                if service
                    .vmce_setup
                    .active
                    .ticket
                    .as_ref()
                    .is_some_and(|active| active.id == request_id)
                {
                    service.vmce_setup.cancel_through(request_id);
                }
                *ticket = None;
                return Ok(BoundaryDecision::Stale);
            }
        }
        if ticket_value.identity.material.preadmitted {
            return Ok(BoundaryDecision::NotApplicable);
        }
        if ticket_value.identity.material.scope.is_none() {
            return Ok(BoundaryDecision::Refused(super::VmceSeriesState::Failed(
                "VMCE LAN setup scope is unavailable".to_owned(),
            )));
        }
        if let Some(active) = service.vmce_setup.active.ticket.as_ref() {
            if active.id != request_id {
                return Ok(BoundaryDecision::Pending);
            }
        } else {
            service.vmce_setup.active.ticket = Some(ticket_value.clone());
        }
        let Some(identity) = ticket_value.identity.material.series.clone() else {
            return Ok(BoundaryDecision::Refused(super::VmceSeriesState::Failed(
                "VMCE LAN Options omit the series identity".to_owned(),
            )));
        };
        identity
    };
    let series = match vmce_series::read_local_series(state, &series_identity.selected_map_file) {
        Ok(series) => series,
        Err(error) => {
            return Ok(BoundaryDecision::Refused(super::VmceSeriesState::Failed(
                format!("{error:#}"),
            )));
        }
    };
    let verification = {
        let Some(service) = state.match_state.lan_service.as_mut() else {
            return Ok(BoundaryDecision::Stale);
        };
        let Some(ticket) = ticket.as_mut() else {
            return Ok(BoundaryDecision::Stale);
        };
        service
            .vmce_setup
            .verify_series_for_boundary(ticket, &series)
    };
    Ok(match verification {
        super::VmceSeriesState::Pending => BoundaryDecision::Pending,
        super::VmceSeriesState::Verified { .. } => {
            if let Some(ticket) = ticket.as_mut() {
                ticket.validated = true;
            }
            BoundaryDecision::Verified
        }
        state @ (super::VmceSeriesState::Mismatch { .. } | super::VmceSeriesState::Failed(_)) => {
            BoundaryDecision::Refused(state)
        }
        super::VmceSeriesState::Idle => BoundaryDecision::Refused(super::VmceSeriesState::Failed(
            "VMCE series verification is idle".to_owned(),
        )),
    })
}

pub(super) fn finish_boundary(state: &mut AppState, ticket: Option<&BoundaryTicket>) {
    let Some(ticket) = ticket else { return };
    if let Some(service) = state.match_state.lan_service.as_mut() {
        if service
            .vmce_setup
            .active
            .ticket
            .as_ref()
            .is_some_and(|active| active.id == ticket.id)
        {
            service.vmce_setup.active.ticket = None;
        }
        if service.vmce_setup.current_handoff(ticket.id) {
            service.vmce_setup.current_handoff_id = None;
        }
    }
}

pub(super) fn cancel_boundaries(state: &mut AppState, request_id: u64) {
    if let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_setup.cancel_through(request_id);
    }
}

/// Invalidate local setup-boundary tickets when a VMCE LAN Go handoff fails.
/// A later user retry receives a newer request ID and can validate afresh.
pub(crate) fn invalidate_pending_vmce_boundaries(state: &mut AppState) {
    if state.match_state.session_mode() != SessionMode::Lan
        || !state
            .match_state
            .session
            .network_setup()
            .is_some_and(|launch| launch.mode.class.is_vmce())
    {
        return;
    }
    let latest = state
        .match_state
        .lan_service
        .as_ref()
        .map_or(0, |service| service.vmce_setup.latest_request_id());
    cancel_boundaries(state, latest);
}

pub(crate) fn ready_admission_generation(state: &AppState) -> u64 {
    state
        .match_state
        .lan_service
        .as_ref()
        .map_or(0, |service| service.vmce_setup.admitted_ready_generation())
}

/// Collect a completed worker and reconcile its verdict through the existing
/// MatchSession map-admission owner. No LAN scan can write admission directly.
pub(super) fn apply_finished_series_scan(state: &mut AppState) {
    observe_current_setup(state);
    let restart_admission = state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(|service| service.vmce_setup.needs_admission_scan());
    if restart_admission {
        options::ensure_current_series_admission(state);
    }
    let Some(verification) = (|| {
        let service = state.match_state.lan_service.as_mut()?;
        service.vmce_setup.poll();
        service.vmce_setup.admission_needed().then(|| {
            service
                .vmce_setup
                .admission_state_for(&state.match_state.session)
        })
    })() else {
        return;
    };
    if reconcile_finished_series_verdict(state, &verification)
        && let Some(service) = state.match_state.lan_service.as_mut()
    {
        service.vmce_setup.mark_admission_applied();
    }
}

/// The VMCE LAN scan owner decides what admission the shared map owner may
/// apply. MatchSession remains the authoritative status and readiness store.
fn series_map_admission(
    verification: &VmceSeriesState,
) -> Option<crate::app::match_runtime::session::NetworkMapAdmission> {
    use crate::app::match_runtime::session::NetworkMapAdmission;
    match verification {
        VmceSeriesState::Verified { .. } => Some(NetworkMapAdmission::Present),
        VmceSeriesState::Mismatch { .. } | VmceSeriesState::Failed(_) => {
            Some(NetworkMapAdmission::Unavailable)
        }
        VmceSeriesState::Pending | VmceSeriesState::Idle => None,
    }
}

fn gate_series_map_admission(
    state: &mut AppState,
    admission: Option<crate::app::match_runtime::session::NetworkMapAdmission>,
) -> Option<crate::app::match_runtime::session::NetworkMapAdmission> {
    let vmce_guest = state.match_state.session_mode() == SessionMode::Lan
        && !state.match_state.session.host()
        && state
            .match_state
            .session
            .network_setup()
            .is_some_and(|launch| launch.mode.class.is_vmce())
        && state.match_state.lan_service.is_some();
    if !vmce_guest
        || state
            .match_state
            .session
            .network_vmce_map_config()
            .is_none()
    {
        return admission;
    }
    let admission = series_map_admission(&admission_state(state));
    if admission.is_none() {
        crate::app::shell_network_setup::withhold_local_map_admission(state);
    }
    admission
}

/// Route a whole-series verdict through the existing MatchSession map
/// admission owner, then mark that scan's admission effect as consumed.
pub(crate) fn admit_vmce_series_map(
    state: &mut AppState,
    admission: Option<crate::app::match_runtime::session::NetworkMapAdmission>,
) {
    let Some(admission) = gate_series_map_admission(state, admission) else {
        return;
    };
    if let Err(error) = crate::app::shell_network_setup::apply_local_map_admission(state, admission)
    {
        log::warn!("Could not apply VMCE map admission: {error:#}");
        return;
    }
    if let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_setup.mark_admission_applied();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::match_runtime::session::NetworkMapAdmission;
    use crate::map::source::test_support::TestDirectory;

    #[test]
    fn accepted_failed_admission_reopens_retry_only_for_the_guest() {
        let mismatch = VmceSeriesState::Mismatch {
            local: "local-series".to_owned(),
        };
        let failed = VmceSeriesState::Failed("missing campaign".to_owned());
        let pending = VmceSeriesState::Pending;
        let verified = VmceSeriesState::Verified {
            digest: "matching-series".to_owned(),
        };

        assert!(retry_allowed_after_options(true, Some(&mismatch)));
        assert!(retry_allowed_after_options(true, Some(&failed)));
        assert!(!retry_allowed_after_options(false, Some(&mismatch)));
        assert!(!retry_allowed_after_options(true, Some(&pending)));
        assert!(!retry_allowed_after_options(true, Some(&verified)));
        assert!(!retry_allowed_after_options(true, None));
    }

    #[test]
    fn vmce_series_admission_consumes_legacy_asset_requests() {
        use crate::app::match_runtime::session::SessionMode::Wol;
        use crate::skirmish_modes::MpModeClass::{Battle, Vmce};

        assert!(uses_series_map_admission(Some(Vmce)));
        assert!(!uses_series_map_admission(Some(Battle)));
        assert!(uses_local_series_map_assets(SessionMode::Lan, Some(Vmce)));
        assert!(!uses_local_series_map_assets(
            SessionMode::Lan,
            Some(Battle)
        ));
        assert!(!uses_local_series_map_assets(Wol, Some(Vmce)));
        for request in [
            NetworkSetupRequest::RequestPreview,
            NetworkSetupRequest::RequestRandomMapImage,
            NetworkSetupRequest::OfferPreview,
            NetworkSetupRequest::SendPreviewFile,
            NetworkSetupRequest::AcknowledgePreview,
            NetworkSetupRequest::RequestPreviewFile,
        ] {
            assert!(consumes_legacy_map_asset_request(
                SessionMode::Lan,
                Some(Vmce),
                &request,
            ));
            assert!(!consumes_legacy_map_asset_request(
                SessionMode::Lan,
                Some(Battle),
                &request,
            ));
            assert!(!consumes_legacy_map_asset_request(
                SessionMode::Wol,
                Some(Vmce),
                &request,
            ));
        }
        assert!(!consumes_legacy_map_asset_request(
            SessionMode::Lan,
            Some(Vmce),
            &NetworkSetupRequest::Ready(true),
        ));
    }

    fn write_series(root: &TestDirectory) {
        root.write("first.map", b"selected map bytes");
        root.write("later.map", b"later map bytes");
        root.write(
            "campaign.ini",
            b"[Campaign]\nEntry=first.map\nScenarioCount=2\nScenario1=first.map\nScenario2=later.map\n",
        );
    }

    fn local_series(root: &TestDirectory) -> vmce_series::LocalSeries {
        let directory_path = root.path().canonicalize().unwrap();
        vmce_series::LocalSeries {
            directory: root
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            manifest: std::fs::read(directory_path.join("campaign.ini")).unwrap(),
            directory_path,
        }
    }

    fn wait_scan(setup: &mut VmceSetupState) -> VmceSeriesState {
        for _ in 0..600 {
            if setup.poll() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        setup.state()
    }

    fn series_digest(series: &vmce_series::LocalSeries) -> String {
        let mut setup = VmceSetupState::new();
        assert!(setup.host_digest(series).is_none());
        match wait_scan(&mut setup) {
            VmceSeriesState::Verified { digest } => digest,
            state => panic!("host series scan did not complete: {state:?}"),
        }
    }

    fn setup_material(
        series: &vmce_series::LocalSeries,
        content_sha256: &str,
        host: bool,
    ) -> SetupMaterial {
        let selected_map_file = format!("{}/first.map", series.directory);
        let local_map_source = Some(crate::map::source::LoadedMapSource::Loose {
            path: std::path::PathBuf::from(&selected_map_file),
            payload_len: b"selected map bytes".len(),
            source_sha256: "a".repeat(64),
            cache_lease: crate::map::source::CacheLease::default(),
        });
        let profile: serde_json::Value = serde_json::from_str(crate::test_fixture::text(
            "tools/map_observation.command-bar.example.json",
        ))
        .unwrap();
        let mut setup: SkirmishLaunchSession =
            serde_json::from_value(profile["launch"].clone()).unwrap();
        setup.mode.class = crate::skirmish_modes::MpModeClass::Vmce;
        setup.selected_map_file = Some(selected_map_file.clone());
        SetupMaterial {
            scope: Some(SessionScope {
                epoch: [7; 16],
                revision: 5,
            }),
            selected_map_file: Some(selected_map_file.clone()),
            local_map_source,
            source_sha256: Some("a".repeat(64)),
            effective_digest: Some("b".repeat(64)),
            series: Some(SeriesIdentity {
                selected_map_file,
                source_sha256: "a".repeat(64),
                effective_digest: "b".repeat(64),
                directory: series.directory.clone(),
                manifest_sha256: series.manifest_sha256(),
                content_sha256: content_sha256.to_owned(),
            }),
            setup,
            map_config: None,
            intermission_map: None,
            host,
            preadmitted: false,
        }
    }

    fn enqueue_ticket(
        setup: &mut VmceSetupState,
        id: u64,
        kind: BoundaryKind,
        material: SetupMaterial,
        publication_dependency: Option<u64>,
    ) -> BoundaryTicket {
        let identity = setup.observe(Some(material)).unwrap();
        setup.enqueue_boundary(id, kind, identity, publication_dependency)
    }

    #[test]
    fn series_scan_verdict_controls_lan_map_admission() {
        assert_eq!(
            series_map_admission(&VmceSeriesState::Verified {
                digest: "a".repeat(64),
            }),
            Some(NetworkMapAdmission::Present)
        );
        assert_eq!(
            series_map_admission(&VmceSeriesState::Mismatch {
                local: "b".repeat(64),
            })
            .map(NetworkMapAdmission::status),
            Some(0)
        );
        assert_eq!(
            series_map_admission(&VmceSeriesState::Failed(
                "campaign map is missing: later.map".to_owned(),
            ))
            .map(NetworkMapAdmission::status),
            Some(0)
        );
        assert_eq!(series_map_admission(&VmceSeriesState::Pending), None);
        assert_eq!(series_map_admission(&VmceSeriesState::Idle), None);
    }

    #[test]
    fn host_go_revalidates_later_map_against_the_admitted_options_digest() {
        let root = TestDirectory::new("vmce-setup-host-boundary");
        write_series(&root);
        let series = local_series(&root);
        let mut setup = VmceSetupState::new();
        assert!(setup.host_digest(&series).is_none());
        let published_digest = match wait_scan(&mut setup) {
            VmceSeriesState::Verified { digest } => digest,
            state => panic!("host Options digest did not complete: {state:?}"),
        };
        let material = setup_material(&series, &published_digest, true);
        let identity = setup.observe(Some(material.clone())).unwrap();
        let mut first_go = setup.enqueue_boundary(10, BoundaryKind::HostGo, identity.clone(), None);
        assert_eq!(
            setup.verify_series_for_boundary(&mut first_go, &series),
            VmceSeriesState::Pending
        );
        assert!(wait_scan(&mut setup).verified());
        assert!(matches!(
            setup.verify_series_for_boundary(&mut first_go, &series),
            VmceSeriesState::Verified { .. }
        ));

        // Only the unselected later-stage bytes change; the selected map and
        // campaign manifest remain the published identity.
        let manifest_before = series.manifest_sha256();
        root.write("later.map", b"edited after published Ready");
        assert_eq!(series.manifest_sha256(), manifest_before);
        let mut later_go = setup.enqueue_boundary(11, BoundaryKind::HostGo, identity, None);
        assert_eq!(
            setup.verify_series_for_boundary(&mut later_go, &series),
            VmceSeriesState::Pending
        );
        assert!(matches!(wait_scan(&mut setup), VmceSeriesState::Failed(_)));
        assert!(matches!(
            setup.verify_series_for_boundary(&mut later_go, &series),
            VmceSeriesState::Failed(_)
        ));
    }

    #[test]
    fn guest_prepare_rescans_unselected_map_and_a_new_ready_recovers_after_restore() {
        let root = TestDirectory::new("vmce-setup-guest-boundary");
        write_series(&root);
        let series = local_series(&root);
        let advertised = series_digest(&series);
        let mut setup = VmceSetupState::new();
        assert_eq!(
            setup.verify_advertised(&series, &advertised),
            VmceSeriesState::Pending
        );
        assert!(wait_scan(&mut setup).verified());
        let material = setup_material(&series, &advertised, false);
        let identity = setup.observe(Some(material)).unwrap();

        let mut ready = setup.enqueue_boundary(20, BoundaryKind::Ready, identity.clone(), None);
        assert_eq!(
            setup.verify_series_for_boundary(&mut ready, &series),
            VmceSeriesState::Pending
        );
        assert!(wait_scan(&mut setup).verified());
        assert!(matches!(
            setup.verify_series_for_boundary(&mut ready, &series),
            VmceSeriesState::Verified { .. }
        ));

        let manifest_before = series.manifest_sha256();
        root.write("later.map", b"edited after Ready");
        let mut prepare =
            setup.enqueue_boundary(21, BoundaryKind::GuestPrepare, identity.clone(), None);
        assert_eq!(
            setup.verify_series_for_boundary(&mut prepare, &series),
            VmceSeriesState::Pending
        );
        assert!(matches!(
            wait_scan(&mut setup),
            VmceSeriesState::Mismatch { .. }
        ));
        assert!(matches!(
            setup.verify_series_for_boundary(&mut prepare, &series),
            VmceSeriesState::Mismatch { .. }
        ));
        assert_eq!(series.manifest_sha256(), manifest_before);

        // Restoring the exact bytes does not revive the refused ticket. A new
        // Ready request starts another scan and passes through the same owner.
        root.write("later.map", b"later map bytes");
        let mut retry = setup.enqueue_boundary(22, BoundaryKind::Ready, identity, None);
        assert_eq!(
            setup.verify_series_for_boundary(&mut retry, &series),
            VmceSeriesState::Pending
        );
        assert!(wait_scan(&mut setup).verified());
        assert!(matches!(
            setup.verify_series_for_boundary(&mut retry, &series),
            VmceSeriesState::Verified { .. }
        ));
    }

    #[test]
    fn same_request_reuses_scan_and_cancellation_keeps_new_generation_valid() {
        let root = TestDirectory::new("vmce-setup-request-generation");
        write_series(&root);
        let series = local_series(&root);
        let advertised = series_digest(&series);
        let mut setup = VmceSetupState::new();
        let material = setup_material(&series, &advertised, false);
        let mut old = enqueue_ticket(&mut setup, 30, BoundaryKind::Ready, material.clone(), None);
        assert_eq!(
            setup.verify_series_for_boundary(&mut old, &series),
            VmceSeriesState::Pending
        );
        setup.consume_boundary(&old);
        assert_eq!(
            setup.admitted_ready_generation(),
            0,
            "pending scans are not admitted"
        );
        let scans = setup.scans_started();
        assert_eq!(
            setup.verify_series_for_boundary(&mut old, &series),
            VmceSeriesState::Pending
        );
        assert_eq!(
            setup.scans_started(),
            scans,
            "same ID reuses backpressure scan"
        );
        setup.active.ticket = Some(old.clone());
        setup.cancel_through(31); // Ready(false) ordered after the old Ready.
        assert_eq!(setup.state(), VmceSeriesState::Idle);
        assert!(!setup.poll(), "the canceled worker result is discarded");

        let same_identity = setup.observe(Some(material.clone())).unwrap();
        let mut fresh_after_cancel =
            setup.enqueue_boundary(32, BoundaryKind::Ready, same_identity, None);
        let fresh_identity = fresh_after_cancel.identity.clone();
        assert_eq!(
            setup.bind_ticket(&mut old, &fresh_identity, BoundaryKind::Ready),
            TicketBinding::Stale
        );
        assert_eq!(
            setup.bind_ticket(
                &mut fresh_after_cancel,
                &fresh_identity,
                BoundaryKind::Ready
            ),
            TicketBinding::Current
        );

        // A material change cancels that pending worker. The next request gets
        // a new revision while the old envelope stays stale.
        assert_eq!(
            setup.verify_series_for_boundary(&mut fresh_after_cancel, &series),
            VmceSeriesState::Pending
        );
        let mut changed = material;
        changed.setup.humans[0].role = crate::skirmish_launch::LaunchHumanRole::Player { group: 1 };
        let current = setup.observe(Some(changed.clone())).unwrap();
        assert_eq!(setup.state(), VmceSeriesState::Idle);
        assert!(!setup.poll(), "the prior material's scan result is stale");
        assert_eq!(
            setup.bind_ticket(&mut fresh_after_cancel, &current, BoundaryKind::Ready),
            TicketBinding::Stale
        );
        let mut current_ready = setup.enqueue_boundary(33, BoundaryKind::Ready, current, None);
        let current = setup.observe(Some(changed.clone())).unwrap();
        assert_eq!(
            setup.bind_ticket(&mut current_ready, &current, BoundaryKind::Ready,),
            TicketBinding::Current
        );
        assert_eq!(
            setup.verify_series_for_boundary(&mut current_ready, &series),
            VmceSeriesState::Pending
        );
        assert!(wait_scan(&mut setup).verified());
        assert!(matches!(
            setup.verify_series_for_boundary(&mut current_ready, &series),
            VmceSeriesState::Verified { .. }
        ));
        setup.consume_boundary(&current_ready);
        assert_eq!(setup.admitted_ready_generation(), 1);

        setup.active.ticket = Some(current_ready.clone());
        let mut same_content_other_source = changed;
        let Some(crate::map::source::LoadedMapSource::Loose { path, .. }) =
            same_content_other_source.local_map_source.as_mut()
        else {
            panic!("test source must be a loose map");
        };
        *path = path.with_file_name("same-content-other-source.map");
        assert_eq!(
            same_content_other_source.source_sha256,
            current_ready.identity.material.source_sha256
        );
        assert_eq!(
            same_content_other_source.effective_digest,
            current_ready.identity.material.effective_digest
        );
        let new_source_identity = setup.observe(Some(same_content_other_source)).unwrap();
        assert_eq!(setup.state(), VmceSeriesState::Idle);
        assert_eq!(
            setup.bind_ticket(
                &mut current_ready,
                &new_source_identity,
                BoundaryKind::Ready
            ),
            TicketBinding::Stale,
            "the same digest at another physical source invalidates the boundary"
        );
    }

    #[test]
    fn a_scope_revision_keeps_admission_but_cancels_its_old_boundary_worker() {
        let root = TestDirectory::new("vmce-setup-admission-scope-revision");
        write_series(&root);
        let series = local_series(&root);
        let advertised = series_digest(&series);
        let mut setup = VmceSetupState::new();
        let mut material = setup_material(&series, &advertised, false);
        material.map_config = Some(
            crate::app::match_bootstrap::NetworkVmceMapConfig::new(
                material.selected_map_file.clone().unwrap(),
                material.source_sha256.clone().unwrap(),
                crate::map::vmce_config::VmceMapConfig::default(),
            )
            .unwrap()
            .with_series_identity(series.directory.clone(), series.manifest_sha256())
            .unwrap(),
        );
        let old_identity = setup.observe(Some(material.clone())).unwrap();
        assert_eq!(
            setup.verify_advertised(&series, &advertised),
            VmceSeriesState::Pending
        );
        setup.record_admission_verdict(&VmceSeriesState::Pending);
        assert!(wait_scan(&mut setup).verified());
        assert!(matches!(
            setup.admission.as_ref().map(|admission| &admission.verdict),
            Some(VmceSeriesState::Verified { .. })
        ));
        setup.mark_admission_applied();

        let mut ready = setup.enqueue_boundary(35, BoundaryKind::Ready, old_identity, None);
        setup.active.ticket = Some(ready.clone());
        assert_eq!(
            setup.verify_series_for_boundary(&mut ready, &series),
            VmceSeriesState::Pending
        );

        let mut revised = material;
        revised.scope = Some(SessionScope {
            epoch: [7; 16],
            revision: 6,
        });
        revised.setup.humans[0].role = crate::skirmish_launch::LaunchHumanRole::Player { group: 1 };
        let current = setup.observe(Some(revised.clone())).unwrap();
        assert_eq!(
            setup.bind_ticket(&mut ready, &current, BoundaryKind::Ready),
            TicketBinding::Stale
        );
        assert_eq!(setup.state(), VmceSeriesState::Idle);
        assert!(!setup.poll(), "the canceled boundary result is discarded");
        let admission = setup.admission.as_ref().expect("cached map admission");
        assert!(matches!(
            admission.verdict,
            VmceSeriesState::Verified { .. }
        ));
        assert!(
            admission.applied,
            "the old boundary cannot rewrite admission"
        );
        assert!(!setup.needs_admission_scan());

        let mut other_source = revised;
        let Some(crate::map::source::LoadedMapSource::Loose { path, .. }) =
            other_source.local_map_source.as_mut()
        else {
            panic!("test source must be a loose map");
        };
        *path = path.with_file_name("same-content-other-source.map");
        assert!(
            !admission.material.same_series_admission(&other_source),
            "map admission is tied to the physical local source"
        );
        setup.observe(Some(other_source));
        assert!(setup.admission.is_none());
    }

    #[test]
    fn host_start_go_binds_only_its_exact_final_options_publication() {
        let root = TestDirectory::new("vmce-setup-publication-binding");
        write_series(&root);
        let series = local_series(&root);
        let mut setup = VmceSetupState::new();
        let old_material = setup_material(&series, &"c".repeat(64), true);
        let old_identity = setup.observe(Some(old_material.clone())).unwrap();
        setup.pending_publication_id = Some(40);
        let mut go = setup.enqueue_boundary(42, BoundaryKind::HostGo, old_identity, Some(40));
        let new_scope = SessionScope {
            epoch: [7; 16],
            revision: 6,
        };
        let new_series_identity = SeriesIdentity {
            selected_map_file: old_material.selected_map_file.clone().unwrap(),
            source_sha256: old_material.source_sha256.clone().unwrap(),
            effective_digest: old_material.effective_digest.clone().unwrap(),
            directory: series.directory.clone(),
            manifest_sha256: series.manifest_sha256(),
            content_sha256: "d".repeat(64),
        };
        setup.record_published(new_scope, new_series_identity.clone(), Some(40));
        let mut final_material = old_material.clone();
        final_material.scope = Some(new_scope);
        final_material.series = Some(new_series_identity);
        let current = setup.observe(Some(final_material)).unwrap();
        assert_eq!(
            setup.bind_ticket(&mut go, &current, BoundaryKind::HostGo),
            TicketBinding::Current
        );
        assert_eq!(
            go.publication_dependency, None,
            "the binding is consumed once"
        );

        let old_identity = setup.observe(Some(old_material.clone())).unwrap();
        let mut unrelated =
            setup.enqueue_boundary(44, BoundaryKind::HostGo, old_identity, Some(45));
        setup.pending_publication_id = None;
        setup.last_published_request_id = Some(46);
        let mut unrelated_material = old_material;
        unrelated_material.scope = Some(new_scope);
        unrelated_material.series = Some(setup.published.as_ref().unwrap().1.clone());
        let current = setup.observe(Some(unrelated_material)).unwrap();
        assert_eq!(
            setup.bind_ticket(&mut unrelated, &current, BoundaryKind::HostGo),
            TicketBinding::Stale,
            "a later unrelated Options revision cannot rebase Go"
        );
    }

    #[test]
    fn abandoned_publication_fails_only_the_current_go_handoff() {
        let root = TestDirectory::new("vmce-setup-current-handoff");
        write_series(&root);
        let series = local_series(&root);
        let mut setup = VmceSetupState::new();
        let material = setup_material(&series, &"c".repeat(64), true);
        let identity = setup.observe(Some(material.clone())).unwrap();

        setup.pending_publication_id = Some(40);
        let mut old_go =
            setup.enqueue_boundary(42, BoundaryKind::HostGo, identity.clone(), Some(40));
        setup.abandon_publication(40);
        let current = setup.observe(Some(material.clone())).unwrap();
        assert_eq!(
            setup.bind_ticket(&mut old_go, &current, BoundaryKind::HostGo),
            TicketBinding::Stale,
            "a failed final Options publication leaves its dependent Go stale"
        );
        assert!(setup.current_handoff(old_go.id));
        setup.cancel_through(old_go.id);
        assert!(
            setup.current_handoff(old_go.id),
            "cancellation retains identity until the stale request reports failure"
        );

        let newer_go = setup.enqueue_boundary(44, BoundaryKind::HostGo, current.clone(), Some(45));
        assert!(!setup.current_handoff(old_go.id));
        assert!(setup.current_handoff(newer_go.id));
        assert_eq!(
            setup.bind_ticket(&mut old_go, &current, BoundaryKind::HostGo),
            TicketBinding::Stale,
            "an older failed publication cannot fail a replacement Go"
        );

        let ready = setup.enqueue_boundary(45, BoundaryKind::Ready, current.clone(), None);
        assert!(
            setup.current_handoff(newer_go.id),
            "a Ready request does not replace the in-flight handoff identity"
        );
        assert!(!setup.current_handoff(ready.id));

        let guest_prepare = setup.enqueue_boundary(46, BoundaryKind::GuestPrepare, current, None);
        assert!(!setup.current_handoff(newer_go.id));
        assert!(setup.current_handoff(guest_prepare.id));
        let mut changed_material = setup.observed.clone().unwrap();
        changed_material.source_sha256 = Some("d".repeat(64));
        let changed = setup.observe(Some(changed_material)).unwrap();
        let mut stale_guest = guest_prepare.clone();
        assert_eq!(
            setup.bind_ticket(&mut stale_guest, &changed, BoundaryKind::GuestPrepare),
            TicketBinding::Stale
        );
        assert!(setup.current_handoff(guest_prepare.id));
        let replacement = setup.enqueue_boundary(48, BoundaryKind::HostGo, changed, None);
        assert!(!setup.current_handoff(guest_prepare.id));
        assert!(setup.current_handoff(replacement.id));

        let mut canceled_prepare = setup.enqueue_boundary(
            50,
            BoundaryKind::GuestPrepare,
            replacement.identity.clone(),
            None,
        );
        setup.cancel_through(canceled_prepare.id);
        assert_eq!(
            setup.bind_ticket(
                &mut canceled_prepare,
                &replacement.identity,
                BoundaryKind::GuestPrepare
            ),
            TicketBinding::Stale
        );
        assert!(setup.current_handoff(canceled_prepare.id));
        let final_go = setup.enqueue_boundary(52, BoundaryKind::HostGo, replacement.identity, None);
        assert!(!setup.current_handoff(canceled_prepare.id));
        assert!(setup.current_handoff(final_go.id));
    }

    #[test]
    fn go_backpressure_keeps_the_validated_ticket_until_reliable_admission() {
        use crate::net::lan_messages::LanMessage;

        let root = TestDirectory::new("vmce-setup-go-send-backpressure");
        write_series(&root);
        let series = local_series(&root);
        let advertised = series_digest(&series);
        let mut setup = VmceSetupState::new();
        let identity = setup
            .observe(Some(setup_material(&series, &advertised, true)))
            .unwrap();
        let mut ticket = setup.enqueue_boundary(1, BoundaryKind::HostGo, identity, None);
        setup.active.ticket = Some(ticket.clone());
        assert_eq!(
            setup.verify_series_for_boundary(&mut ticket, &series),
            VmceSeriesState::Pending
        );
        assert!(wait_scan(&mut setup).verified());
        assert!(matches!(
            setup.verify_series_for_boundary(&mut ticket, &series),
            VmceSeriesState::Verified { .. }
        ));
        setup.active.ticket = Some(ticket.clone());

        let mut host = super::super::tests::service();
        host.vmce_setup = setup;
        let mut guest = super::super::tests::service();
        super::super::tests::global_capacity(&mut host, 1);
        let host_address = host.transport.as_ref().unwrap().primary_addr().unwrap();
        let guest_address = guest.transport.as_ref().unwrap().primary_addr().unwrap();
        assert!(
            host.queue_setup(std::iter::once((
                b"earlier reliable setup".to_vec(),
                super::super::direct(guest_address),
            )))
            .unwrap()
        );

        let scans = host.vmce_setup.scans_started();
        assert!(
            !host
                .queue_go(
                    &LanMessage::go(2),
                    &[super::super::direct(guest_address)],
                    Some(&ticket),
                )
                .unwrap(),
            "the occupied reliable slot backpressures GO"
        );
        assert_eq!(
            host.vmce_setup
                .active
                .ticket
                .as_ref()
                .map(|active| active.id),
            Some(ticket.id),
            "backpressure retains the same boundary grant"
        );
        assert_eq!(host.vmce_setup.scans_started(), scans);

        for frame in super::super::tests::global_frames(&mut host, 31) {
            for ack in super::super::tests::global_receive(&mut guest, host_address, &frame) {
                super::super::tests::global_receive(&mut host, guest_address, &ack);
            }
        }
        super::super::tests::global_frames(&mut host, 33);
        assert_eq!(host.queued_setup_count(), 0);
        assert!(
            host.queue_go(
                &LanMessage::go(2),
                &[super::super::direct(guest_address)],
                Some(&ticket),
            )
            .unwrap()
        );
        assert!(
            host.vmce_setup.active.ticket.is_none(),
            "reliable queue admission consumes the grant"
        );
        assert_eq!(host.vmce_setup.scans_started(), scans);
        assert_eq!(host.queued_setup_count(), 1);
    }
}
