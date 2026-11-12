//! VMCE-only LAN adapters for the shared Cooperative round.
//!
//! The transport, reliable queue, and bounded departure wait remain owned by
//! LanService. This module owns VMCE protocol state and its LAN-specific
//! prepare, receive, send, and intermission transitions.

use super::{AppState, LOBBY_RETRY_TIMING, LanService, direct};
use crate::app::match_runtime::session::{
    CooperativeRoundEvent, CooperativeRoundOptions, NetworkLoadingRequest, SessionMode,
};
use crate::net::global::GlobalMessage;
use crate::net::lan_messages::cooperative::{
    CooperativeOptionsRecord, CooperativeSignal, VmceRoundControl, VmceStageRecord,
};
use crate::net::lan_messages::ingame::{LoadingFailureReason, encode_loading_failure};
use crate::net::lan_messages::{LanMessage, LanMessageKind, SessionScope};
use std::collections::{BTreeSet, VecDeque};
use std::net::SocketAddrV4;

#[derive(Debug, Default)]
pub(crate) struct VmceLanRoundState {
    pending_round: VecDeque<GlobalMessage>,
    used_result_nonces: BTreeSet<[u8; 16]>,
    intermission_epoch: Option<[u8; 16]>,
    intermission_failed: bool,
    intermission_go_queued: bool,
    intermission_entered: bool,
    campaign_completion_pending: bool,
    campaign_completion_enqueue_failed: bool,
}

impl VmceLanRoundState {
    pub(super) fn clear_visit(&mut self) {
        self.used_result_nonces.clear();
        self.intermission_epoch = None;
        self.intermission_failed = false;
        self.intermission_go_queued = false;
        self.intermission_entered = false;
        self.campaign_completion_pending = false;
        self.campaign_completion_enqueue_failed = false;
    }

    pub(super) fn defer_pending_round(&mut self, incoming: GlobalMessage) {
        self.pending_round.push_back(incoming);
        if self.pending_round.len() > 64 {
            self.pending_round.pop_front();
        }
    }

    pub(super) fn take_pending_round(&mut self) -> Option<GlobalMessage> {
        self.pending_round.pop_front()
    }

    pub(super) fn clear_pending_round(&mut self) {
        self.pending_round.clear();
    }

    pub(super) fn intermission_epoch(&self) -> Option<[u8; 16]> {
        self.intermission_epoch
    }

    pub(super) fn intermission_failed(&self) -> bool {
        self.intermission_failed
    }

    pub(super) fn intermission_go_queued(&self) -> bool {
        self.intermission_go_queued
    }

    pub(super) fn intermission_entered(&self) -> bool {
        self.intermission_entered
    }

    pub(super) fn mark_intermission_prepared(&mut self, epoch: [u8; 16], solo: bool) {
        self.intermission_epoch = Some(epoch);
        self.intermission_failed = false;
        self.intermission_go_queued = solo;
    }

    pub(super) fn mark_intermission_failed(&mut self) {
        self.intermission_failed = true;
    }

    pub(super) fn mark_intermission_go_queued(&mut self, queued: bool) {
        self.intermission_go_queued = queued;
        self.intermission_failed |= !queued;
    }

    pub(super) fn mark_intermission_received(&mut self, epoch: [u8; 16]) {
        self.intermission_epoch = Some(epoch);
    }

    pub(super) fn mark_intermission_entered(&mut self, host: bool) {
        self.intermission_entered = !host;
        if host {
            self.intermission_epoch = None;
            self.intermission_go_queued = false;
        }
    }

    pub(super) fn finish_guest_intermission(&mut self) {
        self.intermission_entered = false;
        self.intermission_epoch = None;
    }

    pub(super) fn result_nonce_used(&self, nonce: &[u8; 16]) -> bool {
        self.used_result_nonces.contains(nonce)
    }

    pub(super) fn remember_result_nonce(&mut self, nonce: [u8; 16]) -> bool {
        self.used_result_nonces.insert(nonce)
    }

    pub(super) fn campaign_completion_pending(&self) -> bool {
        self.campaign_completion_pending
    }

    pub(super) fn defer_campaign_completion(&mut self) {
        self.campaign_completion_pending = true;
        self.campaign_completion_enqueue_failed = false;
    }

    pub(super) fn mark_campaign_completion_enqueue_failed(&mut self, failed: bool) {
        self.campaign_completion_enqueue_failed = failed;
    }

    fn take_campaign_completion(
        &mut self,
        observation: Option<super::SetupTransportObservation>,
    ) -> Option<Result<(), ()>> {
        if !self.campaign_completion_pending {
            return None;
        }
        let result = match observation {
            _ if self.campaign_completion_enqueue_failed => Err(()),
            Some(observation) if !observation.service_ok => Err(()),
            Some(observation) if observation.queued != 0 => return None,
            Some(_) => Ok(()),
            None => Err(()),
        };
        self.campaign_completion_pending = false;
        self.campaign_completion_enqueue_failed = false;
        Some(result)
    }
}

