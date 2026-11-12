//! LAN5E3720/5DBB60 options and5E8EA0 roster adapters. The packet is a
//! projection: MatchSession retains participants, choices, map and readiness;
//! the existing HostSetup service performs mode/map admission and its effects.

use std::net::SocketAddrV4;

use anyhow::{Result, anyhow, ensure};

#[cfg(all(test, feature = "vmce"))]
use super::vmce_setup::options::{
    apply_vmce_roles, campaign_identity_matches,
    classify_campaign_failure as classify_vmce_campaign_failure, validate_vmce_series,
};
#[cfg(feature = "vmce")]
use super::vmce_setup::{VmceRoleRow, VmceSeriesIdentity};
use crate::app::AppState;
use crate::app::match_runtime::session::{
    MatchSession, NetworkAiOptions, NetworkMapMetadata, NetworkRosterEntry, NetworkVacancy,
};
use crate::app::shell_network_setup::{HostOptions, NetworkSetupEvent};
use crate::net::lan_messages::setup::{self, LanGameOptions, LanOptionsAi, LanOptionsHuman};
use crate::net::lan_messages::{LanMessage, game_name_hash};
use crate::rules::ini_value::{crt_atoi, strtok};
use crate::skirmish_launch::{
    AiDifficulty, LaunchCountry, LaunchStartPosition, LaunchTeam, NETWORK_PLAYER_SLOT_COUNT,
    SkirmishAiSlot, SkirmishLaunchMode, SkirmishLaunchSession, SkirmishLocalSlot,
};
use crate::util::native_string::{narrow_utf16_c_string, narrow_widened, widen_bytes};

pub(super) fn publish_options(
    state: &mut AppState,
    publication_request_id: Option<u64>,
) -> Result<bool> {
    #[cfg(not(feature = "vmce"))]
    let _ = publication_request_id;
    // Generated dependency preparation can exceed a discovery-query interval.
    // A backpressured retry must yield before that work, allowing the existing
    // transport owner to drain. queue_payload retains its final ACK recheck.
    if !state
        .match_state
        .lan_service
        .as_ref()
        .ok_or_else(|| anyhow!("no LAN service"))?
        .can_publish_options()
    {
        return Ok(false);
    }
    // A VMCE revision always carries both series identities. The whole-series
    // digest is computed on a worker, so a publication whose digest is still
    // pending defers the whole revision instead of blocking the service loop
    // or sending one that claims the series is absent: `drain_setup_requests`
    // keeps this `PublishOptions` in `deferred_setup` and retries it in FIFO
    // order once the scan finishes, before any roster change queued behind it.
    // Checked before the agreement/generated-manifest work so a retry during
    // the scan stays cheap.
    #[cfg(feature = "vmce")]
    let vmce_projection = match super::vmce_setup::published_options_projection(state)? {
        super::vmce_setup::OptionsPublication::NotVmce => None,
        super::vmce_setup::OptionsPublication::Pending => return Ok(false),
        super::vmce_setup::OptionsPublication::Ready(projection) => Some(projection),
    };
    let agreement = crate::app::shell_network_setup::agreement::local(state)?;
    let random_map = crate::app::shell_network_setup::maps::host_generated_manifest(state)?;
    let session = &state.match_state.session;
    ensure!(session.host(), "only the host publishes full LAN options");
    let options = projected_options(
        session,
        state
            .match_state
            .match_presentation
            .in_game_options
            .game_speed as i32,
        state
            .frontend
            .multiplayer_shell_runtime
            .cooperative_difficulty(),
    )?;
    let text = options.encode();
    let service = state.match_state.lan_service.as_ref().expect("LAN visit");
    let admissions = session
        .network_setup()
        .expect("projected setup")
        .humans
        .iter()
        .enumerate()
        .filter_map(|(source, human)| {
            let endpoint = session.network_endpoint(source)?;
            Some(ExtendedAdmission {
                name: human.name.clone(),
                endpoint,
                identity: service.peer_admissions.get(&endpoint)?.identity,
            })
        })
        .collect();
    let launch = session.network_setup().expect("projected setup");
    let selected_map_file = launch
        .selected_map_file
        .clone()
        .ok_or_else(|| anyhow!("no selected map"))?;
    let metadata = session.network_map_metadata().expect("projected metadata");
    let source_sha256 = metadata.source_sha256.clone();
    let effective_digest = metadata.effective_digest.clone();
    #[cfg(feature = "vmce")]
    let (vmce_roles, vmce_series, vmce_series_content, published_identity) = vmce_projection
        .map_or((Vec::new(), None, None, None), |projection| {
            (
                projection.roles,
                Some(projection.series),
                Some(projection.series_content),
                Some(projection.identity),
            )
        });
    let text = serde_json::to_vec(&ExtendedOptions {
        text,
        agreement: agreement.clone(),
        selected_map_file,
        effective_digest,
        source_sha256,
        random_map: random_map.clone(),
        admissions,
        room: Some(
            session
                .network_setup()
                .expect("projected setup")
                .options
                .room_options(),
        ),
        #[cfg(feature = "vmce")]
        vmce_roles,
        #[cfg(feature = "vmce")]
        vmce_series_content,
        #[cfg(feature = "vmce")]
        vmce_series,
        #[cfg(feature = "vmce")]
        vmce_intermission: session.vmce_intermission_map().map(str::to_owned),
    })?;
    let admitted = queue_payload(
        state,
        crate::net::lan_messages::extended_setup::SetupKind::Options,
        text,
    )?;
    if admitted {
        crate::app::shell_network_setup::invalidate_network_agreement(state)?;
        let service = state.match_state.lan_service.as_mut().expect("visit");
        #[cfg(feature = "vmce")]
        if let Some(identity) = published_identity {
            let scope = service.setup_scope().expect("admitted options scope");
            service
                .vmce_setup
                .record_published(scope, identity, publication_request_id);
        }
        service.deferred_setup.retain(|request| {
            !matches!(
                request.request,
                crate::app::shell_network_setup::NetworkSetupRequest::Ready(_)
                    | crate::app::shell_network_setup::NetworkSetupRequest::MapStatus(_)
            )
        });
        service.setup_agreement = Some(agreement);
        let scope = service.setup_scope().expect("admitted options scope");
        super::maps::received_options(state, scope, random_map)?;
    }
    Ok(admitted)
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct ExtendedOptions {
    text: Vec<u8>,
    selected_map_file: String,
    agreement: crate::app::shell_network_setup::agreement::MultiplayerAgreement,
    source_sha256: String,
    effective_digest: String,
    random_map: Option<crate::app::shell_network_setup::maps::GeneratedMapManifest>,
    admissions: Vec<ExtendedAdmission>,
    /// VERA's room options, which the native options text has no field for.
    #[serde(default)]
    room: Option<crate::skirmish_launch::RoomOptions>,
    /// VMCE's map role assignments, keyed by the synchronized lobby row so a
    /// guest's local-first participant order cannot attach a role to the
    /// wrong person; the retail options string has no field for this
    /// project-specific state.
    #[cfg(feature = "vmce")]
    #[serde(default)]
    vmce_roles: Vec<VmceRoleRow>,
    /// Host's full series identity; campaign contents and projected config
    /// remain local to each peer.
    #[cfg(feature = "vmce")]
    #[serde(default)]
    vmce_series: Option<VmceSeriesIdentity>,
    /// One SHA-256 over the whole series directory: `campaign.ini` bytes,
    /// manifest order, and every manifest map's logical name, byte length and
    /// raw bytes. A guest may not report its map admission without it, so a
    /// peer whose later missions differ is refused before it can start.
    #[cfg(feature = "vmce")]
    #[serde(default)]
    vmce_series_content: Option<String>,
    /// A fixed next-stage identity, present throughout an intermission room.
    #[cfg(feature = "vmce")]
    #[serde(default)]
    vmce_intermission: Option<String>,
}

/// Host-authored admission projection for native Type3 assembly, derived from
/// Session and replaced with each complete snapshot. Only Session admits a
/// participant/address; a stale Type3 cannot replace the host's current proof.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct ExtendedAdmission {
    name: String,
    endpoint: SocketAddrV4,
    identity: super::PeerAdmission,
}

/// Exact received host snapshot, replaced on every admitted revision and
/// cleared with the visit. Session remains the editable options/roster owner.
pub(super) struct ReceivedOptions {
    scope: crate::net::lan_messages::SessionScope,
    options: ExtendedOptions,
}

#[cfg(feature = "vmce")]
impl ReceivedOptions {
    pub(super) fn scope(&self) -> crate::net::lan_messages::SessionScope {
        self.scope
    }

    pub(super) fn vmce_series_identity(&self) -> Option<super::vmce_setup::SeriesIdentity> {
        let identity = self.options.vmce_series.as_ref()?;
        Some(super::vmce_setup::SeriesIdentity {
            selected_map_file: self.options.selected_map_file.clone(),
            source_sha256: self.options.source_sha256.clone(),
            effective_digest: self.options.effective_digest.clone(),
            directory: identity.directory.clone(),
            manifest_sha256: identity.sha256.clone(),
            content_sha256: self.options.vmce_series_content.clone()?,
        })
    }
}

/// A failed host setup must not leave its partially applied revision or roster
/// in use. The modal aborts the room, so map-transfer and presentation state
/// are retired with the setup rather than reused for another attempt.
struct SetupCheckpoint {
    session: MatchSession,
    scope: Option<crate::net::lan_messages::SessionScope>,
    peers: std::collections::BTreeMap<SocketAddrV4, super::PeerAdmissionRecord>,
}

impl SetupCheckpoint {
    fn capture(session: &MatchSession, service: &super::LanService) -> Self {
        Self {
            session: session.clone(),
            scope: service.setup_scope,
            peers: service.peer_admissions.clone(),
        }
    }

    fn abort(self, session: &mut MatchSession, service: &mut super::LanService) {
        *session = self.session;
        // The old scope remains available for the room-leave exchange, but no
        // old or partially received agreement can authorize Ready or GO.
        service.setup_scope = self.scope;
        service.peer_admissions = self.peers;
        service.setup_agreement = None;
        service.received_options = None;
        #[cfg(feature = "vmce")]
        service.vmce_setup.reset();
        service.deferred_setup.retain(|request| {
            !matches!(
                request.request,
                crate::app::shell_network_setup::NetworkSetupRequest::Ready(_)
                    | crate::app::shell_network_setup::NetworkSetupRequest::MapStatus(_)
            )
        });
        let _ = session.invalidate_network_agreement();
    }
}

fn abort_received_setup(state: &mut AppState, checkpoint: SetupCheckpoint, error: &anyhow::Error) {
    log::error!("LAN host setup could not be applied: {error:#}");
    if let Some(service) = state.match_state.lan_service.as_mut() {
        checkpoint.abort(&mut state.match_state.session, service);
    }
    crate::app::shell_network_setup::fail_go_handoff_with(
        state,
        Some(crate::i18n::tr("VERA:LanSetupApplyFailed")),
    );
}

