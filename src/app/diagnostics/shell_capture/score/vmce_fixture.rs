//! VMCE-only synthetic settlement-page fixtures for the score capture routes.
//!
//! The stock campaign score capture and its Cooperative result routes stay in
//! `score.rs`; these two installers are VMCE's own page fixtures. They build
//! the production VMCE result pages through the same owners the real flow uses
//! (`App::open_campaign_score_page`, `install_victory_content`,
//! `install_defeat_page`); the shared capture only dispatches to them.

use super::*;

/// Install one synthetic VMCE settlement page state through the production
/// session/UI owners: a VMCE LAN room supplies the real role/country data, the
/// page model stays the retail `0x103` example, and the board carries only the
/// ready/verdict/continuation values this visual fixture reviews. No real
/// match, no transport, no protocol.
pub(super) fn install_vmce_result_fixture(
    state: &mut AppState,
    target: VmceResultTarget,
) -> Result<()> {
    use crate::app::vmce_result::types::{
        VmceResultBoard, VmceResultRow, VmceStatistics, VmceStatisticsRow,
    };
    use crate::ui::campaign_score_shell::CampaignScoreScreenModel;
    let local_host = !matches!(
        target,
        VmceResultTarget::GuestUnready | VmceResultTarget::GuestReady
    );
    let stats_open = matches!(
        target,
        VmceResultTarget::HostVictoryStats | VmceResultTarget::CampaignEndStats
    );
    crate::app::shell_network_setup::install_vmce_lobby_fixture(
        state,
        &crate::app::shell_network_setup::VmceLobbyFixture {
            host: local_host,
            claimed_roles: 4,
            peer_ready: true,
            status: None,
            blocking: false,
        },
    )?;
    let roles = crate::app::shell_network_setup::view::vmce_result_roles(state)
        .ok_or_else(|| anyhow::anyhow!("VMCE result fixture needs authored role metadata"))?;
    let ready = match target {
        VmceResultTarget::HostVictoryNext
        | VmceResultTarget::CampaignEnd
        | VmceResultTarget::HostVictoryStats
        | VmceResultTarget::CampaignEndStats => [Some(true); 4],
        VmceResultTarget::GuestUnready => [Some(false); 4],
        VmceResultTarget::GuestReady => [Some(false), Some(true), Some(false), Some(false)],
    };
    let local_ready = matches!(target, VmceResultTarget::GuestReady);
    let board = VmceResultBoard {
        rows: (0..4)
            .map(|role| VmceResultRow {
                player: if role == 0 {
                    "LiveHost".into()
                } else {
                    format!("LiveGuest{role}")
                },
                role: roles[role].owner.clone(),
                country: roles[role].country.clone(),
                flag: roles[role].flag.clone(),
                won: target.victory(),
                ready: ready[role],
                host: role == 0,
                rgb: roles[role].color,
            })
            .collect(),
        victory: target.victory(),
        local_host,
        local_ready,
        can_continue: ready.iter().all(|ready| *ready == Some(true)),
    };
    let mut model = synthetic_campaign_score_model();
    model.par_time = "00:12:00".into();
    model.your_time = "00:09:47".into();
    model.enemies_destroyed = "23".into();
    model.rank = "Colonel".into();
    let _ = target.victory();
    let total_time = model.your_time.clone();
    let par_time = model.par_time.clone();
    App::open_campaign_score_page(
        state,
        CampaignScoreScreenModel {
            your_time: model.your_time.clone(),
            ..model
        },
        "Synthetic VMCE settlement capture".into(),
        "Synthetic UI fixture; not native-parity evidence.".into(),
    );
    let _ = App::ensure_skirmish_shell_chrome(state);
    const KILLS: [u32; 4] = [12, 7, 4, 2];
    const LOSSES: [u32; 4] = [3, 5, 2, 1];
    const BUILT: [u32; 4] = [40, 31, 18, 9];
    const SCORE: [i32; 4] = [900, 640, 410, 220];
    const INCOME: [i64; 4] = [5_000, 3_200, 2_100, 1_200];
    let statistics = VmceStatistics {
        rows: (0..4)
            .map(|role| VmceStatisticsRow {
                player: if role == 0 {
                    "LiveHost".into()
                } else {
                    format!("LiveGuest{role}")
                },
                owner: roles[role].owner.clone(),
                rgb: roles[role].color,
                kills: KILLS[role],
                losses: LOSSES[role],
                built: BUILT[role],
                score: SCORE[role],
                income: INCOME[role],
            })
            .collect(),
        total_time,
        par_time,
    };
    ensure!(
        state.frontend.campaign_score_page.is_some(),
        "VMCE result fixture did not open the 0x103 page"
    );
    // Install the VMCE content over the stock page through the same owner the
    // real victory/campaign-end flow uses; the fixture never pokes page fields.
    crate::app::vmce_result::install_victory_content(
        state,
        Some(board),
        Some(statistics),
        stats_open,
    );
    Ok(())
}