pub(crate) fn is_round_record(payload: &[u8]) -> bool {
    if payload.len() == crate::net::lan_messages::cooperative::SIGNAL_LEN {
        return CooperativeSignal::decode(payload).is_some_and(|signal| {
            matches!(
                signal,
                CooperativeSignal::StageRejected
                    | CooperativeSignal::StageAbsent
                    | CooperativeSignal::RetryVote
            )
        });
    }
    if payload.len() != crate::net::lan_messages::LAN_MESSAGE_LEN {
        return false;
    }
    let Ok(message) = LanMessage::decode(payload) else {
        return false;
    };
    VmceStageRecord::decode(&message).is_some()
        || VmceRoundControl::decode(&message).is_some()
        || CooperativeOptionsRecord::decode_vmce(&message).is_some()
        || crate::net::lan_messages::cooperative::decode_intermission_epoch(&message).is_some()
        || crate::net::lan_messages::cooperative::VmceResultRecord::decode(&message).is_some()
}

pub(super) fn defer_pending_round_record(state: &mut AppState, incoming: &GlobalMessage) -> bool {
    if state.match_state.session_mode() != SessionMode::Lan
        || state.match_state.session.cooperative_round().is_some()
        || !state
            .match_state
            .session
            .network_setup()
            .is_some_and(|launch| launch.mode.class.is_vmce())
        || !(super::cooperative::is_round_record(&incoming.payload)
            || is_round_record(&incoming.payload))
    {
        return false;
    }
    if let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_round.defer_pending_round(incoming.clone());
        return true;
    }
    false
}

pub(super) fn take_pending_round(service: &mut LanService) -> Option<GlobalMessage> {
    service.vmce_round.take_pending_round()
}

pub(super) fn active_round(state: &AppState) -> bool {
    state.match_state.session_mode() == SessionMode::Lan
        && state
            .match_state
            .session
            .cooperative_round()
            .is_some_and(|round| round.vmce_active())
}

fn peer_sources(state: &AppState) -> Vec<usize> {
    crate::app::match_runtime::network_match::nodes(&state.match_state.session)
        .into_iter()
        .skip(1)
        .collect()
}

fn peer_endpoints(state: &AppState, peers: &[usize]) -> anyhow::Result<Vec<SocketAddrV4>> {
    peers
        .iter()
        .map(|source| {
            state
                .match_state
                .session
                .network_endpoint(*source)
                .ok_or_else(|| anyhow::anyhow!("VMCE participant has no LAN endpoint"))
        })
        .collect()
}

pub(crate) fn load_failure_admitted(
    session: &crate::app::match_runtime::session::MatchSession,
    admitted_scope: Option<SessionScope>,
    active: bool,
    source: SocketAddrV4,
    scope: SessionScope,
    attempt: [u8; 16],
) -> bool {
    let source = session.network_endpoint_source(source);
    active
        && admitted_scope == Some(scope)
        && session.vmce_frozen_loading_attempt(scope.epoch) == Some(attempt)
        && source.is_some_and(|source| {
            session.network_connected_source_indices().contains(&source)
                && session
                    .network_setup()
                    .is_some_and(|launch| source != launch.local_player_index)
        })
}

pub(crate) fn send_retry_load_failure(state: &mut AppState, reason: LoadingFailureReason) -> bool {
    if state.match_state.session_mode() != SessionMode::Lan {
        return false;
    }
    let peers = peer_sources(state);
    let Some(scope) = state
        .match_state
        .lan_service
        .as_ref()
        .and_then(LanService::setup_scope)
    else {
        return false;
    };
    let Some(attempt) = state
        .match_state
        .session
        .vmce_frozen_loading_attempt(scope.epoch)
    else {
        return false;
    };
    let Ok(message) = encode_loading_failure(&super::local_name(state), reason, scope, attempt)
    else {
        return false;
    };
    let Ok(endpoints) = peer_endpoints(state, &peers) else {
        return false;
    };
    state
        .match_state
        .lan_service
        .as_mut()
        .is_some_and(|service| {
            service
                .queue_setup(
                    endpoints
                        .into_iter()
                        .map(|endpoint| (message.as_bytes().to_vec(), direct(endpoint))),
                )
                .unwrap_or(false)
        })
}

pub(crate) fn defer_loading_failure(
    state: &mut AppState,
    reason: LoadingFailureReason,
    notify: bool,
    detail: String,
) -> bool {
    let now = super::super::match_runtime::frame_pacer::frame_bucket(
        super::super::match_runtime::frame_pacer::wall_clock_ms(),
    );
    if state
        .match_state
        .lan_service
        .as_ref()
        .is_none_or(|service| {
            service.departure.is_some()
                || (state.match_state.live_network.is_some() && service.transport.is_some())
        })
    {
        return false;
    }
    if let Some(live) = state.match_state.live_network.take() {
        let Some(service) = state.match_state.lan_service.as_mut() else {
            state.match_state.live_network = Some(live);
            return false;
        };
        if service.transport.is_some() {
            state.match_state.live_network = Some(live);
            return false;
        }
        service.transport = Some(live.into_transport());
    }
    let Some(service) = state.match_state.lan_service.as_mut() else {
        return false;
    };
    if !service.begin_vmce_loading_departure(now, reason, detail) {
        return false;
    }
    if notify && !send_retry_load_failure(state, reason) {
        if let Some(service) = state.match_state.lan_service.as_mut() {
            service.mark_departure_failed();
        }
    }
    state.platform.window.request_redraw();
    true
}

