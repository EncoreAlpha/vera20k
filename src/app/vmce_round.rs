//! App-level continuation for the optional VMCE state attached to BC6.
//!
//! `MatchSession` owns round data and protocol decisions; this module routes
//! its VMCE-only steps to the existing loader, lobby and campaign-page owners.

use crate::app::match_runtime::session::{VmceRoundEnding, VmceRoundStep};
use crate::app::{App, AppState};

pub(crate) fn run_step(state: &mut AppState, step: VmceRoundStep) {
    match step {
        VmceRoundStep::Assign { ending } => {
            if let Err(error) = assign(state, ending) {
                log::error!("VMCE round assignment failed: {error:#}");
                if let Err(error) = state.match_state.session.quit_cooperative_round() {
                    log::warn!("VMCE round quit refused: {error:#}");
                }
                crate::app::cooperative_round::finish_result(state);
            }
        }
        VmceRoundStep::Next => {
            if state.match_state.session_mode()
                == crate::app::match_runtime::session::SessionMode::Lan
            {
                unlock_confirmed_stage(state);
                advance_intermission(state);
            } else {
                crate::app::cooperative_round::begin_next_mission(state);
            }
        }
        VmceRoundStep::Retry => retry_last_mission(state),
        VmceRoundStep::CampaignEnd => {
            if crate::app::lan_service::vmce_round::defer_campaign_completion(state) {
                return;
            }
            App::vmce_campaign_end(state);
        }
    }
}

fn assign(state: &mut AppState, ending: VmceRoundEnding) -> anyhow::Result<()> {
    if ending == VmceRoundEnding::Next
        && state.match_state.session_mode() == crate::app::match_runtime::session::SessionMode::Lan
    {
        crate::app::lan_service::vmce_round::begin_vmce_intermission_transition(state)?;
        if !crate::app::lan_service::vmce_round::vmce_intermission_transition_ready(state)? {
            return Ok(());
        }
    }
    let difficulty = state
        .frontend
        .multiplayer_shell_runtime
        .cooperative_difficulty();
    state
        .match_state
        .session
        .cooperative_round_assigned(difficulty)
}

/// Reopen the resolved next stage after every peer agreed on the result.
pub(crate) fn advance_intermission(state: &mut AppState) {
    if state.match_state.session.host()
        && state
            .match_state
            .session
            .network_connected_source_indices()
            .len()
            == 1
        && let Err(error) =
            crate::app::lan_service::vmce_round::begin_vmce_intermission_transition(state)
    {
        log::error!("VMCE solo intermission preparation failed: {error:#}");
        crate::app::cooperative_round::return_to_shell(state);
        return;
    }
    match crate::app::lan_service::vmce_round::enter_vmce_intermission(state) {
        Ok(false) => {}
        Ok(true) => {
            state.frontend.campaign_score_page = None;
            state.frontend.campaign_score_art = None;
            crate::app::vmce_result::close_vmce_result(state);
            if let Err(error) = crate::app::shell_network_setup::open_vmce_intermission(state) {
                log::error!("Could not open VMCE intermission: {error:#}");
                crate::app::cooperative_round::return_to_shell(state);
            }
        }
        Err(error) => {
            log::error!("VMCE intermission handoff failed: {error:#}");
            crate::app::cooperative_round::return_to_shell(state);
        }
    }
}

/// Result 1: `+0xAC` keeps the agreed seed and continuation timing, then
/// Start_Scenario loads the selected mission: the next one after a win, the
/// same one after a defeat.
/// RESIDUAL: Start_Scenario's retry loop (`0x00685C47`) has no VERA
/// counterpart; a failed load returns to the lobby.
/// RESIDUAL: the scenario movies around a continuation (Win, PostScore and
/// PreMapSelect after a victory, Lose after a defeat, then Intro/Brief and
/// Action on a victory) have no owner. No retail co-op map sets them.
/// VMCE defeat retry reopens the same frozen stage, seed and role assignments;
/// its freeze payload binds the new load attempt to the result nonce.
fn retry_last_mission(state: &mut AppState) {
    state.frontend.campaign_score_page = None;
    state.frontend.coop_score_page = None;
    state.frontend.campaign_score_art = None;
    crate::app::vmce_result::close_vmce_result(state);
    let game_speed = state
        .match_state
        .session
        .network_setup()
        .map_or(0, |launch| launch.options.game_speed);
    let (max_ahead, latency_fudge) =
        crate::net::lockstep::cooperative_continuation_timing(game_speed, &[]);
    match state
        .match_state
        .session
        .freeze_cooperative_round(max_ahead, latency_fudge, None)
    {
        Ok(startup) => crate::app::loading::pump::begin_loading(
            state,
            crate::app::loading::pump::LoadingRequest::network(startup),
        ),
        Err(error) => {
            log::error!("Could not retry the VMCE mission: {error:#}");
            crate::app::cooperative_round::return_to_shell(state);
        }
    }
}

fn unlock_confirmed_stage(state: &AppState) {
    let Some(map) = state.match_state.session.vmce_round_next_stage_map() else {
        return;
    };
    // A completed mission unlocks its resolved successor even if loading it
    // later fails.
    crate::app::vmce_campaign::unlock_catalog_stage(state, map);
}
