//! The LAN Cooperative result dialog BC6 over the match's global route.
//!
//! BC6's loop (`0x0046FC00`) keeps servicing the global manager `0xA8E9C0`
//! that carried the match: its records go with ack to every other node
//! (`0x00541260`, nodes from 1), and it reads only its own kinds. Here the
//! private channels end with the match, but the socket keeps its LAN scope
//! and the admitted setup, so the round's records reach the same peers and
//! the next mission loads over the same admission. The round state belongs
//! to `MatchSession`'s `CooperativeRound` phase; this module only moves its
//! records.

use super::*;
use crate::app::match_runtime::session::{
    CooperativeRoundEvent, CooperativeRoundOptions, NetworkAiOptions, NetworkLoadingRequest,
};
use crate::net::lan_messages::cooperative::{
    CooperativeAiWords, CooperativeHumanWords, CooperativeOptionsRecord, CooperativeSignal,
    OPTIONS_KIND,
};
#[cfg(feature = "vmce")]
use crate::net::lan_messages::cooperative::{
    RetryLoadFailureReason, VmceStageRecord, encode_retry_load_failure,
};
use crate::skirmish_launch::{
    LaunchCountry, LaunchStartPosition, LaunchTeam, SkirmishAiSlot, SkirmishLaunchSession,
    SkirmishLocalSlot,
};

/// At a LAN co-op mission's end, before `retire_finished_scenario`: take the
/// match's transport back without leaving its scope, so the shared teardown
/// finds no match transport to return.
pub(crate) fn hold_transport(state: &mut AppState) {
    if state.match_state.session_mode() != SessionMode::Lan {
        return;
    }
    let Some(live) = state.match_state.live_network.take() else {
        return;
    };
    if let Some(service) = state.match_state.lan_service.as_mut() {
        let mut transport = live.into_transport();
        transport.connections_mut().reset();
        service.transport = Some(transport);
    }
}

/// BC6's exit to the shell: its last records (Quit's `0x24`) go out first,
/// then the visit leaves the match scope as [`return_transport`] does.
/// RESIDUAL: the `0x24` gets one transmission here; native's global manager
/// keeps retrying it from the shell. A lost one leaves the partner on BC6
/// until it quits too.
pub(crate) fn release_transport(state: &mut AppState) {
    if state.match_state.session_mode() != SessionMode::Lan {
        return;
    }
    drain_match_requests(state);
    let now = super::super::match_runtime::frame_pacer::frame_bucket(
        super::super::match_runtime::frame_pacer::wall_clock_ms(),
    );
    service_transport(state, now);
    if let Some(service) = state.match_state.lan_service.as_mut() {
        #[cfg(feature = "vmce")]
        let vmce_exit = service
            .departure
            .as_ref()
            .is_some_and(|wait| wait.reason == DepartureReason::CooperativeRound);
        if let Some(transport) = service.transport.as_mut() {
            transport.connections_mut().reset();
            #[cfg(feature = "vmce")]
            if vmce_exit {
                transport.global_mut().use_native_acknowledgments();
            }
            transport.clear_lan_scope();
        }
        service.pending_round.clear();
        service.clear_setup();
    }
}