pub(crate) fn loading_failure_pending(state: &AppState) -> bool {
    state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(|service| service.vmce_loading_failure_pending())
}

fn connected_source(state: &AppState, sender: SocketAddrV4) -> anyhow::Result<usize> {
    state
        .match_state
        .session
        .network_endpoint_source(sender)
        .filter(|source| {
            state
                .match_state
                .session
                .network_connected_source_indices()
                .contains(source)
        })
        .ok_or_else(|| anyhow::anyhow!("not a round participant"))
}

fn admitted_host(state: &AppState) -> Option<SocketAddrV4> {
    state
        .match_state
        .lan_service
        .as_ref()
        .and_then(|service| service.admitted_host.as_ref())
        .map(|(_, endpoint)| *endpoint)
}

fn control_admitted(
    host: bool,
    from_host: bool,
    sender: SocketAddrV4,
    admitted_host: Option<SocketAddrV4>,
    source: usize,
    local: usize,
    nonce: [u8; 16],
    expected_nonce: Option<[u8; 16]>,
) -> bool {
    expected_nonce == Some(nonce)
        && if from_host {
            !host && admitted_host == Some(sender)
        } else {
            host && source != local
        }
}

/// The VMCE-specific receive path. Generic BC6 packets fall through to the
/// shared Cooperative handler; every VMCE packet gets connected-source
/// admission here before its host/nonce checks or state changes.
pub(super) fn receive(state: &mut AppState, incoming: &GlobalMessage) -> anyhow::Result<bool> {
    if !active_round(state) {
        return Ok(false);
    }
    let payload = incoming.payload.as_slice();
    if payload.len() == crate::net::lan_messages::LAN_MESSAGE_LEN {
        let Ok(message) = LanMessage::decode(payload) else {
            return Ok(false);
        };
        let control = VmceRoundControl::decode(&message);
        let options = CooperativeOptionsRecord::decode_vmce(&message);
        if control.is_some() || options.is_some() {
            let source = connected_source(state, incoming.source)?;
            let (nonce, from_host) = if let Some((kind, nonce)) = control {
                (nonce, !matches!(kind, VmceRoundControl::Ack))
            } else {
                (options.as_ref().expect("VMCE options").1, true)
            };
            let host = state.match_state.session.host();
            let local = state
                .match_state
                .session
                .network_setup()
                .map_or(usize::MAX, |launch| launch.local_player_index);
            if !control_admitted(
                host,
                from_host,
                incoming.source,
                admitted_host(state),
                source,
                local,
                nonce,
                state.match_state.session.vmce_result_nonce(),
            ) {
                return Ok(true);
            }
            let event = match control {
                Some((VmceRoundControl::Begin, _)) => CooperativeRoundEvent::Begin,
                Some((VmceRoundControl::Ack, _)) => CooperativeRoundEvent::Ack { source },
                Some((VmceRoundControl::Go, _)) => CooperativeRoundEvent::Go,
                None => {
                    let launch = state
                        .match_state
                        .session
                        .network_setup()
                        .ok_or_else(|| anyhow::anyhow!("no round roster"))?;
                    CooperativeRoundEvent::Options(super::cooperative::round_options(
                        launch,
                        &options.as_ref().expect("VMCE options").0,
                    )?)
                }
            };
            crate::app::cooperative_round::service_event(state, event)?;
            return Ok(true);
        }
        if CooperativeOptionsRecord::decode(&message).is_some() {
            let _ = connected_source(state, incoming.source)?;
            return Ok(true);
        }
        if let Some(epoch) =
            crate::net::lan_messages::cooperative::decode_intermission_epoch(&message)
        {
            let _ = connected_source(state, incoming.source)?;
            anyhow::ensure!(
                !state.match_state.session.host() && admitted_host(state) == Some(incoming.source),
                "intermission identity did not come from the host"
            );
            let service = state
                .match_state
                .lan_service
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
            anyhow::ensure!(
                service
                    .setup_scope
                    .is_some_and(|scope| scope.epoch != epoch),
                "intermission reused the previous epoch"
            );
            if let Some(previous) = service.vmce_round.intermission_epoch() {
                anyhow::ensure!(previous == epoch, "conflicting intermission identity");
            } else {
                let transport = service
                    .transport
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("VMCE has no scoped transport"))?;
                anyhow::ensure!(
                    transport.lan_scope() == service.setup_scope,
                    "VMCE intermission scope differs from the admitted match"
                );
                transport.prepare_lan_intermission()?;
                service.vmce_round.mark_intermission_received(epoch);
            }
            return Ok(true);
        }
        if message.kind()
            == LanMessageKind::Unknown(crate::net::lan_messages::cooperative::RESULT_KIND)
        {
            let Some(record) =
                crate::net::lan_messages::cooperative::VmceResultRecord::decode(&message)
            else {
                log::warn!("ignoring a stale or malformed VMCE result record");
                return Ok(true);
            };
            let source = connected_source(state, incoming.source)?;
            let first = state.match_state.session.vmce_result_nonce().is_none()
                && !state.match_state.session.host();
            if first {
                let service = state
                    .match_state
                    .lan_service
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
                anyhow::ensure!(
                    !service.vmce_round.result_nonce_used(&record.nonce),
                    "replayed VMCE result nonce"
                );
            }
            if let Err(error) = state
                .match_state
                .session
                .vmce_result_received(source, record)
            {
                log::error!("VMCE players disagree on the result: {error:#}");
                let _ = state.match_state.session.quit_cooperative_round();
                crate::app::cooperative_round::return_to_shell(state);
                return Ok(true);
            }
            if state.match_state.session.vmce_result_nonce() == Some(record.nonce)
                && let Some(service) = state.match_state.lan_service.as_mut()
            {
                service.vmce_round.remember_result_nonce(record.nonce);
            }
            return Ok(true);
        }
        if let Some(record) = VmceStageRecord::decode(&message) {
            let source = connected_source(state, incoming.source)?;
            let event = CooperativeRoundEvent::StageIdentity(
                crate::app::match_bootstrap::VmceStageIdentity {
                    map_file: record.map_file,
                    source_sha256: record.source_sha256,
                    effective_digest: record.effective_digest,
                    config_sha256: record.config_sha256,
                },
            );
            let _ = source;
            crate::app::cooperative_round::service_event(state, event)?;
            return Ok(true);
        }
    }
    if payload.len() == crate::net::lan_messages::cooperative::SIGNAL_LEN
        && let Some(signal) = CooperativeSignal::decode(payload)
    {
        let Some(source) = (match signal {
            CooperativeSignal::StageRejected
            | CooperativeSignal::StageAbsent
            | CooperativeSignal::RetryVote
            | CooperativeSignal::Begin
            | CooperativeSignal::Ack
            | CooperativeSignal::Go => Some(connected_source(state, incoming.source)?),
            _ => None,
        }) else {
            return Ok(false);
        };
        let event = match signal {
            CooperativeSignal::StageRejected => Some(CooperativeRoundEvent::StageRejected),
            CooperativeSignal::StageAbsent => Some(CooperativeRoundEvent::StageAbsent),
            CooperativeSignal::RetryVote => Some(CooperativeRoundEvent::RetryVote { source }),
            CooperativeSignal::Begin | CooperativeSignal::Ack | CooperativeSignal::Go => None,
            _ => unreachable!("only VMCE-specific signals are admitted here"),
        };
        if let Some(event) = event {
            crate::app::cooperative_round::service_event(state, event)?;
        }
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn defer_vmce_round_exit(state: &mut AppState) -> bool {
    if !active_round(state) {
        return false;
    }
    let Some(service) = state.match_state.lan_service.as_mut() else {
        return false;
    };
    let now = super::super::match_runtime::frame_pacer::frame_bucket(
        super::super::match_runtime::frame_pacer::wall_clock_ms(),
    );
    if !service.begin_vmce_round_departure(now) {
        return false;
    }
    super::drain_match_requests(state);
    state.platform.window.request_redraw();
    true
}

pub(crate) fn round_exit_pending(state: &AppState) -> bool {
    state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(LanService::vmce_round_exit_pending)
}

pub(super) fn pause_round_poll(state: &AppState) -> bool {
    round_exit_pending(state)
}

pub(super) fn flush_round_work(state: &mut AppState, service_ok: bool) {
    let round_active = active_round(state);
    let Some(service) = state.match_state.lan_service.as_mut() else {
        return;
    };
    if service.vmce_round.intermission_epoch().is_some() || round_active {
        service.flush_setup();
    }
    if !service_ok && service.vmce_round.intermission_epoch().is_some() {
        service.vmce_round.mark_intermission_failed();
    }
}

pub(super) fn take_campaign_completion(
    state: &mut AppState,
    service_ok: bool,
) -> Option<Result<(), ()>> {
    let service = state.match_state.lan_service.as_mut()?;
    take_campaign_completion_from_service(service, service_ok)
}

pub(super) fn take_campaign_completion_from_service(
    service: &mut LanService,
    service_ok: bool,
) -> Option<Result<(), ()>> {
    // Preserve the completion boundary's second FIFO flush: the first round
    // flush may predate the final Go request, and the reliable observation
    // must include every setup request admitted before completion.
    service.flush_setup();
    let observation = service.go_transport_observation(service_ok);
    service.vmce_round.take_campaign_completion(observation)
}

pub(super) fn campaign_completion_pending(state: &AppState) -> bool {
    state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(|service| service.vmce_round.campaign_completion_pending())
}

pub(crate) fn defer_campaign_completion(state: &mut AppState) -> bool {
    if state.match_state.session_mode() != SessionMode::Lan
        || !state.match_state.session.host()
        || state.match_state.lan_service.is_none()
    {
        return false;
    }
    state
        .match_state
        .lan_service
        .as_mut()
        .expect("checked service")
        .vmce_round
        .defer_campaign_completion();
    super::drain_match_requests(state);
    state.platform.window.request_redraw();
    true
}

pub(crate) fn begin_vmce_intermission_transition(state: &mut AppState) -> anyhow::Result<()> {
    anyhow::ensure!(
        state.match_state.session_mode() == SessionMode::Lan && state.match_state.session.host(),
        "VMCE intermission requires a LAN host"
    );
    if state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(|service| service.vmce_round.intermission_epoch().is_some())
    {
        return Ok(());
    }
    let peers = peer_sources(state);
    let endpoints = peer_endpoints(state, &peers)?;
    let epoch = super::fresh_nonce()?;
    let solo = endpoints.is_empty();
    let message = crate::net::lan_messages::cooperative::encode_intermission_epoch(
        &super::local_name(state),
        epoch,
    )?;
    let service = state
        .match_state
        .lan_service
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
    anyhow::ensure!(
        service
            .setup_scope
            .is_some_and(|scope| scope.epoch != epoch),
        "intermission has no prior scope or reused its epoch"
    );
    let transport = service
        .transport
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("VMCE has no scoped transport"))?;
    anyhow::ensure!(
        transport.lan_scope() == service.setup_scope,
        "VMCE intermission scope differs from the admitted match"
    );
    transport.prepare_lan_intermission()?;
    service.vmce_round.mark_intermission_prepared(epoch, solo);
    if !service.queue_setup(
        endpoints
            .into_iter()
            .map(|endpoint| (message.as_bytes().to_vec(), direct(endpoint))),
    )? {
        service.vmce_round.mark_intermission_failed();
        anyhow::bail!("VMCE intermission queue is full");
    }
    Ok(())
}