pub(super) fn reapply_received_options(state: &mut AppState) -> Result<()> {
    let Some(options) = state.match_state.lan_service.as_ref().and_then(|service| {
        let received = service.received_options.as_ref()?;
        (service.setup_scope() == Some(received.scope)).then(|| received.options.clone())
    }) else {
        return Ok(());
    };
    let checkpoint = SetupCheckpoint::capture(
        &state.match_state.session,
        state
            .match_state
            .lan_service
            .as_ref()
            .expect("received options own a LAN service"),
    );
    #[cfg(feature = "vmce")]
    let result = receive_options_text(
        state,
        &options.text,
        options.source_sha256,
        options.effective_digest,
        options.selected_map_file,
        options.room,
        options.vmce_roles,
        options.vmce_series,
        options.vmce_series_content,
        options.vmce_intermission,
    );
    #[cfg(not(feature = "vmce"))]
    let result = receive_options_text(
        state,
        &options.text,
        options.source_sha256,
        options.effective_digest,
        options.selected_map_file,
        options.room,
    );
    if let Err(error) = &result {
        abort_received_setup(state, checkpoint, error);
    } else {
        #[cfg(feature = "vmce")]
        super::vmce_setup::unlock_verified_intermission(state);
    }
    result
}

fn queue_payload(
    state: &mut AppState,
    kind: crate::net::lan_messages::extended_setup::SetupKind,
    payload: Vec<u8>,
) -> Result<bool> {
    let endpoints = super::remote_sources(state)
        .into_iter()
        .filter_map(|source| state.match_state.session.network_endpoint(source))
        .collect::<Vec<_>>();
    let game = super::active_game_name(state).ok_or_else(|| anyhow!("no active LAN game"))?;
    let mut scoped = game_name_hash(&game).to_le_bytes().to_vec();
    scoped.extend_from_slice(&payload);
    let service = state
        .match_state
        .lan_service
        .as_mut()
        .ok_or_else(|| anyhow!("no LAN service"))?;
    // Packet7 must already be in a new guest's receive queue before any
    // full setup can arrive. The same ACK boundary also serializes complete
    // extended snapshots across native global backpressure and reordering.
    if service.queued_setup_count() != 0 {
        return Ok(false);
    }
    let previous_scope = service.ensure_host_scope()?;
    service.setup_generation = service
        .setup_generation
        .checked_add(1)
        .ok_or_else(|| anyhow!("LAN setup generation exhausted"))?;
    let scope = crate::net::lan_messages::SessionScope {
        epoch: previous_scope.epoch,
        revision: service.setup_generation,
    };
    let parts = crate::net::lan_messages::extended_setup::encode(kind, scope, &scoped)?;
    let packets = endpoints
        .into_iter()
        .flat_map(|endpoint| {
            parts
                .iter()
                .map(move |part| (part.clone(), super::direct(endpoint)))
        })
        .collect::<Vec<_>>();
    let admitted = service.queue_setup(packets.into_iter())?;
    if admitted && kind == crate::net::lan_messages::extended_setup::SetupKind::Options {
        service.setup_scope = Some(scope);
    }
    Ok(admitted)
}

pub(super) fn receive_options(
    state: &mut AppState,
    source: SocketAddrV4,
    message: &LanMessage,
) -> Result<()> {
    validate_host(state, source, message)?;
    super::reject_setup_compatibility(state, &crate::i18n::tr("VERA:LanHostNoAgreement"));
    anyhow::bail!("unscoped legacy full setup is not a VERA admission")
}

fn receive_options_text(
    state: &mut AppState,
    text: &[u8],
    source_sha256: String,
    effective_digest: String,
    selected_map_file: String,
    room: Option<crate::skirmish_launch::RoomOptions>,
    #[cfg(feature = "vmce")] vmce_roles: Vec<VmceRoleRow>,
    #[cfg(feature = "vmce")] vmce_series: Option<VmceSeriesIdentity>,
    #[cfg(feature = "vmce")] vmce_series_content: Option<String>,
    #[cfg(feature = "vmce")] vmce_intermission: Option<String>,
) -> Result<()> {
    ensure!(
        source_sha256.is_empty() || crate::map::source::valid_source_sha256(&source_sha256),
        "invalid map source identity"
    );
    let (mut options, current_speed, ai_options) = parse_options(
        &state.match_state.session,
        &state.frontend.skirmish_modes,
        text,
    )?;
    #[cfg(feature = "vmce")]
    super::vmce_setup::validate_options_series_and_roles(
        &state.match_state.session,
        &mut options.launch,
        &selected_map_file,
        &source_sha256,
        vmce_series.as_ref(),
        vmce_series_content.as_deref(),
        &vmce_roles,
        vmce_intermission.as_deref(),
    )?;
    crate::map::source::validate_network_map_name(&selected_map_file)?;
    // VERA keeps the host's logical UTF-8 identity independently of the
    // bounded native text projection and the local SHA-named cache path.
    options.launch.selected_map_file = Some(selected_map_file.clone());
    options.map_metadata.source_sha256 = source_sha256.clone();
    options.map_metadata.effective_digest = effective_digest;
    if let Some(room) = room {
        options.launch.options.set_room_options(room);
    }
    #[cfg(feature = "vmce")]
    let vmce_effect = super::vmce_setup::prepare_received_options(
        state,
        &mut options,
        selected_map_file.clone(),
        &source_sha256,
        vmce_series,
        vmce_series_content,
        vmce_intermission,
    );
    crate::app::shell_network_setup::service_event(state, NetworkSetupEvent::HostSetup(options))?;
    #[cfg(feature = "vmce")]
    super::vmce_setup::commit_received_options(state, vmce_effect)?;
    state
        .match_state
        .session
        .accept_network_inactive_ai_options(ai_options)?;
    //5E4DE1 writes OptionsClass's applied speed independently of5E4E01's
    // selected setup speed. PregameSetup5E74AA copies the latter at launch;
    // the existing world-install projection then takes over this value.
    state
        .match_state
        .match_presentation
        .in_game_options
        .game_speed = current_speed as u32;
    Ok(())
}

/// Validate the LAN snapshot's typed fields without touching MatchSession or
/// LanService. `receive_extended` calls this only after host, scope, game,
/// agreement and admission proofs pass, but before committing that revision.
fn prevalidate_received_options(
    session: &MatchSession,
    modes: &[crate::skirmish_modes::SkirmishGameMode],
    options: &ExtendedOptions,
) -> Result<()> {
    ensure!(
        options.source_sha256.is_empty()
            || crate::map::source::valid_source_sha256(&options.source_sha256),
        "invalid map source identity"
    );
    let (mut parsed, _, _) = parse_options(session, modes, &options.text)?;
    #[cfg(feature = "vmce")]
    super::vmce_setup::validate_options_series_and_roles(
        session,
        &mut parsed.launch,
        &options.selected_map_file,
        &options.source_sha256,
        options.vmce_series.as_ref(),
        options.vmce_series_content.as_deref(),
        &options.vmce_roles,
        options.vmce_intermission.as_deref(),
    )?;
    crate::map::source::validate_network_map_name(&options.selected_map_file)?;
    Ok(())
}

pub(super) fn publish_roster(state: &mut AppState) -> Result<bool> {
    let session = &state.match_state.session;
    ensure!(session.host(), "only the host publishes a LAN roster");
    // A VERA room's spectator rows may lie past the native eight.
    let rows = if session.network_several_observers()
        || session
            .network_setup_capacity()
            .is_some_and(|capacity| capacity > 8)
    {
        NETWORK_PLAYER_SLOT_COUNT
    } else {
        8
    };
    let names = projected_roster(session)?;
    let names = &names[..rows];
    queue_payload(
        state,
        crate::net::lan_messages::extended_setup::SetupKind::Roster,
        serde_json::to_vec(names)?,
    )
}

