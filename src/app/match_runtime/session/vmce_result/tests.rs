//! VMCE result-round regression tests: agreement under loss, retry votes and
//! the terminal step each ending produces.

use super::*;
use crate::app::match_bootstrap::LoadingStartup;
use crate::app::match_runtime::session::NetworkReturnContext;
use std::sync::Arc;

const SEED: u32 = 0x1234_abcd;

/// A frozen two-player VMCE LAN start; `local` is the local source index.
fn frozen_vera(
    local: usize,
) -> (
    MatchSession,
    Arc<crate::app::match_bootstrap::PreparedNetworkStartup>,
) {
    let mut launch = super::super::tests::network_fixture();
    launch.mode.class = crate::skirmish_modes::MpModeClass::Vmce;
    launch.humans.truncate(2);
    launch.rebuild_house_roster();
    for (index, human) in launch.humans.iter_mut().enumerate() {
        human.role = crate::skirmish_launch::LaunchHumanRole::Player {
            group: index as i32,
        };
    }
    launch.local_player_index = local;
    let mut session = MatchSession::default();
    session.enter_network(NetworkReturnContext::LanLobby);
    session
        .accept_network_setup(launch, local == 0, Some(7), false)
        .unwrap();
    let map_file = session
        .network_setup()
        .unwrap()
        .selected_map_file
        .clone()
        .unwrap();
    let source_sha256 = "a".repeat(64);
    session.network.as_mut().unwrap().map_metadata = Some(super::super::NetworkMapMetadata {
        display_name: map_file.clone(),
        player_capacity: 8,
        official: false,
        byte_length: 100,
        digest: String::new(),
        source_sha256: source_sha256.clone(),
        effective_digest: "b".repeat(64),
    });
    session
        .accept_network_vmce_map_config(Some(
            crate::app::match_bootstrap::NetworkVmceMapConfig::new(
                map_file,
                source_sha256,
                crate::map::vmce_config::VmceMapConfig::default(),
            )
            .unwrap(),
        ))
        .unwrap();
    session.network.as_mut().unwrap().local_map_source =
        Some(crate::map::source::LoadedMapSource::Loose {
            path: std::path::PathBuf::from(
                session
                    .network_setup()
                    .unwrap()
                    .selected_map_file
                    .clone()
                    .unwrap(),
            ),
            payload_len: 100,
            source_sha256: "a".repeat(64),
            cache_lease: Default::default(),
        });
    session.network.as_mut().unwrap().local_map_effective_digest = Some("b".repeat(64));
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
    let LoadingStartup::Network(startup) = session
        .freeze_network_start_with_vmce(SEED, Some(assignments))
        .unwrap()
    else {
        panic!("network start");
    };
    (session, startup)
}

fn record(
    session: &MatchSession,
    nonce: [u8; 16],
    entries: [u8; 4],
    confirmed: bool,
) -> crate::net::lan_messages::vmce::VmceResultRecord {
    let NetworkPhase::VmceResult { round, .. } =
        &session.network.as_ref().expect("network visit").phase
    else {
        panic!("VMCE result round");
    };
    crate::net::lan_messages::vmce::VmceResultRecord {
        nonce,
        entries,
        confirmed,
        stage_digest: round
            .vmce
            .as_ref()
            .expect("VMCE result state")
            .result_stage_digest(nonce),
    }
}

fn agree(host: &mut MatchSession, guest: &mut MatchSession) {
    let entries = [4, 5, 0xff, 0xff];
    host.initialize_vmce_result_agreement(entries, Some([9; 16]))
        .unwrap();
    guest
        .initialize_vmce_result_agreement(entries, None)
        .unwrap();
    // A lost first proposal is redelivered; the guest's proposal then reaches
    // the host, which confirms and waits for the guest's confirmation echo.
    let host_proposal = record(host, [9; 16], entries, false);
    guest
        .vmce_result_received(0, host_proposal.clone())
        .unwrap();
    guest
        .vmce_result_received(0, host_proposal)
        .expect("a duplicate proposal is ignored");
    guest.vmce_result_record_queued(false).unwrap();
    let guest_proposal = record(guest, [9; 16], entries, false);
    host.vmce_result_received(1, guest_proposal).unwrap();
    host.vmce_result_record_queued(false).unwrap();
    let host_confirm = record(host, [9; 16], entries, true);
    guest.vmce_result_received(0, host_confirm.clone()).unwrap();
    guest.vmce_result_record_queued(true).unwrap();
    let guest_confirm = record(guest, [9; 16], entries, true);
    host.vmce_result_received(1, guest_confirm).unwrap();
    host.vmce_result_record_queued(true).unwrap();
}