pub(crate) fn vmce_intermission_transition_ready(state: &AppState) -> anyhow::Result<bool> {
    let service = state
        .match_state
        .lan_service
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
    anyhow::ensure!(
        service.vmce_round.intermission_epoch().is_some(),
        "VMCE intermission was not prepared"
    );
    let observation = service
        .go_transport_observation(true)
        .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN transport"))?;
    anyhow::ensure!(
        !service.vmce_round.intermission_failed() && observation.service_ok,
        "VMCE intermission delivery failed"
    );
    Ok(observation.queued == 0)
}

pub(crate) fn enter_vmce_intermission(state: &mut AppState) -> anyhow::Result<bool> {
    let host = state.match_state.session.host();
    let previous_peers: Vec<_> = state
        .match_state
        .session
        .network_connected_source_indices()
        .iter()
        .filter_map(|source| state.match_state.session.network_endpoint(*source))
        .collect();
    let service = state
        .match_state
        .lan_service
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
    let epoch = service
        .vmce_round
        .intermission_epoch()
        .ok_or_else(|| anyhow::anyhow!("VMCE intermission epoch was not received"))?;
    anyhow::ensure!(
        !service.vmce_round.intermission_failed(),
        "VMCE intermission delivery failed"
    );
    if host {
        if !service.vmce_round.intermission_go_queued() {
            return Ok(false);
        }
        let observation = service
            .go_transport_observation(true)
            .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN transport"))?;
        anyhow::ensure!(observation.service_ok, "VMCE Go delivery failed");
        if observation.queued != 0 {
            return Ok(false);
        }
    } else {
        // Go confirms advancement; unsent old-round controls must not be
        // reframed for the new setup. The reliable channel retains old ACKs.
        service.pending_setup.clear();
    }
    let transport = service
        .transport
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN transport"))?;
    transport.enter_lan_intermission()?;
    if host {
        transport.finish_lan_intermission();
    } else {
        for endpoint in previous_peers {
            transport.cancel_retired_lan_sends_to(endpoint);
        }
    }
    transport.set_retry_timing(LOBBY_RETRY_TIMING, true);
    service.setup_scope = Some(SessionScope { epoch, revision: 0 });
    service.setup_agreement = None;
    service.received_options = None;
    service.pending_admissions.clear();
    service.vmce_round.clear_pending_round();
    service.deferred_setup.clear();
    service.vmce_setup.reset();
    service.setup_reassembly = Default::default();
    service.maps.cancel();
    for peer in service.peer_admissions.values_mut() {
        peer.lease = None;
        peer.heartbeat_sequence = None;
    }
    service.vmce_round.mark_intermission_entered(host);
    service.force_presence = true;
    Ok(true)
}