/// Install one synthetic VMCE defeat page on the production BC6 shell: a VMCE
/// LAN room supplies the role rows, the round supplies the retry-vote state,
/// and the model carries display-only score numbers. No match, no transport.
pub(super) fn install_vmce_defeat_fixture(
    state: &mut AppState,
    target: VmceDefeatTarget,
) -> Result<()> {
    use crate::app::vmce_result::types::{VmceStatistics, VmceStatisticsRow};
    crate::app::shell_network_setup::install_vmce_lobby_fixture(
        state,
        &crate::app::shell_network_setup::VmceLobbyFixture {
            host: target.host(),
            claimed_roles: target.rows(),
            peer_ready: true,
            status: None,
            blocking: false,
        },
    )?;
    let roles = crate::app::shell_network_setup::view::vmce_result_roles(state)
        .ok_or_else(|| anyhow::anyhow!("VMCE defeat fixture needs authored role metadata"))?;
    // A BC6 page follows a frozen start: freeze the synthetic lobby launch the
    // same way the real VMCE start does, then enter the retry round.
    let launch = state
        .match_state
        .session
        .network_setup()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("VMCE defeat fixture has no lobby launch"))?;
    let mutations = |human: &crate::skirmish_launch::SkirmishHumanSlot| human.name.clone();
    let mut used_roles = [false; 4];
    for human in &launch.humans {
        let role = human.role.native_value();
        if role >= 0 && role < 4 {
            used_roles[role as usize] = true;
        }
    }
    let assignments = crate::skirmish_launch::VmceHouseAssignments {
        participants: launch
            .humans
            .iter()
            .enumerate()
            .map(|(index, human)| {
                let authored = human.role.native_value();
                let player_index = if (0..4).contains(&authored) {
                    authored as u8
                } else {
                    let free = used_roles
                        .iter()
                        .position(|used| !used)
                        .expect("a free VMCE role slot");
                    used_roles[free] = true;
                    free as u8
                };
                crate::skirmish_launch::VmceHouseAssignment {
                    participant_name: mutations(human),
                    raw_slot_index: human.raw_slot_index.expect("fixture lobby row"),
                    player_index,
                    owner_name: format!("VeraHouse{index}"),
                }
            })
            .collect(),
        ..Default::default()
    };
    let map_file = launch
        .selected_map_file
        .clone()
        .ok_or_else(|| anyhow::anyhow!("VMCE defeat fixture has no map"))?;
    state
        .match_state
        .session
        .accept_network_vmce_map_config(Some(
            crate::app::match_bootstrap::NetworkVmceMapConfig::new(
                map_file,
                "a".repeat(64),
                crate::map::vmce_config::VmceMapConfig::default(),
            )
            .map_err(|error| anyhow::anyhow!("VMCE fixture map config: {error:?}"))?,
        ))?;
    let session = &mut state.match_state.session;
    for index in 0..launch.humans.len() {
        session.set_network_player_ready(index, true)?;
    }
    session.set_network_map_status(0, 2)?;
    session.set_network_start_seed(1234)?;
    session.prepare_network_timing(2, || 0)?;
    session.freeze_network_start_with_vmce(1234, Some(assignments))?;
    state.match_state.session.enter_vmce_round_ending(
        None,
        true,
        crate::app::match_runtime::session::VmceRoundEnding::Retry,
        target.rows(),
    )?;
    state
        .match_state
        .session
        .vmce_result_set_retry_votes_for_diagnostics(target.votes())?;
    const KILLS: [u32; 4] = [12, 7, 4, 2];
    const LOSSES: [u32; 4] = [3, 5, 2, 1];
    const BUILT: [u32; 4] = [40, 31, 18, 9];
    const SCORE: [i32; 4] = [900, 640, 410, 220];
    let statistics = VmceStatistics {
        rows: (0..target.rows())
            .map(|role| VmceStatisticsRow {
                player: if role == 0 {
                    "LiveHost".into()
                } else {
                    format!("LiveGuest{role}")
                },
                owner: roles[role].owner.clone(),
                rgb: roles[role].color,
                kills: KILLS[role],
                losses: LOSSES[role],
                built: BUILT[role],
                score: SCORE[role],
                income: [5_000, 3_200, 2_100, 1_200][role],
            })
            .collect(),
        total_time: "00:12:00".into(),
        par_time: "00:15:00".into(),
    };
    // The VMCE defeat page owns its content; the fixture only supplies the
    // statistics projection.
    crate::app::vmce_result::defeat::install_defeat_page(state, statistics);
    let _ = App::ensure_skirmish_shell_chrome(state);
    Ok(())
}