pub(super) fn receive_extended(
    state: &mut AppState,
    source: SocketAddrV4,
    bytes: &[u8],
    now: u32,
) -> Result<()> {
    let host = state
        .match_state
        .lan_service
        .as_ref()
        .and_then(|service| service.admitted_host.as_ref())
        .ok_or_else(|| anyhow!("no admitted LAN host"))?
        .clone();
    validate_host_endpoint(&state.match_state.session, &host, source, &host.0)?;
    let scope = crate::net::lan_messages::extended_setup::fragment_scope(bytes)?;
    ensure!(
        state
            .match_state
            .lan_service
            .as_ref()
            .and_then(|service| service.setup_scope())
            .is_some_and(|expected| expected.epoch == scope.epoch),
        "extended setup belongs to an old LAN admission"
    );
    let complete = state
        .match_state
        .lan_service
        .as_mut()
        .expect("admitted LAN service")
        .setup_reassembly
        .receive(bytes, now)?;
    let Some((kind, bytes)) = complete else {
        return Ok(());
    };
    let game = super::active_game_name(state).ok_or_else(|| anyhow!("no active LAN game"))?;
    ensure!(
        bytes.len() >= 4 && bytes[..4] == game_name_hash(&game).to_le_bytes(),
        "different LAN game"
    );
    let bytes = &bytes[4..];
    match kind {
        crate::net::lan_messages::extended_setup::SetupKind::Options => {
            let options: ExtendedOptions = serde_json::from_slice(bytes)?;
            if options.agreement.version != crate::app::shell_network_setup::agreement::version() {
                super::reject_setup_compatibility(
                    state,
                    &crate::i18n::tr("VERA:LanHostBuildDiffers"),
                );
                anyhow::bail!("LAN gameplay build mismatch");
            }
            ensure!(
                crate::map::source::valid_source_sha256(&options.agreement.rules_digest),
                "invalid LAN rules agreement"
            );
            ensure!(
                options.admissions.len() <= NETWORK_PLAYER_SLOT_COUNT,
                "too many LAN admission proofs"
            );
            let mut endpoints = std::collections::BTreeSet::new();
            let mut names = std::collections::BTreeSet::new();
            for admission in &options.admissions {
                ensure!(
                    endpoints.insert(admission.endpoint)
                        && names.insert(admission.name.to_ascii_lowercase())
                        && !admission.name.eq_ignore_ascii_case(&host.0)
                        && admission.identity.join_nonce != [0; 16]
                        && admission.identity.admission_nonce != [0; 16],
                    "invalid LAN admission proof projection"
                );
            }
            // Preserve LAN identity and stale-revision checks above, then
            // validate the gameplay projection before changing any admission
            // or readiness state.
            let stale_revision = state
                .match_state
                .lan_service
                .as_ref()
                .and_then(|service| service.setup_scope())
                .is_some_and(|current| scope.revision < current.revision);
            if stale_revision {
                return Ok(());
            }
            let checkpoint = SetupCheckpoint::capture(
                &state.match_state.session,
                state.match_state.lan_service.as_ref().expect("visit"),
            );
            let result = (|| -> Result<()> {
                prevalidate_received_options(
                    &state.match_state.session,
                    &state.frontend.skirmish_modes,
                    &options,
                )?;
                {
                    let service = state.match_state.lan_service.as_mut().expect("visit");
                    service.setup_scope = Some(scope);
                    service.deferred_setup.retain(|request| {
                        !matches!(
                            request.request,
                            crate::app::shell_network_setup::NetworkSetupRequest::Ready(_)
                                | crate::app::shell_network_setup::NetworkSetupRequest::MapStatus(
                                    _
                                )
                        )
                    });
                    service.setup_agreement = Some(options.agreement.clone());
                    service.received_options = Some(ReceivedOptions {
                        scope,
                        options: options.clone(),
                    });
                    service.replace_peer_admissions(
                        options
                            .admissions
                            .iter()
                            .map(|admission| {
                                (
                                    admission.endpoint,
                                    admission.name.clone(),
                                    admission.identity,
                                )
                            })
                            .chain(std::iter::once((
                                host.1,
                                host.0.clone(),
                                super::PeerAdmission {
                                    join_nonce: [0; 16],
                                    admission_nonce: [0; 16],
                                },
                            ))),
                    );
                }
                // A full host snapshot also retires an old endpoint through the
                // same ParticipantLeft owner as Type10. Otherwise a lost departure
                // could retain a peer while its stale Type3 is correctly refused.
                let launch = state
                    .match_state
                    .session
                    .network_setup()
                    .ok_or_else(|| anyhow!("no setup"))?;
                let departed = launch
                    .humans
                    .iter()
                    .enumerate()
                    .filter_map(|(source, human)| {
                        if source == launch.local_player_index {
                            return None;
                        }
                        let endpoint = state.match_state.session.network_endpoint(source)?;
                        let retained = state
                            .match_state
                            .lan_service
                            .as_ref()
                            .expect("visit")
                            .peer_admissions
                            .get(&endpoint)
                            .is_some_and(|record| record.name.eq_ignore_ascii_case(&human.name));
                        (!retained).then_some(endpoint)
                    })
                    .collect::<Vec<_>>();
                for endpoint in departed {
                    if let Some(index) = state.match_state.session.network_endpoint_source(endpoint)
                    {
                        crate::app::shell_network_setup::service_event(
                            state,
                            NetworkSetupEvent::ParticipantLeft { index },
                        )?;
                    }
                }
                crate::app::shell_network_setup::invalidate_network_agreement(state)?;
                #[cfg(feature = "vmce")]
                receive_options_text(
                    state,
                    &options.text,
                    options.source_sha256,
                    options.effective_digest,
                    options.selected_map_file,
                    options.room,
                    options.vmce_roles,
                    options.vmce_series,
                    options.vmce_series_content,
                    options.vmce_intermission,
                )?;
                #[cfg(not(feature = "vmce"))]
                receive_options_text(
                    state,
                    &options.text,
                    options.source_sha256,
                    options.effective_digest,
                    options.selected_map_file,
                    options.room,
                )?;
                super::maps::received_options(state, scope, options.random_map)
            })();
            if let Err(error) = &result {
                abort_received_setup(state, checkpoint, error);
            } else {
                #[cfg(feature = "vmce")]
                super::vmce_setup::unlock_verified_intermission(state);
            }
            result
        }
        crate::net::lan_messages::extended_setup::SetupKind::Roster => {
            let names: Vec<String> = serde_json::from_slice(bytes)?;
            // A guest reaches the roster names through the host's extended
            // options, whose admissions carry every admitted endpoint. The
            // native Type3 channel is point-to-point here, so the host's
            // roster is also the only place a guest learns its peers.
            add_admitted_roster_participants(state, &names)?;
            let rows = parsed_roster(&state.match_state.session, &names)?;
            state.match_state.session.accept_network_roster(rows)?;
            // A stored options revision precedes this roster when the host
            // publishes both for one change: re-run its parser so positional
            // fields (VMCE roles) now reach every admitted row.
            reapply_received_options(state)?;
            state.platform.window.request_redraw();
            Ok(())
        }
    }
}

/// Add a human row for every admitted roster name the guest has not learned
/// through a direct Type3 packet. The host's options admissions are the
/// authority for that name's endpoint and already passed admission; the
/// roster only assigns its row.
fn add_admitted_roster_participants(state: &mut AppState, names: &[String]) -> Result<()> {
    if state.match_state.session.host() {
        return Ok(());
    }
    let Some(launch) = state.match_state.session.network_setup().cloned() else {
        return Ok(());
    };
    let template = state.frontend.multiplayer_shell_runtime.local_participant();
    let admissions: Vec<(SocketAddrV4, String)> = state
        .match_state
        .lan_service
        .as_ref()
        .map(|service| {
            service
                .peer_admissions
                .iter()
                .map(|(endpoint, record)| (*endpoint, record.name.clone()))
                .collect()
        })
        .unwrap_or_default();
    add_missing_admitted_humans(
        &mut state.match_state.session,
        &launch,
        &template,
        &admissions,
        names,
    )
}