pub(super) fn host_heartbeat_validated(service: &mut LanService) {
    if !service.vmce_round.intermission_entered() {
        return;
    }
    if let Some(transport) = service.transport.as_mut() {
        transport.finish_lan_intermission();
    }
    service.vmce_round.finish_guest_intermission();
}

pub(super) fn should_assign_intermission(state: &AppState) -> bool {
    state.match_state.session.host()
        && state
            .match_state
            .lan_service
            .as_ref()
            .is_some_and(|service| {
                service.vmce_round.intermission_epoch().is_some()
                    && !service.vmce_round.intermission_entered()
            })
        && state
            .match_state
            .session
            .cooperative_round()
            .is_some_and(|round| {
                round.vmce_ending()
                    == Some(crate::app::match_runtime::session::VmceRoundEnding::Next)
                    && !round.vmce_next_accepted()
            })
}

/// Keep the host's established transport-ready gate and assignment order in
/// the VMCE LAN owner so shared Cooperative polling stays protocol-neutral.
/// Returns false after a terminal handoff failure has returned to the lobby.
pub(super) fn assign_intermission_if_ready(state: &mut AppState) -> bool {
    if !should_assign_intermission(state) {
        return true;
    }
    match vmce_intermission_transition_ready(state) {
        Ok(true) => {
            let difficulty = state
                .frontend
                .multiplayer_shell_runtime
                .cooperative_difficulty();
            if let Err(error) = state
                .match_state
                .session
                .cooperative_round_assigned(difficulty)
            {
                log::error!("VMCE intermission assignment failed: {error:#}");
                crate::app::cooperative_round::return_to_shell(state);
                return false;
            }
        }
        Ok(false) => {}
        Err(error) => {
            log::error!("VMCE intermission preparation failed: {error:#}");
            crate::app::cooperative_round::return_to_shell(state);
            return false;
        }
    }
    true
}

