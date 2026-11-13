//! VMCE-only LAN adapters for the VMCE result round.
//!
//! The transport, reliable queue, and bounded departure wait remain owned by
//! LanService. This module owns VMCE protocol state and its LAN-specific
//! prepare, receive, send, and intermission transitions.

use super::{AppState, LOBBY_RETRY_TIMING, LanService, direct};
use crate::app::match_runtime::session::{
    NetworkLoadingRequest, SessionMode, VmceResultEvent, VmceRoundFailure, WallBucketTimer,
};
use crate::net::global::GlobalMessage;

use crate::net::lan_messages::ingame::{LoadingFailureReason, encode_loading_failure};
use crate::net::lan_messages::vmce::{VmceRoundControl, VmceSignal, VmceStageRecord};
use crate::net::lan_messages::{LanMessage, LanMessageKind, SessionScope};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::net::SocketAddrV4;

/// Host ACK wait, host commit-confirmation wait and the guest's longer overall
/// wait. The guest must not time out while the host may still legally be in
/// either of its own bounded phases.
const RETRY_ACK_WAIT_BUCKETS: u32 = 600;
const RETRY_COMMIT_WAIT_BUCKETS: u32 = 600;
const RETRY_GUEST_WAIT_BUCKETS: u32 = 1_500;
/// A lost application commit ACK is repaired by retransmitting the commit as a
/// fresh reliable record; each retransmission is rate limited by this window.
const RETRY_COMMIT_RESEND_BUCKETS: u32 = 120;
/// Result-round application liveness: peers heartbeat while a human decides,
/// and a silent peer is declared gone only after this generous bound.
const ROUND_HEARTBEAT_SEND_BUCKETS: u32 = 60;
const ROUND_HEARTBEAT_TIMEOUT_BUCKETS: u32 = 600;

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
    /// Bounded waits for the terminal handoffs. An unresponsive peer must end
    /// in a local failure, not an endless wait; both use the same 600-bucket
    /// budget as the round departure.
    intermission_deadline: Option<WallBucketTimer>,
    campaign_completion_deadline: Option<WallBucketTimer>,
    assignment_deadline: Option<WallBucketTimer>,
    /// Bounded host ACK / guest commit wait for the retry handoff, plus the
    /// host's commit-admission flag. The retry reload never starts on one peer
    /// while the other is still waiting or has failed.
    retry_wait_deadline: Option<WallBucketTimer>,
    retry_commit_queued: bool,
    /// Host: the overall commit-confirmation phase was entered. Its deadline is
    /// set once here; commit retransmissions never extend it.
    retry_commit_phase_started: bool,
    /// Host: next application-level RetryCommit retransmission. The reliable
    /// queue repairs lost datagrams; this repairs a lost application commit
    /// ACK by sending a new commit that the guest answers again.
    retry_commit_resend: Option<WallBucketTimer>,
    /// Host: RetryStart was queued; the freeze follows in the same pass.
    retry_start_sent: bool,
    /// `Some(None)` is an active result round before the host's nonce arrives;
    /// `Some(Some(nonce))` is the same round after agreement starts.
    round_liveness_nonce: Option<Option<[u8; 16]>>,
    round_heartbeat_sent: Option<WallBucketTimer>,
    peer_heartbeats: BTreeMap<usize, u32>,
    /// The result nonce whose agreed next-stage unlock was already persisted.
    /// Duplicate or stale confirmations keep the same nonce and do not write
    /// again; a new round carries a new nonce.
    result_unlock_nonce: Option<[u8; 16]>,
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
        self.intermission_deadline = None;
        self.campaign_completion_deadline = None;
        self.assignment_deadline = None;
        self.retry_wait_deadline = None;
        self.retry_commit_queued = false;
        self.retry_commit_phase_started = false;
        self.retry_commit_resend = None;
        self.retry_start_sent = false;
        self.round_liveness_nonce = None;
        self.round_heartbeat_sent = None;
        self.peer_heartbeats.clear();
        self.result_unlock_nonce = None;
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

    pub(super) fn mark_intermission_prepared(&mut self, epoch: [u8; 16], solo: bool, now: u32) {
        self.intermission_epoch = Some(epoch);
        self.intermission_failed = false;
        self.intermission_go_queued = solo;
        self.intermission_deadline = Some(WallBucketTimer::start(now, 600));
    }

    pub(super) fn intermission_wait_expired(&self, now: u32) -> bool {
        self.intermission_deadline
            .is_some_and(|deadline| deadline.remaining(now) == 0)
    }

    pub(super) fn mark_intermission_failed(&mut self) {
        self.intermission_failed = true;
    }

    pub(super) fn mark_intermission_go_queued(&mut self, queued: bool) {
        self.intermission_go_queued = queued;
        self.intermission_failed |= !queued;
    }

    pub(super) fn mark_intermission_received(&mut self, epoch: [u8; 16], now: u32) {
        self.intermission_epoch = Some(epoch);
        self.intermission_deadline = Some(WallBucketTimer::start(now, 600));
    }

    pub(super) fn mark_intermission_entered(&mut self, host: bool) {
        self.intermission_entered = !host;
        self.intermission_deadline = None;
        if host {
            self.intermission_epoch = None;
            self.intermission_go_queued = false;
        }
    }

    pub(super) fn finish_guest_intermission(&mut self) {
        self.intermission_entered = false;
        self.intermission_epoch = None;
        self.intermission_deadline = None;
    }

    pub(super) fn result_nonce_used(&self, nonce: &[u8; 16]) -> bool {
        self.used_result_nonces.contains(nonce)
    }

    pub(super) fn remember_result_nonce(&mut self, nonce: [u8; 16]) -> bool {
        self.used_result_nonces.insert(nonce)
    }

    pub(super) fn result_unlock_nonce(&self) -> Option<[u8; 16]> {
        self.result_unlock_nonce
    }

    pub(super) fn mark_result_unlock_saved(&mut self, nonce: [u8; 16]) {
        self.result_unlock_nonce = Some(nonce);
    }

    pub(super) fn campaign_completion_pending(&self) -> bool {
        self.campaign_completion_pending
    }

    pub(super) fn defer_campaign_completion(&mut self, now: u32) {
        self.campaign_completion_pending = true;
        self.campaign_completion_enqueue_failed = false;
        self.campaign_completion_deadline = Some(WallBucketTimer::start(now, 600));
    }

    pub(super) fn campaign_completion_wait_expired(&self, now: u32) -> bool {
        self.campaign_completion_deadline
            .is_some_and(|deadline| deadline.remaining(now) == 0)
    }

    pub(super) fn mark_assignment_wait(&mut self, now: u32) {
        if self.assignment_deadline.is_none() {
            self.assignment_deadline = Some(WallBucketTimer::start(now, 600));
        }
    }

    pub(super) fn assignment_wait_expired(&self, now: u32) -> bool {
        self.assignment_deadline
            .is_some_and(|deadline| deadline.remaining(now) == 0)
    }

    pub(super) fn clear_assignment_wait(&mut self) {
        self.assignment_deadline = None;
    }

    /// The guest waits through the host's ACK phase and its commit-drain
    /// phase, so its single deadline is longer than either host phase. A
    /// duplicate Go does not extend it.
    pub(super) fn mark_guest_retry_wait(&mut self, now: u32) {
        if self.retry_wait_deadline.is_none() {
            self.retry_wait_deadline = Some(WallBucketTimer::start(now, RETRY_GUEST_WAIT_BUCKETS));
        }
        self.retry_commit_queued = false;
        self.retry_commit_phase_started = false;
    }

    pub(super) fn mark_retry_wait(&mut self, now: u32) {
        self.retry_wait_deadline = Some(WallBucketTimer::start(now, RETRY_ACK_WAIT_BUCKETS));
        self.retry_commit_queued = false;
        self.retry_commit_phase_started = false;
        self.retry_commit_resend = None;
        self.retry_start_sent = false;
        self.assignment_deadline = None;
    }

    pub(super) fn retry_wait_started(&self) -> bool {
        self.retry_wait_deadline.is_some()
    }

    pub(super) fn retry_wait_expired(&self, now: u32) -> bool {
        self.retry_wait_deadline
            .is_some_and(|deadline| deadline.remaining(now) == 0)
    }

    pub(super) fn retry_commit_queued(&self) -> bool {
        self.retry_commit_queued
    }

    /// Enter the host's commit-confirmation phase exactly once. The overall
    /// commit bound starts here and only here: retransmitting the commit is a
    /// repair action, not progress, so it can never extend this deadline.
    pub(super) fn begin_retry_commit_phase(&mut self, now: u32) {
        if !self.retry_commit_phase_started {
            self.retry_commit_phase_started = true;
            self.retry_wait_deadline = Some(WallBucketTimer::start(now, RETRY_COMMIT_WAIT_BUCKETS));
        }
    }

    pub(super) fn mark_retry_commit_queued(&mut self, queued: bool, now: u32) {
        self.retry_commit_queued = queued;
        if queued {
            self.retry_commit_resend =
                Some(WallBucketTimer::start(now, RETRY_COMMIT_RESEND_BUCKETS));
        }
    }

    /// A host pass retransmits the commit once the resend window opened.
    pub(super) fn retry_commit_resend_due(&self, now: u32) -> bool {
        self.retry_commit_resend
            .is_some_and(|resend| resend.remaining(now) == 0)
    }

    pub(super) fn retry_start_sent(&self) -> bool {
        self.retry_start_sent
    }

    pub(super) fn mark_retry_start_sent(&mut self) {
        self.retry_start_sent = true;
    }

    /// Clear the retry wait after every guest committed (or after a failure
    /// exit) so a later round cannot read a stale deadline.
    pub(super) fn finish_retry_wait(&mut self) {
        self.retry_wait_deadline = None;
        self.retry_commit_queued = false;
        self.retry_commit_phase_started = false;
        self.retry_commit_resend = None;
        self.retry_start_sent = false;
    }

    /// Start (or keep) the open round's liveness window, including before the
    /// host's result nonce arrives. Learning the nonce does not restart it.
    /// Every currently connected peer begins with a fresh allowance; peers
    /// that left are dropped instead of being mistaken for unresponsive.
    pub(super) fn begin_round_liveness(
        &mut self,
        nonce: Option<[u8; 16]>,
        peers: &[usize],
        now: u32,
    ) {
        if self.round_liveness_nonce.is_none()
            || matches!((self.round_liveness_nonce, nonce), (Some(Some(old)), Some(new)) if old != new)
        {
            self.round_liveness_nonce = Some(nonce);
            self.round_heartbeat_sent = None;
            self.peer_heartbeats.clear();
        } else if nonce.is_some() {
            self.round_liveness_nonce = Some(nonce);
        }
        self.peer_heartbeats
            .retain(|source, _| peers.contains(source));
        for source in peers {
            self.peer_heartbeats.entry(*source).or_insert(now);
        }
    }

    pub(super) fn round_heartbeat_due(&self, now: u32) -> bool {
        self.round_heartbeat_sent
            .is_none_or(|sent| sent.remaining(now) == 0)
    }

    pub(super) fn mark_round_heartbeat_sent(&mut self, now: u32) {
        self.round_heartbeat_sent = Some(WallBucketTimer::start(now, ROUND_HEARTBEAT_SEND_BUCKETS));
    }

    /// Replay/duplicate safe: any admitted heartbeat only refreshes the
    /// sender's window and never touches simulation state.
    pub(super) fn note_peer_heartbeat(&mut self, source: usize, now: u32) {
        if self.round_liveness_nonce.is_some() {
            self.peer_heartbeats
                .entry(source)
                .and_modify(|last| {
                    *last = now;
                })
                .or_insert(now);
        }
    }

    /// The first connected peer whose application heartbeats stopped for the
    /// generous bound. `None` while every peer keeps answering.
    pub(super) fn expired_round_peer(&self, now: u32) -> Option<usize> {
        self.peer_heartbeats.iter().find_map(|(source, last)| {
            (now.wrapping_sub(*last) >= ROUND_HEARTBEAT_TIMEOUT_BUCKETS).then_some(*source)
        })
    }

    pub(super) fn clear_round_liveness(&mut self) {
        self.round_liveness_nonce = None;
        self.round_heartbeat_sent = None;
        self.peer_heartbeats.clear();
    }

    pub(super) fn mark_campaign_completion_enqueue_failed(&mut self, failed: bool) {
        self.campaign_completion_enqueue_failed = failed;
    }

    fn take_campaign_completion(
        &mut self,
        observation: Option<super::SetupTransportObservation>,
        now: u32,
    ) -> Option<Result<(), ()>> {
        if !self.campaign_completion_pending {
            return None;
        }
        let expired = self.campaign_completion_wait_expired(now);
        let result = match observation {
            _ if self.campaign_completion_enqueue_failed => Err(()),
            Some(observation) if !observation.service_ok => Err(()),
            // A final ACK that lands in the deadline bucket is still success.
            Some(observation) if observation.queued == 0 => Ok(()),
            _ if expired => Err(()),
            Some(_) => return None,
            None => Err(()),
        };
        self.campaign_completion_pending = false;
        self.campaign_completion_enqueue_failed = false;
        self.campaign_completion_deadline = None;
        Some(result)
    }
}