#[test]
fn vmce_result_agreement_tolerates_a_lost_proposal_and_duplicate_delivery() {
    let (mut host, _) = frozen_vera(0);
    let (mut guest, _) = frozen_vera(1);
    host.enter_vmce_round(None, true, 2).unwrap();
    guest.enter_vmce_round(None, true, 2).unwrap();
    agree(&mut host, &mut guest);
    assert!(result_agreed(
        host.network_connected_source_indices(),
        host.network_setup().unwrap(),
        host.vmce_result_phase().unwrap(),
    ));
    assert!(result_agreed(
        guest.network_connected_source_indices(),
        guest.network_setup().unwrap(),
        guest.vmce_result_phase().unwrap(),
    ));
}

#[test]
fn vmce_retry_ack_loss_keeps_the_host_waiting_until_the_ack_arrives() {
    let (mut host, _) = frozen_vera(0);
    let (mut guest, _) = frozen_vera(1);
    host.enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, 2)
        .unwrap();
    guest
        .enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, 2)
        .unwrap();
    agree(&mut host, &mut guest);
    host.vmce_result_assigned().unwrap();
    assert!(
        !host.vmce_result_retry_acks_complete(),
        "the host must not reload before the guest consumed the retry Go"
    );
    let step = guest.vmce_result_event(VmceResultEvent::Go).unwrap();
    assert_eq!(
        step,
        VmceRoundStep::None,
        "the guest must not reload before the host commits"
    );
    assert!(guest.vmce_result_retry_go_pending());
    // The Go reached the guest; its ACK was lost in transit, so the host still
    // waits. A duplicate ACK is idempotent.
    host.vmce_result_event(VmceResultEvent::RetryAck { source: 1 })
        .unwrap();
    assert!(host.vmce_result_retry_acks_complete());
    host.vmce_result_event(VmceResultEvent::RetryAck { source: 1 })
        .unwrap();
    assert!(host.vmce_result_retry_acks_complete());
    assert_eq!(
        guest
            .vmce_result_event(VmceResultEvent::RetryCommit)
            .unwrap(),
        VmceRoundStep::Retry
    );
    assert_eq!(
        guest
            .vmce_result_event(VmceResultEvent::RetryCommit)
            .unwrap(),
        VmceRoundStep::None,
        "a repeated commit cannot reload twice"
    );
}

#[test]
fn vmce_go_produces_the_step_named_by_the_terminal_ending() {
    for (ending, expected) in [
        (VmceRoundEnding::Next, VmceRoundStep::Next),
        (VmceRoundEnding::Retry, VmceRoundStep::Retry),
        (VmceRoundEnding::CampaignEnd, VmceRoundStep::CampaignEnd),
    ] {
        let (mut guest, _) = frozen_vera(1);
        guest
            .enter_vmce_round_ending(None, true, ending, 2)
            .unwrap();
        let step = guest.vmce_result_event(VmceResultEvent::Go).unwrap();
        if ending == VmceRoundEnding::Retry {
            assert_eq!(step, VmceRoundStep::None);
            assert!(guest.vmce_result_retry_go_pending());
            assert_eq!(
                guest
                    .vmce_result_event(VmceResultEvent::RetryCommit)
                    .unwrap(),
                expected
            );
        } else {
            assert_eq!(step, expected);
        }
    }
}

#[test]
fn vmce_no_response_leaves_the_retry_ack_gate_closed() {
    let (mut host, _) = frozen_vera(0);
    let (mut guest, _) = frozen_vera(1);
    host.enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, 2)
        .unwrap();
    guest
        .enter_vmce_round_ending(None, true, VmceRoundEnding::Retry, 2)
        .unwrap();
    agree(&mut host, &mut guest);
    host.vmce_result_assigned().unwrap();
    // No guest Go/ACK at all: the host stays on the retry page.
    assert!(!host.vmce_result_retry_acks_complete());
    assert!(
        host.vmce_result_phase().is_some(),
        "the host waits on the retry page"
    );
}