pub(super) fn next_stage_accepted(state: &AppState) -> bool {
    state
        .match_state
        .session
        .cooperative_round()
        .is_some_and(|round| round.vmce_next_accepted())
}

pub(super) fn result_nonce(state: &AppState) -> Option<[u8; 16]> {
    state.match_state.session.vmce_result_nonce()
}

pub(super) fn service_result_agreement(state: &mut AppState) -> anyhow::Result<()> {
    if !active_round(state) {
        return Ok(());
    }
    if !state
        .match_state
        .session
        .vmce_result_agreement_initialized()
    {
        let sim = &state
            .match_state
            .sim_runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("VMCE finished world is absent"))?
            .simulation;
        let assignments = state
            .match_state
            .session
            .network_vmce_assignments()
            .ok_or_else(|| anyhow::anyhow!("VMCE frozen roles are absent"))?;
        let mut entries = [0xff; 4];
        for assignment in &assignments.participants {
            let slot = usize::from(assignment.raw_slot_index);
            anyhow::ensure!(
                slot < entries.len() && assignment.player_index < 4 && entries[slot] == 0xff,
                "invalid VMCE terminal role roster"
            );
            let owner = sim
                .interner()
                .get(&assignment.owner_name)
                .ok_or_else(|| anyhow::anyhow!("VMCE result House is absent"))?;
            let kind = sim
                .session()
                .vmce_outcome(owner)
                .ok_or_else(|| anyhow::anyhow!("VMCE player result is absent"))?;
            entries[slot] = assignment.player_index
                | if kind == crate::sim::house_state::HouseOutcomeKind::Victory {
                    4
                } else {
                    0
                };
        }
        let nonce = if state.match_state.session.host() {
            let nonce = super::fresh_nonce()?;
            let service = state
                .match_state
                .lan_service
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
            anyhow::ensure!(
                service.vmce_round.remember_result_nonce(nonce),
                "VMCE result nonce was reused"
            );
            Some(nonce)
        } else {
            None
        };
        state
            .match_state
            .session
            .initialize_vmce_result_agreement(entries, nonce)?;
    }
    if let Some(record) = state.match_state.session.vmce_result_next_record() {
        let bytes = record
            .encode(&super::local_name(state))?
            .as_bytes()
            .to_vec();
        let local = state
            .match_state
            .session
            .network_setup()
            .ok_or_else(|| anyhow::anyhow!("VMCE roster is absent"))?
            .local_player_index;
        let endpoints: Vec<_> = state
            .match_state
            .session
            .network_connected_source_indices()
            .iter()
            .copied()
            .filter(|source| *source != local)
            .map(|source| {
                state
                    .match_state
                    .session
                    .network_endpoint(source)
                    .ok_or_else(|| anyhow::anyhow!("VMCE result peer has no endpoint"))
            })
            .collect::<anyhow::Result<_>>()?;
        let service = state
            .match_state
            .lan_service
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
        if service.queue_setup(
            endpoints
                .into_iter()
                .map(|endpoint| (bytes.clone(), direct(endpoint))),
        )? {
            state
                .match_state
                .session
                .vmce_result_record_queued(record.confirmed)?;
        }
    }
    Ok(())
}

/// Dispatch a VMCE continuation produced by LAN's result-agreement phase,
/// then retry the accepted-next intermission until the round advances. This
/// preserves the native Go/ACK wait while keeping VMCE dispatch out of the
/// shared Cooperative poll.
pub(super) fn continue_after_result(state: &mut AppState) -> bool {
    if let Ok(Some(step)) = state.match_state.session.vmce_result_take_deferred_go() {
        crate::app::cooperative_round::run_step(state, step);
    }
    if next_stage_accepted(state) {
        crate::app::vmce_round::advance_intermission(state);
        return state.match_state.session.cooperative_round().is_some();
    }
    true
}

fn queue_encoded_to_peers(state: &mut AppState, bytes: &[u8], peers: &[usize]) -> bool {
    let Ok(endpoints) = peer_endpoints(state, peers) else {
        return false;
    };
    state
        .match_state
        .lan_service
        .as_mut()
        .is_some_and(|service| {
            service
                .queue_setup(
                    endpoints
                        .into_iter()
                        .map(|endpoint| (bytes.to_vec(), direct(endpoint))),
                )
                .unwrap_or(false)
        })
}