pub(crate) fn is_round_record(payload: &[u8]) -> bool {
    if payload.len() == crate::net::lan_messages::vmce::SIGNAL_LEN {
        return VmceSignal::decode(payload).is_some();
    }
    if payload.len() != crate::net::lan_messages::LAN_MESSAGE_LEN {
        return false;
    }
    let Ok(message) = LanMessage::decode(payload) else {
        return false;
    };
    VmceStageRecord::decode(&message).is_some()
        || VmceRoundControl::decode(&message).is_some()
        || crate::net::lan_messages::vmce::decode_intermission_epoch(&message).is_some()
        || crate::net::lan_messages::vmce::VmceResultRecord::decode(&message).is_some()
}

pub(super) fn defer_pending_round_record(state: &mut AppState, incoming: &GlobalMessage) -> bool {
    if state.match_state.session_mode() != SessionMode::Lan
        || state.match_state.session.vmce_result_phase().is_some()
        || !state
            .match_state
            .session
            .network_setup()
            .is_some_and(|launch| launch.mode.class.is_vmce())
        || !is_round_record(&incoming.payload)
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
        && state.match_state.session.vmce_result_phase().is_some()
}

/// The other connected result-round participants, in native node order.
pub(crate) fn round_peers(
    session: &crate::app::match_runtime::session::MatchSession,
) -> Vec<usize> {
    crate::app::match_runtime::network_match::nodes(session)
        .into_iter()
        .skip(1)
        .collect()
}

fn peer_sources(state: &AppState) -> Vec<usize> {
    round_peers(&state.match_state.session)
}