/// One service pass while the round is open; false outside it.
pub(super) fn poll(state: &mut AppState, now: u32, service_ok: bool) -> bool {
    #[cfg(feature = "vmce")]
    if state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(|service| {
            service
                .departure
                .as_ref()
                .is_some_and(|wait| wait.reason == DepartureReason::CooperativeRound)
        })
    {
        return false;
    }
    #[cfg(feature = "vmce")]
    if let Some(service) = state.match_state.lan_service.as_mut()
        && (service.vmce_intermission_epoch.is_some()
            || state
                .match_state
                .session
                .cooperative_round()
                .is_some_and(|round| round.vera()))
    {
        service.flush_setup();
    }
    #[cfg(feature = "vmce")]
    if !service_ok
        && let Some(service) = state.match_state.lan_service.as_mut()
        && service.vmce_intermission_epoch.is_some()
    {
        service.vmce_intermission_failed = true;
    }
    #[cfg(feature = "vmce")]
    if state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(|service| service.vmce_campaign_completion_pending)
    {
        let service = state
            .match_state
            .lan_service
            .as_mut()
            .expect("pending service");
        service.flush_setup();
        match service.take_vmce_campaign_completion(service_ok) {
            None => {}
            Some(Ok(())) => crate::app::App::vmce_campaign_end(state),
            Some(Err(())) => {
                log::error!("VMCE campaign completion could not be delivered to every peer");
                crate::app::cooperative_round::return_to_shell(state);
            }
        }
        return true;
    }
    if state.match_state.live_network.is_some()
        || state.match_state.session.cooperative_round().is_none()
    {
        return false;
    }
    #[cfg(feature = "vmce")]
    if !service_ok && fail_vmce_round_delivery(state) {
        return true;
    }
    #[cfg(feature = "vmce")]
    if state
        .match_state
        .session
        .cooperative_round()
        .is_some_and(|round| round.vera())
    {
        if let Err(error) = service_vmce_result_agreement(state) {
            log::error!("VMCE result agreement failed: {error:#}");
            let _ = state.match_state.session.quit_cooperative_round();
            crate::app::cooperative_round::return_to_shell(state);
            return false;
        }
    }
    #[cfg(feature = "vmce")]
    if state.match_state.session.host()
        && state
            .match_state
            .lan_service
            .as_ref()
            .is_some_and(|service| {
                service.vmce_intermission_epoch.is_some() && !service.vmce_intermission_entered
            })
        && state
            .match_state
            .session
            .cooperative_round()
            .is_some_and(|round| {
                round.vera_ending() == crate::app::match_runtime::session::VmceRoundEnding::Next
                    && !round.vmce_next_accepted()
            })
    {
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
    }
    // Records that arrived between the mission's end and this round's phase
    // opening were queued by the transport dispatcher; apply them first in
    // arrival order so a peer's begin/ready/stage packet is not lost.
    loop {
        let Some(incoming) = state
            .match_state
            .lan_service
            .as_mut()
            .and_then(|service| service.pending_round.pop_front())
        else {
            break;
        };
        if let Err(error) = receive(state, incoming) {
            log::debug!("Ignored queued co-op round packet: {error:#}");
        }
        #[cfg(feature = "vmce")]
        if vmce_round_exit_pending(state) {
            return true;
        }
        #[cfg(feature = "vmce")]
        if state
            .match_state
            .lan_service
            .as_ref()
            .is_some_and(|service| service.vmce_campaign_completion_pending)
        {
            return true;
        }
        if state.match_state.session.cooperative_round().is_none() {
            return false;
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
        if let Err(error) = receive(state, incoming) {
            log::debug!("Ignored co-op round packet: {error:#}");
        }
        #[cfg(feature = "vmce")]
        if vmce_round_exit_pending(state) {
            return true;
        }
        #[cfg(feature = "vmce")]
        if state
            .match_state
            .lan_service
            .as_ref()
            .is_some_and(|service| service.vmce_campaign_completion_pending)
        {
            return true;
        }
        let service_ok = service_transport(state, now);
        #[cfg(feature = "vmce")]
        if !service_ok && fail_vmce_round_delivery(state) {
            return true;
        }
        #[cfg(not(feature = "vmce"))]
        let _ = service_ok;
        // Go or Quit closed the round: later packets (the next mission's
        // loading) belong to the ordinary service.
        if state.match_state.session.cooperative_round().is_none() {
            return false;
        }
    }
    drain_match_requests(state);
    #[cfg(feature = "vmce")]
    if state
        .match_state
        .session
        .cooperative_round()
        .is_some_and(|round| round.vera())
    {
        if let Err(error) = service_vmce_result_agreement(state) {
            log::error!("VMCE result agreement failed: {error:#}");
            let _ = state.match_state.session.quit_cooperative_round();
            crate::app::cooperative_round::return_to_shell(state);
            return false;
        }
        if let Ok(Some(step)) = state.match_state.session.vmce_result_take_deferred_go() {
            crate::app::cooperative_round::run_step(state, step);
        }
    }
    #[cfg(feature = "vmce")]
    if state
        .match_state
        .session
        .cooperative_round()
        .is_some_and(|round| round.vmce_next_accepted())
    {
        crate::app::cooperative_round::advance_vmce_intermission(state);
        if state.match_state.session.cooperative_round().is_none() {
            return false;
        }
    }
    true
}

/// Keep the current scope and reliable queue until the result-page exit
/// records have been acknowledged or the bounded departure wait fails.
#[cfg(feature = "vmce")]
pub(crate) fn defer_vmce_round_exit(state: &mut AppState) -> bool {
    if state.match_state.session_mode() != SessionMode::Lan
        || !state
            .match_state
            .session
            .cooperative_round()
            .is_some_and(|round| round.vera())
    {
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
    drain_match_requests(state);
    state.platform.window.request_redraw();
    true
}

#[cfg(feature = "vmce")]
fn fail_vmce_round_delivery(state: &mut AppState) -> bool {
    if !state
        .match_state
        .session
        .cooperative_round()
        .is_some_and(|round| round.vera())
    {
        return false;
    }
    log::error!("VMCE round delivery failed; returning to the lobby");
    let _ = state.match_state.session.quit_cooperative_round();
    crate::app::cooperative_round::return_to_shell(state);
    report_vmce_round_failure(state);
    true
}

#[cfg(feature = "vmce")]
fn report_vmce_round_failure(state: &mut AppState) {
    if let Some(wait) = state
        .match_state
        .lan_service
        .as_mut()
        .and_then(|service| service.departure.as_mut())
        .filter(|wait| wait.reason == DepartureReason::CooperativeRound)
    {
        wait.failed = true;
    } else if state.frontend.multiplayer_setup.is_some() {
        crate::app::shell_network_setup::show_blocking_notice(
            state,
            crate::i18n::tr("VERA:VmceRoundSendFailed"),
        );
    } else {
        crate::app::shell_lan::report_error(state, "VERA:VmceRoundSendFailed");
    }
}

#[cfg(feature = "vmce")]
fn vmce_round_exit_pending(state: &AppState) -> bool {
    state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(|service| {
            service
                .departure
                .as_ref()
                .is_some_and(|wait| wait.reason == DepartureReason::CooperativeRound)
        })
}

#[cfg(feature = "vmce")]
fn service_vmce_result_agreement(state: &mut AppState) -> anyhow::Result<()> {
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
            let nonce = fresh_nonce()?;
            let service = state
                .match_state
                .lan_service
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
            anyhow::ensure!(
                service.vmce_used_result_nonces.insert(nonce),
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
        let name = local_name(state);
        let bytes = record.encode(&name)?.as_bytes().to_vec();
        let local = state
            .match_state
            .session
            .network_setup()
            .ok_or_else(|| anyhow::anyhow!("VMCE roster is absent"))?
            .local_player_index;
        let endpoints = state
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
            .collect::<anyhow::Result<Vec<_>>>()?;
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

/// Host only: send a fresh lobby epoch through the current scoped reliable
/// channel. Call while the result round is open, before its Go is queued.
#[cfg(feature = "vmce")]
pub(crate) fn begin_vmce_intermission_transition(state: &mut AppState) -> anyhow::Result<()> {
    anyhow::ensure!(
        state.match_state.session_mode() == SessionMode::Lan && state.match_state.session.host(),
        "VMCE intermission requires a LAN host"
    );
    if state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(|service| service.vmce_intermission_epoch.is_some())
    {
        return Ok(());
    }
    let peers: Vec<_> = state
        .match_state
        .session
        .network_connected_source_indices()
        .iter()
        .copied()
        .filter(|source| {
            Some(*source)
                != state
                    .match_state
                    .session
                    .network_setup()
                    .map(|launch| launch.local_player_index)
        })
        .map(|source| {
            state
                .match_state
                .session
                .network_endpoint(source)
                .ok_or_else(|| anyhow::anyhow!("VMCE participant has no LAN endpoint"))
        })
        .collect::<anyhow::Result<_>>()?;
    let epoch = fresh_nonce()?;
    let solo = peers.is_empty();
    let message = crate::net::lan_messages::cooperative::encode_intermission_epoch(
        &local_name(state),
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
    service.vmce_intermission_epoch = Some(epoch);
    service.vmce_intermission_failed = false;
    service.vmce_intermission_go_queued = solo;
    if !service.queue_setup(
        peers
            .into_iter()
            .map(|endpoint| (message.as_bytes().to_vec(), direct(endpoint))),
    )? {
        service.vmce_intermission_failed = true;
        anyhow::bail!("VMCE intermission queue is full");
    }
    Ok(())
}

/// Host must wait for this to return true before queuing Go; Err is a
/// delivery failure, not a successful drain.
#[cfg(feature = "vmce")]
pub(crate) fn vmce_intermission_transition_ready(state: &AppState) -> anyhow::Result<bool> {
    let service = state
        .match_state
        .lan_service
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
    anyhow::ensure!(
        service.vmce_intermission_epoch.is_some(),
        "VMCE intermission was not prepared"
    );
    let observation = service
        .go_transport_observation(true)
        .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN transport"))?;
    anyhow::ensure!(
        !service.vmce_intermission_failed && observation.service_ok,
        "VMCE intermission delivery failed"
    );
    Ok(observation.queued == 0)
}

/// Enter the next ordinary LAN setup after Go. On the host, false means Go
/// still awaits its reliable ACK and the caller must keep polling the round.
#[cfg(feature = "vmce")]
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
        .vmce_intermission_epoch
        .ok_or_else(|| anyhow::anyhow!("VMCE intermission epoch was not received"))?;
    anyhow::ensure!(
        !service.vmce_intermission_failed,
        "VMCE intermission delivery failed"
    );
    if host {
        if !service.vmce_intermission_go_queued {
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
        // Go confirms the result round advanced. Any not-yet-admitted BC6
        // controls are obsolete and must never be reframed as lobby records;
        // controls already in the old global owner retain their retry state.
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
    service.pending_round.clear();
    service.deferred_setup.clear();
    service.setup_reassembly = Default::default();
    service.maps.cancel();
    for peer in service.peer_admissions.values_mut() {
        peer.lease = None;
        peer.heartbeat_sequence = None;
    }
    service.vmce_intermission_entered = !host;
    if host {
        service.vmce_intermission_epoch = None;
        service.vmce_intermission_go_queued = false;
    }
    service.force_presence = true;
    Ok(true)
}

/// Queue the host's final Go and keep the round alive until its reliable
/// global sends retire. Guests can leave once they receive Go.
#[cfg(feature = "vmce")]
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
        .defer_vmce_campaign_completion();
    drain_match_requests(state);
    state.platform.window.request_redraw();
    true
}

#[cfg(feature = "vmce")]
fn vmce_round_control_admitted(
    host: bool,
    from_host: bool,
    sender: std::net::SocketAddrV4,
    admitted_host: Option<std::net::SocketAddrV4>,
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

/// The loop's dispatch (`0x0047009F`) for a packet from a round participant.
fn receive(state: &mut AppState, incoming: GlobalMessage) -> anyhow::Result<()> {
    anyhow::ensure!(incoming.product_id == 0xaa04, "different LAN product");
    let source = state
        .match_state
        .session
        .network_endpoint_source(incoming.source)
        .filter(|source| {
            state
                .match_state
                .session
                .network_connected_source_indices()
                .contains(source)
        })
        .ok_or_else(|| anyhow::anyhow!("not a round participant"))?;
    #[cfg(feature = "vmce")]
    let vmce = state
        .match_state
        .session
        .cooperative_round()
        .is_some_and(|round| round.vera());
    #[cfg(feature = "vmce")]
    if vmce && incoming.payload.len() == crate::net::lan_messages::LAN_MESSAGE_LEN {
        use crate::net::lan_messages::cooperative::VmceRoundControl;
        let message = LanMessage::decode(&incoming.payload)?;
        let control = VmceRoundControl::decode(&message);
        let options = CooperativeOptionsRecord::decode_vmce(&message);
        if control.is_some() || options.is_some() {
            let (nonce, from_host) = if let Some((_, nonce)) = control {
                (nonce, !matches!(control, Some((VmceRoundControl::Ack, _))))
            } else {
                (options.as_ref().expect("options").1, true)
            };
            let host = state.match_state.session.host();
            let admitted_host = state
                .match_state
                .lan_service
                .as_ref()
                .and_then(|service| service.admitted_host.as_ref())
                .map(|(_, endpoint)| *endpoint);
            if !vmce_round_control_admitted(
                host,
                from_host,
                incoming.source,
                admitted_host,
                source,
                state
                    .match_state
                    .session
                    .network_setup()
                    .map_or(usize::MAX, |launch| launch.local_player_index),
                nonce,
                state.match_state.session.vmce_result_nonce(),
            ) {
                return Ok(());
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
                    CooperativeRoundEvent::Options(round_options(
                        launch,
                        &options.expect("options").0,
                    )?)
                }
            };
            return crate::app::cooperative_round::service_event(state, event);
        }
    }
    let event = if let Some(signal) = CooperativeSignal::decode(&incoming.payload) {
        #[cfg(feature = "vmce")]
        if vmce
            && matches!(
                signal,
                CooperativeSignal::Begin | CooperativeSignal::Ack | CooperativeSignal::Go
            )
        {
            return Ok(());
        }
        match signal {
            CooperativeSignal::Ack => CooperativeRoundEvent::Ack { source },
            CooperativeSignal::Left => CooperativeRoundEvent::PeerLeft { source },
            CooperativeSignal::Waiting => CooperativeRoundEvent::PeerWaiting { source },
            CooperativeSignal::Ready => CooperativeRoundEvent::PeerReady { source },
            CooperativeSignal::Go => CooperativeRoundEvent::Go,
            CooperativeSignal::Begin => CooperativeRoundEvent::Begin,
            #[cfg(feature = "vmce")]
            CooperativeSignal::StageRejected => CooperativeRoundEvent::StageRejected,
            #[cfg(feature = "vmce")]
            CooperativeSignal::StageAbsent => CooperativeRoundEvent::StageAbsent,
            #[cfg(feature = "vmce")]
            CooperativeSignal::RetryVote => CooperativeRoundEvent::RetryVote { source },
        }
    } else {
        let message = LanMessage::decode(&incoming.payload)?;
        #[cfg(feature = "vmce")]
        if message.kind()
            == LanMessageKind::Unknown(crate::net::lan_messages::cooperative::RESULT_KIND)
        {
            // A result record from an older build leaves its digest range
            // zeroed. Refuse it here instead of letting it fall through as a
            // keepalive that silently leaves the page waiting.
            let Some(record) =
                crate::net::lan_messages::cooperative::VmceResultRecord::decode(&message)
            else {
                log::warn!("ignoring a stale or malformed VMCE result record");
                return Ok(());
            };
            let first = state.match_state.session.vmce_result_nonce().is_none()
                && !state.match_state.session.host();
            if first {
                let service = state
                    .match_state
                    .lan_service
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("VMCE has no LAN service"))?;
                anyhow::ensure!(
                    !service.vmce_used_result_nonces.contains(&record.nonce),
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
                return Ok(());
            }
            if state.match_state.session.vmce_result_nonce() == Some(record.nonce) {
                state
                    .match_state
                    .lan_service
                    .as_mut()
                    .expect("result service")
                    .vmce_used_result_nonces
                    .insert(record.nonce);
            }
            return Ok(());
        }
        #[cfg(feature = "vmce")]
        if let Some(epoch) =
            crate::net::lan_messages::cooperative::decode_intermission_epoch(&message)
        {
            let host_endpoint = state
                .match_state
                .lan_service
                .as_ref()
                .and_then(|service| service.admitted_host.as_ref())
                .map(|(_, endpoint)| *endpoint);
            anyhow::ensure!(
                !state.match_state.session.host() && host_endpoint == Some(incoming.source),
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
            if let Some(previous) = service.vmce_intermission_epoch {
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
                service.vmce_intermission_epoch = Some(epoch);
            }
            return Ok(());
        }
        #[cfg(feature = "vmce")]
        let stage_event = VmceStageRecord::decode(&message).map(|record| {
            CooperativeRoundEvent::StageIdentity(crate::app::match_bootstrap::VmceStageIdentity {
                map_file: record.map_file,
                source_sha256: record.source_sha256,
                effective_digest: record.effective_digest,
                config_sha256: record.config_sha256,
            })
        });
        #[cfg(not(feature = "vmce"))]
        let stage_event: Option<CooperativeRoundEvent> = None;
        if let Some(event) = stage_event {
            event
        } else if let Some(record) = CooperativeOptionsRecord::decode(&message) {
            #[cfg(feature = "vmce")]
            if vmce {
                return Ok(());
            }
            let launch = state
                .match_state
                .session
                .network_setup()
                .ok_or_else(|| anyhow::anyhow!("no round roster"))?;
            CooperativeRoundEvent::Options(round_options(launch, &record)?)
        } else if let Some(chat) = message.in_game_chat_fields() {
            let game = super::active_game_name(state).unwrap_or_default();
            let Some(event) = chat_event(source, &chat, &game) else {
                return Ok(());
            };
            event
        } else {
            // Every other kind, the 20 s `0x0D` among them, falls through
            // to the loop's tail (`0x004702FD`).
            CooperativeRoundEvent::KeepAlive
        }
    };
    crate::app::cooperative_round::service_event(state, event)
}

/// Whether `payload` is one of the co-op round's records: a native BC6
/// signal (`0x23..0x26`, `0x31`, `0x34`), the options `0x32`, or the
/// VERA-only stage records (`0x36..0x38`).
pub(super) fn is_round_record(payload: &[u8]) -> bool {
    if payload.len() == crate::net::lan_messages::cooperative::SIGNAL_LEN {
        return CooperativeSignal::decode(payload).is_some();
    }
    if payload.len() == crate::net::lan_messages::LAN_MESSAGE_LEN {
        let Ok(message) = LanMessage::decode(payload) else {
            return false;
        };
        return message.kind() == LanMessageKind::Unknown(OPTIONS_KIND) || {
            #[cfg(feature = "vmce")]
            {
                VmceStageRecord::decode(&message).is_some()
                    || crate::net::lan_messages::cooperative::VmceRoundControl::decode(&message)
                        .is_some()
                    || CooperativeOptionsRecord::decode_vmce(&message).is_some()
                    || crate::net::lan_messages::cooperative::decode_intermission_epoch(&message)
                        .is_some()
                    || crate::net::lan_messages::cooperative::VmceResultRecord::decode(&message)
                        .is_some()
            }
            #[cfg(not(feature = "vmce"))]
            {
                false
            }
        };
    }
    false
}

/// `0x0C` (`0x004706E3`): only this game's hash posts a line, in the
/// packet's colour `+0x110`.
fn chat_event(
    source: usize,
    chat: &crate::net::lan_messages::ingame::InGameChat<'_>,
    game: &str,
) -> Option<CooperativeRoundEvent> {
    (chat.game_name_hash == crate::net::lan_messages::game_name_hash(game)).then(|| {
        CooperativeRoundEvent::Chat {
            source,
            text: chat.text(),
            color: chat.color,
        }
    })
}

/// A queued round record to every other node.
pub(super) fn send(state: &mut AppState, request: NetworkLoadingRequest) {
    if state.match_state.session_mode() != SessionMode::Lan {
        return;
    }
    let name = local_name(state);
    let peers: Vec<_> =
        super::super::match_runtime::network_match::nodes(&state.match_state.session)
            .into_iter()
            .skip(1)
            .collect();
    #[cfg(feature = "vmce")]
    if state
        .match_state
        .session
        .cooperative_round()
        .is_some_and(|round| round.vera())
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
        let endpoints: Option<Vec<_>> = peers
            .iter()
            .map(|source| state.match_state.session.network_endpoint(*source))
            .collect();
        let queued = match (
            signal.encode(&name),
            endpoints,
            state.match_state.lan_service.as_mut(),
        ) {
            (Ok(bytes), Some(endpoints), Some(service)) => service
                .queue_setup(
                    endpoints
                        .into_iter()
                        .map(|endpoint| (bytes.to_vec(), direct(endpoint))),
                )
                .unwrap_or(false),
            _ => false,
        };
        if !queued {
            if let Some(wait) = state
                .match_state
                .lan_service
                .as_mut()
                .and_then(|service| service.departure.as_mut())
            {
                wait.failed = true;
            }
            log::error!("VMCE exit record could not be queued");
        }
        return;
    }
    #[cfg(feature = "vmce")]
    let vmce_wire = if let NetworkLoadingRequest::CooperativeVmceGo { nonce } = &request {
        Some(
            crate::net::lan_messages::cooperative::VmceRoundControl::Go
                .encode(&name, *nonce)
                .ok(),
        )
    } else if state
        .match_state
        .session
        .cooperative_round()
        .is_some_and(|round| round.vera())
    {
        use crate::net::lan_messages::cooperative::VmceRoundControl;
        let nonce = state.match_state.session.vmce_result_nonce();
        match &request {
            NetworkLoadingRequest::CooperativeBegin => {
                Some(nonce.and_then(|nonce| VmceRoundControl::Begin.encode(&name, nonce).ok()))
            }
            NetworkLoadingRequest::CooperativeAck => {
                Some(nonce.and_then(|nonce| VmceRoundControl::Ack.encode(&name, nonce).ok()))
            }
            NetworkLoadingRequest::CooperativeGo => {
                Some(nonce.and_then(|nonce| VmceRoundControl::Go.encode(&name, nonce).ok()))
            }
            NetworkLoadingRequest::CooperativeOptions(options) => {
                Some(nonce.and_then(|nonce| options_record(options).encode_vmce(&name, nonce).ok()))
            }
            _ => None,
        }
    } else {
        None
    };
    #[cfg(feature = "vmce")]
    if matches!(request, NetworkLoadingRequest::CooperativeVmceGo { .. })
        && state
            .match_state
            .lan_service
            .as_ref()
            .is_some_and(|service| {
                service.vmce_intermission_epoch.is_some() && !service.vmce_intermission_entered
            })
    {
        let encoded = vmce_wire.as_ref().and_then(|message| message.as_ref());
        let endpoints: Option<Vec<_>> = peers
            .iter()
            .map(|source| state.match_state.session.network_endpoint(*source))
            .collect();
        let service = state
            .match_state
            .lan_service
            .as_mut()
            .expect("intermission service");
        let queued = match (encoded, endpoints) {
            (Some(bytes), Some(endpoints)) => service
                .queue_setup(
                    endpoints
                        .into_iter()
                        .map(|endpoint| (bytes.as_bytes().to_vec(), direct(endpoint))),
                )
                .unwrap_or(false),
            _ => false,
        };
        service.vmce_intermission_go_queued = queued;
        service.vmce_intermission_failed |= !queued;
        return;
    }
    #[cfg(feature = "vmce")]
    if matches!(request, NetworkLoadingRequest::CooperativeVmceGo { .. })
        && state
            .match_state
            .lan_service
            .as_ref()
            .is_some_and(|service| service.vmce_campaign_completion_pending)
    {
        let encoded = vmce_wire.as_ref().and_then(|message| message.as_ref());
        let endpoints: Option<Vec<_>> = peers
            .iter()
            .map(|source| state.match_state.session.network_endpoint(*source))
            .collect();
        let service = state
            .match_state
            .lan_service
            .as_mut()
            .expect("pending service");
        let queued = match (encoded, endpoints) {
            (Some(bytes), Some(endpoints)) => service
                .queue_setup(
                    endpoints
                        .into_iter()
                        .map(|endpoint| (bytes.as_bytes().to_vec(), direct(endpoint))),
                )
                .unwrap_or(false),
            _ => false,
        };
        service.vmce_campaign_completion_enqueue_failed = !queued;
        return;
    }
    #[cfg(feature = "vmce")]
    if let Some(message) = vmce_wire {
        let endpoints: Option<Vec<_>> = peers
            .iter()
            .map(|source| state.match_state.session.network_endpoint(*source))
            .collect();
        let queued = match (message, endpoints, state.match_state.lan_service.as_mut()) {
            (Some(message), Some(endpoints), Some(service)) => service
                .queue_setup(
                    endpoints
                        .into_iter()
                        .map(|endpoint| (message.as_bytes().to_vec(), direct(endpoint))),
                )
                .unwrap_or(false),
            _ => false,
        };
        if !queued {
            log::error!("VMCE round control could not be queued; returning to the lobby");
            let _ = state.match_state.session.quit_cooperative_round();
            crate::app::cooperative_round::return_to_shell(state);
            report_vmce_round_failure(state);
        }
        return;
    }
    let signal = match request {
        NetworkLoadingRequest::CooperativeReady => CooperativeSignal::Ready,
        NetworkLoadingRequest::CooperativeLeave => CooperativeSignal::Left,
        #[cfg(feature = "vmce")]
        NetworkLoadingRequest::CooperativeRetryVote => CooperativeSignal::RetryVote,
        NetworkLoadingRequest::CooperativeBegin => CooperativeSignal::Begin,
        NetworkLoadingRequest::CooperativeAck => CooperativeSignal::Ack,
        NetworkLoadingRequest::CooperativeGo => CooperativeSignal::Go,
        NetworkLoadingRequest::CooperativeOptions(options) => {
            if let Ok(message) = options_record(&options).encode(&name) {
                send_sources(state, message.as_bytes(), peers);
            }
            return;
        }
        #[cfg(feature = "vmce")]
        NetworkLoadingRequest::CooperativeStage(stage) => {
            let record = VmceStageRecord {
                map_file: stage.map_file,
                source_sha256: stage.source_sha256,
                effective_digest: stage.effective_digest,
                config_sha256: stage.config_sha256,
            };
            match record.encode(&name) {
                Ok(message) => send_sources(state, message.as_bytes(), peers),
                Err(error) => {
                    // A record that cannot be sent must not silently skip the
                    // next-stage agreement: reject the round instead of letting
                    // the peers load different maps.
                    log::error!(
                        "VMCE stage identity could not be encoded ({error}); rejecting the round"
                    );
                    if let Ok(bytes) = CooperativeSignal::StageRejected.encode(&name) {
                        send_sources(state, &bytes, peers);
                    }
                }
            }
            return;
        }
        #[cfg(feature = "vmce")]
        NetworkLoadingRequest::CooperativeStageRejected => CooperativeSignal::StageRejected,
        #[cfg(feature = "vmce")]
        NetworkLoadingRequest::CooperativeStageAbsent => CooperativeSignal::StageAbsent,
        _ => return,
    };
    if let Ok(bytes) = signal.encode(&name) {
        send_sources(state, &bytes, peers);
    }
}

/// Retry-load failure from one peer: one record to every other node. Called
/// by the loading failure owner before the transport is retired.
#[cfg(feature = "vmce")]
pub(super) fn send_retry_load_failure(
    state: &mut AppState,
    reason: RetryLoadFailureReason,
) -> bool {
    if state.match_state.session_mode() != SessionMode::Lan {
        return false;
    }
    let name = local_name(state);
    let peers: Vec<_> =
        super::super::match_runtime::network_match::nodes(&state.match_state.session)
            .into_iter()
            .skip(1)
            .collect();
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
    let Ok(message) = encode_retry_load_failure(&name, reason, scope, attempt) else {
        return false;
    };
    let endpoints: Option<Vec<_>> = peers
        .iter()
        .map(|source| state.match_state.session.network_endpoint(*source))
        .collect();
    match (endpoints, state.match_state.lan_service.as_mut()) {
        (Some(endpoints), Some(service)) => service
            .queue_setup(
                endpoints
                    .into_iter()
                    .map(|endpoint| (message.as_bytes().to_vec(), direct(endpoint))),
            )
            .unwrap_or(false),
        _ => false,
    }
}

/// `0x0046D4A0`: each raw slot's human words, `-1` where no human is bound,
/// and the AI rows. The round keeps no inactive AI words; those rows go out
/// as a fresh setup's (country `-1`, which every reader skips).
fn options_record(options: &CooperativeRoundOptions) -> CooperativeOptionsRecord {
    let humans = std::array::from_fn(|row| {
        options
            .humans
            .iter()
            .find(|(slot, _)| usize::from(*slot) == row)
            .map_or(
                CooperativeHumanWords {
                    start: -1,
                    country: -1,
                    color: -1,
                },
                |(_, choices)| CooperativeHumanWords {
                    start: options::start_value(choices.start_position),
                    country: if choices.country_random {
                        -2
                    } else {
                        i32::from(choices.country.raw_index())
                    },
                    color: if choices.color_random {
                        -2
                    } else {
                        i32::from(choices.color_index)
                    },
                },
            )
    });
    let ai = std::array::from_fn(|row| {
        let words = options
            .opponents
            .iter()
            .find(|ai| usize::from(ai.raw_slot_index) == row)
            .map_or(NetworkAiOptions::INITIAL_INACTIVE, NetworkAiOptions::of);
        CooperativeAiWords {
            country: words.country,
            color: words.color,
            difficulty: words.difficulty,
            start: words.start,
            team: words.team,
        }
    });
    CooperativeOptionsRecord {
        difficulty: options.difficulty as u8,
        humans,
        ai,
    }
}

/// The receiver's view of a `0x32` (`0x0046D650` applies it): every bound
/// human row's words, and every AI row whose country is set. The record
/// carries no AI count, so the receiver's own stays.
fn round_options(
    launch: &SkirmishLaunchSession,
    record: &CooperativeOptionsRecord,
) -> anyhow::Result<CooperativeRoundOptions> {
    let mut humans = Vec::new();
    for human in &launch.humans {
        let Some(row) = human.raw_slot_index else {
            continue;
        };
        let words = record.humans[usize::from(row)];
        if words.country == -1 {
            continue;
        }
        let choices = options::decoded_choices(
            &human.slot,
            words.country,
            words.color,
            words.start,
            options::team_value(human.slot.team),
        )?;
        humans.push((row, choices));
    }
    let mut opponents = Vec::new();
    for (row, words) in record.ai.iter().enumerate() {
        if words.country == -1 {
            continue;
        }
        // Every field of this placeholder is overwritten: an AI row's
        // country is never the observer's -3.
        let placeholder = SkirmishLocalSlot {
            country: LaunchCountry::America,
            country_random: true,
            color_index: 0,
            color_random: true,
            start_position: LaunchStartPosition::Auto,
            team: LaunchTeam::None,
        };
        let choices = options::decoded_choices(
            &placeholder,
            words.country,
            words.color,
            words.start,
            words.team,
        )?;
        opponents.push(SkirmishAiSlot {
            raw_slot_index: row as u8,
            country: choices.country,
            country_random: choices.country_random,
            color_index: choices.color_index,
            color_random: choices.color_random,
            start_position: choices.start_position,
            team: choices.team,
            difficulty: options::decoded_difficulty(words.difficulty)?,
        });
    }
    Ok(CooperativeRoundOptions {
        difficulty: i32::from(record.difficulty),
        humans,
        ai_player_count: launch.ai_player_count,
        opponents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish_launch::AiDifficulty;

    #[cfg(feature = "vmce")]
    #[test]
    fn vmce_control_admission_checks_the_host_and_round() {
        let master = "127.0.0.1:1000".parse().unwrap();
        let guest = "127.0.0.1:1001".parse().unwrap();
        let nonce = [5; 16];
        assert!(vmce_round_control_admitted(
            false,
            true,
            master,
            Some(master),
            0,
            1,
            nonce,
            Some(nonce)
        ));
        assert!(!vmce_round_control_admitted(
            false,
            true,
            guest,
            Some(master),
            2,
            1,
            nonce,
            Some(nonce)
        ));
        assert!(!vmce_round_control_admitted(
            false,
            true,
            master,
            Some(master),
            0,
            1,
            [6; 16],
            Some(nonce)
        ));
        assert!(!vmce_round_control_admitted(
            true,
            true,
            guest,
            None,
            1,
            0,
            nonce,
            Some(nonce)
        ));
        assert!(vmce_round_control_admitted(
            true,
            false,
            guest,
            None,
            1,
            0,
            nonce,
            Some(nonce)
        ));
        assert!(!vmce_round_control_admitted(
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

    fn round_roster() -> SkirmishLaunchSession {
        let profile: serde_json::Value = serde_json::from_str(crate::test_fixture::text(
            "tools/map_observation.command-bar.example.json",
        ))
        .unwrap();
        let mut launch: SkirmishLaunchSession =
            serde_json::from_value(profile["launch"].clone()).unwrap();
        let mut guest = launch.local_player().clone();
        guest.name = "Guest".into();
        guest.raw_slot_index = Some(1);
        launch.humans.push(guest);
        launch
    }

    /// BC6's partner line keeps the sender's `0xF0F0`; another game's
    /// `0x0C` posts nothing.
    #[test]
    fn chat_posts_this_games_lines_in_the_packet_colour() {
        let message = LanMessage::in_game_chat("Host", 0xF0F0, "Host's game", "gg").unwrap();
        let chat = message.in_game_chat_fields().unwrap();
        assert_eq!(
            chat_event(1, &chat, "Host's game"),
            Some(CooperativeRoundEvent::Chat {
                source: 1,
                text: "gg".into(),
                color: 0xF0F0,
            })
        );
        assert_eq!(chat_event(1, &chat, "Other game"), None);
    }

    /// The master's `0x32` reaches the guest as the same round options: the
    /// human rows by raw slot and the AI rows with their difficulty.
    #[test]
    fn options_cross_the_wire_unchanged() {
        let mut launch = round_roster();
        // 0x32 has no AI count: the receiver keeps its own.
        launch.ai_player_count = 1;
        let mut guest_choices = launch.humans[1].slot.clone();
        guest_choices.country_random = true;
        guest_choices.color_index = 2;
        guest_choices.color_random = false;
        guest_choices.start_position = LaunchStartPosition::Position(1);
        let options = CooperativeRoundOptions {
            difficulty: 2,
            humans: vec![(0, launch.humans[0].slot.clone()), (1, guest_choices)],
            ai_player_count: 1,
            opponents: vec![SkirmishAiSlot {
                raw_slot_index: 3,
                country: LaunchCountry::from_country_index(2),
                country_random: false,
                color_index: 5,
                color_random: false,
                start_position: LaunchStartPosition::Position(4),
                team: LaunchTeam::Team(1),
                difficulty: AiDifficulty::Hard,
            }],
        };
        let message = options_record(&options).encode("Host").unwrap();
        let record = CooperativeOptionsRecord::decode(&message).unwrap();
        let mut received = round_options(&launch, &record).unwrap();
        // A random country's placeholder is not on the wire.
        received.humans[1].1.country = options.humans[1].1.country;
        assert_eq!(received, options);
        #[cfg(feature = "vmce")]
        {
            let encoded = options_record(&options)
                .encode_vmce("Host", [7; 16])
                .unwrap();
            let (record, nonce) = CooperativeOptionsRecord::decode_vmce(&encoded).unwrap();
            assert_eq!(nonce, [7; 16]);
            let mut received = round_options(&launch, &record).unwrap();
            received.humans[1].1.country = options.humans[1].1.country;
            assert_eq!(received, options);
            assert!(CooperativeOptionsRecord::decode(&encoded).is_none());
        }
    }

    /// The pre-round hold must recognize exactly the round's records, so a
    /// peer's ready/options/stage packet is queued but neither setup traffic
    /// nor chat is.
    #[test]
    fn the_pending_round_hold_recognizes_the_round_records() {
        assert!(is_round_record(
            &CooperativeSignal::Ready.encode("Guest").unwrap()
        ));
        #[cfg(feature = "vmce")]
        assert!(is_round_record(
            &CooperativeSignal::StageAbsent.encode("Guest").unwrap()
        ));
        let options = CooperativeOptionsRecord {
            difficulty: 1,
            humans: [CooperativeHumanWords {
                start: -1,
                country: -1,
                color: -1,
            }; 8],
            ai: [CooperativeAiWords {
                country: -1,
                color: -1,
                difficulty: 0,
                start: -2,
                team: -2,
            }; 8],
        };
        assert!(is_round_record(
            &options.encode("Host").unwrap().into_bytes()
        ));
        #[cfg(feature = "vmce")]
        {
            use crate::net::lan_messages::cooperative::VmceRoundControl;
            assert!(is_round_record(
                VmceRoundControl::Begin
                    .encode("Host", [7; 16])
                    .unwrap()
                    .as_bytes()
            ));
            assert!(is_round_record(
                options.encode_vmce("Host", [7; 16]).unwrap().as_bytes()
            ));
            let stage = VmceStageRecord {
                map_file: "mission02.map".into(),
                source_sha256: "a".repeat(64),
                effective_digest: "b".repeat(64),
                config_sha256: "c".repeat(64),
            };
            assert!(
                is_round_record(&stage.encode("Host").unwrap().into_bytes()),
                "the VMCE stage record is a round record"
            );
        }
        // A chat record is not part of the round's establishment.
        assert!(!is_round_record(&[0x0C, 0, 0, 0]));
    }
}