fn encode_scoped_request(
    request: &NetworkLoadingRequest,
    name: &str,
    result_nonce: Option<[u8; 16]>,
) -> Option<Option<LanMessage>> {
    use crate::net::lan_messages::cooperative::VmceRoundControl;

    match request {
        NetworkLoadingRequest::CooperativeBegin => {
            Some(result_nonce.and_then(|nonce| VmceRoundControl::Begin.encode(name, nonce).ok()))
        }
        NetworkLoadingRequest::CooperativeAck => {
            Some(result_nonce.and_then(|nonce| VmceRoundControl::Ack.encode(name, nonce).ok()))
        }
        NetworkLoadingRequest::CooperativeGo => {
            Some(result_nonce.and_then(|nonce| VmceRoundControl::Go.encode(name, nonce).ok()))
        }
        NetworkLoadingRequest::CooperativeOptions(options) => {
            Some(result_nonce.and_then(|nonce| {
                super::cooperative::options_record(options)
                    .encode_vmce(name, nonce)
                    .ok()
            }))
        }
        _ => None,
    }
}

fn report_round_failure(state: &mut AppState) {
    let departure_pending = state
        .match_state
        .lan_service
        .as_mut()
        .is_some_and(|service| {
            if service.vmce_round_exit_pending() {
                service.mark_departure_failed();
                true
            } else {
                false
            }
        });
    if departure_pending {
        return;
    }
    if state.frontend.multiplayer_setup.is_some() {
        crate::app::shell_network_setup::show_blocking_notice(
            state,
            crate::i18n::tr("VERA:VmceRoundSendFailed"),
        );
    } else {
        crate::app::shell_lan::report_error(state, "VERA:VmceRoundSendFailed");
    }
}

pub(super) fn fail_round_delivery(state: &mut AppState) -> bool {
    if !active_round(state) {
        return false;
    }
    log::error!("VMCE round delivery failed; returning to the lobby");
    let _ = state.match_state.session.quit_cooperative_round();
    crate::app::cooperative_round::return_to_shell(state);
    report_round_failure(state);
    true
}

pub(super) fn release_round_transport(service: &mut LanService) {
    let vmce_exit = service.vmce_round_exit_pending();
    if let Some(transport) = service.transport.as_mut() {
        transport.connections_mut().reset();
        if vmce_exit {
            transport.global_mut().use_native_acknowledgments();
        }
        transport.clear_lan_scope();
    }
    service.vmce_round.clear_pending_round();
}