fn peer_endpoints(
    session: &crate::app::match_runtime::session::MatchSession,
    peers: &[usize],
) -> anyhow::Result<Vec<SocketAddrV4>> {
    peers
        .iter()
        .map(|source| {
            session
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

/// A VMCE retry/next-stage network load whose native peer wait expired must
/// not install with timed-out peers. Returns the visible failure reason when
/// the frozen attempt belongs to VMCE; stock Cooperative/native loads return
/// `None` and keep their existing install behavior.
pub(crate) fn vmce_loading_peer_timeout(
    session: &crate::app::match_runtime::session::MatchSession,
    service: &LanService,
) -> Option<LoadingFailureReason> {
    let scope = service.setup_scope?;
    session
        .vmce_frozen_loading_attempt(scope.epoch)
        .map(|_| LoadingFailureReason::PeerTimeout)
}

/// Queue the scoped VMCE loading-failure record to every connected peer.
fn send_loading_failure(
    session: &crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    local_name: &str,
    peers: &[usize],
    reason: LoadingFailureReason,
) -> bool {
    let Some(scope) = service.setup_scope else {
        return false;
    };
    let Some(attempt) = session.vmce_frozen_loading_attempt(scope.epoch) else {
        return false;
    };
    let Ok(message) = encode_loading_failure(local_name, reason, scope, attempt) else {
        return false;
    };
    let Ok(endpoints) = peer_endpoints(session, peers) else {
        return false;
    };
    queue_round_bytes(service, message.as_bytes(), endpoints.into_iter())
}

/// Production VMCE loading-failure transition: abort the loader, start the
/// bounded departure, and (when this side owns the failure) tell the peers the
/// reason. Returns false when the departure could not start.
pub(crate) fn begin_vmce_loading_failure(
    session: &crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    local_name: &str,
    peers: &[usize],
    reason: LoadingFailureReason,
    detail: &str,
    notify: bool,
    now: u32,
) -> bool {
    if !service.begin_vmce_loading_departure(now, reason, detail.to_owned()) {
        return false;
    }
    if notify && !send_loading_failure(session, service, local_name, peers, reason) {
        service.mark_departure_failed();
    }
    true
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
    let name = super::local_name(state);
    let peers = peer_sources(state);
    let session = &state.match_state.session;
    let started = state
        .match_state
        .lan_service
        .as_mut()
        .is_some_and(|service| {
            begin_vmce_loading_failure(
                session, service, &name, &peers, reason, &detail, notify, now,
            )
        });
    if !started {
        return false;
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

fn connected_source_index(
    session: &crate::app::match_runtime::session::MatchSession,
    sender: SocketAddrV4,
) -> anyhow::Result<usize> {
    session
        .network_endpoint_source(sender)
        .filter(|source| session.network_connected_source_indices().contains(source))
        .ok_or_else(|| anyhow::anyhow!("not a round participant"))
}

fn connected_source(state: &AppState, sender: SocketAddrV4) -> anyhow::Result<usize> {
    connected_source_index(&state.match_state.session, sender)
}

/// What one admitted VMCE result-round record produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VmceRoundAction {
    /// Admitted; the app must run this session step.
    Step(crate::app::match_runtime::session::VmceRoundStep),
    /// Admitted; no app step is needed.
    Consumed,
}

/// Production receive path for the retry handoff, application liveness and the
/// visible handoff-failure records. Returns `None` for records the shared
/// result owner still handles (ready/vote/leave, stage identity, agreement).
pub(crate) fn receive_round_record(
    session: &mut crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    incoming: &GlobalMessage,
    now: u32,
) -> anyhow::Result<Option<VmceRoundAction>> {
    use crate::app::match_runtime::session::{VmceResultEvent, VmceRoundEnding, VmceRoundFailure};
    let payload = incoming.payload.as_slice();
    if payload.len() == crate::net::lan_messages::vmce::SIGNAL_LEN {
        let Some(signal) = VmceSignal::decode(payload) else {
            return Ok(None);
        };
        return match signal {
            VmceSignal::RetryAck
            | VmceSignal::RetryCommitAck
            | VmceSignal::Heartbeat
            | VmceSignal::RoundFailed
            | VmceSignal::PeerUnresponsive => {
                let source = connected_source_index(session, incoming.source)?;
                match signal {
                    // Liveness is replay/duplicate safe: any admitted heartbeat
                    // only refreshes the sender's window, never simulation state.
                    VmceSignal::Heartbeat => {
                        service.vmce_round.note_peer_heartbeat(source, now);
                        Ok(Some(VmceRoundAction::Consumed))
                    }
                    VmceSignal::RetryAck => Ok(Some(VmceRoundAction::Step(
                        session.vmce_result_event(VmceResultEvent::RetryAck { source })?,
                    ))),
                    VmceSignal::RetryCommitAck => {
                        anyhow::ensure!(session.host(), "VMCE commit proof outside the host");
                        Ok(Some(VmceRoundAction::Step(session.vmce_result_event(
                            VmceResultEvent::RetryCommitAck { source },
                        )?)))
                    }
                    VmceSignal::RoundFailed => {
                        Ok(Some(VmceRoundAction::Step(session.vmce_result_event(
                            VmceResultEvent::HandoffFailed(VmceRoundFailure::Delivery),
                        )?)))
                    }
                    _ => Ok(Some(VmceRoundAction::Step(session.vmce_result_event(
                        VmceResultEvent::HandoffFailed(VmceRoundFailure::PeerUnresponsive),
                    )?))),
                }
            }
            _ => Ok(None),
        };
    }
    if payload.len() != crate::net::lan_messages::LAN_MESSAGE_LEN {
        return Ok(None);
    }
    let Ok(message) = LanMessage::decode(payload) else {
        return Ok(None);
    };
    let Some((control, nonce)) = VmceRoundControl::decode(&message) else {
        return Ok(None);
    };
    if !matches!(
        control,
        VmceRoundControl::Go | VmceRoundControl::RetryCommit | VmceRoundControl::RetryStart
    ) {
        // Begin/Ack remain VMCE records with no live host sender or guest
        // receiver; consume them instead of leaking them to the stock path.
        return Ok(Some(VmceRoundAction::Consumed));
    }
    let source = connected_source_index(session, incoming.source)?;
    let host = session.host();
    let local = session
        .network_setup()
        .map_or(usize::MAX, |launch| launch.local_player_index);
    let admitted_host = service
        .admitted_host
        .as_ref()
        .map(|(_, endpoint)| *endpoint);
    if !control_admitted(
        host,
        true,
        incoming.source,
        admitted_host,
        source,
        local,
        nonce,
        session.vmce_result_nonce(),
    ) {
        return Ok(Some(VmceRoundAction::Consumed));
    }
    match control {
        VmceRoundControl::Go => {
            let retry = session.vmce_result_ending() == Some(VmceRoundEnding::Retry);
            let step = session.vmce_result_event(VmceResultEvent::Go)?;
            if retry {
                // The guest answered RetryAck and now waits for the commit;
                // its bounded phase covers the host's whole handoff.
                service.vmce_round.mark_guest_retry_wait(now);
                Ok(Some(VmceRoundAction::Consumed))
            } else {
                Ok(Some(VmceRoundAction::Step(step)))
            }
        }
        VmceRoundControl::RetryCommit => Ok(Some(VmceRoundAction::Step(
            session.vmce_result_event(VmceResultEvent::RetryCommit)?,
        ))),
        VmceRoundControl::RetryStart => {
            let step = session.vmce_result_event(VmceResultEvent::RetryStart)?;
            if step != crate::app::match_runtime::session::VmceRoundStep::None {
                service.vmce_round.finish_retry_wait();
            }
            Ok(Some(VmceRoundAction::Step(step)))
        }
        _ => Ok(Some(VmceRoundAction::Consumed)),
    }
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

fn admitted_host(state: &AppState) -> Option<SocketAddrV4> {
    state
        .match_state
        .lan_service
        .as_ref()
        .and_then(|service| service.admitted_host.as_ref())
        .map(|(_, endpoint)| *endpoint)
}

/// The VMCE-specific receive path. Generic BC6 packets fall through to the
/// shared Cooperative handler; every VMCE packet gets connected-source
/// admission here before its host/nonce checks or state changes.
pub(super) fn receive(state: &mut AppState, incoming: &GlobalMessage) -> anyhow::Result<bool> {
    if !active_round(state) {
        return Ok(false);
    }
    let now = super::super::match_runtime::frame_pacer::frame_bucket(
        super::super::match_runtime::frame_pacer::wall_clock_ms(),
    );
    // Retry handoff, application liveness and visible failure records are
    // decided and applied by the production core shared with the tests.
    let action = {
        let session = &mut state.match_state.session;
        let Some(service) = state.match_state.lan_service.as_mut() else {
            return Ok(false);
        };
        receive_round_record(session, service, incoming, now)?
    };
    match action {
        Some(VmceRoundAction::Consumed) => {
            crate::app::vmce_result::refresh(state);
            return Ok(true);
        }
        Some(VmceRoundAction::Step(step)) => {
            crate::app::vmce_round::run_step(state, step);
            crate::app::vmce_result::refresh(state);
            return Ok(true);
        }
        None => {}
    }
    let payload = incoming.payload.as_slice();
    if payload.len() == crate::net::lan_messages::LAN_MESSAGE_LEN {
        let Ok(message) = LanMessage::decode(payload) else {
            return Ok(false);
        };
        if let Some(epoch) = crate::net::lan_messages::vmce::decode_intermission_epoch(&message) {
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
                let now = super::super::match_runtime::frame_pacer::frame_bucket(
                    super::super::match_runtime::frame_pacer::wall_clock_ms(),
                );
                service.vmce_round.mark_intermission_received(epoch, now);
            }
            return Ok(true);
        }
        if message.kind() == LanMessageKind::Unknown(crate::net::lan_messages::vmce::RESULT_KIND) {
            let Some(record) = crate::net::lan_messages::vmce::VmceResultRecord::decode(&message)
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
                let _ = state.match_state.session.quit_vmce_result();
                crate::app::vmce_round::return_to_shell(state);
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
            let event =
                VmceResultEvent::StageIdentity(crate::app::match_bootstrap::VmceStageIdentity {
                    map_file: record.map_file,
                    source_sha256: record.source_sha256,
                    effective_digest: record.effective_digest,
                    config_sha256: record.config_sha256,
                });
            let _ = source;
            crate::app::vmce_round::service_event(state, event)?;
            return Ok(true);
        }
    }
    if payload.len() == crate::net::lan_messages::vmce::SIGNAL_LEN {
        if let Some(signal) = VmceSignal::decode(payload) {
            let source = connected_source(state, incoming.source)?;
            let event = match signal {
                VmceSignal::StageRejected => VmceResultEvent::StageRejected,
                VmceSignal::StageAbsent => VmceResultEvent::StageAbsent,
                VmceSignal::RetryVote => VmceResultEvent::RetryVote { source },
                VmceSignal::Ready => VmceResultEvent::PeerReady { source },
                VmceSignal::Leave => VmceResultEvent::PeerLeft { source },
                // The handoff/liveness core consumed this record above.
                VmceSignal::RetryAck
                | VmceSignal::RetryCommitAck
                | VmceSignal::Heartbeat
                | VmceSignal::RoundFailed
                | VmceSignal::PeerUnresponsive => return Ok(true),
            };
            crate::app::vmce_round::service_event(state, event)?;
            return Ok(true);
        }
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
    now: u32,
) -> Option<Result<(), ()>> {
    let service = state.match_state.lan_service.as_mut()?;
    take_campaign_completion_from_service(service, service_ok, now)
}

pub(super) fn take_campaign_completion_from_service(
    service: &mut LanService,
    service_ok: bool,
    now: u32,
) -> Option<Result<(), ()>> {
    // Preserve the completion boundary's second FIFO flush: the first round
    // flush may predate the final Go request, and the reliable observation
    // must include every setup request admitted before completion.
    service.flush_setup();
    let observation = service.go_transport_observation(service_ok);
    service
        .vmce_round
        .take_campaign_completion(observation, now)
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
    let now = super::super::match_runtime::frame_pacer::frame_bucket(
        super::super::match_runtime::frame_pacer::wall_clock_ms(),
    );
    state
        .match_state
        .lan_service
        .as_mut()
        .expect("checked service")
        .vmce_round
        .defer_campaign_completion(now);
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
    let now = super::super::match_runtime::frame_pacer::frame_bucket(
        super::super::match_runtime::frame_pacer::wall_clock_ms(),
    );
    let peers = peer_sources(state);
    let endpoints = peer_endpoints(&state.match_state.session, &peers)?;
    let epoch = super::fresh_nonce()?;
    let solo = endpoints.is_empty();
    let message = crate::net::lan_messages::vmce::encode_intermission_epoch(
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
    service
        .vmce_round
        .mark_intermission_prepared(epoch, solo, now);
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

pub(crate) fn vmce_intermission_transition_ready(
    state: &AppState,
    now: u32,
) -> anyhow::Result<bool> {
    let service = state
        .match_state
        .lan_service
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
    anyhow::ensure!(
        service.vmce_round.intermission_epoch().is_some(),
        "VMCE intermission was not prepared"
    );
    anyhow::ensure!(
        !service.vmce_round.intermission_wait_expired(now),
        "VMCE intermission delivery timed out"
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
        let now = super::super::match_runtime::frame_pacer::frame_bucket(
            super::super::match_runtime::frame_pacer::wall_clock_ms(),
        );
        if !service.vmce_round.intermission_go_queued() {
            anyhow::ensure!(
                !service.vmce_round.intermission_wait_expired(now),
                "VMCE intermission delivery timed out"
            );
            return Ok(false);
        }
        let observation = service
            .go_transport_observation(true)
            .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN transport"))?;
        anyhow::ensure!(observation.service_ok, "VMCE Go delivery failed");
        if observation.queued != 0 {
            anyhow::ensure!(
                !service.vmce_round.intermission_wait_expired(now),
                "VMCE intermission delivery timed out"
            );
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
            .vmce_result_phase()
            .is_some_and(|round| {
                round.vmce_ending()
                    == Some(crate::app::match_runtime::session::VmceRoundEnding::Next)
                    && !round.vmce_next_accepted()
            })
}

/// Keep the host's established transport-ready gate and assignment order in
/// the VMCE LAN owner so shared Cooperative polling stays protocol-neutral.
/// Returns false after a terminal handoff failure has returned to the lobby.
pub(super) fn assign_intermission_if_ready(state: &mut AppState, now: u32) -> bool {
    if !should_assign_intermission(state) {
        return true;
    }
    match vmce_intermission_transition_ready(state, now) {
        Ok(true) => {
            if let Err(error) = state.match_state.session.vmce_result_assigned() {
                log::error!("VMCE intermission assignment failed: {error:#}");
                crate::app::vmce_round::return_to_shell(state);
                return false;
            }
            if let Err(error) = state.match_state.session.vmce_result_queue_go() {
                log::error!("VMCE intermission Go queue failed: {error:#}");
                crate::app::vmce_round::return_to_shell(state);
                return false;
            }
        }
        Ok(false) => {}
        Err(error) => {
            log::error!("VMCE intermission preparation failed: {error:#}");
            crate::app::vmce_round::return_to_shell(state);
            report_round_send_failed(state);
            return false;
        }
    }
    true
}

pub(super) fn next_stage_accepted(state: &AppState) -> bool {
    state
        .match_state
        .session
        .vmce_result_phase()
        .is_some_and(|round| round.vmce_next_accepted())
}

/// Persist the next-stage unlock as soon as every online participant has
/// confirmed the round's result and next-stage identity. Continue, entering
/// the intermission room and loading the next stage are not required. The
/// progress store is idempotent; this owner remembers the result nonce so a
/// settled round does not touch it every frame, while a new round (new nonce)
/// unlocks again.
pub(super) fn unlock_agreed_next_stage(state: &mut AppState) {
    let Some((map, nonce)) = state.match_state.session.vmce_agreed_unlock() else {
        return;
    };
    let map = map.to_owned();
    let Some(service) = state.match_state.lan_service.as_mut() else {
        return;
    };
    if service.vmce_round.result_unlock_nonce() == Some(nonce) {
        return;
    }
    match crate::app::vmce_campaign::try_unlock_catalog_stage(state, &map) {
        Ok(_) => {
            if let Some(service) = state.match_state.lan_service.as_mut() {
                service.vmce_round.mark_result_unlock_saved(nonce);
            }
        }
        // Not remembered as saved: the next poll retries the same agreement.
        Err(error) => log::warn!("Could not persist the agreed VMCE unlock: {error:#}"),
    }
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
            .runtime()
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
        crate::app::vmce_round::run_step(state, step);
    }
    if next_stage_accepted(state) {
        crate::app::vmce_round::advance_intermission(state);
        return state.match_state.session.vmce_result_phase().is_some();
    }
    let awaiting_assignment = state
        .match_state
        .session
        .vmce_result_phase()
        .is_some_and(|round| round.awaiting_assignment());
    if awaiting_assignment {
        let now = super::super::match_runtime::frame_pacer::frame_bucket(
            super::super::match_runtime::frame_pacer::wall_clock_ms(),
        );
        let expired = state
            .match_state
            .lan_service
            .as_ref()
            .is_some_and(|service| service.vmce_round.assignment_wait_expired(now));
        if expired {
            log::error!("VMCE round assignment wait timed out");
            let _ = state.match_state.session.quit_vmce_result();
            crate::app::vmce_round::return_to_shell(state);
            report_round_send_failed(state);
            return false;
        }
    } else if let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_round.clear_assignment_wait();
    }
    // The result is settled but the host's Go has not arrived. A peer that
    // stopped answering must not keep the page waiting forever.
    let waiting_go = state
        .match_state
        .session
        .vmce_result_phase()
        .is_some_and(|round| {
            round.vmce_ending() == Some(crate::app::match_runtime::session::VmceRoundEnding::Next)
                && !round.vmce_next_accepted()
        });
    if waiting_go {
        let now = super::super::match_runtime::frame_pacer::frame_bucket(
            super::super::match_runtime::frame_pacer::wall_clock_ms(),
        );
        let expired = state
            .match_state
            .lan_service
            .as_ref()
            .is_some_and(|service| service.vmce_round.intermission_wait_expired(now));
        if expired {
            log::error!("VMCE intermission Go wait timed out");
            crate::app::vmce_round::return_to_shell(state);
            report_round_send_failed(state);
            return false;
        }
    }
    true
}

fn queue_encoded_to_peers(state: &mut AppState, bytes: &[u8], peers: &[usize]) -> bool {
    let Ok(endpoints) = peer_endpoints(&state.match_state.session, peers) else {
        return false;
    };
    state
        .match_state
        .lan_service
        .as_mut()
        .is_some_and(|service| queue_round_bytes(service, bytes, endpoints.into_iter()))
}

/// Admit one VMCE round packet to the shared reliable setup FIFO. A full queue
/// or transport failure is reported to the caller; VMCE never treats a
/// rejected enqueue as delivered.
pub(super) fn queue_round_bytes(
    service: &mut LanService,
    bytes: &[u8],
    endpoints: impl ExactSizeIterator<Item = SocketAddrV4>,
) -> bool {
    service
        .queue_setup(endpoints.map(|endpoint| (bytes.to_vec(), direct(endpoint))))
        .unwrap_or(false)
}

/// What a production round-record send did with one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmceRoundSend {
    /// The request is not a VMCE result-round record.
    NotMine,
    /// Every peer admitted it to the shared reliable setup FIFO.
    Queued,
    /// Encoding or the reliable queue refused it; the caller must fail visibly.
    Failed,
}

/// One retry-handoff pass. The app wrapper only runs the freeze/reload or the
/// visible departure owner; the decision, sends and bounds live here, so
/// in-process production-path tests can drive the same state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmceRoundHandoff {
    /// Nothing to do this pass.
    Continue,
    /// The host queued RetryStart; freeze and reload the retry stage now.
    FreezeRetry,
    /// The round ended locally; the caller exits with a visible reason.
    Failed(VmceRoundFailure),
}

fn queue_round_signal_bytes(
    session: &crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    bytes: &[u8],
    peers: &[usize],
) -> bool {
    let Ok(endpoints) = peer_endpoints(session, peers) else {
        return false;
    };
    queue_round_bytes(service, bytes, endpoints.into_iter())
}

/// Queue one nonce-scoped VMCE control record to every connected peer.
pub(crate) fn queue_round_control(
    session: &crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    local_name: &str,
    peers: &[usize],
    control: VmceRoundControl,
) -> bool {
    let Some(nonce) = session.vmce_result_nonce() else {
        return false;
    };
    let Ok(message) = control.encode(local_name, nonce) else {
        return false;
    };
    queue_round_signal_bytes(session, service, message.as_bytes(), peers)
}

/// Queue one header-only VMCE signal to every connected peer.
pub(crate) fn queue_round_signal(
    session: &crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    local_name: &str,
    peers: &[usize],
    signal: VmceSignal,
) -> bool {
    let Ok(bytes) = signal.encode(local_name) else {
        return false;
    };
    queue_round_signal_bytes(session, service, &bytes, peers)
}

/// Encode and enqueue one VMCE result-round request over the reliable setup
/// FIFO. Only the retry handoff records are this owner's; the app wrapper
/// keeps the intermission and campaign-end delivery branches.
pub(crate) fn send_round_request(
    session: &crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    local_name: &str,
    peers: &[usize],
    request: &NetworkLoadingRequest,
) -> VmceRoundSend {
    let queued = match request {
        NetworkLoadingRequest::VmceResultGo { .. } => Some(queue_round_control(
            session,
            service,
            local_name,
            peers,
            VmceRoundControl::Go,
        )),
        NetworkLoadingRequest::VmceResultRetryAck => Some(queue_round_signal(
            session,
            service,
            local_name,
            peers,
            VmceSignal::RetryAck,
        )),
        NetworkLoadingRequest::VmceResultRetryCommitAck => Some(queue_round_signal(
            session,
            service,
            local_name,
            peers,
            VmceSignal::RetryCommitAck,
        )),
        _ => None,
    };
    match queued {
        Some(true) => VmceRoundSend::Queued,
        Some(false) => VmceRoundSend::Failed,
        None => VmceRoundSend::NotMine,
    }
}

/// Production send drain for requests the session queued while handling a
/// result event. Returns false when one could not be queued.
#[cfg(test)]
pub(crate) fn flush_round_requests(
    session: &mut crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    local_name: &str,
    peers: &[usize],
) -> bool {
    let requests = session.take_network_loading_requests();
    let mut queued = true;
    for request in &requests {
        if send_round_request(session, service, local_name, peers, request) == VmceRoundSend::Failed
        {
            queued = false;
        }
    }
    queued
}

fn transport_healthy(service: &LanService, service_ok: bool) -> bool {
    service
        .go_transport_observation(service_ok)
        .is_some_and(|observation| observation.service_ok)
}

/// One bounded retry-handoff pass:
/// a) every guest acknowledged the Go, b) every guest's *application* applied
/// the commit, c) the host queues RetryStart and freezes. A queue drain is
/// never used as application proof; a lost commit/ACK is repaired by
/// retransmitting the commit as a new reliable record, which the guest
/// re-answers, and every other outcome ends in a visible failure.
pub(crate) fn retry_handoff_step(
    session: &mut crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    local_name: &str,
    peers: &[usize],
    now: u32,
    service_ok: bool,
) -> VmceRoundHandoff {
    use crate::app::match_runtime::session::VmceRoundEnding;
    if session.vmce_result_ending() != Some(VmceRoundEnding::Retry)
        || !session
            .vmce_result_phase()
            .is_some_and(|round| round.outcome_committed())
        || session.vmce_result_failure().is_some()
    {
        return VmceRoundHandoff::Continue;
    }
    if session.host() {
        if !service.vmce_round.retry_wait_started() {
            // The Go was queued by the app's assignment owner; its retry wait
            // starts with the first handoff pass.
            service.vmce_round.mark_retry_wait(now);
        }
        if !session.vmce_result_retry_acks_complete() {
            if !transport_healthy(service, service_ok) {
                return VmceRoundHandoff::Failed(VmceRoundFailure::Delivery);
            }
            if service.vmce_round.retry_wait_expired(now) {
                return VmceRoundHandoff::Failed(VmceRoundFailure::PeerUnresponsive);
            }
            return VmceRoundHandoff::Continue;
        }
        if !service.vmce_round.retry_commit_queued() {
            let queued = queue_round_control(
                session,
                service,
                local_name,
                peers,
                VmceRoundControl::RetryCommit,
            );
            service.vmce_round.mark_retry_commit_queued(queued, now);
            if !queued {
                return VmceRoundHandoff::Failed(VmceRoundFailure::Delivery);
            }
            // The overall commit-confirmation bound starts once, here; later
            // retransmissions are repair and must not extend it.
            service.vmce_round.begin_retry_commit_phase(now);
            return VmceRoundHandoff::Continue;
        }
        if !session.vmce_result_retry_commit_acks_complete() {
            service.vmce_round.begin_retry_commit_phase(now);
            if !transport_healthy(service, service_ok) {
                return VmceRoundHandoff::Failed(VmceRoundFailure::Delivery);
            }
            if service.vmce_round.retry_wait_expired(now) {
                return VmceRoundHandoff::Failed(VmceRoundFailure::PeerUnresponsive);
            }
            if service.vmce_round.retry_commit_resend_due(now) {
                // A lost application commit ACK is repaired by a fresh commit
                // record; a duplicate datagram would not reach the guest's
                // application through the reliable channel's dedup.
                let queued = queue_round_control(
                    session,
                    service,
                    local_name,
                    peers,
                    VmceRoundControl::RetryCommit,
                );
                service.vmce_round.mark_retry_commit_queued(queued, now);
                if !queued {
                    return VmceRoundHandoff::Failed(VmceRoundFailure::Delivery);
                }
            }
            return VmceRoundHandoff::Continue;
        }
        if !service.vmce_round.retry_start_sent() {
            let queued = queue_round_control(
                session,
                service,
                local_name,
                peers,
                VmceRoundControl::RetryStart,
            );
            if !queued {
                return VmceRoundHandoff::Failed(VmceRoundFailure::Delivery);
            }
            service.vmce_round.mark_retry_start_sent();
        }
        service.vmce_round.finish_retry_wait();
        return VmceRoundHandoff::FreezeRetry;
    }
    // The guest never reloads before RetryStart; its own wait is bounded once
    // the host's Go started it, and the heartbeat window bounds the silence of
    // a host that never commits.
    if !service.vmce_round.retry_wait_started() {
        return VmceRoundHandoff::Continue;
    }
    if !transport_healthy(service, service_ok) {
        return VmceRoundHandoff::Failed(VmceRoundFailure::Delivery);
    }
    if service.vmce_round.retry_wait_expired(now) {
        return VmceRoundHandoff::Failed(VmceRoundFailure::PeerUnresponsive);
    }
    VmceRoundHandoff::Continue
}

/// Application-level liveness for the open result round. A peer that keeps
/// heartbeating while its human decides is never treated as disconnected; a
/// peer that stops for the generous bound ends the round in bounded time.
pub(crate) fn round_liveness_step(
    session: &crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    local_name: &str,
    peers: &[usize],
    now: u32,
) -> Option<VmceRoundFailure> {
    if session.vmce_result_phase().is_none() {
        return None;
    }
    let nonce = session.vmce_result_nonce();
    service.vmce_round.begin_round_liveness(nonce, peers, now);
    if service.vmce_round.round_heartbeat_due(now) {
        if let Ok(bytes) = VmceSignal::Heartbeat.encode(local_name)
            && let Ok(endpoints) = peer_endpoints(session, peers)
        {
            for endpoint in endpoints {
                service.send(&bytes, direct(endpoint), false);
            }
        }
        service.vmce_round.mark_round_heartbeat_sent(now);
    }
    service
        .vmce_round
        .expired_round_peer(now)
        .map(|_| VmceRoundFailure::PeerUnresponsive)
}

/// Production failure transition shared by both peers: notify the others over
/// the reliable FIFO, stop the page, and record the visible reason for the
/// departure owner. Returns the i18n key the player sees.
pub(crate) fn begin_round_failure(
    session: &mut crate::app::match_runtime::session::MatchSession,
    service: &mut LanService,
    local_name: &str,
    peers: &[usize],
    failure: VmceRoundFailure,
) -> &'static str {
    let signal = match failure {
        VmceRoundFailure::Delivery => VmceSignal::RoundFailed,
        VmceRoundFailure::PeerUnresponsive => VmceSignal::PeerUnresponsive,
    };
    let _ = session.vmce_result_abort(failure);
    // Remove only unsent records of the abandoned VMCE round. Other setup
    // traffic stays with its existing owner. A bounded priority reservation
    // lets the terminal failure precede that traffic even when the FIFO is
    // full; the global reliable queue remains the sole delivery owner.
    service
        .pending_setup
        .retain(|(bytes, _)| !is_round_record(bytes));
    let queued = signal
        .encode(local_name)
        .ok()
        .and_then(|bytes| {
            let endpoints = peer_endpoints(session, peers).ok()?;
            let limit = 4096 + crate::skirmish_launch::NETWORK_PLAYER_SLOT_COUNT;
            if service.pending_setup.len().saturating_add(endpoints.len()) > limit {
                return None;
            }
            for endpoint in endpoints.into_iter().rev() {
                service
                    .pending_setup
                    .push_front((bytes.to_vec(), direct(endpoint)));
            }
            service.flush_setup();
            Some(())
        })
        .is_some();
    if !queued {
        log::warn!("VMCE failure notice could not be queued");
    }
    service.vmce_round.finish_retry_wait();
    service.vmce_round.clear_round_liveness();
    failure.i18n_key()
}

/// A retry handoff that cannot finish is a local, visible failure. Neither
/// peer may sit on the page while the other reloads; the bounded waits above
/// call this on both the host and the guest.
fn fail_round_handoff(
    state: &mut AppState,
    failure: crate::app::match_runtime::session::VmceRoundFailure,
) {
    log::error!("VMCE result handoff failed: {failure:?}");
    let name = super::local_name(state);
    let peers = peer_sources(state);
    let session = &mut state.match_state.session;
    let key = match state.match_state.lan_service.as_mut() {
        Some(service) => begin_round_failure(session, service, &name, &peers, failure),
        None => {
            let _ = session.vmce_result_abort(failure);
            failure.i18n_key()
        }
    };
    crate::app::vmce_round::return_to_shell(state);
    report_round_failure(state, key);
}

pub(crate) fn mark_assignment_wait(state: &mut AppState) {
    let now = super::super::match_runtime::frame_pacer::frame_bucket(
        super::super::match_runtime::frame_pacer::wall_clock_ms(),
    );
    if let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_round.mark_assignment_wait(now);
    }
}

pub(crate) fn report_round_failure(state: &mut AppState, key: &'static str) {
    let departure_pending = state
        .match_state
        .lan_service
        .as_mut()
        .is_some_and(|service| {
            if service.vmce_round_exit_pending() {
                service.mark_vmce_round_failure_notice(key);
                true
            } else {
                false
            }
        });
    if departure_pending {
        return;
    }
    if state.frontend.multiplayer_setup.is_some() {
        crate::app::shell_network_setup::show_blocking_notice(state, crate::i18n::tr(key));
    } else {
        crate::app::shell_lan::report_error(state, key);
    }
}

/// The shared visible reason for a round delivery failure (the existing key).
pub(crate) fn report_round_send_failed(state: &mut AppState) {
    report_round_failure(
        state,
        crate::app::match_runtime::session::VmceRoundFailure::Delivery.i18n_key(),
    );
}

pub(super) fn fail_round_delivery(state: &mut AppState) -> bool {
    if !active_round(state) {
        return false;
    }
    fail_round_handoff(state, VmceRoundFailure::Delivery);
    true
}

/// Take the match transport into the LAN service before the scenario is
/// retired, keeping its scope for the result page.
pub(crate) fn hold_round_transport(state: &mut AppState) {
    if state.match_state.session_mode() != SessionMode::Lan {
        return;
    }
    let Some(live) = state.match_state.live_network.take() else {
        return;
    };
    if let Some(service) = state.match_state.lan_service.as_mut() {
        service.vmce_round.clear_round_liveness();
        let mut transport = live.into_transport();
        transport.connections_mut().reset();
        service.transport = Some(transport);
    }
}

pub(crate) fn release_round_transport(state: &mut AppState) {
    if let Some(service) = state.match_state.lan_service.as_mut() {
        release_service_transport(service);
    }
}

fn release_service_transport(service: &mut LanService) {
    service.vmce_round.clear_round_liveness();
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
    let vmce_go = matches!(request, NetworkLoadingRequest::VmceResultGo { .. });
    if !vmce && !vmce_go {
        return false;
    }
    let name = super::local_name(state);
    let peers = peer_sources(state);
    if vmce
        && matches!(
            request,
            NetworkLoadingRequest::VmceResultLeave | NetworkLoadingRequest::VmceResultStageRejected
        )
    {
        let message = if matches!(request, NetworkLoadingRequest::VmceResultLeave) {
            VmceSignal::Leave.encode(&name).ok()
        } else {
            VmceSignal::StageRejected.encode(&name).ok()
        };
        let queued = message.is_some_and(|message| queue_encoded_to_peers(state, &message, &peers));
        if !queued {
            if let Some(service) = state.match_state.lan_service.as_mut() {
                service.mark_departure_failed();
            }
            log::error!("VMCE exit record could not be queued");
        }
        return true;
    }
    if let NetworkLoadingRequest::VmceResultGo { nonce } = request {
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
        // Normal path: the retry/campaign-end Go shares the handoff record
        // encoding with the in-process tests.
        let queued = {
            let session = &state.match_state.session;
            let Some(service) = state.match_state.lan_service.as_mut() else {
                return false;
            };
            send_round_request(session, service, &name, &peers, request) == VmceRoundSend::Queued
        };
        if !queued && !fail_round_delivery(state) {
            // The retry/campaign-end continuation already froze the round;
            // the shell still needs the local failure instead of a silent wait.
            crate::app::vmce_round::return_to_shell(state);
            report_round_send_failed(state);
        };
        return true;
    }

    if vmce {
        if matches!(
            request,
            NetworkLoadingRequest::VmceResultRetryAck
                | NetworkLoadingRequest::VmceResultRetryCommitAck
        ) {
            let queued = {
                let session = &state.match_state.session;
                let Some(service) = state.match_state.lan_service.as_mut() else {
                    return true;
                };
                send_round_request(session, service, &name, &peers, request)
                    != VmceRoundSend::Failed
            };
            if !queued {
                fail_round_delivery(state);
            }
            return true;
        }
        match request {
            NetworkLoadingRequest::VmceResultReady => {
                if let Ok(message) = VmceSignal::Ready.encode(&name) {
                    let queued = queue_encoded_to_peers(state, &message, &peers);
                    if !queued {
                        fail_round_delivery(state);
                    }
                }
                return true;
            }
            NetworkLoadingRequest::VmceResultRetryVote => {
                if let Ok(message) = VmceSignal::RetryVote.encode(&name) {
                    if !queue_encoded_to_peers(state, &message, &peers) {
                        fail_round_delivery(state);
                    }
                }
                return true;
            }
            NetworkLoadingRequest::VmceResultStage(stage) => {
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
                        VmceSignal::StageRejected
                            .encode(&name)
                            .is_ok_and(|message| queue_encoded_to_peers(state, &message, &peers))
                    }
                };
                if !queued {
                    fail_round_delivery(state);
                }
                return true;
            }
            NetworkLoadingRequest::VmceResultStageAbsent => {
                if let Ok(message) = VmceSignal::StageAbsent.encode(&name) {
                    if !queue_encoded_to_peers(state, &message, &peers) {
                        fail_round_delivery(state);
                    }
                }
                return true;
            }
            NetworkLoadingRequest::VmceResultStageRejected => {
                if let Ok(message) = VmceSignal::StageRejected.encode(&name) {
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

// Production-path regression tests for the three-phase VMCE retry handoff,
// result-round application liveness and the loading-stage peer fault. They
// use the real `LanService` global reliable queue (real bound UDP sockets and
// the production encoding), drop specific datagrams/ACKs, and assert each
// peer's session/failure state plus the player-visible i18n reason.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::lan_service::tests::service;
    use crate::app::match_runtime::session::{
        MatchSession, NetworkReturnContext, VmceResultEvent, VmceRoundEnding, VmceRoundStep,
    };
    use crate::net::global::GlobalDestination;
    use crate::net::lan_messages::ingame::LoadingFailureReason;
    use crate::skirmish_launch::{LaunchHumanRole, LaunchStartPosition};
    use std::net::SocketAddrV4;

    const NONCE: [u8; 16] = [9; 16];

    /// A real loopback UDP service whose reliable FIFO emits its first attempt
    /// on the tick the test services it (the native lobby timing waits 30).
    fn test_service() -> LanService {
        let mut service = service();
        service.transport.as_mut().unwrap().set_retry_timing(
            crate::net::reliable::RetryTiming {
                retry_delta: 0,
                max_retries: u32::MAX,
                timeout: 600,
            },
            true,
        );
        service
    }

    fn launch() -> crate::skirmish_launch::SkirmishLaunchSession {
        let profile: serde_json::Value = serde_json::from_str(crate::test_fixture::text(
            "tools/map_observation.command-bar.example.json",
        ))
        .unwrap();
        let mut launch: crate::skirmish_launch::SkirmishLaunchSession =
            serde_json::from_value(profile["launch"].clone()).unwrap();
        launch.opponents.clear();
        launch.ai_player_count = 0;
        launch.humans[0].name = "Host".into();
        let mut guest = launch.local_player().clone();
        guest.name = "Guest".into();
        guest.raw_slot_index = Some(1);
        guest.slot.color_index = 2;
        guest.slot.start_position = crate::skirmish_launch::LaunchStartPosition::Position(2);
        launch.humans.push(guest);
        launch
    }
    const SEED: u32 = 0x1234_abcd;
    const FAILURE_KEY: &str = "VERA:VmceRoundSendFailed";
    const UNRESPONSIVE_KEY: &str = "VERA:VmcePeerUnresponsive";
    const LOAD_TIMEOUT_KEY: &str = "VERA:VmceLoadPeerTimeout";

    // ---------------------------------------------------------------- fixtures

    fn vmce_launch(players: usize) -> crate::skirmish_launch::SkirmishLaunchSession {
        let mut launch = launch();
        while launch.humans.len() < players {
            let index = launch.humans.len();
            let mut human = launch.humans[1].clone();
            human.name = format!("Guest{index}");
            human.raw_slot_index = Some(index as u8);
            human.slot.color_index = index as u8;
            human.slot.start_position = LaunchStartPosition::Position(index as u8);
            launch.humans.push(human);
        }
        launch.local_player_index = 0;
        launch.rebuild_house_roster();
        launch
    }

    /// A frozen LAN start that entered the VMCE retry result round, with every
    /// peer endpoint bound to a real loopback UDP service.
    fn retry_session(players: usize, local: usize, endpoints: &[SocketAddrV4]) -> MatchSession {
        let mut session = MatchSession::default();
        session.enter_network(NetworkReturnContext::LanLobby);
        let mut launch = vmce_launch(players);
        launch.local_player_index = local;
        session
            .accept_network_setup(launch.clone(), local == 0, Some(7), false)
            .unwrap();
        for (source, endpoint) in endpoints.iter().enumerate() {
            if source != local {
                session
                    .bind_network_endpoint(&launch.humans[source].name, *endpoint)
                    .unwrap();
            }
        }
        session.prepare_network_timing(2, || 0).unwrap();
        session.freeze_network_start(77).unwrap();
        session
            .enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, players)
            .unwrap();
        session
    }

    /// Run the production propose/confirm agreement to completion on every
    /// peer; the retry handoff is what these tests drive, not the agreement.
    fn agree(sessions: &mut [MatchSession]) {
        assert!(sessions.len() >= 2 && sessions.len() <= 4);
        let mut entries = [0xff; 4];
        for row in 0..sessions.len() {
            entries[row] = row as u8 | 4;
        }
        sessions[0]
            .initialize_vmce_result_agreement(entries, Some(NONCE))
            .unwrap();
        for session in sessions[1..].iter_mut() {
            session
                .initialize_vmce_result_agreement(entries, None)
                .unwrap();
        }
        let proposal = sessions[0].vmce_result_next_record().unwrap();
        sessions[0].vmce_result_record_queued(false).unwrap();
        for session in sessions[1..].iter_mut() {
            session.vmce_result_received(0, proposal.clone()).unwrap();
        }
        for source in 1..sessions.len() {
            let record = sessions[source].vmce_result_next_record().unwrap();
            assert!(!record.confirmed);
            sessions[source].vmce_result_record_queued(false).unwrap();
            for (index, session) in sessions.iter_mut().enumerate() {
                if index != source {
                    session
                        .vmce_result_received(source, record.clone())
                        .unwrap();
                }
            }
        }
        let confirmation = sessions[0].vmce_result_next_record().unwrap();
        assert!(confirmation.confirmed);
        sessions[0].vmce_result_record_queued(true).unwrap();
        for session in sessions[1..].iter_mut() {
            session
                .vmce_result_received(0, confirmation.clone())
                .unwrap();
        }
        for source in 1..sessions.len() {
            let record = sessions[source].vmce_result_next_record().unwrap();
            assert!(record.confirmed);
            sessions[source].vmce_result_record_queued(true).unwrap();
            for (index, session) in sessions.iter_mut().enumerate() {
                if index != source {
                    session
                        .vmce_result_received(source, record.clone())
                        .unwrap();
                }
            }
        }
    }

    fn agreed_pair() -> (
        LanService,
        MatchSession,
        LanService,
        MatchSession,
        SocketAddrV4,
        SocketAddrV4,
    ) {
        let mut host = test_service();
        let mut guest = test_service();
        let host_addr = host.transport.as_ref().unwrap().primary_addr().unwrap();
        let guest_addr = guest.transport.as_ref().unwrap().primary_addr().unwrap();
        guest.admitted_host = Some(("Host".to_owned(), host_addr));
        let mut sessions = [
            retry_session(2, 0, &[host_addr, guest_addr]),
            retry_session(2, 1, &[host_addr, guest_addr]),
        ];
        agree(&mut sessions);
        let [host_session, guest_session] = sessions;
        (
            host,
            host_session,
            guest,
            guest_session,
            host_addr,
            guest_addr,
        )
    }

    // ------------------------------------------------------- datagram routing

    /// Transmit `from`'s queued global records that address `to_addr`; `keep`
    /// chooses which payloads survive (a lost datagram) and `ack_loss` drops
    /// the transport acknowledgements travelling back (a lost ACK).
    #[allow(clippy::too_many_arguments)]
    fn transmit_to(
        from: &mut LanService,
        from_addr: SocketAddrV4,
        to: &mut LanService,
        to_addr: SocketAddrV4,
        now: u32,
        ack_loss: bool,
        keep: impl Fn(&[u8]) -> bool,
    ) {
        let mut frames = Vec::new();
        from.transport
            .as_mut()
            .unwrap()
            .global_mut()
            .service(now, &mut |destination, frame| {
                if matches!(
                    destination,
                    GlobalDestination::Direct { endpoint, .. } if endpoint == to_addr
                ) {
                    frames.push(frame.to_vec());
                }
            });
        for frame in frames {
            if !keep(&frame) {
                continue;
            }
            let mut replies = Vec::new();
            to.transport
                .as_mut()
                .unwrap()
                .global_mut()
                .receive(from_addr, 0, &frame, now, &mut |_, reply| {
                    replies.push(reply.to_vec())
                })
                .unwrap();
            if ack_loss {
                continue;
            }
            for reply in replies {
                from.transport
                    .as_mut()
                    .unwrap()
                    .global_mut()
                    .receive(to_addr, 0, &reply, now, &mut |_, _| {})
                    .unwrap();
            }
        }
    }

    fn transmit(
        from: &mut LanService,
        from_addr: SocketAddrV4,
        to: &mut LanService,
        to_addr: SocketAddrV4,
        now: u32,
        ack_loss: bool,
    ) {
        transmit_to(from, from_addr, to, to_addr, now, ack_loss, |_| true);
    }

    /// Transmit `from`'s queued records to several peers in one service pass,
    /// so two destinations admitted on the same tick are all emitted.
    fn transmit_all(
        from: &mut LanService,
        from_addr: SocketAddrV4,
        to: &mut [(&mut LanService, SocketAddrV4)],
        now: u32,
    ) {
        let mut frames = Vec::new();
        from.transport
            .as_mut()
            .unwrap()
            .global_mut()
            .service(now, &mut |destination, frame| {
                if let GlobalDestination::Direct { endpoint, .. } = destination {
                    frames.push((endpoint, frame.to_vec()));
                }
            });
        for (endpoint, frame) in frames {
            let Some((service, service_addr)) =
                to.iter_mut().find(|(_, address)| *address == endpoint)
            else {
                continue;
            };
            let service_addr = *service_addr;
            let mut replies = Vec::new();
            service
                .transport
                .as_mut()
                .unwrap()
                .global_mut()
                .receive(from_addr, 0, &frame, now, &mut |_, reply| {
                    replies.push(reply.to_vec())
                })
                .unwrap();
            for reply in replies {
                from.transport
                    .as_mut()
                    .unwrap()
                    .global_mut()
                    .receive(service_addr, 0, &reply, now, &mut |_, _| {})
                    .unwrap();
            }
        }
    }

    /// A sent record whose transport ACK and whose retransmission are both
    /// lost: cancel it at the reliable sender, the production best effort.
    fn lose_queued_sends(service: &mut LanService, endpoint: SocketAddrV4) {
        service
            .transport
            .as_mut()
            .unwrap()
            .global_mut()
            .cancel_sends_to_endpoint(endpoint);
    }

    fn drain(
        service: &mut LanService,
        session: &mut MatchSession,
        now: u32,
    ) -> Vec<VmceRoundAction> {
        let mut actions = Vec::new();
        loop {
            // The production transport service pass retires consumed front
            // entries; without it a consumed entry hides the next datagram.
            service
                .transport
                .as_mut()
                .unwrap()
                .global_mut()
                .retire_consumed();
            let Some(incoming) = service.transport.as_mut().unwrap().pop_global_received() else {
                break;
            };
            match receive_round_record(session, service, &incoming, now).unwrap() {
                Some(action) => actions.push(action),
                None => panic!("unexpected VMCE result record"),
            }
        }
        actions
    }

    fn reloads(actions: &[VmceRoundAction]) -> usize {
        actions
            .iter()
            .filter(|action| matches!(action, VmceRoundAction::Step(VmceRoundStep::Retry)))
            .count()
    }

    fn flush(session: &mut MatchSession, service: &mut LanService, name: &str) -> bool {
        let peers = round_peers(session);
        flush_round_requests(session, service, name, &peers)
    }

    fn message(key: &str) -> String {
        let text = crate::i18n::tr(key);
        assert!(!text.is_empty(), "{key} has no player-visible text");
        assert_ne!(text, key, "{key} is missing from the shipped catalog");
        text
    }

    fn freeze(session: &mut MatchSession) {
        let (max_ahead, latency_fudge) = crate::net::lockstep::continuation_timing(0, &[]);
        session
            .freeze_vmce_result(max_ahead, latency_fudge, None)
            .unwrap();
    }

    // ------------------------------------------------------------- handoff

    #[test]
    fn vmce_application_commit_proof_survives_a_lost_transport_ack() {
        let (mut host, mut host_session, mut guest, mut guest_session, host_addr, guest_addr) =
            agreed_pair();
        host_session.vmce_result_assigned().unwrap();
        host_session.vmce_result_queue_go().unwrap();
        assert!(flush(&mut host_session, &mut host, "Host"));
        // Host -> Go, guest answers RetryAck.
        transmit(&mut host, host_addr, &mut guest, guest_addr, 1, false);
        assert_eq!(
            drain(&mut guest, &mut guest_session, 1),
            vec![VmceRoundAction::Consumed]
        );
        assert!(flush(&mut guest_session, &mut guest, "Guest"));
        transmit(&mut guest, guest_addr, &mut host, host_addr, 2, false);
        drain(&mut host, &mut host_session, 2);
        assert!(host_session.vmce_result_retry_acks_complete());

        // Host queues the commit. The guest applies it and answers with the
        // application ACK; the transport ACK for the commit datagram is lost.
        let peers = round_peers(&host_session);
        assert_eq!(
            retry_handoff_step(&mut host_session, &mut host, "Host", &peers, 3, true),
            VmceRoundHandoff::Continue
        );
        assert!(host.vmce_round.retry_commit_queued());
        transmit(&mut host, host_addr, &mut guest, guest_addr, 4, true);
        assert_eq!(reloads(&drain(&mut guest, &mut guest_session, 4)), 0);
        assert!(guest_session.vmce_result_retry_commit_received());
        assert!(flush(&mut guest_session, &mut guest, "Guest"));

        // The application proof is what lets the host start; the commit's own
        // queue state is irrelevant.
        transmit(&mut guest, guest_addr, &mut host, host_addr, 5, false);
        drain(&mut host, &mut host_session, 5);
        assert!(host_session.vmce_result_retry_commit_acks_complete());
        let peers = round_peers(&host_session);
        assert_eq!(
            retry_handoff_step(&mut host_session, &mut host, "Host", &peers, 6, true),
            VmceRoundHandoff::FreezeRetry
        );
        assert!(host_session.vmce_result_phase().is_some());
        freeze(&mut host_session);
        assert!(host_session.vmce_result_phase().is_none());

        // The guest reloads only on RetryStart; a duplicate datagram adds no
        // second reload.
        transmit(&mut host, host_addr, &mut guest, guest_addr, 7, false);
        let actions = drain(&mut guest, &mut guest_session, 7);
        assert_eq!(reloads(&actions), 1);
        assert!(flush(&mut guest_session, &mut guest, "Guest"));
        transmit(&mut host, host_addr, &mut guest, guest_addr, 8, false);
        assert!(drain(&mut guest, &mut guest_session, 8).is_empty());
        assert_eq!(
            guest_session
                .vmce_result_event(VmceResultEvent::RetryStart)
                .unwrap(),
            VmceRoundStep::None,
            "a repeated start cannot reload twice"
        );
        freeze(&mut guest_session);
        assert!(guest_session.vmce_result_phase().is_none());
    }

    #[test]
    fn vmce_lost_application_commit_ack_is_repaired_by_a_commit_retransmission() {
        let (mut host, mut host_session, mut guest, mut guest_session, host_addr, guest_addr) =
            agreed_pair();
        host_session.vmce_result_assigned().unwrap();
        host_session.vmce_result_queue_go().unwrap();
        assert!(flush(&mut host_session, &mut host, "Host"));
        transmit(&mut host, host_addr, &mut guest, guest_addr, 1, false);
        drain(&mut guest, &mut guest_session, 1);
        assert!(flush(&mut guest_session, &mut guest, "Guest"));
        transmit(&mut guest, guest_addr, &mut host, host_addr, 2, false);
        drain(&mut host, &mut host_session, 2);
        let peers = round_peers(&host_session);
        assert_eq!(
            retry_handoff_step(&mut host_session, &mut host, "Host", &peers, 3, true),
            VmceRoundHandoff::Continue
        );
        // The commit reaches the guest's application, but its ACK datagram is
        // lost and its own reliable retransmission is cancelled too.
        transmit(&mut host, host_addr, &mut guest, guest_addr, 4, true);
        drain(&mut guest, &mut guest_session, 4);
        assert!(guest_session.vmce_result_retry_commit_received());
        assert!(flush(&mut guest_session, &mut guest, "Guest"));
        lose_queued_sends(&mut guest, host_addr);
        assert!(!host_session.vmce_result_retry_commit_acks_complete());
        let peers = round_peers(&host_session);
        assert_eq!(
            retry_handoff_step(&mut host_session, &mut host, "Host", &peers, 5, true),
            VmceRoundHandoff::Continue,
            "the host keeps waiting instead of reloading"
        );
        // Retransmission window: a fresh commit record makes the guest answer
        // again even though the reliable channel already delivered the first.
        let resend_at = 4 + RETRY_COMMIT_RESEND_BUCKETS;
        let peers = round_peers(&host_session);
        assert_eq!(
            retry_handoff_step(
                &mut host_session,
                &mut host,
                "Host",
                &peers,
                resend_at,
                true
            ),
            VmceRoundHandoff::Continue
        );
        transmit(
            &mut host, host_addr, &mut guest, guest_addr, resend_at, false,
        );
        drain(&mut guest, &mut guest_session, resend_at);
        assert!(flush(&mut guest_session, &mut guest, "Guest"));
        transmit(
            &mut guest,
            guest_addr,
            &mut host,
            host_addr,
            resend_at + 1,
            false,
        );
        drain(&mut host, &mut host_session, resend_at + 1);
        assert!(host_session.vmce_result_retry_commit_acks_complete());
        let peers = round_peers(&host_session);
        assert_eq!(
            retry_handoff_step(
                &mut host_session,
                &mut host,
                "Host",
                &peers,
                resend_at + 2,
                true
            ),
            VmceRoundHandoff::FreezeRetry
        );
        assert!(host_session.vmce_result_phase().is_some());
    }

    #[test]
    fn vmce_partial_commit_three_players_never_starts_and_every_peer_exits_visibly() {
        let mut host = test_service();
        let mut first = test_service();
        let mut second = test_service();
        let host_addr = host.transport.as_ref().unwrap().primary_addr().unwrap();
        let first_addr = first.transport.as_ref().unwrap().primary_addr().unwrap();
        let second_addr = second.transport.as_ref().unwrap().primary_addr().unwrap();
        first.admitted_host = Some(("Host".to_owned(), host_addr));
        second.admitted_host = Some(("Host".to_owned(), host_addr));
        let endpoints = [host_addr, first_addr, second_addr];
        let mut sessions = [
            retry_session(3, 0, &endpoints),
            retry_session(3, 1, &endpoints),
            retry_session(3, 2, &endpoints),
        ];
        agree(&mut sessions);
        let [mut host_session, mut first_session, mut second_session] = sessions;
        host_session.vmce_result_assigned().unwrap();
        host_session.vmce_result_queue_go().unwrap();
        assert!(flush(&mut host_session, &mut host, "Host"));
        // Both guests consume Go and ACK it.
        transmit_all(
            &mut host,
            host_addr,
            &mut [(&mut first, first_addr), (&mut second, second_addr)],
            1,
        );
        assert_eq!(
            drain(&mut first, &mut first_session, 1),
            vec![VmceRoundAction::Consumed]
        );
        assert_eq!(
            drain(&mut second, &mut second_session, 1),
            vec![VmceRoundAction::Consumed]
        );
        assert!(flush(&mut first_session, &mut first, "Guest1"));
        assert!(flush(&mut second_session, &mut second, "Guest2"));
        transmit(&mut first, first_addr, &mut host, host_addr, 2, false);
        transmit(&mut second, second_addr, &mut host, host_addr, 2, false);
        drain(&mut host, &mut host_session, 2);
        assert!(host_session.vmce_result_retry_acks_complete());
        // The host commits; only the first guest receives and confirms it.
        let peers = round_peers(&host_session);
        assert_eq!(
            retry_handoff_step(&mut host_session, &mut host, "Host", &peers, 3, true),
            VmceRoundHandoff::Continue
        );
        transmit(&mut host, host_addr, &mut first, first_addr, 4, false);
        assert_eq!(
            drain(&mut first, &mut first_session, 4),
            vec![VmceRoundAction::Step(VmceRoundStep::None)]
        );
        assert!(first_session.vmce_result_retry_commit_received());
        assert!(flush(&mut first_session, &mut first, "Guest1"));
        transmit(&mut first, first_addr, &mut host, host_addr, 5, false);
        drain(&mut host, &mut host_session, 5);
        assert!(
            !host_session.vmce_result_retry_commit_acks_complete(),
            "the starved guest never confirmed the commit"
        );
        // The second guest's commit confirmation never arrives. Drive every
        // application resend window with a healthy transport: the overall
        // commit bound must NOT be extended by those retransmissions.
        let mut failed_at = None;
        for now in
            (123..=RETRY_COMMIT_WAIT_BUCKETS + 3).step_by(RETRY_COMMIT_RESEND_BUCKETS as usize)
        {
            let peers = round_peers(&host_session);
            let before = host
                .transport
                .as_ref()
                .unwrap()
                .global()
                .queued_send_count();
            match retry_handoff_step(&mut host_session, &mut host, "Host", &peers, now, true) {
                VmceRoundHandoff::Continue => {
                    assert!(
                        host.transport
                            .as_ref()
                            .unwrap()
                            .global()
                            .queued_send_count()
                            > before,
                        "each resend window queues a fresh commit at t={now}"
                    );
                }
                VmceRoundHandoff::Failed(failure) => {
                    assert_eq!(failure, VmceRoundFailure::PeerUnresponsive);
                    failed_at = Some(now);
                    break;
                }
                VmceRoundHandoff::FreezeRetry => panic!("the host must never start"),
            }
        }
        assert_eq!(
            failed_at,
            Some(3 + RETRY_COMMIT_WAIT_BUCKETS),
            "a resend must never extend the overall commit bound"
        );
        assert!(
            host_session.vmce_result_phase().is_some(),
            "the host stays in the round until the failure transition"
        );
        assert!(!host.vmce_round.retry_start_sent(), "no start may be sent");
        let key = begin_round_failure(
            &mut host_session,
            &mut host,
            "Host",
            &peers,
            VmceRoundFailure::PeerUnresponsive,
        );
        assert_eq!(key, UNRESPONSIVE_KEY);
        assert!(message(key).contains("stopped responding"));
        assert!(host_session.vmce_result_failure().is_some());
        // Production exit owner: notify the peers, then leave within the bound.
        assert!(host.begin_vmce_round_departure(700));
        host.mark_vmce_round_failure_notice(key);
        assert_eq!(host.departure.as_ref().unwrap().notice, Some(key));
        // The responsive guest is told and exits with the same visible reason.
        transmit(&mut host, host_addr, &mut first, first_addr, 701, false);
        let actions = drain(&mut first, &mut first_session, 701);
        assert_eq!(reloads(&actions), 0);
        assert!(
            matches!(
                actions.last(),
                Some(VmceRoundAction::Step(VmceRoundStep::Exit(true)))
            ),
            "the peer ends on the visible failure after any queued resends"
        );
        let guest_key = first_session
            .vmce_result_failure()
            .expect("the guest leaves with the host's reason")
            .i18n_key();
        assert_eq!(guest_key, UNRESPONSIVE_KEY);
        assert!(first.begin_vmce_round_departure(701));
        first.mark_vmce_round_failure_notice(guest_key);
        assert!(flush(&mut first_session, &mut first, "Guest1"));
        // The starved guest never saw the commit; its own bounded wait ends it
        // with the same player-visible reason instead of hanging.
        let peers = round_peers(&second_session);
        assert_eq!(
            retry_handoff_step(
                &mut second_session,
                &mut second,
                "Guest2",
                &peers,
                1 + RETRY_GUEST_WAIT_BUCKETS + 1,
                true
            ),
            VmceRoundHandoff::Failed(VmceRoundFailure::PeerUnresponsive)
        );
        let key = begin_round_failure(
            &mut second_session,
            &mut second,
            "Guest2",
            &peers,
            VmceRoundFailure::PeerUnresponsive,
        );
        assert_eq!(key, UNRESPONSIVE_KEY);
        let _ = message(key);
        assert!(second.begin_vmce_round_departure(1_602));
        second.mark_vmce_round_failure_notice(key);
        // Host and first guest leave the room within their bounded departure.
        transmit(&mut first, first_addr, &mut host, host_addr, 702, false);
        drain(&mut host, &mut host_session, 702);
        transmit(&mut host, host_addr, &mut first, first_addr, 703, false);
        drain(&mut first, &mut first_session, 703);
        assert_eq!(
            host.departure_ready(1_301, true),
            Some(crate::app::lan_service::DepartureReason::RoundResult)
        );
        assert!(host_session.vmce_result_phase().is_some());
        assert_eq!(
            first.departure_ready(1_301, true),
            Some(crate::app::lan_service::DepartureReason::RoundResult)
        );
        assert_eq!(
            second.departure_ready(2_203, true),
            Some(crate::app::lan_service::DepartureReason::RoundResult)
        );
    }

    #[test]
    fn vmce_full_reliable_queue_ends_in_a_visible_bounded_failure() {
        let (mut host, mut host_session, mut guest, mut guest_session, host_addr, guest_addr) =
            agreed_pair();
        host_session.vmce_result_assigned().unwrap();
        host_session.vmce_result_queue_go().unwrap();
        assert!(flush(&mut host_session, &mut host, "Host"));
        transmit(&mut host, host_addr, &mut guest, guest_addr, 1, false);
        drain(&mut guest, &mut guest_session, 1);
        assert!(flush(&mut guest_session, &mut guest, "Guest"));
        transmit(&mut guest, guest_addr, &mut host, host_addr, 2, false);
        drain(&mut host, &mut host_session, 2);
        // Fill the guest's reliable FIFO before it can answer the commit.
        let endpoint = SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 30999);
        for sequence in 0..4096u32 {
            guest
                .pending_setup
                .push_back((vec![sequence as u8], direct(endpoint)));
        }
        let peers = round_peers(&host_session);
        assert_eq!(
            retry_handoff_step(&mut host_session, &mut host, "Host", &peers, 3, true),
            VmceRoundHandoff::Continue
        );
        transmit(&mut host, host_addr, &mut guest, guest_addr, 4, false);
        drain(&mut guest, &mut guest_session, 4);
        assert!(guest_session.vmce_result_retry_commit_received());
        assert!(
            !flush(&mut guest_session, &mut guest, "Guest"),
            "a full queue is a visible enqueue failure"
        );
        assert_eq!(guest.pending_setup.len(), 4096);
        // The production failure transition the send owner performs.
        let guest_peers = round_peers(&guest_session);
        let key = begin_round_failure(
            &mut guest_session,
            &mut guest,
            "Guest",
            &guest_peers,
            VmceRoundFailure::Delivery,
        );
        assert_eq!(key, FAILURE_KEY);
        assert!(message(key).contains("Could not send the round message"));
        assert!(
            guest_session.vmce_result_phase().is_some(),
            "the page stays until the departure owner takes it"
        );
        assert!(guest.begin_vmce_round_departure(5));
        guest.mark_vmce_round_failure_notice(key);
        assert_eq!(guest.departure.as_ref().unwrap().notice, Some(key));
        assert_eq!(
            guest.departure_ready(606, true),
            Some(crate::app::lan_service::DepartureReason::RoundResult)
        );
        assert!(guest_session.vmce_result_failure().is_some());
        // The host has no commit proof and never reloads.
        assert!(!host_session.vmce_result_retry_commit_acks_complete());
        assert!(!host.vmce_round.retry_start_sent());
    }

    // ------------------------------------------------------------- liveness

    #[test]
    fn vmce_guest_detects_silent_host_before_first_result_nonce() {
        let mut host = test_service();
        let mut guest = test_service();
        let host_addr = host.transport.as_ref().unwrap().primary_addr().unwrap();
        let guest_addr = guest.transport.as_ref().unwrap().primary_addr().unwrap();
        let mut host_session = retry_session(2, 0, &[host_addr, guest_addr]);
        let mut guest_session = retry_session(2, 1, &[host_addr, guest_addr]);
        let entries = [4, 5, 0xff, 0xff];
        host_session
            .initialize_vmce_result_agreement(entries, Some(NONCE))
            .unwrap();
        guest_session
            .initialize_vmce_result_agreement(entries, None)
            .unwrap();
        let host_peers = round_peers(&host_session);
        let guest_peers = round_peers(&guest_session);
        assert_eq!(guest_session.vmce_result_nonce(), None);

        // Before the proposal, live host heartbeats allow a slow player to
        // wait even beyond the silence bound.
        for now in (0..=1_200).step_by(60) {
            assert_eq!(
                round_liveness_step(&guest_session, &mut guest, "Guest", &guest_peers, now),
                None
            );
            assert_eq!(
                round_liveness_step(&host_session, &mut host, "Host", &host_peers, now),
                None
            );
            transmit(&mut host, host_addr, &mut guest, guest_addr, now, false);
            drain(&mut guest, &mut guest_session, now);
            transmit(&mut guest, guest_addr, &mut host, host_addr, now, false);
            drain(&mut host, &mut host_session, now);
        }
        assert_eq!(guest_session.vmce_result_nonce(), None);
        assert_eq!(
            round_liveness_step(&guest_session, &mut guest, "Guest", &guest_peers, 1_799),
            None
        );
        assert_eq!(
            round_liveness_step(&guest_session, &mut guest, "Guest", &guest_peers, 1_800),
            Some(VmceRoundFailure::PeerUnresponsive)
        );
    }

    #[test]
    fn vmce_failure_notice_survives_a_backpressured_departure() {
        let (mut host, mut host_session, mut guest, mut guest_session, host_addr, guest_addr) =
            agreed_pair();
        let blocked = SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 30999);
        while guest.send(&[1], direct(blocked), true) {}
        for _ in 0..4096 {
            guest.pending_setup.push_back((vec![1], direct(blocked)));
        }
        let peers = round_peers(&guest_session);
        let key = begin_round_failure(
            &mut guest_session,
            &mut guest,
            "Guest",
            &peers,
            VmceRoundFailure::Delivery,
        );
        assert_eq!(key, FAILURE_KEY);
        assert_eq!(
            guest.pending_setup.len(),
            4097,
            "the bounded priority slot preserves other pending traffic"
        );
        assert_eq!(
            VmceSignal::decode(&guest.pending_setup.front().unwrap().0),
            Some(VmceSignal::RoundFailed)
        );
        assert!(guest.begin_vmce_round_departure(5));
        guest.mark_vmce_round_failure_notice(key);
        assert_eq!(
            guest.departure_ready(6, true),
            None,
            "the pending notice must not be dropped"
        );
        assert_eq!(guest.pending_setup.len(), 4097);

        // The existing global queue clears, and the production FIFO flushes
        // and delivers the notice to the other result owner.
        guest
            .transport
            .as_mut()
            .unwrap()
            .global_mut()
            .cancel_sends_to_endpoint(blocked);
        guest.flush_setup();
        transmit(&mut guest, guest_addr, &mut host, host_addr, 7, false);
        let actions = drain(&mut host, &mut host_session, 7);
        assert!(matches!(
            actions.last(),
            Some(VmceRoundAction::Step(VmceRoundStep::Exit(true)))
        ));
        assert_eq!(
            host_session.vmce_result_failure(),
            Some(VmceRoundFailure::Delivery)
        );
        assert_eq!(
            guest.departure_ready(8, true),
            None,
            "other queued traffic remains within the departure bound"
        );
        assert!(guest.pending_setup.iter().all(|(bytes, _)| bytes == &[1]));
        assert_eq!(
            guest.departure_ready(605, true),
            Some(crate::app::lan_service::DepartureReason::RoundResult)
        );
    }

    #[test]
    fn vmce_idle_but_alive_host_keeps_the_guest_waiting_indefinitely() {
        let (mut host, mut host_session, mut guest, mut guest_session, host_addr, guest_addr) =
            agreed_pair();
        let host_peers = round_peers(&host_session);
        let guest_peers = round_peers(&guest_session);
        // Far beyond both the retry Go and the commit budgets: the human host
        // is merely idle, and its heartbeats prove it is alive.
        for now in (0..=60_000).step_by(60) {
            assert_eq!(
                round_liveness_step(&host_session, &mut host, "Host", &host_peers, now),
                None
            );
            assert_eq!(
                round_liveness_step(&guest_session, &mut guest, "Guest", &guest_peers, now),
                None
            );
            transmit(&mut host, host_addr, &mut guest, guest_addr, now, false);
            transmit(&mut guest, guest_addr, &mut host, host_addr, now, false);
            drain(&mut guest, &mut guest_session, now);
            drain(&mut host, &mut host_session, now);
        }
        assert!(host_session.vmce_result_phase().is_some());
        assert!(guest_session.vmce_result_phase().is_some());
    }

    #[test]
    fn vmce_silent_host_is_detected_in_bounded_time() {
        let (mut host, _host_session, mut guest, mut guest_session, host_addr, guest_addr) =
            agreed_pair();
        let guest_peers = round_peers(&guest_session);
        let mut detected = None;
        for now in (0..=2_000).step_by(60) {
            // The host process is gone: it never sends a heartbeat. Any queued
            // datagram is still transmitted, proving silence is the trigger.
            transmit(&mut host, host_addr, &mut guest, guest_addr, now, false);
            drain(&mut guest, &mut guest_session, now);
            if let Some(failure) =
                round_liveness_step(&guest_session, &mut guest, "Guest", &guest_peers, now)
            {
                detected = Some((now, failure));
                break;
            }
        }
        let (now, failure) = detected.expect("the silent host must be detected");
        assert_eq!(failure, VmceRoundFailure::PeerUnresponsive);
        assert!(now >= ROUND_HEARTBEAT_TIMEOUT_BUCKETS);
        assert!(now < ROUND_HEARTBEAT_TIMEOUT_BUCKETS + 120);
        let key = begin_round_failure(
            &mut guest_session,
            &mut guest,
            "Guest",
            &guest_peers,
            failure,
        );
        assert_eq!(key, UNRESPONSIVE_KEY);
        assert!(message(key).contains("stopped responding"));
        assert!(guest.begin_vmce_round_departure(now));
        guest.mark_vmce_round_failure_notice(key);
        assert_eq!(
            guest.departure_ready(now + 600, true),
            Some(crate::app::lan_service::DepartureReason::RoundResult)
        );
        assert!(guest_session.vmce_result_phase().is_some());
    }

    // ------------------------------------------------------------- loading

    /// A frozen VMCE LAN start: the retry/next-stage loading attempt identity
    /// comes from the frozen startup, exactly what the pump's peer wait checks.
    fn vmce_frozen_session(
        players: usize,
        local: usize,
        endpoints: &[SocketAddrV4],
    ) -> MatchSession {
        let mut session = MatchSession::default();
        session.enter_network(NetworkReturnContext::LanLobby);
        let mut launch = vmce_launch(players);
        launch.mode.class = crate::skirmish_modes::MpModeClass::Vmce;
        for (index, human) in launch.humans.iter_mut().enumerate() {
            human.role = LaunchHumanRole::Player {
                group: index as i32,
            };
        }
        launch.local_player_index = local;
        session
            .accept_network_setup(launch.clone(), local == 0, Some(7), false)
            .unwrap();
        for (source, endpoint) in endpoints.iter().enumerate() {
            if source != local {
                session
                    .bind_network_endpoint(&launch.humans[source].name, *endpoint)
                    .unwrap();
            }
        }
        let map_file = session
            .network_setup()
            .unwrap()
            .selected_map_file
            .clone()
            .unwrap();
        let source_sha256 = "a".repeat(64);
        session.set_network_test_map_identity(
            crate::app::match_runtime::session::NetworkMapMetadata {
                display_name: map_file.clone(),
                player_capacity: 8,
                official: false,
                byte_length: 100,
                digest: String::new(),
                source_sha256: source_sha256.clone(),
                effective_digest: "b".repeat(64),
            },
            crate::map::source::LoadedMapSource::Loose {
                path: std::path::PathBuf::from(map_file.clone()),
                payload_len: 100,
                source_sha256: source_sha256.clone(),
                cache_lease: Default::default(),
            },
            "b".repeat(64),
        );
        session
            .accept_network_vmce_map_config(Some(
                crate::app::match_bootstrap::NetworkVmceMapConfig::new(
                    map_file.clone(),
                    source_sha256.clone(),
                    crate::map::vmce_config::VmceMapConfig::default(),
                )
                .unwrap(),
            ))
            .unwrap();
        session.set_network_player_ready(0, true).unwrap();
        session.set_network_map_status(0, 2).unwrap();
        session.set_network_start_seed(SEED).unwrap();
        session.prepare_network_timing(2, || 0).unwrap();
        let assignments = crate::skirmish_launch::VmceHouseAssignments {
            participants: session
                .network_setup()
                .unwrap()
                .humans
                .iter()
                .enumerate()
                .map(
                    |(index, human)| crate::skirmish_launch::VmceHouseAssignment {
                        participant_name: human.name.clone(),
                        raw_slot_index: human.raw_slot_index.expect("fixture lobby row"),
                        player_index: index as u8,
                        owner_name: format!("VeraHouse{index}"),
                    },
                )
                .collect(),
            ..Default::default()
        };
        session
            .freeze_network_start_with_vmce(SEED, Some(assignments))
            .unwrap();
        session
    }

    #[test]
    fn vmce_loading_peer_timeout_returns_both_peers_to_the_lobby_visibly() {
        let mut host = test_service();
        let mut guest = test_service();
        let host_addr = host.transport.as_ref().unwrap().primary_addr().unwrap();
        let guest_addr = guest.transport.as_ref().unwrap().primary_addr().unwrap();
        guest.admitted_host = Some(("Host".to_owned(), host_addr));
        let host_session = vmce_frozen_session(2, 0, &[host_addr, guest_addr]);
        let guest_session = vmce_frozen_session(2, 1, &[host_addr, guest_addr]);
        // Stock Cooperative/native loading keeps the old install behavior.
        let (stock_host, stock_session, _, _, _, _) = crate::app::lan_service::loading_test_pair();
        assert_eq!(vmce_loading_peer_timeout(&stock_session, &stock_host), None);
        // Production decision: the retry/next-stage peer wait timed out.
        let reason = vmce_loading_peer_timeout(&host_session, &host)
            .expect("a VMCE network loading attempt owns the failure");
        assert_eq!(reason, LoadingFailureReason::PeerTimeout);
        assert!(message(LOAD_TIMEOUT_KEY).contains("did not finish loading"));
        assert_eq!(
            crate::app::loading::pump::vmce_load_failure_message(reason),
            crate::i18n::tr(LOAD_TIMEOUT_KEY)
        );
        // Both peers abort through the existing VMCE loading-failure owner.
        let peers = round_peers(&host_session);
        assert!(begin_vmce_loading_failure(
            &host_session,
            &mut host,
            "Host",
            &peers,
            reason,
            "",
            true,
            10
        ));
        assert!(host.vmce_loading_failure_pending());
        transmit(&mut host, host_addr, &mut guest, guest_addr, 11, false);
        let mut decoded = None;
        while let Some(incoming) = guest.transport.as_mut().unwrap().pop_global_received() {
            if let Some(failure) =
                crate::net::lan_messages::ingame::decode_loading_failure(&incoming.payload)
            {
                decoded = Some((failure, incoming.source));
            }
        }
        let ((decoded_reason, scope, attempt), source) =
            decoded.expect("the scoped failure record reached the peer");
        assert_eq!(decoded_reason, LoadingFailureReason::PeerTimeout);
        assert_eq!(source, host_addr);
        assert!(load_failure_admitted(
            &guest_session,
            guest.setup_scope(),
            true,
            source,
            scope,
            attempt
        ));
        assert!(begin_vmce_loading_failure(
            &guest_session,
            &mut guest,
            "Guest",
            &round_peers(&guest_session),
            decoded_reason,
            "",
            false,
            10
        ));
        assert!(guest.vmce_loading_failure_pending());
        // Both bounded departures complete into the LAN-lobby reason.
        assert_eq!(
            host.departure_ready(611, true),
            Some(crate::app::lan_service::DepartureReason::VmceLoading { reason: 4 })
        );
        assert_eq!(
            guest.departure_ready(611, true),
            Some(crate::app::lan_service::DepartureReason::VmceLoading { reason: 4 })
        );
    }

    // ------------------------------------------------------------ unit bounds

    #[test]
    fn terminal_waits_expire_and_clear_with_the_handoff() {
        let mut state = VmceLanRoundState::default();
        state.mark_intermission_prepared([7; 16], false, 10);
        assert!(!state.intermission_wait_expired(609));
        assert!(state.intermission_wait_expired(610));
        state.mark_intermission_entered(false);
        assert!(!state.intermission_wait_expired(1_000));

        state.mark_intermission_received([8; 16], 20);
        assert!(state.intermission_wait_expired(620));
        state.finish_guest_intermission();
        assert!(!state.intermission_wait_expired(2_000));

        state.defer_campaign_completion(30);
        assert!(!state.campaign_completion_wait_expired(629));
        assert!(state.campaign_completion_wait_expired(630));

        state.mark_assignment_wait(40);
        assert!(!state.assignment_wait_expired(639));
        assert!(state.assignment_wait_expired(640));
        state.clear_assignment_wait();
        assert!(!state.assignment_wait_expired(2_000));
        state.clear_visit();
        assert!(!state.campaign_completion_wait_expired(3_000));
        assert!(!state.assignment_wait_expired(3_000));
    }

    #[test]
    fn result_unlock_nonce_is_remembered_per_round_and_clears_with_the_visit() {
        let mut state = VmceLanRoundState::default();
        assert_eq!(state.result_unlock_nonce(), None);
        state.mark_result_unlock_saved([3; 16]);
        assert_eq!(state.result_unlock_nonce(), Some([3; 16]));
        state.clear_visit();
        assert_eq!(state.result_unlock_nonce(), None);
    }

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
    }

    #[test]
    fn vmce_early_records_are_owned_separately_from_generic_bc6_records() {
        let absent = VmceSignal::StageAbsent.encode("Guest").unwrap();
        assert!(is_round_record(&absent));
        let begin = VmceRoundControl::Begin.encode("Host", [7; 16]).unwrap();
        assert!(is_round_record(begin.as_bytes()));
        let start = VmceRoundControl::RetryStart
            .encode("Host", [7; 16])
            .unwrap();
        assert!(is_round_record(start.as_bytes()));
    }

    #[test]
    fn retry_controls_require_their_round_nonce() {
        for control in [VmceRoundControl::RetryCommit, VmceRoundControl::RetryStart] {
            let message = control.encode("Host", [5; 16]).unwrap();
            assert_eq!(VmceRoundControl::decode(&message), Some((control, [5; 16])));
            assert!(control.encode("Host", [0; 16]).is_err());
        }
    }

    #[test]
    fn retry_deadlines_and_commit_resend_stay_bounded() {
        let mut state = VmceLanRoundState::default();
        state.mark_retry_wait(0);
        assert!(state.retry_wait_started());
        assert!(!state.retry_wait_expired(599));
        assert!(state.retry_wait_expired(600));
        // The commit-confirmation bound starts once; retransmissions never
        // extend it.
        state.begin_retry_commit_phase(1_000);
        assert!(!state.retry_wait_expired(1_599));
        assert!(state.retry_wait_expired(1_600));
        state.mark_retry_commit_queued(true, 1_000);
        assert!(state.retry_commit_queued());
        assert!(!state.retry_commit_resend_due(1_000 + RETRY_COMMIT_RESEND_BUCKETS - 1));
        assert!(state.retry_commit_resend_due(1_000 + RETRY_COMMIT_RESEND_BUCKETS));
        state.mark_retry_commit_queued(true, 1_120);
        state.begin_retry_commit_phase(1_120);
        assert!(
            !state.retry_wait_expired(1_599),
            "a resend pass cannot move the commit bound"
        );
        assert!(state.retry_wait_expired(1_600));
        // A duplicate Go cannot extend the guest's single wait either.
        let mut guest = VmceLanRoundState::default();
        guest.mark_guest_retry_wait(0);
        guest.mark_guest_retry_wait(500);
        assert!(!guest.retry_wait_expired(1_499));
        assert!(guest.retry_wait_expired(1_500));
        state.finish_retry_wait();
        assert!(!state.retry_wait_expired(2_000));
        assert!(!state.retry_commit_queued());
        state.clear_visit();
        assert!(!state.retry_wait_expired(2_000));
    }

    #[test]
    fn round_liveness_window_resets_for_a_new_round_and_drops_departed_peers() {
        let mut state = VmceLanRoundState::default();
        state.begin_round_liveness(Some([1; 16]), &[1], 0);
        state.note_peer_heartbeat(1, 100);
        assert_eq!(
            state.expired_round_peer(ROUND_HEARTBEAT_TIMEOUT_BUCKETS + 99),
            None
        );
        assert_eq!(
            state.expired_round_peer(ROUND_HEARTBEAT_TIMEOUT_BUCKETS + 100),
            Some(1)
        );
        // A departed peer is dropped, not reported as unresponsive; the peer
        // still present keeps only its own window.
        state.begin_round_liveness(Some([1; 16]), &[1, 2], 700);
        state.note_peer_heartbeat(2, 700);
        state.begin_round_liveness(Some([1; 16]), &[2], 700);
        assert_eq!(state.expired_round_peer(1_000), None);
        // A later round starts every window fresh.
        state.begin_round_liveness(Some([2; 16]), &[1, 2], 5_000);
        assert_eq!(
            state.expired_round_peer(5_000 + ROUND_HEARTBEAT_TIMEOUT_BUCKETS - 1),
            None
        );
    }
}

/// VMCE's result-phase service loop. Stock Cooperative keeps its own poll; a
/// VMCE result phase never enters it.
pub(super) fn poll(state: &mut AppState, now: u32, service_ok: bool) -> bool {
    if state.match_state.session_mode() != SessionMode::Lan {
        return false;
    }
    if pause_round_poll(state) {
        return false;
    }
    flush_round_work(state, service_ok);
    if campaign_completion_pending(state) {
        match take_campaign_completion(state, service_ok, now) {
            None => {}
            Some(Ok(())) => crate::app::App::vmce_campaign_end(state),
            Some(Err(())) => {
                log::error!("VMCE campaign completion could not be delivered to every peer");
                crate::app::vmce_round::return_to_shell(state);
                report_round_send_failed(state);
            }
        }
        return true;
    }
    if !active_round(state) {
        return false;
    }
    if !service_ok && fail_round_delivery(state) {
        return true;
    }
    if let Err(error) = service_result_agreement(state) {
        log::error!("VMCE result agreement failed: {error:#}");
        let _ = state.match_state.session.quit_vmce_result();
        crate::app::vmce_round::return_to_shell(state);
        return true;
    }
    if !assign_intermission_if_ready(state, now) {
        return true;
    }
    // Records that arrived between the mission's end and this phase opening.
    loop {
        let Some(incoming) = state
            .match_state
            .lan_service
            .as_mut()
            .and_then(take_pending_round)
        else {
            break;
        };
        if let Err(error) = receive(state, &incoming) {
            log::debug!("Ignored queued VMCE result packet: {error:#}");
        }
        unlock_agreed_next_stage(state);
        if round_exit_pending(state) || campaign_completion_pending(state) {
            super::drain_match_requests(state);
            return true;
        }
        if state.match_state.session.vmce_result_phase().is_none() {
            return true;
        }
    }
    loop {
        let Some(incoming) = state
            .match_state
            .lan_service
            .as_mut()
            .and_then(|service| service.transport.as_mut()?.pop_global_received())
        else {
            break;
        };
        if let Err(error) = receive(state, &incoming) {
            log::debug!("Ignored VMCE result packet: {error:#}");
        }
        unlock_agreed_next_stage(state);
        if round_exit_pending(state) || campaign_completion_pending(state) {
            break;
        }
        let service_ok = super::service_transport(state, now);
        if !service_ok && fail_round_delivery(state) {
            return true;
        }
        if state.match_state.session.vmce_result_phase().is_none() {
            return true;
        }
    }
    if let Err(error) = service_result_agreement(state) {
        log::error!("VMCE result agreement failed: {error:#}");
        let _ = state.match_state.session.quit_vmce_result();
        crate::app::vmce_round::return_to_shell(state);
        return true;
    }
    unlock_agreed_next_stage(state);
    super::drain_match_requests(state);
    if state.match_state.session.vmce_result_phase().is_none() {
        return true;
    }
    let now = super::super::match_runtime::frame_pacer::frame_bucket(
        super::super::match_runtime::frame_pacer::wall_clock_ms(),
    );
    let name = super::local_name(state);
    let peers = peer_sources(state);
    // The bounded retry handoff runs first: while it owns an active 600-bucket
    // deadline, a starved unreliable heartbeat channel must not pre-empt the
    // handoff with an earlier liveness verdict. Outside that bounded handoff
    // (the pre-Go human wait) application liveness decides whether a peer is
    // merely idle or gone.
    let handoff = {
        let session = &mut state.match_state.session;
        let Some(service) = state.match_state.lan_service.as_mut() else {
            return true;
        };
        retry_handoff_step(session, service, &name, &peers, now, service_ok)
    };
    match handoff {
        VmceRoundHandoff::Continue => {}
        VmceRoundHandoff::FreezeRetry => {
            crate::app::vmce_round::retry_last_mission(state);
            return true;
        }
        VmceRoundHandoff::Failed(failure) => {
            fail_round_handoff(state, failure);
            return true;
        }
    }
    // A bounded retry handoff owns its own deadline; do not let a starved
    // unreliable heartbeat channel end it with a different (and earlier)
    // verdict. The handoff itself reports PeerUnresponsive when it expires.
    let retry_bound_active = state.match_state.session.vmce_result_ending()
        == Some(crate::app::match_runtime::session::VmceRoundEnding::Retry)
        && state
            .match_state
            .session
            .vmce_result_phase()
            .is_some_and(|round| round.outcome_committed())
        && state
            .match_state
            .lan_service
            .as_ref()
            .is_some_and(|service| {
                service.vmce_round.retry_wait_started()
                    && !service.vmce_round.retry_wait_expired(now)
            });
    let liveness = if retry_bound_active {
        None
    } else {
        let session = &state.match_state.session;
        state
            .match_state
            .lan_service
            .as_mut()
            .and_then(|service| round_liveness_step(session, service, &name, &peers, now))
    };
    if let Some(failure) = liveness {
        fail_round_handoff(state, failure);
        return true;
    }
    continue_after_result(state)
}