fn add_missing_admitted_humans(
    session: &mut MatchSession,
    launch: &SkirmishLaunchSession,
    template: &crate::skirmish_launch::SkirmishHumanSlot,
    admissions: &[(SocketAddrV4, String)],
    names: &[String],
) -> Result<()> {
    for name in names {
        if matches!(
            name.as_str(),
            setup::OPEN
                | setup::CLOSED
                | setup::OPEN_OBSERVER
                | setup::EASY_AI
                | setup::MEDIUM_AI
                | setup::HARD_AI
        ) || launch.humans.iter().any(|human| human.name == *name)
        {
            continue;
        }
        let Some((endpoint, _)) = admissions
            .iter()
            .find(|(_, admitted)| admitted.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        let mut human = template.clone();
        human.name = name.clone();
        human.raw_slot_index = None;
        human.role = crate::skirmish_launch::LaunchHumanRole::Player { group: -2 };
        session.add_network_participant(human)?;
        session.bind_network_endpoint(name, *endpoint)?;
    }
    Ok(())
}

pub(super) fn receive_roster(
    state: &mut AppState,
    source: SocketAddrV4,
    message: &LanMessage,
) -> Result<()> {
    validate_host(state, source, message)?;
    anyhow::bail!("unscoped native roster is not a VERA admission")
}

fn validate_host(state: &AppState, source: SocketAddrV4, message: &LanMessage) -> Result<()> {
    let host = state
        .match_state
        .lan_service
        .as_ref()
        .and_then(|service| service.admitted_host.as_ref())
        .ok_or_else(|| anyhow!("no admitted LAN host"))?;
    validate_host_endpoint(&state.match_state.session, host, source, &message.name())
}

fn validate_host_endpoint(
    session: &MatchSession,
    host: &(String, SocketAddrV4),
    source: SocketAddrV4,
    name: &str,
) -> Result<()> {
    ensure!(
        !session.host(),
        "host does not receive guest-authored setup"
    );
    ensure!(
        source == host.1 && name.eq_ignore_ascii_case(&host.0),
        "setup is not from the admitted LAN host"
    );
    if let Some(index) = session.network_endpoint_source(source) {
        ensure!(
            session.network_setup().expect("bound participant").humans[index]
                .name
                .eq_ignore_ascii_case(&host.0),
            "host endpoint belongs to another participant"
        );
    }
    Ok(())
}

fn projected_options(
    session: &MatchSession,
    current_speed: i32,
    cooperative_difficulty: i32,
) -> Result<LanGameOptions<NETWORK_PLAYER_SLOT_COUNT>> {
    let launch = session
        .network_setup()
        .ok_or_else(|| anyhow!("no admitted setup"))?;
    let metadata = session
        .network_map_metadata()
        .ok_or_else(|| anyhow!("no selected map metadata"))?;
    let options = &launch.options;
    let humans = launch
        .humans
        .iter()
        .map(|human| LanOptionsHuman {
            name: human.name.clone(),
            country: super::country_value(human),
            color: super::color_value(human),
            start: start_value(human.slot.start_position),
            team: team_value(human.slot.team),
        })
        .collect();
    let ai = std::array::from_fn(|row| {
        let raw = session
            .network_ai_options(row as u8)
            .expect("eight setup rows");
        LanOptionsAi {
            difficulty: raw.difficulty,
            country: raw.country,
            color: raw.color,
            start: raw.start,
            team: raw.team,
        }
    });
    Ok(LanGameOptions {
        seed: session
            .network_start_seed()
            .ok_or_else(|| anyhow!("host seed is not initialized"))?,
        current_speed,
        game_speed: options.game_speed,
        credits: options.starting_credits,
        unit_count: options.unit_count,
        short_game: options.short_game,
        superweapons: options.super_weapons,
        build_off_ally: options.build_off_ally,
        mcv_redeploy: options.mcv_redeploy,
        crates: options.crates,
        mode_index: launch.mode.id,
        cooperative_difficulty,
        map_description: metadata.display_name.clone(),
        map_player_capacity: metadata.player_capacity,
        official_map: metadata.official,
        map_file_size: metadata.byte_length,
        scenario_filename: launch
            .selected_map_file
            .clone()
            .ok_or_else(|| anyhow!("no selected map filename"))?,
        map_digest: metadata.digest.clone(),
        humans,
        ai,
    })
}

///5E4DA6 uses comma-only strtok through the filename, colon-only for its
/// digest, then comma/colon for names and arrays. Widening bytes (not UTF-8)
/// preserves the packed map title for7B83E0 and the native narrow names.
fn parse_options(
    session: &MatchSession,
    modes: &[crate::skirmish_modes::SkirmishGameMode],
    bytes: &[u8],
) -> Result<(
    HostOptions,
    i32,
    [NetworkAiOptions; NETWORK_PLAYER_SLOT_COUNT],
)> {
    let text = widen_bytes(bytes);
    let (header, tail) = text
        .split_once(':')
        .ok_or_else(|| anyhow!("options have no participant section"))?;
    let mut fields = strtok(header, &[',']);
    let mut next = || {
        fields
            .next()
            .ok_or_else(|| anyhow!("truncated LAN options header"))
    };
    let seed = crt_atoi(next()?) as u32;
    let current_speed = crt_atoi(next()?);
    let mut launch = session
        .network_setup()
        .ok_or_else(|| anyhow!("no admitted setup"))?
        .clone();
    launch.options.game_speed = crt_atoi(next()?);
    launch.options.starting_credits = crt_atoi(next()?);
    launch.options.unit_count = crt_atoi(next()?);
    launch.options.short_game = crt_atoi(next()?) != 0;
    launch.options.super_weapons = crt_atoi(next()?) != 0;
    launch.options.build_off_ally = crt_atoi(next()?) != 0;
    launch.options.mcv_redeploy = crt_atoi(next()?) != 0;
    launch.options.crates = crt_atoi(next()?) != 0;
    let mode_id = crt_atoi(next()?);
    let mode = crate::skirmish_modes::mode_by_id(modes, mode_id)
        .ok_or_else(|| anyhow!("unknown LAN game mode {mode_id}"))?;
    launch.mode = SkirmishLaunchMode::from_game_mode(mode);
    let cooperative_difficulty = crt_atoi(next()?);
    let description = setup::unpack_map_description(&narrow_widened(next()?));
    let map_metadata = NetworkMapMetadata {
        display_name: String::from_utf16_lossy(&description),
        player_capacity: crt_atoi(next()?),
        official: crt_atoi(next()?) != 0,
        byte_length: crt_atoi(next()?),
        digest: String::new(),
        source_sha256: String::new(),
        effective_digest: String::new(),
    };
    launch.selected_map_file = Some(next()?.to_owned());
    let digest = next()?.to_owned();
    ensure!(fields.next().is_none(), "extra LAN options header fields");
    let map_metadata = NetworkMapMetadata {
        digest,
        ..map_metadata
    };
    let mut fields = strtok(tail, &[',', ':']);
    loop {
        let name = fields
            .next()
            .ok_or_else(|| anyhow!("options have no AI sentinel"))?;
        if name == "@" {
            break;
        }
        let mut next_int = || {
            fields
                .next()
                .map(crt_atoi)
                .ok_or_else(|| anyhow!("truncated human options"))
        };
        let country = next_int()?;
        let color = next_int()?;
        let start = next_int()?;
        let team = next_int()?;
        //5E58FD..5E5A45 consumes three network-address fields; LAN does not
        //use them. Endpoint authority remains the received type3 envelope.
        for _ in 0..3 {
            fields
                .next()
                .ok_or_else(|| anyhow!("truncated human address fields"))?;
        }
        if let Some(human) = launch.humans.iter_mut().find(|human| {
            widen_bytes(&narrow_utf16_c_string(human.name.encode_utf16()))
                .eq_ignore_ascii_case(name)
        }) {
            human.slot = decoded_choices(&human.slot, country, color, start, team)?;
            human.set_observer(country == -3);
        }
    }
    let mut opponents = Vec::new();
    let mut ai_options = [NetworkAiOptions {
        difficulty: 0,
        country: -1,
        color: -1,
        start: -2,
        team: -2,
    }; NETWORK_PLAYER_SLOT_COUNT];
    for row in 0..if map_metadata.player_capacity > 8 {
        NETWORK_PLAYER_SLOT_COUNT as u8
    } else {
        8
    } {
        let mut next_int = || {
            fields
                .next()
                .map(crt_atoi)
                .ok_or_else(|| anyhow!("truncated AI options"))
        };
        let difficulty = next_int()?;
        let country = next_int()?;
        let color = next_int()?;
        let start = next_int()?;
        let team = next_int()?;
        ai_options[usize::from(row)] = NetworkAiOptions {
            difficulty,
            country,
            color,
            start,
            team,
        };
        if country == -1 {
            continue;
        }
        let old = session.network_row_choices(row).expect("eight setup rows");
        let choices = decoded_choices(&old, country, color, start, team)?;
        opponents.push(SkirmishAiSlot {
            raw_slot_index: row,
            country: choices.country,
            country_random: choices.country_random,
            color_index: choices.color_index,
            color_random: choices.color_random,
            start_position: choices.start_position,
            team: choices.team,
            difficulty: decoded_difficulty(difficulty)?,
        });
    }
    ensure!(fields.next().is_none(), "extra LAN AI fields");
    launch.ai_player_count = opponents.len() as i32;
    launch.opponents = opponents;
    launch.rebuild_house_roster();
    let vacancies = std::array::from_fn(|row| {
        session
            .network_vacancy(row as u8)
            .unwrap_or(NetworkVacancy::Open)
    });
    Ok((
        HostOptions {
            launch,
            map_metadata,
            vacancies,
            observer_row: session.network_observer_row(),
            cooperative_difficulty,
            seed: Some(seed),
            #[cfg(feature = "vmce")]
            vmce_map_config: None,
        },
        current_speed,
        ai_options,
    ))
}

pub(super) fn decoded_choices(
    old: &SkirmishLocalSlot,
    country: i32,
    color: i32,
    start: i32,
    team: i32,
) -> Result<SkirmishLocalSlot> {
    let mut slot = old.clone();
    ensure!(
        country == -3 || country == -2 || (0..=i32::from(u16::MAX)).contains(&country),
        "country outside native registry domain"
    );
    slot.country_random = country == -2;
    if country >= 0 {
        slot.country = LaunchCountry::from_country_index(country as u32);
    } else if country == -2 {
        //696F99 clears the concrete country when SetCountry receives Random.
        //The AI-array parser5E5BAB..5E5C3C likewise replaces its word with -2; there
        //is no previous display-row country to retain in that representation.
        slot.country = LaunchCountry::America;
    }
    ensure!(
        color == -2 || (0..=i32::from(u8::MAX)).contains(&color),
        "color outside native slot domain"
    );
    slot.color_random = color == -2;
    if color >= 0 {
        slot.color_index = color as u8;
    } else {
        //696FC9 sets the concrete color to -1 for Random. The Rust slot
        //represents that sentinel with color_random; its unused u8 payload
        //has canonical value0, independent of whichever row was displayed.
        slot.color_index = 0;
    }
    slot.start_position = match start {
        -2 | -1 => LaunchStartPosition::Auto,
        0..=29 => LaunchStartPosition::Position(start as u8),
        _ => anyhow::bail!("start outside supported waypoint domain"),
    };
    slot.team = match team {
        -2 | -1 => LaunchTeam::None,
        0..=255 => LaunchTeam::Team(team as u8),
        _ => anyhow::bail!("team outside native slot domain"),
    };
    Ok(slot)
}

pub(super) fn start_value(start: LaunchStartPosition) -> i32 {
    match start {
        LaunchStartPosition::Auto => -2,
        LaunchStartPosition::Position(index) => i32::from(index),
    }
}
pub(super) fn team_value(team: LaunchTeam) -> i32 {
    match team {
        LaunchTeam::None => -2,
        LaunchTeam::Team(index) => i32::from(index),
    }
}
pub(super) fn decoded_difficulty(value: i32) -> Result<AiDifficulty> {
    match value {
        0 => Ok(AiDifficulty::Hard),
        1 => Ok(AiDifficulty::Normal),
        2 => Ok(AiDifficulty::Easy),
        _ => anyhow::bail!("invalid native AI difficulty"),
    }
}

fn projected_roster(session: &MatchSession) -> Result<[String; NETWORK_PLAYER_SLOT_COUNT]> {
    let launch = session
        .network_setup()
        .ok_or_else(|| anyhow!("no admitted setup"))?;
    Ok(std::array::from_fn(|row| {
        if let Some(human) = launch
            .humans
            .iter()
            .find(|human| human.raw_slot_index == Some(row as u8))
        {
            human.name.clone()
        } else if let Some(ai) = session.network_display_ai(row as u8) {
            match ai {
                AiDifficulty::Easy => setup::EASY_AI,
                AiDifficulty::Normal => setup::MEDIUM_AI,
                AiDifficulty::Hard => setup::HARD_AI,
            }
            .to_owned()
        } else {
            match session
                .network_vacancy(row as u8)
                .unwrap_or(NetworkVacancy::Closed)
            {
                NetworkVacancy::Open => setup::OPEN,
                NetworkVacancy::Closed => setup::CLOSED,
                NetworkVacancy::OpenObserver => setup::OPEN_OBSERVER,
                NetworkVacancy::Waiting => setup::WAITING,
            }
            .to_owned()
        }
    }))
}

fn parsed_roster(
    session: &MatchSession,
    names: &[String],
) -> Result<[NetworkRosterEntry; NETWORK_PLAYER_SLOT_COUNT]> {
    ensure!(
        names.len() <= NETWORK_PLAYER_SLOT_COUNT,
        "LAN roster exceeds thirty rows"
    );
    let launch = session
        .network_setup()
        .ok_or_else(|| anyhow!("no admitted setup"))?;
    let mut rows = [NetworkRosterEntry::Vacancy(NetworkVacancy::Open); NETWORK_PLAYER_SLOT_COUNT];
    for (row, name) in names.iter().enumerate() {
        rows[row] = match name.as_str() {
            setup::OPEN => NetworkRosterEntry::Vacancy(NetworkVacancy::Open),
            setup::CLOSED => NetworkRosterEntry::Vacancy(NetworkVacancy::Closed),
            setup::OPEN_OBSERVER => NetworkRosterEntry::Vacancy(NetworkVacancy::OpenObserver),
            setup::EASY_AI => NetworkRosterEntry::Ai(AiDifficulty::Easy),
            setup::MEDIUM_AI => NetworkRosterEntry::Ai(AiDifficulty::Normal),
            setup::HARD_AI => NetworkRosterEntry::Ai(AiDifficulty::Hard),
            _ => launch
                .humans
                .iter()
                .position(|human| human.name == *name)
                .map_or(
                    NetworkRosterEntry::Vacancy(NetworkVacancy::Waiting),
                    NetworkRosterEntry::Human,
                ),
        };
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::match_runtime::session::{NetworkReturnContext, NetworkRowKind};
    use crate::skirmish_launch::SkirmishLaunchSession;
    use std::net::Ipv4Addr;

    fn fixture(host: bool) -> MatchSession {
        let profile: serde_json::Value = serde_json::from_str(crate::test_fixture::text(
            "tools/map_observation.command-bar.example.json",
        ))
        .unwrap();
        let mut launch: SkirmishLaunchSession =
            serde_json::from_value(profile["launch"].clone()).unwrap();
        launch.opponents.clear();
        launch.ai_player_count = 0;
        launch.mode =
            SkirmishLaunchMode::from_game_mode(&crate::skirmish_modes::stock_skirmish_modes()[0]);
        launch.selected_map_file = Some("test.map".into());
        launch.humans.truncate(1);
        launch.humans[0].name = "Host".into();
        launch.humans[0].raw_slot_index = Some(0);
        launch.humans[0].slot.country_random = false;
        launch.humans[0].slot.country = LaunchCountry::America;
        launch.humans[0].slot.color_random = false;
        launch.humans[0].slot.color_index = 0;
        let mut guest = launch.humans[0].clone();
        guest.name = "Guest".into();
        guest.raw_slot_index = Some(1);
        guest.slot.color_index = 2;
        launch.humans.push(guest);
        launch.local_player_index = usize::from(!host);
        launch.options.game_speed = 4;
        launch.options.starting_credits = 9000;
        launch.rebuild_house_roster();
        let mut session = MatchSession::default();
        session.enter_network(NetworkReturnContext::LanLobby);
        session
            .accept_network_setup(launch, host, Some(7), false)
            .unwrap();
        session.set_network_start_seed(0xf1234567).unwrap();
        session
            .accept_network_map_metadata(
                NetworkMapMetadata {
                    display_name: "Høyde地".into(),
                    player_capacity: 8,
                    official: true,
                    byte_length: 123456,
                    digest: "0123456789abcdef0123456789abcdef".into(),
                    source_sha256: String::new(),
                    effective_digest: String::new(),
                },
                None,
                true,
            )
            .unwrap();
        session
    }

    #[cfg(feature = "vmce")]
    fn fixed_extended_options_wire_fixture() -> ExtendedOptions {
        use crate::app::shell_network_setup::agreement::MultiplayerAgreement;

        ExtendedOptions {
            text: b"17,4,1,9000,5,0,1,0,1,0,1,3,Test,2,1,123,stage01.map,0123456789abcdef:Host,1,2,0,0,0,0,0,@,".to_vec(),
            selected_map_file: "stage01/lda-2p.map".to_owned(),
            agreement: MultiplayerAgreement {
                version: "VERA20K-OPTIONS-FIXTURE/0.2.9-vmce.6".to_owned(),
                rules_digest: "a".repeat(64),
            },
            source_sha256: "b".repeat(64),
            effective_digest: "c".repeat(64),
            random_map: None,
            admissions: vec![
                ExtendedAdmission {
                    name: "Host".to_owned(),
                    endpoint: SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 10), 30123),
                    identity: super::super::PeerAdmission {
                        join_nonce: [0x11; 16],
                        admission_nonce: [0x22; 16],
                    },
                },
                ExtendedAdmission {
                    name: "Guest".to_owned(),
                    endpoint: SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 11), 30124),
                    identity: super::super::PeerAdmission {
                        join_nonce: [0x33; 16],
                        admission_nonce: [0x44; 16],
                    },
                },
            ],
            room: Some(crate::skirmish_launch::RoomOptions {
                bridges_destroyable: true,
                ally_change_allowed: false,
                bans: crate::sim::game_options::RoomBans::default(),
            }),
            vmce_roles: vec![
                VmceRoleRow {
                    row: 0,
                    role: crate::skirmish_launch::LaunchHumanRole::Player { group: 0 },
                },
                VmceRoleRow {
                    row: 1,
                    role: crate::skirmish_launch::LaunchHumanRole::Player { group: 1 },
                },
            ],
            vmce_series: Some(VmceSeriesIdentity {
                directory: "LDA-2p".to_owned(),
                sha256: "d".repeat(64),
            }),
            vmce_series_content: Some("e".repeat(64)),
            vmce_intermission: Some("stage02/lda-2p.map".to_owned()),
        }
    }

    /// Pin the production DTO bytes and their existing Options fragmenter to
    /// the output captured from B3. The native text's first token fixes seed 17.
    #[cfg(feature = "vmce")]
    #[test]
    fn extended_options_raw_json_matches_the_fixed_b3_wire_fixture() {
        let bytes = serde_json::to_vec(&fixed_extended_options_wire_fixture()).unwrap();
        assert_eq!(
            bytes,
            include_bytes!("fixtures/extended_options_wire_b3.json"),
            "the production ExtendedOptions serializer preserves the B3 wire bytes"
        );
        let fragments = crate::net::lan_messages::extended_setup::encode(
            crate::net::lan_messages::extended_setup::SetupKind::Options,
            crate::net::lan_messages::SessionScope {
                epoch: [0x5a; 16],
                revision: 7,
            },
            &bytes,
        )
        .unwrap();
        let mut complete_fragments = Vec::new();
        for fragment in fragments {
            complete_fragments.extend_from_slice(&fragment);
        }
        assert_eq!(
            crate::util::sha256::sha256_hex(&complete_fragments),
            "011dbf0762530da0542619b6a409db8aa82f5404fb746f178400568ef7a085b5",
            "fixed-scope Options fragments preserve the B3 wire bytes"
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn failed_vmce_options_restore_setup_and_revoke_agreement() {
        let mut session = fixture(false);
        let mut service = super::super::tests::service();
        let original_launch = session.network_setup().unwrap().clone();
        let original_scope = service.setup_scope.unwrap();
        service.setup_agreement = Some(
            crate::app::shell_network_setup::agreement::MultiplayerAgreement {
                version: crate::app::shell_network_setup::agreement::version(),
                rules_digest: "a".repeat(64),
            },
        );
        let checkpoint = SetupCheckpoint::capture(&session, &service);

        let mut incoming = original_launch.clone();
        incoming.mode.class = crate::skirmish_modes::MpModeClass::Vmce;
        incoming.humans[0].slot.color_index = 5;
        session
            .accept_network_host_setup(
                incoming,
                [NetworkVacancy::Open; NETWORK_PLAYER_SLOT_COUNT],
                8,
                None,
                false,
            )
            .unwrap();
        service.setup_scope.as_mut().unwrap().revision += 1;
        assert!(session.accept_network_vmce_map_config(None).is_err());

        checkpoint.abort(&mut session, &mut service);
        assert_eq!(session.network_setup(), Some(&original_launch));
        assert_eq!(service.setup_scope, Some(original_scope));
        let proof = crate::net::lan_messages::LanSessionProof {
            scope: original_scope,
            join_nonce: [0; 16],
            admission_nonce: [0; 16],
            version: crate::app::shell_network_setup::agreement::version(),
            rules_digest: "a".repeat(64),
        };
        assert!(!service.validate_peer_agreement(&proof));
        assert!(service.received_options.is_none());
    }

    /// The host's VMCE role projection keyed by synchronized seat, matching
    /// `publish_options`.
    #[cfg(feature = "vmce")]
    fn vmce_role_rows(session: &MatchSession) -> Vec<VmceRoleRow> {
        session
            .network_setup()
            .expect("admitted setup")
            .humans
            .iter()
            .filter_map(|human| {
                Some(VmceRoleRow {
                    row: human.raw_slot_index?,
                    role: human.role,
                })
            })
            .collect()
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn guest_role_only_change_republishes_to_other_guests() {
        use crate::skirmish_launch::LaunchHumanRole;

        let mut host = fixture(true);
        let mut launch = host.network_setup().unwrap().clone();
        launch.mode.class = crate::skirmish_modes::MpModeClass::Vmce;
        for human in &mut launch.humans {
            human.role = LaunchHumanRole::Player { group: -2 };
        }
        for row in 2..4 {
            let mut human = launch.humans[1].clone();
            human.name = format!("Guest{row}");
            human.raw_slot_index = Some(row);
            human.slot.color_index = row;
            launch.humans.push(human);
        }
        launch.rebuild_house_roster();
        host.accept_network_setup(launch, true, Some(7), false)
            .unwrap();
        host.set_network_vmce_role(0, LaunchHumanRole::Player { group: 0 })
            .unwrap();
        let unchanged = host.network_setup().unwrap().humans[1].slot.clone();
        let change = host
            .accept_lan_peer_choices(
                1,
                unchanged,
                false,
                Some(LaunchHumanRole::Player { group: 1 }),
            )
            .unwrap();
        assert!(change.options, "a role-only edit must publish host options");

        let mut other_guest = host.network_setup().unwrap().clone();
        other_guest.humans.rotate_left(2);
        for human in &mut other_guest.humans {
            human.role = LaunchHumanRole::Player { group: -2 };
        }
        apply_vmce_roles(&mut other_guest, &vmce_role_rows(&host)).unwrap();
        assert_eq!(
            other_guest
                .humans
                .iter()
                .find(|human| human.name == "Guest")
                .unwrap()
                .role,
            LaunchHumanRole::Player { group: 1 }
        );
        let unchanged = host.network_setup().unwrap().humans[1].slot.clone();
        let duplicate = host
            .accept_lan_peer_choices(
                1,
                unchanged,
                false,
                Some(LaunchHumanRole::Player { group: 1 }),
            )
            .unwrap();
        assert!(!duplicate.options, "a repeated role must not republish");
        let before = host.clone();
        let mut invalid_choices = host.network_setup().unwrap().humans[1].slot.clone();
        invalid_choices.color_index = 7;
        assert!(
            host.accept_lan_peer_choices(
                1,
                invalid_choices,
                false,
                Some(LaunchHumanRole::Player { group: 0 }),
            )
            .is_err()
        );
        assert_eq!(
            host, before,
            "a conflicting role must not change other choices"
        );
    }

    #[test]
    fn extended_options_and_roster_retain_thirtieth_participant() {
        let mut host = fixture(true);
        let mut launch = host.network_setup().unwrap().clone();
        let template = launch.humans[0].clone();
        launch.humans = (0..30)
            .map(|row| {
                let mut human = template.clone();
                human.name = format!("Player{row}");
                human.raw_slot_index = Some(row);
                human.slot.start_position = LaunchStartPosition::Position(row);
                human
            })
            .collect();
        launch.rebuild_house_roster();
        host.accept_network_setup(launch.clone(), true, None, false)
            .unwrap();
        host.select_network_map(launch.mode.clone(), "thirty.map".into(), 30)
            .unwrap();
        host.accept_network_map_metadata(
            NetworkMapMetadata {
                player_capacity: 30,
                ..host.network_map_metadata().unwrap().clone()
            },
            None,
            true,
        )
        .unwrap();
        let mut guest = fixture(false);
        launch.local_player_index = 29;
        guest
            .accept_network_setup(launch, false, None, false)
            .unwrap();
        let payload = projected_options(&host, 3, 0).unwrap().encode();
        let parts = crate::net::lan_messages::extended_setup::encode(
            crate::net::lan_messages::extended_setup::SetupKind::Options,
            crate::net::lan_messages::SessionScope {
                epoch: [7; 16],
                revision: 1,
            },
            &payload,
        )
        .unwrap();
        assert!(parts.len() > 1);
        let mut receive = crate::net::lan_messages::extended_setup::Reassembly::default();
        let mut complete = None;
        for part in parts.iter().rev() {
            complete = receive.receive(part, 1).unwrap().or(complete);
        }
        let (_, bytes) = complete.unwrap();
        let (options, _, ai) = parse_options(
            &guest,
            &crate::skirmish_modes::stock_skirmish_modes(),
            &bytes,
        )
        .unwrap();
        guest
            .accept_network_host_setup(options.launch, options.vacancies, 30, None, false)
            .unwrap();
        guest.accept_network_inactive_ai_options(ai).unwrap();
        let names = projected_roster(&host).unwrap();
        guest
            .accept_network_roster(parsed_roster(&guest, &names).unwrap())
            .unwrap();
        assert_eq!(
            guest.network_setup().unwrap().local_player().name,
            "Player29"
        );
        assert_eq!(
            guest.network_setup().unwrap().local_player().raw_slot_index,
            Some(29)
        );
        assert_eq!(guest.network_setup_capacity(), Some(30));
    }

    /// A guest learns its peers from the host's admitted roster: every name
    /// with a host options admission becomes a real participant row, so the
    /// next roster bind can place it.
    #[test]
    fn a_guest_adds_admitted_roster_names_it_has_not_met() {
        let mut guest = fixture(false);
        let template = guest.network_setup().unwrap().local_player().clone();
        let launch = guest.network_setup().unwrap().clone();
        let names = vec![
            "Host".to_owned(),
            "Guest".to_owned(),
            "Guest2".to_owned(),
            "Guest3".to_owned(),
        ];
        let second = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 30102);
        let third = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 30103);
        let admissions = vec![(second, "Guest2".to_owned()), (third, "Guest3".to_owned())];
        add_missing_admitted_humans(&mut guest, &launch, &template, &admissions, &names).unwrap();
        let launch = guest.network_setup().unwrap();
        assert_eq!(launch.humans.len(), 4);
        let index = |name: &str| {
            launch
                .humans
                .iter()
                .position(|human| human.name == name)
                .unwrap()
        };
        assert_eq!(guest.network_endpoint(index("Guest2")), Some(second));
        assert_eq!(guest.network_endpoint(index("Guest3")), Some(third));
        assert!(
            guest
                .network_setup()
                .unwrap()
                .humans
                .iter()
                .filter(|human| human.name != "Host" && human.name != "Guest")
                .all(|human| human.raw_slot_index.is_none()),
            "the roster bind assigns rows, not the admission"
        );
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_wire_roles_reject_observers_even_before_roster_binding() {
        let mut launch = fixture(false).network_setup().unwrap().clone();
        launch.mode.class = crate::skirmish_modes::MpModeClass::Vmce;
        launch.humans[1].set_observer(true);
        assert!(apply_vmce_roles(&mut launch, &[]).is_err());

        launch.humans[1].set_observer(false);
        let role = VmceRoleRow {
            row: 7,
            role: crate::skirmish_launch::LaunchHumanRole::Observer,
        };
        assert!(apply_vmce_roles(&mut launch, &[role]).is_err());
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_role_roster_round_trips_in_extended_host_options() {
        use crate::skirmish_launch::LaunchHumanRole;
        let mut host = fixture(true);
        let modes = crate::skirmish_modes::stock_skirmish_modes();
        let mode = modes.iter().find(|mode| mode.class.is_vmce()).unwrap();
        let mut launch = host.network_setup().unwrap().clone();
        launch.mode = SkirmishLaunchMode::from_game_mode(mode);
        for human in &mut launch.humans {
            human.role = LaunchHumanRole::Player { group: -2 };
        }
        host.accept_network_setup(launch, true, Some(7), false)
            .unwrap();
        host.set_network_vmce_role(0, LaunchHumanRole::Player { group: 0 })
            .unwrap();
        let roles = vmce_role_rows(&host);
        let projected = projected_options(&host, 3, 0).unwrap();
        let snapshot = ExtendedOptions {
            text: projected.encode(),
            selected_map_file: "test.map".into(),
            agreement: crate::app::shell_network_setup::agreement::MultiplayerAgreement {
                version: "test".into(),
                rules_digest: "test".into(),
            },
            source_sha256: String::new(),
            effective_digest: String::new(),
            random_map: None,
            admissions: Vec::new(),
            room: None,
            vmce_roles: roles,
            vmce_series: None,
            vmce_series_content: None,
            vmce_intermission: None,
        };
        let snapshot: ExtendedOptions =
            serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let mut guest = fixture(false);
        let (mut received, _, _) = parse_options(&guest, &modes, &snapshot.text).unwrap();
        let teams = received
            .launch
            .humans
            .iter()
            .map(|human| human.slot.team)
            .collect::<Vec<_>>();
        apply_vmce_roles(&mut received.launch, &snapshot.vmce_roles).unwrap();
        guest
            .accept_network_host_setup(
                received.launch,
                received.vacancies,
                received.map_metadata.player_capacity,
                None,
                false,
            )
            .unwrap();
        let guest = guest.network_setup().unwrap();
        let by_row = |row: u8| {
            guest
                .humans
                .iter()
                .find(|human| human.raw_slot_index == Some(row))
                .expect("bound role row")
        };
        assert_eq!(by_row(0).role.native_value(), 0);
        assert_eq!(by_row(1).role.native_value(), -2);
        assert_eq!(
            guest
                .humans
                .iter()
                .map(|human| human.slot.team)
                .collect::<Vec<_>>(),
            teams
        );
    }

    /// Regression: `publish_options` projects roles in the host's participant
    /// order, but a LAN guest admits participants local-first (its own row
    /// first). Keying the roster by the synchronized seat keeps every role on
    /// its own participant instead of mirroring the two peers.
    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_roles_follow_the_synchronized_seat_on_a_local_first_guest() {
        use crate::skirmish_launch::LaunchHumanRole;
        let modes = crate::skirmish_modes::stock_skirmish_modes();
        let mode = modes.iter().find(|mode| mode.class.is_vmce()).unwrap();
        let mut host = fixture(true);
        let mut launch = host.network_setup().unwrap().clone();
        launch.mode = SkirmishLaunchMode::from_game_mode(mode);
        for human in &mut launch.humans {
            human.role = LaunchHumanRole::Player { group: -2 };
        }
        host.accept_network_setup(launch, true, Some(7), false)
            .unwrap();
        host.set_network_vmce_role(0, LaunchHumanRole::Player { group: 0 })
            .unwrap();
        host.set_network_vmce_role(1, LaunchHumanRole::Player { group: 1 })
            .unwrap();
        let snapshot = ExtendedOptions {
            text: projected_options(&host, 3, 0).unwrap().encode(),
            selected_map_file: "test.map".into(),
            agreement: crate::app::shell_network_setup::agreement::MultiplayerAgreement {
                version: "test".into(),
                rules_digest: "test".into(),
            },
            source_sha256: String::new(),
            effective_digest: String::new(),
            random_map: None,
            admissions: Vec::new(),
            room: None,
            vmce_roles: vmce_role_rows(&host),
            vmce_series: None,
            vmce_series_content: None,
            vmce_intermission: None,
        };

        // The guest's admitted source order is its own row first, the host's
        // row second: the reverse of the host's list.
        let mut guest = MatchSession::default();
        guest.enter_network(NetworkReturnContext::LanLobby);
        let mut guest_launch = host.network_setup().unwrap().clone();
        guest_launch.humans.swap(0, 1);
        guest_launch.local_player_index = 0;
        guest_launch.rebuild_house_roster();
        guest
            .accept_network_setup(guest_launch, false, Some(7), false)
            .unwrap();

        let (mut received, _, _) = parse_options(&guest, &modes, &snapshot.text).unwrap();
        apply_vmce_roles(&mut received.launch, &snapshot.vmce_roles).unwrap();
        guest
            .accept_network_host_setup(
                received.launch,
                received.vacancies,
                received.map_metadata.player_capacity,
                None,
                false,
            )
            .unwrap();

        let guest = guest.network_setup().unwrap();
        assert_eq!(guest.humans[0].name, "Guest");
        assert_eq!(guest.humans[0].raw_slot_index, Some(1));
        assert_eq!(guest.humans[0].role.native_value(), 1);
        assert_eq!(guest.humans[1].name, "Host");
        assert_eq!(guest.humans[1].raw_slot_index, Some(0));
        assert_eq!(guest.humans[1].role.native_value(), 0);
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_role_roster_lagging_the_local_roster_defers_instead_of_dropping() {
        use crate::skirmish_launch::LaunchHumanRole;
        let modes = crate::skirmish_modes::stock_skirmish_modes();
        let mode = modes.iter().find(|mode| mode.class.is_vmce()).unwrap();
        let mut host = fixture(true);
        let mut launch = host.network_setup().unwrap().clone();
        launch.mode = SkirmishLaunchMode::from_game_mode(mode);
        for human in &mut launch.humans {
            human.role = LaunchHumanRole::Player { group: -2 };
        }
        host.accept_network_setup(launch, true, Some(7), false)
            .unwrap();
        // A guest can receive the options snapshot before its roster revision,
        // so take a one-row baseline from the all-Random roster first.
        let mut lagging = host.network_setup().unwrap().clone();
        lagging.humans.truncate(1);
        lagging.local_player_index = 0;
        let complete = host.network_setup().unwrap().clone();
        host.set_network_vmce_role(0, LaunchHumanRole::Player { group: 0 })
            .unwrap();
        host.set_network_vmce_role(1, LaunchHumanRole::Player { group: 1 })
            .unwrap();
        let roles = vmce_role_rows(&host);

        // A roster that does not yet carry every published seat defers the
        // whole assignment without refusing the packet, so the stored
        // revision can be re-applied once it arrives.
        apply_vmce_roles(&mut lagging, &roles).unwrap();
        assert_eq!(
            lagging.humans[0].role,
            LaunchHumanRole::Player { group: -2 }
        );

        // The complete roster applies each role to its own synchronized row.
        let mut applied = complete;
        apply_vmce_roles(&mut applied, &roles).unwrap();
        assert_eq!(applied.humans[0].role, LaunchHumanRole::Player { group: 0 });
        assert_eq!(applied.humans[1].role, LaunchHumanRole::Player { group: 1 });
    }

    #[test]
    fn full_options_preserve_native_text_and_apply_to_existing_participants() {
        let mut host = fixture(true);
        host.set_network_row_kind(3, NetworkRowKind::Ai(AiDifficulty::Easy))
            .unwrap();
        let mut choices = host.network_row_choices(3).unwrap();
        choices.country = LaunchCountry::Yuri;
        choices.country_random = false;
        choices.color_index = 5;
        choices.color_random = false;
        choices.start_position = LaunchStartPosition::Position(6);
        choices.team = LaunchTeam::Team(2);
        host.set_network_row_choices(3, choices).unwrap();
        let wire = projected_options(&host, 2, 1).unwrap();
        let packet =
            LanMessage::options("Host", &wire.encode(), 0, game_name_hash("Host")).unwrap();
        let mut guest = fixture(false);
        guest.set_network_player_ready(0, true).unwrap();
        let old_names: Vec<_> = guest
            .network_setup()
            .unwrap()
            .humans
            .iter()
            .map(|h| h.name.clone())
            .collect();
        let (parsed, current_speed, ai_options) = parse_options(
            &guest,
            &crate::skirmish_modes::stock_skirmish_modes(),
            packet.options_fields().unwrap().text,
        )
        .unwrap();
        assert_eq!(current_speed, 2);
        assert_eq!(
            ai_options[0].start, -2,
            "human row choices do not replace AI arrays"
        );
        assert_eq!(parsed.launch.options.game_speed, 4);
        assert_eq!(parsed.map_metadata.display_name, "Høyde地");
        assert_eq!(parsed.map_metadata.byte_length, 123456);
        assert_eq!(parsed.seed, Some(0xf1234567));
        assert_eq!(parsed.launch.opponents[0].raw_slot_index, 3);
        assert_eq!(parsed.launch.opponents[0].country, LaunchCountry::Yuri);
        assert_eq!(parsed.launch.opponents[0].difficulty, AiDifficulty::Easy);
        assert_eq!(
            parsed.launch.opponents[0].start_position,
            LaunchStartPosition::Position(6)
        );
        guest
            .accept_network_host_setup(
                parsed.launch,
                parsed.vacancies,
                parsed.map_metadata.player_capacity,
                parsed.observer_row,
                false,
            )
            .unwrap();
        assert_eq!(
            guest
                .network_setup()
                .unwrap()
                .humans
                .iter()
                .map(|h| h.name.clone())
                .collect::<Vec<_>>(),
            old_names
        );
        assert!(
            guest.network_player_ready(0),
            "full setup does not replace source acknowledgements"
        );
    }

    #[test]
    fn options_use_crt_tokens_and_reject_truncated_input_without_mutation() {
        let host = fixture(true);
        let wire = projected_options(&host, 3, 2).unwrap().encode();
        let text = widen_bytes(&wire).replacen("9000,", "9000credits,,", 1);
        let guest = fixture(false);
        let before = guest.clone();
        let (parsed, _, _) = parse_options(
            &guest,
            &crate::skirmish_modes::stock_skirmish_modes(),
            &narrow_widened(&text),
        )
        .unwrap();
        assert_eq!(parsed.launch.options.starting_credits, 9000);
        assert_eq!(parsed.seed, Some(0xf1234567));
        assert!(
            parse_options(
                &guest,
                &crate::skirmish_modes::stock_skirmish_modes(),
                b"1,2,3"
            )
            .is_err()
        );
        assert_eq!(guest, before);
    }

    #[test]
    fn roster_rebinds_rows_without_creating_nodes_and_late_type3_fills_waiting() {
        let mut guest = fixture(false);
        let host_address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 1234);
        guest.bind_network_endpoint("Host", host_address).unwrap();
        guest.set_network_player_ready(0, true).unwrap();
        let names = [
            "Guest",
            setup::OPEN,
            "Late",
            setup::HARD_AI,
            "Host",
            setup::CLOSED,
            setup::OPEN,
            setup::OPEN_OBSERVER,
        ]
        .map(str::to_owned);
        guest
            .accept_network_roster(parsed_roster(&guest, &names).unwrap())
            .unwrap();
        let launch = guest.network_setup().unwrap();
        assert_eq!(launch.humans.len(), 2);
        assert_eq!(launch.humans[0].name, "Host");
        assert_eq!(launch.humans[0].raw_slot_index, Some(4));
        assert_eq!(launch.local_player_index, 1);
        assert_eq!(launch.local_player().raw_slot_index, Some(0));
        assert!(guest.network_player_ready(0));
        assert_eq!(guest.network_endpoint(0), Some(host_address));
        assert_eq!(guest.network_vacancy(2), Some(NetworkVacancy::Waiting));
        assert_eq!(projected_roster(&guest).unwrap()[2], setup::WAITING);
        let mut late = launch.humans[0].clone();
        late.name = "Late".into();
        late.raw_slot_index = Some(2);
        guest.add_network_participant(late).unwrap();
        assert_eq!(guest.network_setup().unwrap().humans.len(), 3);
        assert_eq!(guest.network_vacancy(2), None);
        assert_eq!(projected_roster(&guest).unwrap()[2], "Late");
    }

    #[test]
    fn duplicate_roster_is_atomic_and_host_endpoint_must_match() {
        let mut guest = fixture(false);
        let before = guest.clone();
        let names = ["Guest", "Host", "Host"].map(str::to_owned);
        assert!(
            guest
                .accept_network_roster(parsed_roster(&guest, &names).unwrap())
                .is_err()
        );
        assert_eq!(guest, before);
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 1234);
        let host = ("Host".into(), address);
        assert!(validate_host_endpoint(&guest, &host, address, "Host").is_ok());
        assert!(
            validate_host_endpoint(
                &guest,
                &host,
                SocketAddrV4::new(Ipv4Addr::LOCALHOST, 1236),
                "Host"
            )
            .is_err()
        );
        assert!(validate_host_endpoint(&guest, &host, address, "Guest").is_err());
        assert!(validate_host_endpoint(&fixture(true), &host, address, "Host").is_err());
    }

    #[test]
    fn join_and_peer_changes_share_human_and_ai_color_occupancy() {
        let mut host = fixture(true);
        assert!(host.network_color_in_use(2));
        assert!(!host.network_color_in_use(5));
        host.set_network_row_kind(3, NetworkRowKind::Ai(AiDifficulty::Hard))
            .unwrap();
        let mut choices = host.network_row_choices(3).unwrap();
        choices.color_index = 5;
        choices.color_random = false;
        host.set_network_row_choices(3, choices).unwrap();
        assert!(host.network_color_in_use(5));
    }

    #[test]
    fn inactive_array_words_survive_options_separately_from_human_choices() {
        let mut host = fixture(true);
        host.set_network_row_kind(3, NetworkRowKind::Ai(AiDifficulty::Easy))
            .unwrap();
        host.set_network_row_kind(3, NetworkRowKind::Open).unwrap();
        let mut human_choices = host.network_setup().unwrap().local_slot().clone();
        human_choices.start_position = LaunchStartPosition::Position(5);
        human_choices.team = LaunchTeam::Team(2);
        host.set_network_row_choices(0, human_choices).unwrap();
        let wire = projected_options(&host, 3, 0).unwrap();
        assert_eq!(wire.humans[0].start, 5);
        assert_eq!(wire.ai[0].start, -2);
        assert_eq!(
            wire.ai[3],
            LanOptionsAi {
                difficulty: -1,
                country: -1,
                color: -1,
                start: -1,
                team: -2
            }
        );
        let mut guest = fixture(false);
        let (options, _, arrays) = parse_options(
            &guest,
            &crate::skirmish_modes::stock_skirmish_modes(),
            &wire.encode(),
        )
        .unwrap();
        guest
            .accept_network_host_setup(
                options.launch,
                options.vacancies,
                options.map_metadata.player_capacity,
                options.observer_row,
                false,
            )
            .unwrap();
        guest.accept_network_inactive_ai_options(arrays).unwrap();
        assert_eq!(
            guest.network_ai_options(3).unwrap(),
            NetworkAiOptions {
                difficulty: -1,
                country: -1,
                color: -1,
                start: -1,
                team: -2
            }
        );
        assert_eq!(
            guest.network_setup().unwrap().humans[0].slot.start_position,
            LaunchStartPosition::Position(5)
        );
    }

    #[test]
    fn reordered_roster_and_options_keep_display_and_ai_arrays_independent() {
        let mut host = fixture(true);
        let mut launch = host.network_setup().unwrap().clone();
        launch.humans[1].raw_slot_index = Some(2);
        host.accept_network_setup(launch, true, Some(7), false)
            .unwrap();
        host.set_network_row_kind(1, NetworkRowKind::Ai(AiDifficulty::Hard))
            .unwrap();
        let options = projected_options(&host, 3, 0).unwrap().encode();
        let roster = projected_roster(&host).unwrap();
        let mut final_states = Vec::new();
        for roster_first in [false, true] {
            let mut guest = fixture(false);
            let apply_roster = |guest: &mut MatchSession| {
                guest
                    .accept_network_roster(parsed_roster(guest, &roster).unwrap())
                    .unwrap();
            };
            if roster_first {
                apply_roster(&mut guest);
                assert_eq!(guest.network_display_ai(1), Some(AiDifficulty::Hard));
                assert!(
                    guest.network_setup().unwrap().opponents.is_empty(),
                    "roster does not write gameplay AI arrays"
                );
            }
            let (parsed, _, raw_ai) = parse_options(
                &guest,
                &crate::skirmish_modes::stock_skirmish_modes(),
                &options,
            )
            .unwrap();
            guest
                .accept_network_host_setup(
                    parsed.launch,
                    parsed.vacancies,
                    parsed.map_metadata.player_capacity,
                    parsed.observer_row,
                    false,
                )
                .unwrap();
            guest.accept_network_inactive_ai_options(raw_ai).unwrap();
            if !roster_first {
                assert_eq!(
                    guest.network_setup().unwrap().humans[1].raw_slot_index,
                    Some(1)
                );
                assert_eq!(
                    guest.network_setup().unwrap().opponents[0].raw_slot_index,
                    1,
                    "options may precede the new display bindings"
                );
                assert_eq!(guest.network_display_ai(1), None);
                apply_roster(&mut guest);
            }
            assert_eq!(
                guest.network_setup().unwrap().humans[1].raw_slot_index,
                Some(2)
            );
            let ai = &guest.network_setup().unwrap().opponents[0];
            assert!(ai.country_random && ai.color_random);
            assert_eq!(ai.country.raw_index(), 0);
            assert_eq!(ai.color_index, 0);
            final_states.push(guest);
        }
        assert_eq!(final_states[0], final_states[1]);
    }

    #[test]
    fn lan_peer_policy_keeps_direct_team_and_shared_collision_checks() {
        let mut lan = fixture(true);
        let mut own = lan.network_setup().unwrap().local_slot().clone();
        own.start_position = LaunchStartPosition::Position(3);
        lan.set_network_row_choices(0, own.clone()).unwrap();
        let mut request = lan.network_setup().unwrap().humans[1].slot.clone();
        request.color_index = own.color_index;
        request.color_random = false;
        request.start_position = own.start_position;
        request.team = LaunchTeam::Team(7);
        request.country = LaunchCountry::Yuri;
        request.country_random = false;
        let mut wol = lan.clone();
        let result = lan
            .accept_lan_peer_choices(1, request.clone(), false, None)
            .unwrap();
        assert!(result.options);
        assert!(!result.roster);
        let slot = &lan.network_setup().unwrap().humans[1].slot;
        assert!(slot.color_random);
        // VERA shared control: the guest shares the host's start and House.
        assert_eq!(slot.start_position, LaunchStartPosition::Position(3));
        assert_eq!(slot.team, LaunchTeam::Team(7));
        assert_eq!(slot.country, LaunchCountry::Yuri);
        wol.accept_network_peer_choices(1, request, false, Some(&[LaunchCountry::America]))
            .unwrap();
        let slot = &wol.network_setup().unwrap().humans[1].slot;
        assert_eq!(slot.team, LaunchTeam::None);
        assert_eq!(slot.country, LaunchCountry::America);
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn lan_coop_snapshot_carries_only_campaign_identity_while_other_modes_keep_none() {
        let mut launch = fixture(true).network_setup().unwrap().clone();
        launch.mode.class = crate::skirmish_modes::MpModeClass::Vmce;
        launch.selected_map_file = Some("test.map".into());
        let identity = VmceSeriesIdentity {
            directory: "campaign-a".into(),
            sha256: "a".repeat(64),
        };
        let local_config = crate::app::match_bootstrap::NetworkVmceMapConfig::new(
            "test.map".into(),
            "a".repeat(64),
            crate::map::vmce_config::VmceMapConfig::default(),
        )
        .unwrap()
        .with_series_identity(identity.directory.clone(), identity.sha256.clone())
        .unwrap();
        assert!(campaign_identity_matches(
            &identity,
            &local_config,
            &identity.sha256
        ));
        let mut stale_identity = identity.clone();
        stale_identity.sha256 = "b".repeat(64);
        assert!(!campaign_identity_matches(
            &identity,
            &local_config,
            &stale_identity.sha256
        ));
        let changed_full_campaign = crate::app::match_bootstrap::NetworkVmceMapConfig::new(
            "test.map".into(),
            "a".repeat(64),
            crate::map::vmce_config::VmceMapConfig::default(),
        )
        .unwrap()
        .with_series_identity("campaign-a".into(), "b".repeat(64))
        .unwrap();
        assert_ne!(
            local_config, changed_full_campaign,
            "full campaign changes must invalidate Coop readiness even if the current overlay is equal"
        );
        let content = "d".repeat(64);
        assert!(
            validate_vmce_series(
                true,
                &launch,
                "test.map",
                &"a".repeat(64),
                Some(&identity),
                Some(&content),
            )
            .is_ok()
        );
        // A VMCE LAN revision must authenticate the whole series: an absent
        // identity or absent/invalid content digest is refused, never
        // silently accepted as an unverified series.
        assert!(
            validate_vmce_series(
                true,
                &launch,
                "test.map",
                &"a".repeat(64),
                Some(&identity),
                None
            )
            .is_err()
        );
        assert!(
            validate_vmce_series(
                true,
                &launch,
                "test.map",
                &"a".repeat(64),
                None,
                Some(&content)
            )
            .is_err()
        );
        assert!(
            validate_vmce_series(
                true,
                &launch,
                "test.map",
                &"a".repeat(64),
                Some(&identity),
                Some("bad")
            )
            .is_err()
        );
        // A non-LAN (WOL) revision keeps the identity optional: VMCE map sync
        // there is refused later by its own owner.
        assert!(
            validate_vmce_series(false, &launch, "test.map", &"a".repeat(64), None, None).is_ok()
        );
        assert!(
            validate_vmce_series(
                true,
                &launch,
                "../other.map",
                &"a".repeat(64),
                Some(&identity),
                Some(&content)
            )
            .is_err()
        );
        let mut tampered = identity.clone();
        tampered.sha256 = "bad".into();
        assert!(
            validate_vmce_series(
                true,
                &launch,
                "test.map",
                &"a".repeat(64),
                Some(&tampered),
                Some(&content)
            )
            .is_err()
        );

        launch.mode.class = crate::skirmish_modes::MpModeClass::Battle;
        assert!(
            validate_vmce_series(true, &launch, "test.map", &"a".repeat(64), None, None).is_ok()
        );
        assert!(
            validate_vmce_series(
                true,
                &launch,
                "test.map",
                &"a".repeat(64),
                Some(&identity),
                Some(&content)
            )
            .is_err()
        );

        let payload = serde_json::to_value(ExtendedOptions {
            text: Vec::new(),
            selected_map_file: "test.map".into(),
            agreement: crate::app::shell_network_setup::agreement::MultiplayerAgreement {
                version: "test".into(),
                rules_digest: "a".repeat(64),
            },
            source_sha256: "a".repeat(64),
            effective_digest: "b".repeat(64),
            random_map: None,
            admissions: Vec::new(),
            room: None,
            vmce_roles: Vec::new(),
            vmce_series: Some(identity),
            vmce_series_content: Some(content.clone()),
            vmce_intermission: None,
        })
        .unwrap();
        assert!(payload.get("vmce_series").is_some());
        assert_eq!(
            payload.get("vmce_series_content").and_then(|v| v.as_str()),
            Some(content.as_str()),
            "the whole-series digest round-trips as its own field"
        );
        assert!(payload.get("vmce_map_config").is_none());
        let mut old_host = payload;
        old_host.as_object_mut().unwrap().remove("vmce_series");
        old_host
            .as_object_mut()
            .unwrap()
            .remove("vmce_series_content");
        let old_snapshot: ExtendedOptions = serde_json::from_value(old_host).unwrap();
        assert!(old_snapshot.vmce_series.is_none());
        assert!(old_snapshot.vmce_series_content.is_none());
    }

    #[cfg(feature = "vmce")]
    #[test]
    fn received_coop_identity_prevalidation_leaves_admitted_session_unchanged() {
        use crate::skirmish_launch::LaunchHumanRole;

        let modes = crate::skirmish_modes::stock_skirmish_modes();
        let coop_mode = modes.iter().find(|mode| mode.class.is_vmce()).unwrap();
        let mut host = fixture(true);
        let mut launch = host.network_setup().unwrap().clone();
        launch.mode = SkirmishLaunchMode::from_game_mode(coop_mode);
        for (index, human) in launch.humans.iter_mut().enumerate() {
            human.role = LaunchHumanRole::Player {
                group: index as i32,
            };
        }
        host.accept_network_setup(launch, true, Some(7), false)
            .unwrap();
        let text = projected_options(&host, 2, 0).unwrap().encode();
        let source_sha256 = "a".repeat(64);
        let identity = VmceSeriesIdentity {
            directory: "campaign-a".into(),
            sha256: "c".repeat(64),
        };
        let snapshot = ExtendedOptions {
            text,
            selected_map_file: "test.map".into(),
            agreement: crate::app::shell_network_setup::agreement::MultiplayerAgreement {
                version: "test".into(),
                rules_digest: source_sha256.clone(),
            },
            source_sha256,
            effective_digest: "b".repeat(64),
            random_map: None,
            admissions: Vec::new(),
            room: None,
            vmce_roles: vmce_role_rows(&host),
            vmce_series: Some(identity.clone()),
            vmce_series_content: Some("d".repeat(64)),
            vmce_intermission: None,
        };

        let guest = fixture(false);
        let before = guest.clone();
        assert!(prevalidate_received_options(&guest, &modes, &snapshot).is_ok());

        let mut missing_series = snapshot.clone();
        missing_series.vmce_series = None;
        assert!(
            prevalidate_received_options(&guest, &modes, &missing_series).is_err(),
            "a VMCE LAN revision without the series identity is refused"
        );
        let mut missing_content = snapshot.clone();
        missing_content.vmce_series_content = None;
        assert!(
            prevalidate_received_options(&guest, &modes, &missing_content).is_err(),
            "a VMCE LAN revision without the whole-series digest is refused"
        );
        assert_eq!(
            guest, before,
            "a refused revision cannot alter accepted session state"
        );

        let mut tampered = snapshot;
        tampered.vmce_series.as_mut().unwrap().sha256 = "invalid".into();
        assert!(prevalidate_received_options(&guest, &modes, &tampered).is_err());
        assert_eq!(
            guest, before,
            "tampered config cannot alter accepted session state"
        );
    }

    /// A local copy that exists but differs is a digest/source mismatch; a
    /// missing map, directory or campaign.ini is an install problem. The
    /// lobby's inline status bar picks its message from this.
    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_campaign_failure_classification_names_missing_or_mismatched_bytes() {
        use crate::app::shell_network_setup::VmceLobbyStatus;
        let mismatch = [
            "local VMCE map differs from host map",
            "local campaign does not list the host's selected map",
            "host-advertised VMCE series does not match the selected map",
        ];
        for message in mismatch {
            assert_eq!(
                classify_vmce_campaign_failure(Some(&anyhow::anyhow!(message))),
                VmceLobbyStatus::MapMismatch,
                "{message}"
            );
        }
        let missing = [
            "local VMCE campaign.ini is missing",
            "local VMCE series directory is missing",
        ];
        for message in missing {
            assert_eq!(
                classify_vmce_campaign_failure(Some(&anyhow::anyhow!(message))),
                VmceLobbyStatus::MissingAssets,
                "{message}"
            );
        }
        assert_eq!(
            classify_vmce_campaign_failure(None),
            VmceLobbyStatus::MissingAssets
        );
    }

    /// The intermission unlock is written only from `map_status` 1, which the
    /// whole-series verification now produces; a waiting or refused
    /// verification never unlocks the next stage early.
    #[cfg(feature = "vmce")]
    #[test]
    fn intermission_unlock_requires_the_verified_map_status() {
        let modes = crate::skirmish_modes::stock_skirmish_modes();
        let coop_mode = modes.iter().find(|mode| mode.class.is_vmce()).unwrap();
        let mut guest = fixture(false);
        let source_sha256 = "a".repeat(64);
        guest
            .accept_network_map_metadata(
                NetworkMapMetadata {
                    display_name: "Next".into(),
                    player_capacity: 8,
                    official: false,
                    byte_length: 1,
                    digest: "0".repeat(32),
                    source_sha256: source_sha256.clone(),
                    effective_digest: "e".repeat(64),
                },
                None,
                true,
            )
            .unwrap();
        let mut launch = guest.network_setup().unwrap().clone();
        launch.mode = SkirmishLaunchMode::from_game_mode(coop_mode);
        launch.selected_map_file = Some("next.map".into());
        guest
            .accept_network_setup(launch, false, None, false)
            .unwrap();
        let config = crate::app::match_bootstrap::NetworkVmceMapConfig::new(
            "next.map".into(),
            source_sha256,
            crate::map::vmce_config::VmceMapConfig::default(),
        )
        .unwrap();
        guest.accept_network_vmce_map_config(Some(config)).unwrap();
        guest.accept_vmce_intermission_map("next.map").unwrap();
        let local = guest.network_setup().unwrap().local_player_index;
        // Session starts the local node at status 1 ("the local node always
        // holds its map"); a pending revision must withhold that.
        guest.set_network_map_status(local, 0).unwrap();
        assert_eq!(
            super::super::vmce_setup::intermission_unlock_earned(&guest),
            None
        );
        guest.set_network_map_status(local, 1).unwrap();
        assert_eq!(
            super::super::vmce_setup::intermission_unlock_earned(&guest),
            Some("next.map")
        );
    }
}