/// Handle only requests whose wire behavior belongs to VMCE. The caller
/// retains ordinary BC6 Ready, chat, and transport handling.
pub(super) fn send(state: &mut AppState, request: &NetworkLoadingRequest) -> bool {
    if state.match_state.session_mode() != SessionMode::Lan {
        return false;
    }
    let vmce = active_round(state);
    let vmce_go = matches!(request, NetworkLoadingRequest::CooperativeVmceGo { .. });
    if !vmce && !vmce_go {
        return false;
    }
    let name = super::local_name(state);
    let peers = peer_sources(state);
    if vmce
        && matches!(
            request,
            NetworkLoadingRequest::CooperativeLeave
                | NetworkLoadingRequest::CooperativeStageRejected
        )
    {
        let signal = if matches!(request, NetworkLoadingRequest::CooperativeLeave) {
            CooperativeSignal::Left
        } else {
            CooperativeSignal::StageRejected
        };
        let queued = signal
            .encode(&name)
            .is_ok_and(|message| queue_encoded_to_peers(state, &message, &peers));
        if !queued {
            if let Some(service) = state.match_state.lan_service.as_mut() {
                service.mark_departure_failed();
            }
            log::error!("VMCE exit record could not be queued");
        }
        return true;
    }
    if let NetworkLoadingRequest::CooperativeVmceGo { nonce } = request {
        let message = VmceRoundControl::Go.encode(&name, *nonce).ok();
        let intermission_pending = state
            .match_state
            .lan_service
            .as_ref()
            .is_some_and(|service| {
                service.vmce_round.intermission_epoch().is_some()
                    && !service.vmce_round.intermission_entered()
            });
        if intermission_pending {
            let queued = message
                .as_ref()
                .is_some_and(|message| queue_encoded_to_peers(state, message.as_bytes(), &peers));
            if let Some(service) = state.match_state.lan_service.as_mut() {
                service.vmce_round.mark_intermission_go_queued(queued);
            }
            return true;
        }
        if campaign_completion_pending(state) {
            let queued = message
                .as_ref()
                .is_some_and(|message| queue_encoded_to_peers(state, message.as_bytes(), &peers));
            if let Some(service) = state.match_state.lan_service.as_mut() {
                service
                    .vmce_round
                    .mark_campaign_completion_enqueue_failed(!queued);
            }
            return true;
        }
        if let Some(message) = message {
            let queued = queue_encoded_to_peers(state, message.as_bytes(), &peers);
            if !queued {
                fail_round_delivery(state);
            }
        } else {
            fail_round_delivery(state);
        }
        return true;
    }

    if let Some(encoded) = encode_scoped_request(request, &name, result_nonce(state)) {
        let queued = encoded
            .as_ref()
            .is_some_and(|message| queue_encoded_to_peers(state, message.as_bytes(), &peers));
        if !queued {
            // Recognized VMCE controls remain consumed on missing nonce or
            // encoding failure; never retry them as unscoped native BC6.
            fail_round_delivery(state);
        }
        return true;
    }
    if vmce {
        match request {
            NetworkLoadingRequest::CooperativeRetryVote => {
                if let Ok(message) = CooperativeSignal::RetryVote.encode(&name) {
                    if !queue_encoded_to_peers(state, &message, &peers) {
                        fail_round_delivery(state);
                    }
                }
                return true;
            }
            NetworkLoadingRequest::CooperativeStage(stage) => {
                let record = VmceStageRecord {
                    map_file: stage.map_file.clone(),
                    source_sha256: stage.source_sha256.clone(),
                    effective_digest: stage.effective_digest.clone(),
                    config_sha256: stage.config_sha256.clone(),
                };
                let queued = match record.encode(&name) {
                    Ok(message) => queue_encoded_to_peers(state, message.as_bytes(), &peers),
                    Err(error) => {
                        log::error!(
                            "VMCE stage identity could not be encoded ({error}); rejecting the round"
                        );
                        CooperativeSignal::StageRejected
                            .encode(&name)
                            .is_ok_and(|message| queue_encoded_to_peers(state, &message, &peers))
                    }
                };
                if !queued {
                    fail_round_delivery(state);
                }
                return true;
            }
            NetworkLoadingRequest::CooperativeStageAbsent => {
                if let Ok(message) = CooperativeSignal::StageAbsent.encode(&name) {
                    if !queue_encoded_to_peers(state, &message, &peers) {
                        fail_round_delivery(state);
                    }
                }
                return true;
            }
            NetworkLoadingRequest::CooperativeStageRejected => {
                if let Ok(message) = CooperativeSignal::StageRejected.encode(&name) {
                    if !queue_encoded_to_peers(state, &message, &peers) {
                        fail_round_delivery(state);
                    }
                }
                return true;
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmce_control_admission_checks_the_host_and_round() {
        let master = "127.0.0.1:1000".parse().unwrap();
        let guest = "127.0.0.1:1001".parse().unwrap();
        let nonce = [5; 16];
        assert!(control_admitted(
            false,
            true,
            master,
            Some(master),
            0,
            1,
            nonce,
            Some(nonce)
        ));
        assert!(!control_admitted(
            false,
            true,
            guest,
            Some(master),
            2,
            1,
            nonce,
            Some(nonce)
        ));
        assert!(!control_admitted(
            false,
            true,
            master,
            Some(master),
            0,
            1,
            [6; 16],
            Some(nonce)
        ));
        assert!(!control_admitted(
            true,
            true,
            guest,
            None,
            1,
            0,
            nonce,
            Some(nonce)
        ));
        assert!(control_admitted(
            true,
            false,
            guest,
            None,
            1,
            0,
            nonce,
            Some(nonce)
        ));
        assert!(!control_admitted(
            false,
            false,
            guest,
            Some(master),
            2,
            1,
            nonce,
            Some(nonce)
        ));
    }

    #[test]
    fn missing_nonce_is_a_failed_scoped_control_not_an_unhandled_request() {
        assert!(matches!(
            encode_scoped_request(&NetworkLoadingRequest::CooperativeBegin, "Host", None),
            Some(None)
        ));
        assert!(matches!(
            encode_scoped_request(
                &NetworkLoadingRequest::CooperativeOptions(CooperativeRoundOptions {
                    difficulty: 1,
                    humans: Vec::new(),
                    ai_player_count: 0,
                    opponents: Vec::new(),
                }),
                "Host",
                None
            ),
            Some(None)
        ));
        assert_eq!(
            encode_scoped_request(&NetworkLoadingRequest::CooperativeReady, "Host", None),
            None,
            "ordinary BC6 Ready remains for the shared sender"
        );
    }

    #[test]
    fn vmce_early_records_are_owned_separately_from_generic_bc6_records() {
        let absent = CooperativeSignal::StageAbsent.encode("Guest").unwrap();
        assert!(is_round_record(&absent));
        assert!(!crate::app::lan_service::cooperative::is_round_record(
            &absent
        ));

        let begin = VmceRoundControl::Begin.encode("Host", [7; 16]).unwrap();
        assert!(is_round_record(begin.as_bytes()));
        let stage = VmceStageRecord {
            map_file: "mission02.map".into(),
            source_sha256: "a".repeat(64),
            effective_digest: "b".repeat(64),
            config_sha256: "c".repeat(64),
        };
        assert!(is_round_record(&stage.encode("Host").unwrap().into_bytes()));
    }

    #[test]
    fn early_round_fifo_keeps_the_newest_sixty_four_packets_in_order() {
        let source = "127.0.0.1:1000".parse().unwrap();
        let mut round = VmceLanRoundState::default();
        for sequence in 0..65 {
            round.defer_pending_round(GlobalMessage {
                source,
                product_id: 0xaa04,
                sequence,
                payload: vec![sequence as u8],
            });
        }

        assert_eq!(round.pending_round.len(), 64);
        assert_eq!(round.take_pending_round().unwrap().sequence, 1);
        assert_eq!(round.take_pending_round().unwrap().sequence, 2);
        assert_eq!(round.take_pending_round().unwrap().sequence, 3);
        assert_eq!(round.pending_round.back().unwrap().sequence, 64);
    }
}
